//! The second oracle: the docstrings **inside the reference pages**.
//!
//! `tools/render-reference.sh` ran the whole of `experiments/stage7d/render.ts`
//! over the target package's IR with the real `--link-index` and wrote 432
//! pages. Those pages are the byte target for M1-d, and each of them already
//! contains this milestone step's output — every docstring, rendered with the
//! page's *own* root and its *own* declaration list.
//!
//! **Neither the script nor the prototype is in this tree any more.** Both went
//! with `experiments/` on 2026-08-16 and exist only at tag `experiments-frozen`;
//! the pages themselves only ever lived under /private/tmp, so this test is
//! env-gated and `#[ignore]`d.
//!
//! # Why this exists next to `tests/autolink.rs`
//!
//! That file compares against the prototype's `renderDocString` sliced out of
//! render.ts and given a context this repository builds. One input to it is not
//! a slice: `moduleDeclNames`, the list `nameToLink`'s last branch scans, which
//! the generator lifts out of `pageHtml` as an expression. If that lift were
//! wrong, both sides would be handed the same wrong list and would agree.
//!
//! The reference pages were produced by the whole program, so the list in them
//! is the real one. This file is therefore what stands behind the claim that
//! the last branch scans what doc-gen4 scans — and, unlike a page-level byte
//! diff, it says *which* docstring moved rather than which file.
//!
//! # What is compared
//!
//! Docstrings appear on a page in three places, and none of them can be
//! confused with the surrounding template, because a rendered docstring
//! contains no `<div>` and no `<details>` — `MD_FLAG_NOHTML` sees to that and
//! `renderBlock` emits neither:
//!
//! | | delimiter | checked |
//! |---|---|---|
//! | module docs | `<div class="mod_doc">` … `</div>` | in order, byte for byte |
//! | declaration docs | after the `decl_header`'s `</div>` | byte for byte, plus the byte that follows |
//! | structure field docs | `<div class="structure_field_doc">` … `</div>` | each must be some member's docstring |
//!
//! # Why it is `#[ignore]`d
//!
//! The IR, the `.lidx` and the reference tree all live outside the repository,
//! so this test does not run under a plain `cargo test` — and, being ignored
//! rather than silently returning, it says so in the result line. Point
//! `LITEDOC4_IR`, `LITEDOC4_LINK_INDEX` and `LITEDOC4_REF_PAGES` elsewhere to
//! run it against another corpus.

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use litedoc4_ir::{IrTree, ModuleFile};
use litedoc4_render::{
    ExternalLinks, LinkIndex, NameIndex, PageLinks, escape_html, module_decl_names, page_root,
};

const DEFAULT_IR: &str = "/private/tmp/lean-doc-relay/w7h/base-ir";
const DEFAULT_LINK_INDEX: &str = "/private/tmp/lean-doc-relay/w7c/linkindex/link-index.lidx";
const DEFAULT_REF_PAGES: &str = "/private/tmp/lean-doc-relay/m1/ref-pages";

/// What may follow a declaration's docstring on a page: the rest of the
/// declaration, or the end of it. None of these can begin a docstring.
const TERMINATORS: [&str; 4] = ["</div>", "<div", "<details", "<ul class=\"structure_"];

/// The one docstring of the package where the prototype's hand-written
/// CommonMark subset and md4c disagree — a code span with nested backticks,
/// which the subset's `indexOf` scan closes in the wrong place.
///
/// It is *not* an autolink difference: the same input differs under `NoLinks`,
/// and doc-gen4 produces this crate's bytes 【実測 → `litedoc4-md`'s
/// `tests/ts_docstring.rs`, where it is the only real docstring in a list of
/// 41】. Plan §5 decides such cases in md4c's favour, so the reference page is
/// wrong here and this test says which one rather than how many.
const KNOWN_SUBSET_DIVERGENCE: &str =
    "InformationTheory.Shannon.TimeBandLimiting.Count module doc 1";

fn env_path(var: &str, default: &str) -> PathBuf {
    PathBuf::from(std::env::var(var).unwrap_or_else(|_| default.to_owned()))
}

/// The IR, the `.lidx` and the reference pages, or a panic naming what to set.
///
/// The only caller is `#[ignore]`d, so reaching this function at all means the
/// corpus gate asked for the test by name. Returning "not here, never mind"
/// there would be a green result for a comparison that never ran.
fn inputs() -> (PathBuf, PathBuf, PathBuf) {
    let ir = env_path("LITEDOC4_IR", DEFAULT_IR);
    let lidx = env_path("LITEDOC4_LINK_INDEX", DEFAULT_LINK_INDEX);
    let pages = env_path("LITEDOC4_REF_PAGES", DEFAULT_REF_PAGES);
    // Counted in **files**: `/private/tmp` is swept, and an emptied `ref-pages`
    // leaves its directory behind — `exists()` said yes to that and every page
    // then failed to read, which is an environmental failure wearing a
    // regression's clothes (`litedoc4-incr/tests/impact.rs` documents it).
    for (what, var, path) in [
        ("IR", "LITEDOC4_IR", &ir),
        (".lidx", "LITEDOC4_LINK_INDEX", &lidx),
        ("reference pages", "LITEDOC4_REF_PAGES", &pages),
    ] {
        assert!(
            file_count(path) != 0,
            "no {what} at {} (empty or missing): set {var}, or run this test through \
             tools/corpus-gate.sh, which is the only thing that should be asking for it",
            path.display()
        );
    }
    (ir, lidx, pages)
}

/// Regular files at or under `path`; 1 for a file, 0 for a missing path.
fn file_count(path: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(path) else {
        return usize::from(path.is_file());
    };
    entries
        .flatten()
        .map(|entry| match entry.file_type() {
            Ok(kind) if kind.is_dir() => file_count(&entry.path()),
            Ok(kind) if kind.is_file() => 1,
            _ => 0,
        })
        .sum()
}

/// The text between `open` and the next `</div>`, and where it started.
///
/// No nesting count is needed: a rendered docstring contains no `<div>`, so the
/// first close is the wrapper's.
fn wrapped<'a>(page: &'a str, open: &str, from: usize) -> Option<(usize, &'a str)> {
    let at = page[from..].find(open)? + from + open.len();
    let end = page[at..].find("</div>")? + at;
    Some((at, &page[at..end]))
}

/// The byte just past the `</div>` that closes the `<div` starting at `start`.
///
/// The nesting count is not optional here: `decl_header` contains a
/// `div.decl_type`, so its *first* `</div>` is not its own — reading it as such
/// puts the docstring's start one element too early and every comparison fails
/// with an empty page-side region.
fn after_matching_div(page: &str, start: usize) -> Option<usize> {
    debug_assert!(page[start..].starts_with("<div"));
    let mut depth = 0usize;
    let mut at = start;
    loop {
        let open = page[at..].find("<div");
        let close = page[at..].find("</div>")?;
        if open.is_some_and(|o| o < close) {
            depth += 1;
            at += open.expect("checked") + "<div".len();
        } else {
            depth -= 1;
            at += close + "</div>".len();
            if depth == 0 {
                return Some(at);
            }
        }
    }
}

/// Every docstring of every reference page, compared with what this crate
/// renders for it.
#[test]
#[ignore = "corpus: needs LITEDOC4_IR + LITEDOC4_LINK_INDEX + LITEDOC4_REF_PAGES (tools/corpus-gate.sh)"]
fn every_docstring_in_the_reference_pages_is_reproduced() {
    let (ir, lidx, pages) = inputs();

    let tree = IrTree::open(&ir).expect("the IR opens");
    let modules = tree.load_modules().expect("every module reads");
    let mut builder = NameIndex::builder();
    for dep in tree.load_dep_maps().expect("every dependency slice reads") {
        builder.dep_map(&dep);
    }
    for module in &modules {
        builder.module(module);
    }
    // M7-c: the prototype had no dependency map, so its bytes are the
    // **fallback** branch — which an empty [`ExternalLinks`] reproduces
    // exactly. With a map every link into a dependency moves, on purpose
    // (`docs/implementation-plan.md` §1).
    // 2026-08-17: and the prototype rendered the whole environment, so its
    // links point at pages it wrote. A run's world has pages for the target
    // package alone; the oracle is resolved in the world it was recorded in.
    let index = builder.build_with_a_page_for_every_module(
        LinkIndex::read(&lidx).expect("the .lidx reads"),
        ExternalLinks::default(),
    );

    // `DocInfo.ofConstant` sets `render := false` for projection functions and
    // constructors, i.e. exactly the names that appear as another declaration's
    // members (`render.ts:2039-2048`). Those get no `div.decl` on any page.
    let suppressed: HashSet<&str> = modules
        .iter()
        .flat_map(|m| m.declarations.iter())
        .flat_map(|d| d.members.iter())
        .map(|m| m.name.as_str())
        .collect();

    let mut counts = Counts::default();
    let mut failures = Vec::new();
    for module in &modules {
        let path = page_path(&pages, &module.module);
        let page =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        check_page(
            module,
            &page,
            &index,
            &suppressed,
            &mut counts,
            &mut failures,
        );
    }

    eprintln!(
        "{} pages: {} module docs, {} declaration docs, {} field docs, {} anchors",
        modules.len(),
        counts.module_docs,
        counts.decl_docs,
        counts.field_docs,
        counts.anchors,
    );
    assert!(
        failures.is_empty(),
        "{} of {} docstrings differ from the reference pages:\n{}",
        failures.len(),
        counts.total(),
        failures
            .iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    // A run that found nothing would pass every assertion above.
    assert!(
        counts.module_docs > 500,
        "{} module docs",
        counts.module_docs
    );
    assert!(
        counts.decl_docs > 2_000,
        "{} declaration docs",
        counts.decl_docs
    );
    assert!(counts.anchors > 1_000, "{} anchors", counts.anchors);
    // Named, not tolerated: if a second reference docstring starts differing,
    // it is a new fault and not this one.
    assert_eq!(
        counts.known_divergences, 1,
        "{KNOWN_SUBSET_DIVERGENCE} is meant to be the only reference docstring \
         this crate does not reproduce"
    );
}

#[derive(Default)]
struct Counts {
    known_divergences: usize,
    module_docs: usize,
    decl_docs: usize,
    field_docs: usize,
    anchors: usize,
}

impl Counts {
    fn total(&self) -> usize {
        self.module_docs + self.decl_docs + self.field_docs
    }
}

fn page_path(pages: &Path, module: &str) -> PathBuf {
    let mut path = pages.to_path_buf();
    for part in module.split('.') {
        path.push(part);
    }
    path.set_extension("html");
    path
}

fn check_page(
    module: &ModuleFile,
    page: &str,
    index: &NameIndex,
    suppressed: &HashSet<&str>,
    counts: &mut Counts,
    failures: &mut Vec<String>,
) {
    let root = page_root(&module.module);
    let names = module_decl_names(module);
    let links = PageLinks::new(index, &root, &names);
    let renderer = links.renderer();

    // Module docs, in page order, which is file order among themselves.
    let mut from = 0;
    for (i, doc) in module.module_docs.iter().enumerate() {
        let what = format!("{} module doc {i}", module.module);
        let Some((at, region)) = wrapped(page, "<div class=\"mod_doc\">", from) else {
            failures.push(format!("{what}: no mod_doc left on the page"));
            continue;
        };
        from = at + region.len();
        counts.module_docs += 1;
        counts.anchors += region.matches("<a href=").count();
        record(
            failures,
            counts,
            &what,
            region,
            &renderer.docstring(&doc.text),
        );
    }

    // Declaration docs: no wrapper of their own, so they are found by the
    // element that always precedes them and bounded by the one that follows.
    for decl in &module.declarations {
        let Some(doc) = &decl.doc else { continue };
        if suppressed.contains(decl.name.as_str()) {
            continue;
        }
        let what = decl.name.clone();
        let key = format!("<div class=\"decl\" id=\"{}\">", escape_html(&decl.name));
        let Some(start) = page.find(&key) else {
            failures.push(format!("{what}: no div.decl on the page"));
            continue;
        };
        let Some(header) = page[start..].find("<div class=\"decl_header\">") else {
            failures.push(format!("{what}: no decl_header"));
            continue;
        };
        let Some(at) = after_matching_div(page, start + header) else {
            failures.push(format!("{what}: unterminated decl_header"));
            continue;
        };
        let got = renderer.docstring(doc);
        counts.decl_docs += 1;
        counts.anchors += got.matches("<a href=").count();
        let rest = &page[at..];
        if !rest.starts_with(&got) {
            let want_len = TERMINATORS
                .iter()
                .filter_map(|t| rest.find(t))
                .min()
                .unwrap_or(rest.len())
                .max(got.len() + 60)
                .min(rest.len());
            record(failures, counts, &what, &rest[..want_len], &got);
            continue;
        }
        // A truncated render would still be a prefix, so the boundary is
        // checked too: what follows has to be the next element, not more text.
        let after = &rest[got.len()..];
        assert!(
            TERMINATORS.iter().any(|t| after.starts_with(t)),
            "{what}: the docstring does not end where the page says it does, \
             next bytes are {:?}",
            &after[..after.len().min(40)]
        );
    }

    // Structure field docs: which members get one is `structureHtml`'s
    // business, i.e. M1-d's. What is checked here is that each one on the page
    // is a docstring of this module rendered by this crate.
    let expected: BTreeSet<String> = module
        .declarations
        .iter()
        .flat_map(|d| d.members.iter())
        .filter_map(|m| m.doc.as_deref())
        .map(|doc| renderer.docstring(doc))
        .collect();
    let mut from = 0;
    while let Some((at, region)) = wrapped(page, "<div class=\"structure_field_doc\">", from) {
        from = at + region.len();
        counts.field_docs += 1;
        counts.anchors += region.matches("<a href=").count();
        if !expected.contains(region) {
            failures.push(format!(
                "{}: a structure_field_doc is not any member's docstring rendered here\n  page: {}",
                module.module,
                &region[..region.len().min(200)]
            ));
        }
    }
}

/// Records a mismatch, pointing at the first byte where the two part company.
///
/// The one docstring the prototype's CommonMark subset gets wrong is counted
/// rather than reported; everything else is a fault of this crate.
fn record(failures: &mut Vec<String>, counts: &mut Counts, what: &str, want: &str, got: &str) {
    if want == got {
        return;
    }
    if what == KNOWN_SUBSET_DIVERGENCE {
        counts.known_divergences += 1;
        return;
    }
    failures.push(format!(
        "{what}\n  page: {}\n  here: {}",
        first_difference(want, got),
        first_difference(got, want)
    ));
}

/// The first place two strings part company, with context, so a failure is
/// readable when both sides are 2 kB of HTML.
fn first_difference(a: &str, b: &str) -> String {
    let at = a
        .char_indices()
        .zip(b.char_indices())
        .find(|((_, x), (_, y))| x != y)
        .map_or_else(|| a.len().min(b.len()), |((i, _), _)| i);
    let from = a[..at].char_indices().rev().nth(40).map_or(0, |(i, _)| i);
    let to = a[at..]
        .char_indices()
        .nth(60)
        .map_or(a.len(), |(i, _)| at + i);
    format!("…{}", &a[from..to])
}
