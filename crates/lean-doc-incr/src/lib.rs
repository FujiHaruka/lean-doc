//! Incremental rebuild: what to re-extract and what to re-render.
//!
//! Milestone **M3** — see `docs/implementation-plan.md`. **M3-a is here**: the
//! `detect` stage, ported from `experiments/stage5/ledger.ts` (frozen). The
//! other four stages (ownership, merge, impact, prune) and the pipeline attach
//! to it later.
//!
//! These are two questions asked at two different times, not one question
//! asked twice. What to re-extract is decided before Lean runs, from the
//! `.olean` files; what to re-render is decided after, from the IR.
//!
//! Order within a run is constrained, and the constraints are not obvious
//! from the stage names:
//!
//! - ownership runs *before* merge, because merge overwrites the previous
//!   owner of every name it touches;
//! - the global artifacts run *before* impact, because their diff is what
//!   tells impact which pages have a docstring link that just went stale;
//! - extract/ownership/merge form a loop, bounded by `--max-rounds`;
//! - a `renderKey` change overrides the impact mode and re-renders everything.
//!
//! Key comparison is a union: a key present on only one side counts as a
//! change. Under-rendering has to be loud, never silent.
//!
//! ```no_run
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use lean_doc_incr::{Algorithm, BuildOptions, build_ledger};
//! let modules = lean_doc_incr::read_module_list(std::path::Path::new("modules.txt"))?;
//! let summary = build_ledger(&BuildOptions {
//!     modules: &modules,
//!     target: "/path/to/repo",
//!     out: std::path::Path::new("ledger.json"),
//!     ir: None,
//!     source_url: "",
//!     algorithm: &Algorithm::sha256(),
//!     concurrency: 1,
//!     timings: None,
//! })?;
//! println!("{} modules, {} olean files", summary.modules, summary.files);
//! # Ok(()) }
//! ```

pub mod detect;
pub mod ledger;

pub use detect::{
    BuildOptions, BuildSummary, CheckOptions, CheckSummary, Error, TouchOptions, build_ledger,
    check_ledger, read_module_list, touch_ledger,
};
pub use ledger::{
    Algorithm, EXTRACTOR_ID, FileEntry, KeySet, LEDGER_SCHEMA, Ledger, ModuleEntry, OLEAN_SUFFIXES,
    RENDERER_ID, extract_key, hash_module, module_paths, render_key, sha256_hex, sha256_text,
};
