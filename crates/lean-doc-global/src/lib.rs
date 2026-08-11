//! Whole-site artifacts: declaration data, name map, navigation, references.
//!
//! Milestone **M2-a** — see `docs/implementation-plan.md`. Ported from
//! `experiments/stage7h/global.ts` (frozen, 492 lines), of which this crate is
//! the from-scratch half: read every module, derive six files, write them.
//!
//! ```text
//! IrTree ──facts_for──> [ModuleFacts] ──Artifacts::derive──> six files
//!            ▲                              ▲
//!            │                              └─ every sort is UTF-16 (plan §7 U1)
//!            └─ where M2-b's contentHash cache goes (plan §3)
//! ```
//!
//! # This is the widest net in the pipeline
//!
//! Instance lists are in no page's bytes: the browser fills them in from
//! `declaration-data.bmp`. A moved instance is only ever right because these
//! artifacts were rebuilt, so "the pages are unchanged" is not a reason to skip
//! the run.
//!
//! # What M2-a leaves to M2-b
//!
//! The `contentHash` cache (`--state`), the whole-package map delta
//! (`--before` / `--print-set` / `--delta-json`) and the timings record.
//! [`ModuleFacts`] and [`facts_for`] are the seam those three attach to, and
//! they are here now so that adding the cache is a change to one function.
//! [`ModuleFacts::tokens`] is built for the delta and reaches no artifact — it
//! is the one thing in this crate a byte comparison of the six files cannot
//! check.
//!
//! ```no_run
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let summary = lean_doc_global::build_global(&lean_doc_global::GlobalOptions {
//!     ir: std::path::Path::new("/path/to/ir"),
//!     out: std::path::Path::new("/path/to/site"),
//! })?;
//! println!("{} declarations", summary.declarations);
//! # Ok(()) }
//! ```

pub mod artifacts;
pub mod facts;
mod site;

pub use artifacts::{ARTIFACT_PATHS, Artifacts, page_path};
pub use facts::{ModuleFacts, autolink_tokens, head_const};
pub use site::{Error, GlobalOptions, GlobalSummary, build_global, facts_for};
