//! Reads the real IR tree of the target package.
//!
//! The fixture is the 432-module IR of `lean-projects` (the `InformationTheory`
//! package on Mathlib), which is what every number in `docs/` is measured on.
//! It lives outside the repository — 16 MB of generated JSON — so these tests
//! are `#[ignore]`d rather than silently skipped: `cargo test` has to pass on a
//! machine that has never run the extractor, and a run that reports them as
//! ignored says out loud that they did not run. A `return` did not.
//!
//! Point the environment variable `LEAN_DOC_BASE_IR` at another tree to run the
//! structural half against it; the exact counts below are specific to this
//! fixture and are then skipped too.
//!
//! Every count asserted here was taken from the fixture directly (実測, by
//! enumerating the JSON), not from a previous run of this reader. The point is
//! that the reader agrees with the writer, so the expected values must not come
//! from the reader.

use std::path::PathBuf;

use lean_doc_ir::{Decl, IrTree, Member, ModuleFile, Span, SpanKind, Utf16Text};

const DEFAULT_IR: &str = "/private/tmp/lean-doc-relay/w7h/base-ir";

/// The fixture, or a panic naming what to set.
///
/// Every caller is `#[ignore]`d, so reaching this function at all means the
/// corpus gate asked for the test by name. Returning "not here, never mind"
/// there would be a green result for a comparison that never ran.
fn fixture() -> PathBuf {
    let path =
        PathBuf::from(std::env::var("LEAN_DOC_BASE_IR").unwrap_or_else(|_| DEFAULT_IR.to_owned()));
    assert!(
        path.join("index.json").is_file(),
        "no IR tree at {}: set LEAN_DOC_BASE_IR, or run this test through \
         tools/corpus-gate.sh, which is the only thing that should be asking for it",
        path.display()
    );
    path
}

/// True when the fixture is the one the exact counts were measured on.
fn is_default_fixture(path: &std::path::Path) -> bool {
    path == std::path::Path::new(DEFAULT_IR)
}

/// Every (text, spans) pair in a declaration. The five carriers of plan §7's
/// UTF-16 offsets, and the only fields that pair a text with positions.
fn tagged(decl: &Decl) -> Vec<(&Utf16Text, &[Span])> {
    let mut out: Vec<(&Utf16Text, &[Span])> = Vec::new();
    for (i, text) in decl.binders.iter().enumerate() {
        out.push((text, decl.binder_code.get(i).map_or(&[][..], Vec::as_slice)));
    }
    out.push((&decl.ty, &decl.type_code));
    for (i, text) in decl.equations.iter().enumerate() {
        out.push((
            text,
            decl.equation_code.get(i).map_or(&[][..], Vec::as_slice),
        ));
    }
    for member in &decl.members {
        out.push((&member.text, &member.code));
        for (i, text) in member.binders.iter().enumerate() {
            out.push((
                text,
                member.binder_code.get(i).map_or(&[][..], Vec::as_slice),
            ));
        }
    }
    out
}

#[derive(Default)]
struct Counts {
    modules: usize,
    declarations: usize,
    module_docs: usize,
    tactics: usize,
    members: usize,
    members_field: usize,
    /// Field members that actually carry `isDirect`.
    ///
    /// Counted because it is what makes the byte comparison blind to the
    /// default: if every field has the key, `Option<bool>` and a `false`
    /// default produce the same pages, and only the type says which reading is
    /// right (plan §5, `Member::is_direct`).
    members_field_is_direct_present: usize,
    members_field_inherited: usize,
    members_ctor: usize,
    members_parent: usize,
    with_attrs: usize,
    with_inst_class: usize,
    refs: usize,
    /// Tagged fragments, and how many of them are not pure ASCII.
    fragments: usize,
    fragments_non_ascii: usize,
    /// Fragments containing at least one scalar above U+FFFF — the case where
    /// UTF-16 and UTF-8 offsets disagree by more than a constant factor and a
    /// slice can land inside a surrogate pair.
    fragments_astral: usize,
    /// Spans whose UTF-16 start is not equal to its byte offset.
    spans_offset_shifted: usize,
    spans_by_arity: [usize; 3],
    spans_by_kind: [usize; 3],
    /// Fragments whose parallel arrays disagree in length.
    ragged_arrays: usize,
}

impl Counts {
    fn add_module(&mut self, module: &ModuleFile) {
        self.modules += 1;
        self.module_docs += module.module_docs.len();
        self.tactics += module.tactics.len();
        for decl in &module.declarations {
            self.declarations += 1;
            self.refs += decl.refs.len();
            if !decl.attrs.is_empty() {
                self.with_attrs += 1;
            }
            if decl.inst_class.is_some() {
                self.with_inst_class += 1;
            }
            if decl.binders.len() != decl.implicits.len()
                || decl.binders.len() != decl.binder_code.len()
                || decl.equations.len() != decl.equation_code.len()
            {
                self.ragged_arrays += 1;
            }
            for member in &decl.members {
                self.add_member(member);
            }
            for (text, spans) in tagged(decl) {
                self.add_fragment(module, decl, text, spans);
            }
        }
    }

    fn add_member(&mut self, member: &Member) {
        self.members += 1;
        match member.label.as_str() {
            "field" => {
                self.members_field += 1;
                if member.is_direct.is_some() {
                    self.members_field_is_direct_present += 1;
                }
                if member.is_inherited() {
                    self.members_field_inherited += 1;
                }
            }
            "ctor" => self.members_ctor += 1,
            "parent" => self.members_parent += 1,
            other => panic!("unknown member label {other:?}"),
        }
        // The five schema-4 keys arrive as a group, on field members only.
        let has_extras = !member.binders.is_empty()
            || !member.implicits.is_empty()
            || !member.binder_code.is_empty()
            || member.doc.is_some()
            || member.is_direct.is_some();
        assert!(
            member.is_field() || !has_extras,
            "{} is a {:?} member but carries schema-4 field keys",
            member.name,
            member.label
        );
        if member.binders.len() != member.implicits.len()
            || member.binders.len() != member.binder_code.len()
        {
            self.ragged_arrays += 1;
        }
    }

    fn add_fragment(&mut self, module: &ModuleFile, decl: &Decl, text: &Utf16Text, spans: &[Span]) {
        self.fragments += 1;
        if !text.is_ascii() {
            self.fragments_non_ascii += 1;
        }
        if text.as_str().chars().any(|c| c as u32 > 0xFFFF) {
            self.fragments_astral += 1;
        }
        let where_ = || format!("{}::{} fragment {text:?}", module.module, decl.name);
        for span in spans {
            match span.name {
                Some(_) => {
                    self.spans_by_arity[if span.front == 0 && span.back == 0 {
                        1
                    } else {
                        2
                    }] += 1;
                }
                None => self.spans_by_arity[0] += 1,
            }
            match span.kind {
                SpanKind::Fn => self.spans_by_kind[0] += 1,
                SpanKind::Const => self.spans_by_kind[1] += 1,
                SpanKind::Sort => self.spans_by_kind[2] += 1,
                SpanKind::Other(code) => panic!("unexpected span kind {code} in {}", where_()),
            }
            assert_eq!(
                span.name.is_some(),
                span.kind == SpanKind::Const,
                "a name and kind 1 must imply each other, in {}",
                where_()
            );

            // The claim under test: an IR offset is a UTF-16 code unit offset,
            // and it always lands on a scalar boundary of the fragment.
            assert!(
                span.stop <= text.len_utf16(),
                "span {}..{} past the {} units of {}",
                span.start,
                span.stop,
                text.len_utf16(),
                where_()
            );
            assert!(
                text.get(span.range()).is_some(),
                "span {}..{} is not a slice boundary of {}",
                span.start,
                span.stop,
                where_()
            );
            if text.byte_offset(span.start) != Some(span.start as usize) {
                self.spans_offset_shifted += 1;
            }

            // The whitespace `splitWhitespaces` rewrites as plain spaces must
            // really be whitespace, or the widths do not mean what schema 3
            // says they mean.
            for range in [span.front_range(), span.back_range()]
                .into_iter()
                .flatten()
            {
                assert!(
                    range.end <= text.len_utf16(),
                    "whitespace width {range:?} past the end of {}",
                    where_()
                );
                for at in range {
                    let unit = text.unit(at).expect("unit inside the fragment");
                    assert!(
                        char::from_u32(u32::from(unit)).is_some_and(char::is_whitespace),
                        "whitespace width covers U+{unit:04X} in {}",
                        where_()
                    );
                }
            }
        }
    }
}

#[test]
#[ignore = "corpus: needs LEAN_DOC_BASE_IR (tools/corpus-gate.sh)"]
fn reads_every_module_of_the_target_package() {
    let root = fixture();
    let tree = IrTree::open(&root).expect("the fixture is a schema-4 IR");
    let index = tree.index();

    assert_eq!(index.schema_version, 4);
    assert!(index.ablations.is_empty());
    assert_eq!(index.modules.len(), index.module_count as usize);

    let mut counts = Counts::default();
    for (entry, module) in index.modules.iter().zip(tree.modules()) {
        let module = module.unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(module.module, entry.module);
        assert_eq!(module.declarations.len(), entry.declarations as usize);
        // The content hash is carried, never recomputed (plan §7).
        assert_eq!(entry.content_hash.len(), 16, "{}", entry.module);
        assert!(
            entry.content_hash.bytes().all(|b| b.is_ascii_hexdigit()),
            "{}: contentHash {:?} is not hex",
            entry.module,
            entry.content_hash
        );
        assert_eq!(
            std::fs::metadata(tree.path(&entry.file))
                .expect("module file")
                .len(),
            entry.bytes,
            "{}: index.bytes disagrees with the file",
            entry.module
        );
        counts.add_module(&module);
    }

    assert_eq!(counts.declarations, index.declaration_count as usize);
    assert_eq!(
        counts.ragged_arrays, 0,
        "parallel arrays (binders/implicits/binderCode, equations/equationCode) must line up"
    );

    let deps = tree.load_dep_maps().expect("dependency slices");
    assert_eq!(deps.len(), index.dependency_maps.len());
    for (entry, dep) in index.dependency_maps.iter().zip(&deps) {
        assert_eq!(dep.package, entry.package);
        assert_eq!(dep.declarations.len(), entry.entries as usize);
        assert_eq!(dep.schema_version, index.schema_version);
    }

    if !is_default_fixture(&root) {
        eprintln!(
            "structural checks only: {} is not the fixture the exact counts were measured on",
            root.display()
        );
        return;
    }

    // 実測, enumerated from the fixture's JSON.
    assert_eq!(counts.modules, 432);
    assert_eq!(counts.declarations, 4_750);
    assert_eq!(counts.module_docs, 1_515);
    assert_eq!(counts.tactics, 0, "this package declares no tactics");
    assert_eq!(counts.members, 194);
    assert_eq!(counts.members_field, 156);
    // Every field carries `isDirect`, which is exactly why the default cannot
    // be checked by comparing pages: `Option<bool>` and a `false` default agree
    // on all 156. Only 4 of them are inherited.
    assert_eq!(counts.members_field_is_direct_present, 156);
    assert_eq!(counts.members_field_inherited, 4);
    assert_eq!(counts.members_ctor, 37);
    assert_eq!(counts.members_parent, 1);
    assert_eq!(counts.with_attrs, 145);
    assert_eq!(counts.with_inst_class, 91);
    assert_eq!(counts.refs, 56_552);
    assert_eq!(counts.spans_by_arity, [266_722, 112_983, 29_251]);
    assert_eq!(counts.spans_by_kind, [259_172, 142_234, 7_550]);
    assert_eq!(counts.fragments, 55_514);
    assert_eq!(counts.fragments_non_ascii, 45_498);
    assert_eq!(counts.fragments_astral, 41);
    assert_eq!(
        deps.iter().map(|d| d.declarations.len()).sum::<usize>(),
        533
    );
    assert_eq!(index.lean_version, "4.31.0");
    assert_eq!(index.hash_algorithm, "lean-string-hash-64/hex16");
    // If this were zero the UTF-16 translation would be untested by the fixture.
    assert!(
        counts.spans_offset_shifted > 100_000,
        "only {} spans had a UTF-16 offset different from their byte offset",
        counts.spans_offset_shifted
    );
}

/// The one case plan §9 says the port fails on: a name above U+FFFF, where a
/// UTF-16 offset is two units for one scalar. `𝓧` (U+1D4E7) really occurs in
/// this package's binders.
#[test]
#[ignore = "corpus: needs LEAN_DOC_BASE_IR (tools/corpus-gate.sh)"]
fn astral_binders_slice_correctly() {
    let root = fixture();
    let tree = IrTree::open(&root).expect("the fixture is a schema-4 IR");
    let mut checked = 0;
    for module in tree.modules() {
        let module = module.expect("module");
        for decl in &module.declarations {
            for (text, spans) in tagged(decl) {
                if !text.as_str().chars().any(|c| c as u32 > 0xFFFF) {
                    continue;
                }
                for span in spans {
                    let slice = text.slice(span.range());
                    // A byte-indexed reader would have produced something else.
                    let naive = text.as_str().get(span.start as usize..span.stop as usize);
                    if naive != Some(slice) {
                        checked += 1;
                    }
                }
            }
        }
    }
    if is_default_fixture(&root) {
        assert!(
            checked > 0,
            "no span in an astral fragment distinguished UTF-16 from byte offsets"
        );
    }
}

/// A field the extractor starts emitting must not be dropped silently.
#[test]
fn unknown_fields_are_rejected() {
    let json = r#"{"col":0,"line":1,"text":"hi","surprise":true}"#;
    let err = serde_json::from_str::<lean_doc_ir::ModuleDoc>(json).unwrap_err();
    assert!(err.to_string().contains("surprise"), "{err}");
}
