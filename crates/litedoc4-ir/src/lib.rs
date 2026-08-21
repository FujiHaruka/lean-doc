//! Reading the intermediate representation (IR schema 5) and the ledger.
//!
//! Filled in by milestone **M1** — see `docs/implementation-plan.md`.
//!
//! Every read of the IR goes through this crate. That is a deliberate
//! structural constraint, not an accident of layering: the incremental
//! pipeline still reads the whole IR five times (ownership, merge twice,
//! impact, render), and the `contentHash` cache that removes those reads
//! belongs in one place rather than five. See plan §3.
//!
//! The same constraint is what makes [`metrics`] possible: a run's IR reads are
//! counted where they happen, so "how many full passes did that cost" is a
//! deterministic integer rather than a wall clock. That is the unit the V2 cache
//! will have to argue in — see the module's own heading.
//!
//! Two things the IR is not:
//!
//! - It is not binary. Each module is one `.json` text file named after the
//!   module's full name, with keys in alphabetical order (Lean's `Json.mkObj`).
//! - Its spans are **UTF-16 code unit offsets**, not byte offsets. Anything
//!   that indexes into source text has to convert. See plan §7 (U2).
//!
//! # What is here, and what is not
//!
//! Reading only. The extractor stays in Lean, so Lean writes the IR; nothing
//! in this crate serialises. The two consequences worth stating:
//! [`IndexEntry::content_hash`] is read and carried, never recomputed (that
//! would mean porting `lean_string_hash`), and no ordering decision — key
//! order, sort order — has to be reproduced here.
//!
//! The ledger and the cache keys named in the module heading are **M3**; this
//! crate carries the IR half today.
//!
//! # Shape
//!
//! ```text
//! IrTree::open(dir)  ->  index.json               Index
//!   .modules()       ->  modules/<Module>.json    ModuleFile -> Decl -> Member
//!   .load_dep_maps() ->  deps/<Package>.json      DepMap
//! ```
//!
//! Text that spans point into is a [`Utf16Text`], which cannot be indexed by
//! bytes; every other string is a plain `String`. That split is the whole of
//! the UTF-16 defence — see the [`utf16`] module for why it is offsets rather
//! than `Vec<u16>`.
//!
//! ```no_run
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let tree = litedoc4_ir::IrTree::open("/path/to/ir")?;
//! for module in tree.modules() {
//!     let module = module?;
//!     for decl in &module.declarations {
//!         for span in &decl.type_code {
//!             let tagged = decl.ty.slice(span.range());
//!             println!("{} {:?} {tagged}", decl.name, span.kind);
//!         }
//!     }
//! }
//! # Ok(()) }
//! ```

mod error;
pub mod metrics;
mod model;
pub mod name;
mod reader;
mod span;
pub mod utf16;

pub use error::{Error, Result};
pub use metrics::{IrFile, IrReads};
pub use model::{
    Decl, DepMap, DepMapEntry, Index, IndexEntry, Member, ModuleDoc, ModuleFile, Ref, SorryFact,
    SorryKind, Tactic,
};
pub use name::{
    escape_component, escape_module, is_id_first, is_id_rest, module_components, module_path,
    unescape_component,
};
pub use reader::{IrTree, MIN_SCHEMA_VERSION, SORRY_SCHEMA_VERSION, read_module_file};
pub use span::{Span, SpanKind};
pub use utf16::{Utf16Text, cmp_utf16, sort_utf16};
