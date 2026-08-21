//! The IR's schema-5 shapes, as the extractor writes them.
//!
//! The authority for every field here is `experiments/stage7d/Extract.lean`
//! (`declToIrJson` / `writeIRTree`), not the TypeScript prototype: the writer is
//! what decides which keys exist and which are omitted. Where the prototype's
//! `render.ts` reads *fewer* fields than the writer emits, this module still
//! models the writer's — see the notes on [`IndexEntry::content_hash`] and
//! [`ModuleFile::tactics`], both of which other stages consume.
//!
//! Every struct is `deny_unknown_fields`. A field the extractor starts emitting
//! and this crate does not know about is then a loud parse failure instead of a
//! silent drop; the schema version is what governs compatibility, so there is no
//! forward-compatibility left for tolerant parsing to buy.
//!
//! Keys are alphabetically ordered on the wire (Lean's `Json.mkObj` is backed by
//! a sorted map). That matters for writing, which this crate does not do; for
//! reading it is irrelevant, and nothing here depends on field order.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;

use serde::de::{self, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};

use crate::{Span, Utf16Text};

/// `index.json` — the package index.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Index {
    pub schema_version: u32,
    /// The extractor's identity string, part of the extraction cache key
    /// (plan §6: it must change when the implementation does).
    pub generator: String,
    pub lean_version: String,
    /// Names the algorithm behind [`IndexEntry::content_hash`]; currently
    /// `lean-string-hash-64/hex16`.
    pub hash_algorithm: String,
    pub module_count: u32,
    pub declaration_count: u32,
    /// Present only when the extractor ran with an ablation flag, and then it
    /// is a refusal marker: the IR is deliberately incomplete and rendering it
    /// would produce a page that looks fine and is wrong. See
    /// [`Index::require_renderable`].
    #[serde(default)]
    pub ablations: Vec<String>,
    pub modules: Vec<IndexEntry>,
    pub dependency_maps: Vec<DepMapEntry>,
}

/// One module's entry in `index.json`.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IndexEntry {
    pub module: String,
    /// Path relative to the IR root, e.g. `modules/Foo.Bar.json`. The file name
    /// is the module's full name with no directories.
    pub file: String,
    /// The writer's `String.utf8ByteSize` of the module file.
    pub bytes: u64,
    pub declarations: u32,
    /// Lean's `String.hash` of the module JSON, 16 hex digits.
    ///
    /// **Never recomputed on this side** (plan §7): the extractor stays in
    /// Lean, so reading the value is enough, and recomputing would mean porting
    /// `lean_string_hash`. It is the key the IR cache of plan §3 hangs off.
    pub content_hash: String,
}

/// One dependency slice's entry in `index.json`.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DepMapEntry {
    /// The dependency's package root, e.g. `Mathlib`.
    pub package: String,
    /// Path relative to the IR root, e.g. `deps/Mathlib.json`.
    pub file: String,
    pub entries: u32,
    pub bytes: u64,
}

impl Index {
    /// Rejects an IR that must not be rendered: too old a schema, or written
    /// with an ablation. `render.ts` exits on both before doing anything else.
    pub fn require_renderable(&self) -> Result<(), crate::Error> {
        if self.schema_version < crate::MIN_SCHEMA_VERSION {
            return Err(crate::Error::Schema {
                found: self.schema_version,
                required: crate::MIN_SCHEMA_VERSION,
                what: "index.json".to_owned(),
            });
        }
        if !self.ablations.is_empty() {
            return Err(crate::Error::Ablated {
                ablations: self.ablations.clone(),
            });
        }
        Ok(())
    }
}

/// `modules/<Module.Full.Name>.json` — one module.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModuleFile {
    pub schema_version: u32,
    pub module: String,
    pub imports: Vec<String>,
    pub module_docs: Vec<ModuleDoc>,
    /// Tactic docstrings declared by this module.
    ///
    /// `render.ts` does not model this field at all; `global.ts` counts it for
    /// `tactics.html`, so it is modelled here. **Empty for every module of the
    /// target package** (432 at the revision this was measured at, 422 on
    /// 2026-08-21) — the shape below follows the writer
    /// (`Extract.lean:2007-2011`), not observed data.
    pub tactics: Vec<Tactic>,
    pub declarations: Vec<Decl>,
}

impl ModuleFile {
    /// What this file says about one of its declarations and `sorry`.
    ///
    /// **Three-valued, and it has to be** — the same lesson as
    /// [`Member::is_direct`], in the other direction. A schema-5 writer omits
    /// the key to mean "no `sorry`"; a schema-4 file has no key to omit, so the
    /// same absence means "nobody was asked". Reading [`Decl::sorry`] directly
    /// conflates them into "this package has no holes", which is a claim about
    /// the package made from a fact about the extractor's version.
    ///
    /// This is the only thing that should read [`Decl::sorry`].
    pub fn sorry_of(&self, decl: &Decl) -> SorryFact {
        if self.schema_version < crate::SORRY_SCHEMA_VERSION {
            return SorryFact::Unknown;
        }
        match decl.sorry {
            None => SorryFact::Clean,
            Some(SorryKind::Direct) => SorryFact::Direct,
            Some(SorryKind::Transitive) => SorryFact::Transitive,
        }
    }
}

/// [`ModuleFile::sorry_of`]'s answer: the two claims of doc-gen4 #270, plus the
/// two ways of not making one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SorryFact {
    /// The file predates schema 5. Nothing is known — in particular this is
    /// **not** [`SorryFact::Clean`].
    Unknown,
    /// Schema 5 or newer, and the writer said nothing: no `sorry`.
    Clean,
    /// This declaration's own statement or proof mentions `sorryAx`.
    Direct,
    /// It does not, but something it depends on does.
    Transitive,
}

/// A module-level docstring (`/-! ... -/`).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModuleDoc {
    pub line: u32,
    pub col: u32,
    /// Markdown. Not tagged, so no UTF-16 offsets point into it.
    pub text: String,
}

/// A tactic docstring. Never observed non-empty on the target package.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Tactic {
    pub internal_name: String,
    pub user_name: String,
    pub tags: Vec<String>,
    pub doc_string: String,
}

/// One declaration.
///
/// The fields that carry tag spans come in pairs — text plus span list — and
/// the text side is a [`Utf16Text`] because the spans index it in UTF-16 code
/// units:
///
/// | text | spans |
/// |---|---|
/// | [`Decl::binders`]`[i]` | [`Decl::binder_code`]`[i]` |
/// | [`Decl::ty`] | [`Decl::type_code`] |
/// | [`Decl::equations`]`[i]` | [`Decl::equation_code`]`[i]` |
/// | [`Member::text`] | [`Member::code`] |
/// | [`Member::binders`]`[i]` | [`Member::binder_code`]`[i]` |
///
/// Every other string is an ordinary `String`: names, docstrings and attributes
/// are never indexed by a span.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Decl {
    pub name: String,
    /// `def`, `theorem`, `structure`, `class`, `instance`, ...
    pub kind: String,
    pub modifiers: Vec<String>,
    pub binders: Vec<Utf16Text>,
    /// Parallel to `binders`: whether each binder is implicit.
    pub implicits: Vec<bool>,
    /// Parallel to `binders`.
    pub binder_code: Vec<Vec<Span>>,
    /// The result type. Named `ty` because `type` is a Rust keyword.
    #[serde(rename = "type")]
    pub ty: Utf16Text,
    pub type_code: Vec<Span>,
    pub line: u32,
    pub col: u32,
    pub end_line: u32,
    pub end_col: u32,
    /// Position in the order the extractor enumerated the module. Two
    /// declarations in this package share a `(line, col)`, so the range alone
    /// does not order the page.
    pub index: u32,
    pub members: Vec<Member>,
    pub doc: Option<String>,
    pub equations: Vec<Utf16Text>,
    /// Parallel to `equations`.
    pub equation_code: Vec<Vec<Span>>,
    /// Deduplicated references, `(defining module, name)`.
    pub refs: Vec<Ref>,
    /// Schema 4. Omitted by the writer when empty, so an empty vector here
    /// means "no attributes", never "unknown".
    ///
    /// The **element** shape moved in schema 5 (`docs/plans/feature-sweep.md`
    /// B-2): `[name, value]` where schema 4 had `"name value"`. Both parse —
    /// see [`Attr`].
    #[serde(default)]
    pub attrs: Vec<Attr>,
    /// Schema 4, instances only: the class this instance is for. `None` for
    /// everything else, and then `inst_types` is empty as well.
    #[serde(default)]
    pub inst_class: Option<String>,
    /// Schema 4, instances only.
    #[serde(default)]
    pub inst_types: Vec<String>,
    /// Schema 5 (doc-gen4 #270): whether this declaration is a hole, and whose.
    ///
    /// **Read it through [`ModuleFile::sorry_of`], not directly.** The writer
    /// omits the key when neither value applies, so `None` means "no `sorry`" —
    /// but only in a file that says `schemaVersion` 5, and a schema-4 file has
    /// no key to omit. On its own this field cannot tell the two apart.
    #[serde(default)]
    pub sorry: Option<SorryKind>,
}

/// Which of doc-gen4 #270's two claims a declaration makes.
///
/// They are **different claims** and a reader treats them differently, so this
/// is two values rather than a flag: `Direct` is a hole in this declaration,
/// `Transitive` is a hole somewhere underneath it. A declaration that is both is
/// `Direct` — the stronger claim, and the one that is acted on.
///
/// What the IR deliberately does *not* carry is the axiom set. Every
/// Mathlib-dependent declaration transitively uses `Classical.choice` /
/// `propext` / `Quot.sound`, so the full list is a large field with almost no
/// information in it (`docs/plans/feature-sweep.md` §6 決定 2).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SorryKind {
    /// This declaration's own statement or proof mentions `sorryAx`.
    Direct,
    /// It does not, but something it depends on does.
    Transitive,
}

/// One attribute on a declaration: the attribute's name, and the value the
/// extractor printed for it.
///
/// On the wire (schema 5) it is a two-element array, `["deprecated", "Foo
/// (since := \"2026-05-21\")"]` — the shape [`Ref`] already uses, rather than an
/// object, because the reader is hand-written either way and keys would be two
/// more strings per attribute in every module file.
///
/// # Why the two shapes
///
/// Schema 4 wrote one concatenated string per attribute, `"deprecated Foo"`,
/// and **that file still parses**: a bare string arrives here as a name with an
/// empty [`Attr::value`]. It has to, because
/// [`crate::MIN_SCHEMA_VERSION`] is still 4 and because curated schema-4 IR is
/// frozen inside `litedoc4-global/tests/data/global-expected.json` **as test
/// input**, where hand-editing it is the thing C-4's procedure exists to avoid.
///
/// # What the reader must not do
///
/// Split a schema-4 string on its first space. An attribute value can contain
/// spaces (`deprecated`) and brackets (`specialize #[0, 1]`), so where the
/// boundary is is a fact about the attribute rather than about the string, and
/// the extractor is the only side that has it. Guessing here would be a second
/// answer to a question already answered there; a schema-4 file simply does not
/// carry the answer, and says so by leaving `value` empty.
///
/// The consequence for a consumer that wants to *act* on an attribute — link
/// `@[deprecated Foo]` to `Foo`, style by name — is that it must check
/// [`Attr::value`] is non-empty rather than assume the pair was split.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attr {
    /// `simp`, `deprecated`, `instance`, ... Never empty in practice; the
    /// extractor's four collectors all name their attribute.
    pub name: String,
    /// Empty for the attributes that take no argument, and for **every**
    /// attribute of a schema-4 file.
    pub value: String,
}

impl Attr {
    /// The one string schema 4 carried: the name alone, or `name value`.
    ///
    /// This is what makes the shape change invisible to a renderer that only
    /// prints attributes — a schema-4 string round-trips through
    /// [`Attr::name`] unchanged, and a schema-5 pair rejoins to what the same
    /// extractor used to write.
    pub fn text(&self) -> Cow<'_, str> {
        if self.value.is_empty() {
            Cow::Borrowed(&self.name)
        } else {
            Cow::Owned(format!("{} {}", self.name, self.value))
        }
    }
}

impl<'de> Deserialize<'de> for Attr {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(AttrVisitor)
    }
}

/// Accepts the two wire shapes and **nothing else**.
///
/// In particular an array of any arity but two is an error rather than a
/// best-effort read: a one-element array would otherwise become a name with no
/// value, which is indistinguishable from a legitimate schema-4 string, and a
/// three-element one would silently drop whatever the writer added.
struct AttrVisitor;

impl<'de> Visitor<'de> for AttrVisitor {
    type Value = Attr;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(
            "an attribute: a two-element [name, value] array (schema 5), \
             or a string (schema 4)",
        )
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Attr, E> {
        Ok(Attr {
            name: value.to_owned(),
            value: String::new(),
        })
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Attr, A::Error> {
        let name: String = seq
            .next_element()?
            .ok_or_else(|| de::Error::invalid_length(0, &self))?;
        let value: String = seq
            .next_element()?
            .ok_or_else(|| de::Error::invalid_length(1, &self))?;
        let mut extra = 2;
        while seq.next_element::<de::IgnoredAny>()?.is_some() {
            extra += 1;
        }
        if extra > 2 {
            return Err(de::Error::invalid_length(extra, &self));
        }
        Ok(Attr { name, value })
    }
}

/// A structure field, constructor or parent, as listed under a declaration.
///
/// Only `label == "field"` members carry the five schema-4 keys
/// (`binders` / `implicits` / `binder_code` / `doc` / `is_direct`); the writer
/// omits them for `ctor` and `parent` rather than paying five empty keys per
/// structure. They default here — and for [`Member::is_direct`] the default has
/// to be a third state rather than `false`, see below.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Member {
    /// `field`, `ctor` or `parent`.
    pub label: String,
    pub name: String,
    pub text: Utf16Text,
    pub code: Vec<Span>,
    #[serde(default)]
    pub binders: Vec<Utf16Text>,
    #[serde(default)]
    pub implicits: Vec<bool>,
    #[serde(default)]
    pub binder_code: Vec<Vec<Span>>,
    #[serde(default)]
    pub doc: Option<String>,
    /// Whether the field is declared by this structure rather than inherited —
    /// **three-valued on purpose**.
    ///
    /// `fieldToHtml` selects the inherited branch with `f.isDirect === false`
    /// (`render.ts:1799`), so in the prototype a *missing* key is a direct
    /// field. A plain `#[serde(default)] bool` would make a missing key
    /// `false` = inherited: the opposite reading, and one that no byte
    /// comparison on this package can catch, because all 156 field members of
    /// the target package's IR carry the key 【実測】. The default is therefore
    /// `None`, and [`Member::is_inherited`] is the only thing that reads it.
    #[serde(default)]
    pub is_direct: Option<bool>,
}

impl Member {
    /// True for the members that carry the schema-4 field keys.
    pub fn is_field(&self) -> bool {
        self.label == "field"
    }

    /// `f.isDirect === false` — the inherited branch of `fieldToHtml`.
    ///
    /// Absent is **not** inherited. See [`Member::is_direct`].
    pub fn is_inherited(&self) -> bool {
        self.is_direct == Some(false)
    }
}

/// A resolved reference: which module defines the constant a declaration
/// mentions. On the wire it is a two-element array, `[module, name]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ref {
    pub module: String,
    pub name: String,
}

impl<'de> Deserialize<'de> for Ref {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (module, name) = <(String, String)>::deserialize(deserializer)?;
        Ok(Self { module, name })
    }
}

/// `deps/<PackageRoot>.json` — the name -> defining module map for the
/// constants this package refers to from one dependency package.
///
/// Two columns is all a link needs (approach.md §5.3); `kind` is only wanted by
/// a search UI, and `docLink` is recoverable from `(module, name)`.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DepMap {
    pub schema_version: u32,
    pub package: String,
    /// Constant name -> defining module.
    ///
    /// A `BTreeMap` rather than an order-preserving map: the from-scratch
    /// writer emits these keys sorted anyway, the incremental merger emits them
    /// in insertion order, and no consumer does anything but look names up.
    pub declarations: BTreeMap<String, String>,
}
