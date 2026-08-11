//! The IR's schema-4 shapes, as the extractor writes them.
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

use std::collections::BTreeMap;

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
    /// `tactics.html`, so it is modelled here. **Empty for all 432 modules of
    /// the target package** — the shape below follows the writer
    /// (`Extract.lean:2007-2011`), not observed data.
    pub tactics: Vec<Tactic>,
    pub declarations: Vec<Decl>,
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
    #[serde(default)]
    pub attrs: Vec<String>,
    /// Schema 4, instances only: the class this instance is for. `None` for
    /// everything else, and then `inst_types` is empty as well.
    #[serde(default)]
    pub inst_class: Option<String>,
    /// Schema 4, instances only.
    #[serde(default)]
    pub inst_types: Vec<String>,
}

/// A structure field, constructor or parent, as listed under a declaration.
///
/// Only `label == "field"` members carry the five schema-4 keys
/// (`binders` / `implicits` / `binder_code` / `doc` / `is_direct`); the writer
/// omits them for `ctor` and `parent` rather than paying five empty keys per
/// structure. They default here, so `is_direct == false` on a non-field member
/// means "absent", which is also how the prototype read it (`undefined`, and
/// only `fieldToHtml` ever looks).
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
    #[serde(default)]
    pub is_direct: bool,
}

impl Member {
    /// True for the members that carry the schema-4 field keys.
    pub fn is_field(&self) -> bool {
        self.label == "field"
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
