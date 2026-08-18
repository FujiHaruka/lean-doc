//! Whole-site artifacts: the name map, the site's entry pages and the two
//! indexes the browser reads.
//!
//! Milestone **M2-a** — see `docs/implementation-plan.md`. Ported from
//! `experiments/stage7h/global.ts` (frozen, 492 lines), of which this crate is
//! the from-scratch half: read every module, derive the whole-package files,
//! write them.
//!
//! ```text
//! IrTree ──facts_for──> [ModuleFacts] ──Artifacts::derive──> seven files
//!            ▲              │               ▲
//!            │              │               └─ every sort is UTF-16 (plan §7 U1)
//!            │              └─ .tokens ──Delta::compute──> --print-set
//!            └─ State: the contentHash cache (plan §3)
//! ```
//!
//! **M8-d changed which files those are** — five of the six existed only for
//! doc-gen4's JavaScript and went when it did, and six new ones took their
//! place. See [`artifacts`] for the list and the reason, and [`entry`] for the
//! four that are pages rather than data.
//!
//! # This is the widest net in the pipeline
//!
//! Instance lists and importer lists are in no page's bytes: the browser fills
//! them in from `search-index.json` and `modules.json`. A moved instance is only
//! ever right because these artifacts were rebuilt, so "the pages are unchanged"
//! is not a reason to skip the run.
//!
//! # What M2-b added
//!
//! The `contentHash` cache ([`State`], `--state`), the whole-package map delta
//! ([`Delta`], `--before` / `--print-set` / `--delta-json`) and the timings
//! record. [`facts_for`] is the seam all three attach to: the cache decides
//! whether a module is read, the delta consumes the [`ModuleFacts::tokens`] it
//! produces either way, and the artifacts below it did not move by a byte.
//!
//! [`ModuleFacts::tokens`] reaches no artifact, so the six-file byte comparison
//! cannot see it at all. Two things do: the state file, which is compared with
//! one the prototype wrote, and the delta, which is compared with the
//! prototype's own answer over the same mutated map.
//!
//! The one rule here that is nobody's transcription is
//! [`is_token_separator`]: the tokeniser splits on the **union** of the
//! prototype's separator set and the renderer's, because a token too few is a
//! stale page and a token too many is a re-render (plan §8, V6).
//!
//! ```no_run
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut options = litedoc4_global::GlobalOptions::new(
//!     std::path::Path::new("/path/to/ir"),
//!     std::path::Path::new("/path/to/site"),
//! );
//! options.state = Some(std::path::Path::new("/path/to/state"));
//! let summary = litedoc4_global::build_global(&options)?;
//! println!("{} declarations", summary.declarations);
//! println!("{} cached, {} read", summary.cache_hits, summary.cache_misses);
//! # Ok(()) }
//! ```

pub mod artifacts;
pub mod delta;
pub mod entry;
pub mod facts;
pub mod search_index;
mod site;
pub mod state;
pub mod v8_gc;

pub use artifacts::{ARTIFACT_PATHS, Artifacts, Counts, page_path};
pub use delta::{Delta, DeltaTimings, Witness};
pub use facts::{ModuleFacts, autolink_tokens, head_const, is_token_separator};
pub use site::{Error, FactsRun, GlobalOptions, GlobalSummary, build_global, facts_for};
pub use state::{STATE_DERIVATION, STATE_FILE, STATE_VERSION, State};
