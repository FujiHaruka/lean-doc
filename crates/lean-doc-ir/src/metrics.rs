//! How much of the IR one process read — a **counter, not a clock**.
//!
//! # Why a counter
//!
//! This project's product is speed, and speed has no gate: the oleans are
//! `mmap`ed, so the same run's environment load moves by 5x with the page cache
//! (2.5 s cold ↔ 13 s warm 【実測】, CLAUDE.md). A wall clock measured in CI
//! decides nothing, and a threshold over it is either so loose that a regression
//! passes or so tight that a cold runner fails an unchanged build.
//!
//! What *is* decidable is the **work**: the same input reads the same files, the
//! same number of times, on every machine and in every cache state. Every number
//! here is a deterministic integer, and a change to one of them is a change to
//! what the pipeline does rather than to what the machine felt like doing.
//!
//! # Why here
//!
//! `approach.md` §5.6's claim about this pipeline is in exactly these units —
//! "the outer half's limit is not rendering but **reading the whole IR**, and
//! **five of those reads are left**" — so the counter that checks it has to count
//! the same thing: **IR files opened and parsed**. One full pass over a package
//! is `modules` module-file reads, so `module / modules` is the number of full
//! passes a run made, which is the number §5.6 quotes.
//!
//! It lives beside [`crate::IrTree`] for the reason plan §3 gives for the loader
//! itself: **every read of the IR is in this crate**, so there is one place for
//! the `contentHash` cache that V2 will put in front of the remaining passes —
//! and one place to count what that cache is supposed to remove. A counter
//! sprinkled over the five call sites would be the thing it is measuring: five
//! copies of one decision.
//!
//! # What is counted, and what is not
//!
//! Counted: every IR file read **into memory and parsed** — `index.json`,
//! `modules/<Module>.json`, `deps/<Package>.json` — wherever the read happens.
//! Three call sites are outside this crate (`lean_doc_incr`'s merge, ledger and
//! prune read `index.json` as plain JSON rather than through [`crate::IrTree`]),
//! and they record here too. A counter that missed one of them would not be
//! wrong by a constant — it would report "unchanged" for a stage that grew a
//! whole extra pass.
//!
//! Not counted: `fs::copy` of a module file (`merge` moving the extracted files
//! into the tree). It transfers bytes without parsing them, and it is
//! proportional to the **changed** set rather than to the package, so folding it
//! in would make a full-pass count that is not a count of full passes.
//!
//! # Process-wide
//!
//! The counters are `static`, so they are the *process's*, not a run's. That is
//! what makes them free at the call site — no plumbing through six stages — and
//! it is why [`reset`] exists: a command that wants "this run's" number says so
//! at its start. `lean-doc build` is one run per process and resets anyway,
//! because "nothing else ran first" is an assumption and resetting is a fact.

use std::sync::atomic::{AtomicU64, Ordering};

/// Which of the IR tree's three kinds of file was read.
///
/// A single total would hide the one ratio that matters: `index.json` and the
/// dependency slices are read a fixed number of times per run, while the module
/// files are read once *per module per pass* — so only the module count divides
/// into a number of full passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IrFile {
    /// `index.json`.
    Index,
    /// `modules/<Module>.json`.
    Module,
    /// `deps/<Package>.json`.
    DepMap,
}

static INDEX: AtomicU64 = AtomicU64::new(0);
static MODULE: AtomicU64 = AtomicU64::new(0);
static DEP_MAP: AtomicU64 = AtomicU64::new(0);

/// One IR file was read.
///
/// Called **before** the read rather than after it: a read that fails still
/// opened the file and still cost the work, and counting on the way out would
/// let an early return be the one path that does not count.
///
/// `Relaxed` is the whole ordering requirement: nothing else is published
/// through these counters, and a reader only ever wants the sum.
pub fn record(kind: IrFile) {
    let counter = match kind {
        IrFile::Index => &INDEX,
        IrFile::Module => &MODULE,
        IrFile::DepMap => &DEP_MAP,
    };
    counter.fetch_add(1, Ordering::Relaxed);
}

/// What one process has read so far.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IrReads {
    pub index: u64,
    pub module: u64,
    pub dep_map: u64,
}

impl IrReads {
    /// Every IR file read, of whatever kind.
    pub fn total(&self) -> u64 {
        self.index + self.module + self.dep_map
    }

    /// How many times this run read **the whole package**, given how many
    /// modules the package has.
    ///
    /// This is §5.6's unit. It is a float on purpose: a run that reads a
    /// *subset* (an incremental round's ownership diff, the whole-package
    /// derivation with a warm cache) lands between two integers, and rounding it
    /// to one would turn "half a pass" into either "none" or "a whole one".
    ///
    /// `None` for an empty package, where the question has no answer.
    pub fn full_passes(&self, modules: usize) -> Option<f64> {
        (modules > 0).then(|| self.module as f64 / modules as f64)
    }
}

/// The counters as they stand.
///
/// Not atomic **as a group**: the three loads are three instructions, so a
/// snapshot taken while another thread is reading the IR can straddle one file.
/// Every caller in this workspace reads it between stages, on the thread that
/// ran them, where there is nothing to straddle — and a gate that needs more
/// than that wants a different tool than a counter.
pub fn snapshot() -> IrReads {
    IrReads {
        index: INDEX.load(Ordering::Relaxed),
        module: MODULE.load(Ordering::Relaxed),
        dep_map: DEP_MAP.load(Ordering::Relaxed),
    }
}

/// Back to zero.
///
/// For a command that wants its own run's number rather than the process's. It
/// is a *statement* that the count starts here, which is why `build` calls it
/// even though nothing could have run before it.
pub fn reset() {
    INDEX.store(0, Ordering::Relaxed);
    MODULE.store(0, Ordering::Relaxed);
    DEP_MAP.store(0, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **One test, deliberately** 【判断】: the counters are process-wide, so two
    /// tests in this binary would race each other rather than test anything. That
    /// the *readers* increment them is asserted where it can be — through the
    /// whole pipeline, in `crates/lean-doc/tests/build.rs`
    /// (`the_marker_records_the_work`) — because a mock that calls [`record`]
    /// itself would only prove this file is arithmetically sound, which is the
    /// half that was never in doubt.
    #[test]
    fn counts_by_kind_and_resets() {
        reset();
        assert_eq!(snapshot(), IrReads::default());
        assert_eq!(snapshot().total(), 0);

        for _ in 0..4 {
            record(IrFile::Module);
        }
        record(IrFile::Index);
        record(IrFile::DepMap);
        record(IrFile::DepMap);

        let reads = snapshot();
        assert_eq!(reads.index, 1);
        assert_eq!(reads.module, 4);
        assert_eq!(reads.dep_map, 2);
        assert_eq!(reads.total(), 7);

        // §5.6's unit: two full passes over a two-module package.
        assert_eq!(reads.full_passes(2), Some(2.0));
        // A partial pass stays partial rather than rounding to a whole one — a
        // cached derivation that read one module of eight did not read the IR.
        assert_eq!(reads.full_passes(8), Some(0.5));
        assert_eq!(reads.full_passes(0), None);

        reset();
        assert_eq!(snapshot(), IrReads::default());
    }
}
