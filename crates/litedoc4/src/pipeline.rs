//! `litedoc4 incremental` — the pipeline that sequences the six Rust stages, and
//! `litedoc4 modules` — the source glob its `--modules` list comes from.
//!
//! Milestone **M3-d2**. Ported from `experiments/stage7h/incremental.sh` (441
//! lines, frozen). This is the last piece of the incremental path: every stage
//! it drives already lives in a library, so what moves here is **the ordering,
//! the round loop and the union of the two render-set derivations** — the three
//! things that belong to no stage.
//!
//! ```text
//!  1 detect     litedoc4_incr::check_ledger      changed / removed / render-all
//!  2 extract    --extractor <program>            the only external process
//!  3 ownership  litedoc4_incr::ownership   ┐ L3-1: who points at a name that moved
//!  4 merge      litedoc4_incr::merge       ┘ rounds, bounded by --max-rounds
//!  5 prune      litedoc4_incr::prune             the deleted modules' pages
//!  6 global     litedoc4_global::build_global    six artifacts + the map delta
//!  7 render     litedoc4_render::render_site     the union of the two sets
//! ```
//!
//! **The stages are library calls, not subprocesses.** `litedoc4` never
//! re-invokes itself: a pipeline made of processes would have to serialise every
//! intermediate answer through a file, and two of the files the prototype used
//! that way are exactly where it loses data (see "the prototype's hole" below).
//! The one external process is the extractor, because it is Lean.
//!
//! # The six ordering constraints, all measured (plan §6)
//!
//! 1. **`ownership` before `merge`** — merge overwrites the base IR's idea of who
//!    owns each name, which is ownership's only input.
//! 2. **`global` before `impact`** — the whole-package map delta is half of the
//!    render set, and `impact` does not take it as an input.
//! 3. **extract → ownership → merge is a loop**, bounded by `--max-rounds`
//!    (default 5); the bound reached with modules still stale is **exit 5**.
//! 4. **A moved `renderKey` overrides `--mode` with `all`** — the one page set
//!    that does not follow from any changed module.
//! 5. **An empty regeneration set skips the renderer.** In the prototype this was
//!    a *correctness* guard; here it is only an optimisation — see below.
//! 6. **`--jobs` is the resident extractor's start-time configuration**
//!    (`Extract.lean:2751`), so it is a flag of this command **only with
//!    `--serve`** — which is exactly when this command is the thing that starts
//!    the server. Behind `--extractor` the parallelism belongs to the extraction
//!    program and reaches it through `--extractor-arg --jobs --extractor-arg 4`
//!    like any other of its settings.
//!
//! # Residency (M4-c)
//!
//! `--serve` replaces `--extractor` with **one Lean environment for the whole
//! run**: imported at the first round that has something to extract, asked by
//! every round after it, released on the way out of the loop. There is no
//! `--serve-dir` and no `--serve-from`; [`crate::resident`] carries the whole of
//! why — correctness comes from the server's olean generation and never from the
//! round number 【実測, stage 6a】, so a server this process did not start is one
//! it cannot vouch for, and the generation it *can* vouch for is checked rather
//! than argued.
//!
//! # Constraint 5 is a type here, not a guard
//!
//! `render.ts` treats "no `--only`" as "every module", so the prototype had to
//! test the set in shell before calling it (`incremental.sh:367`) — one `if` in
//! one script standing between an empty regeneration set, which is the *common*
//! case, and a full re-render. [`ModuleSet::These`] of an empty set means "render
//! nothing" (plan §5), so **the shell guard has no counterpart here**: the skip
//! below saves a full IR read and nothing else. `tests/incremental.rs` asserts
//! both halves — that the skip happens, and that removing it would still render
//! nothing.
//!
//! # The prototype's hole, and why it cannot be reproduced
//!
//! `incremental.sh:354-360` unions the two render-set derivations with
//! `sort -u "$WORK/impact-set.txt" "$GLOBALSET"`. But `impact` writes **no
//! `--print-set` at all** when the changed set is empty and the mode is not
//! `all`, and `sort` on a missing file fails, and the `|| : > "$RENDERSET"`
//! that catches it **empties the render set** — so a run whose only stale pages
//! come from the global map renders none of them, silently (plan §7, debt 1 / 2 /
//! 3). The union here is over two `Vec<String>` values in memory: one from
//! `ImpactRun::summary`, one from `GlobalSummary::delta`. There is no file in
//! the path, so "the file is missing" is not a state the union can be in.
//!
//! `global-set.txt`, `impact-set.txt` and `render-set.txt` are still written into
//! `--work`, under the prototype's names, **as diagnostics** —
//! `tools/incremental-reference.sh` (M3-d3) records them and
//! `tools/incremental-compare.sh` compares them across the two implementations.
//! `impact-set.txt` is written by `impact` itself and is therefore still absent
//! exactly when the prototype's is absent — measured on both sides in the
//! `nochange` and `removed-one` scenarios 【実測 2026-08-15】 — and that absence
//! is now inert, because nothing reads it back. The prototype's `sort -u` over
//! the missing file still writes `sort: No such file or directory` to stderr in
//! exactly those two runs, which is the debt itself showing up as a diagnostic.
//!
//! # What a round is allowed to touch
//!
//! - **`prune` is called without an IR tree.** Pointed at a site, its orphan rule
//!   deletes every `.html` that is not a live module page, which on the target is
//!   the whole-package HTML artifacts — **438 → 435** at M6, and **439 → 435**
//!   since M8-d made four of them the site's entry pages 【実測, plan §7 debt
//!   4】. [`prune_removed`] is the only call site and its signature cannot name an
//!   IR tree, so the pipeline cannot ask for orphan sweeping by accident.
//! - **`name-map.json` is snapshotted before anything runs.** It is both the
//!   "before" side of the map delta and the file step 6 overwrites in place
//!   (`incremental.sh:199-202`). Snapshot it late and the delta compares the new
//!   map with itself: always empty, and every page that went stale through the
//!   global map is dropped without a word — debt 1's failure reached by a
//!   different road.
//!
//! # What this command is not
//!
//! - **It does not rewrite the ledger.** Neither does the prototype: `run.sh`
//!   re-seeds `base-ledger.json` for every variant it measures. A chain of
//!   incremental runs therefore needs its caller to rebuild the ledger between
//!   them (`litedoc4 ledger build`), or the second run re-extracts the first
//!   run's changed set again — wasteful, not wrong. **That caller is
//!   [`crate::build`]** (M4-d): this function hands it the ledger `detect`
//!   computed, in [`Run::detected`], and `build` writes it **after** the last
//!   step that can fail. The reason it is not written here is the same reason
//!   the prototype does not write it: a stage that answers a question must not
//!   move the state its answer was about, or a caller that stops on the answer
//!   has already lost.
//! - **It has no `--count-reads`.** The prototype's read counter wraps every
//!   `deno` step to answer "how many times does one run read the whole IR"; it
//!   makes the timings meaningless and is a measurement tool, not a product flag.
//! - **It has no `--l3-1 on|off`.** `off` was the ablation that measured L3-1's
//!   contribution, and it **produces a wrong site** — a referring module keeps an
//!   IR that names the module a declaration used to live in. A switch whose
//!   `off` position is "be incorrect" does not belong on a product's surface
//!   【判断】.
//! - **It has no `--global old|new`.** `old` was stage 5's two-process
//!   derivation, kept as the control of stage 7h's A/B. The product is always the
//!   cached one, which is why **`--state` is required** rather than optional.
//! - **It has no `--module`.** The prototype's is a label: `incremental.sh:386`
//!   passes it straight into the timings record and nothing reads it on the way
//!   (`analyze.ts:121` already falls back when it is absent). A harness that
//!   needs to tell four variants apart adds the label to the line it appends, as
//!   `run.sh:178-193` does 【判断】.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use litedoc4_global::{GlobalOptions, build_global};
use litedoc4_incr::{
    CheckOptions, ImpactOptions, Ledger, MergeOptions, Mode, OwnershipOptions, PruneOptions,
    check_ledger, impact as run_impact, link_index_digest, merge as run_merge,
    ownership as run_ownership, prune as run_prune, read_module_list,
};
use litedoc4_ir::sort_utf16;
use litedoc4_render::{ModuleSet, RenderOptions, render_site};

use crate::resident::{Resident, Serve};
use crate::{Failure, LINK_INDEX_COST, USAGE, print_global_summary, refused, usage};

/// `--max-rounds` reached with modules still stale. The prototype's exit code
/// (`incremental.sh:293`), and the one number a caller may branch on: it means
/// "the loop did not converge", not "something is broken".
pub(crate) const EXIT_ROUNDS: u8 = 5;

/// The extractor exited non-zero.
///
/// **Deliberately not the child's own code** 【判断】. `set -euo pipefail` gave
/// the prototype whatever the extractor exited with, which can be 5 — and 5
/// already means "the round loop did not converge" here. A caller that has to
/// tell those apart cannot, so the child's code is put in the message instead of
/// on the process.
pub(crate) const EXIT_EXTRACTOR: u8 = 4;

/// `opt("--max-rounds", 5)`.
const DEFAULT_MAX_ROUNDS: usize = 5;

/// How many decimal digits a git revision has in `--source-url`.
///
/// Plan 決定 1: `coverage.ts:512` normalises `/blob/[0-9a-f]{40}/` and nothing
/// else, so a tag or a branch name here scores **3.1103 points lower** with no
/// diagnostic 【実測】.
const REV_HEX_DIGITS: usize = 40;

// ----------------------------------------------------------------- the driver

/// Everything one incremental round needs to know.
pub(crate) struct Incremental<'a> {
    pub ir: &'a Path,
    pub pages: &'a Path,
    pub ledger: &'a Path,
    pub work: &'a Path,
    /// The current module list, from a glob over the sources — `litedoc4
    /// modules`. **Required**, unlike the prototype's, where it is optional
    /// (`${MODULES:+--modules …}`): without it `check` re-reads the ledger's own
    /// list and **cannot see a module that appeared or vanished**, which are two
    /// of the seven states this pipeline is judged on.
    pub modules: Vec<String>,
    pub source_url: &'a str,
    pub link_index: &'a Path,
    /// Where each **dependency's** source lives (M7-c). Resolved by the caller,
    /// once, and used twice in here: `detect` hashes it into the render key and
    /// step 7 renders with it. A round that resolved it a second time could
    /// disagree with the ledger it just checked.
    pub external_links: &'a litedoc4_render::ExternalLinks,
    pub state: &'a Path,
    pub mode: Mode,
    pub max_rounds: usize,
}

/// How a round's extraction is done: one process per round, or one environment
/// for the whole run.
///
/// The two are the same interface — a module list in, an IR tree and a timings
/// record out — and they differ in **who owns the process**. Nothing downstream
/// of this enum can tell which one ran, which is the M4-c gate stated as a type.
pub(crate) enum Extractor {
    /// `--extractor <program>`: a program called once per round.
    ///
    /// **There is no default, and that is the design** 【判断】. Two jobs:
    ///
    /// 1. **It is the seam.** A default here would bake a path into the shipped
    ///    binary. The contract is three flags, so anything that accepts them can
    ///    be handed to `--extractor` — which is how the prototype's own
    ///    `stage7g/extract-once.sh` used to be driven, and `litedoc4 extract`
    ///    takes the same three, so the product can be its own extractor.
    /// 2. **It is what lets the pipeline be tested without Lean.** The tests pass
    ///    a fake extractor that copies a baked partial IR tree into `--ir-dir`.
    ///    Without a seam here every test of this file would need a built Lean
    ///    toolchain and a 30-second extraction, which in practice means the
    ///    pipeline is not tested at all.
    OneShot {
        program: String,
        /// `--extractor-arg`, in order, placed **before** the three flags below
        /// so that a wrapper script sees its own configuration first.
        args: Vec<String>,
        /// How many times the program has been started.
        ///
        /// The one-shot path's answer to the resident path's `serve stopped
        /// after N request(s)`: both count *the extraction the run paid for*,
        /// which is a process here and a round trip there. `build` reports one
        /// number under one name (`work.extractorRequests`) because the gate's
        /// question — "did an unchanged run start Lean at all" — is the same on
        /// both paths.
        requests: usize,
    },
    /// `--serve`: one Lean environment, imported once and asked every round.
    ///
    /// See [`crate::resident`] for why the pipeline owns it, why there is no
    /// `--serve-dir`, and what the olean generation is checked against.
    Resident(Box<Resident>),
}

impl Extractor {
    /// One extraction round.
    ///
    /// ```text
    /// <program> [<extractor-arg>…] --modules <round-in> --ir-dir <dir> --timings <file>
    /// ```
    ///
    /// The three flags are `stage7g/extract-once.sh`'s required arguments, in its
    /// order. `--events` is not passed: that script defaults it to
    /// `<timings>-events.jsonl`, and it is an implementation detail of how the
    /// Lean side reports its phase timers — one the resident path reproduces
    /// exactly, so that two records of the same round stay comparable.
    pub(crate) fn run(
        &mut self,
        modules: &Path,
        ir_dir: &Path,
        timings: &Path,
    ) -> Result<(), Failure> {
        let (program, args, requests) = match self {
            Self::Resident(resident) => return resident.extract(modules, ir_dir, timings),
            Self::OneShot {
                program,
                args,
                requests,
            } => (program, args, requests),
        };
        let mut command = Command::new(&*program);
        command
            .args(&*args)
            .arg("--modules")
            .arg(modules)
            .arg("--ir-dir")
            .arg(ir_dir)
            .arg("--timings")
            .arg(timings);
        let status = command.status().map_err(|source| Failure::Refused {
            code: EXIT_EXTRACTOR,
            message: format!("--extractor {program}: {source}"),
        })?;
        // Counted on the way out of the process rather than on the way in, and
        // **before** the exit code is judged: the run started an extractor and
        // paid for it whichever way it ended, which is what the resident path's
        // own counter records too.
        *requests += 1;
        if status.success() {
            return Ok(());
        }
        Err(Failure::Refused {
            code: EXIT_EXTRACTOR,
            message: format!(
                "--extractor {program} exited {} for {}; the IR was not updated and nothing was \
                 rendered",
                status
                    .code()
                    .map_or_else(|| "on a signal".to_owned(), |code| code.to_string()),
                modules.display(),
            ),
        })
    }

    /// How many extractions this run asked for — processes started on the
    /// one-shot path, requests sent on the resident one.
    ///
    /// A run over an unchanged package must answer **0**, and that is the
    /// sharpest of the work counters: it is the only one whose zero says Lean
    /// was never started, which is where the whole incremental path's saving
    /// comes from.
    pub(crate) fn requests(&self) -> usize {
        match self {
            Self::OneShot { requests, .. } => *requests,
            Self::Resident(resident) => resident.requests(),
        }
    }

    /// Releases the resident environment, if there is one.
    ///
    /// Idempotent, and **not the only thing that stops the server**: dropping
    /// [`Resident`] closes the request pipe, and the pipe is what keeps the
    /// server alive at all (see [`crate::resident`]). This exists so the ordinary
    /// stop is reported and its cost is inside the run's clock, not so that a
    /// failure needs it.
    pub(crate) fn release(&mut self) {
        if let Self::Resident(resident) = self {
            resident.stop();
        }
    }
}

/// The files one run leaves in `--work`, under the prototype's names.
///
/// Every one of them is a **diagnostic**: the pipeline writes them and never
/// reads one back. That is the whole of the fix for plan §7's debts 1-3 — the
/// two render-set halves meet in memory, so a file that is missing, empty or
/// stale cannot change what is rendered.
struct Work {
    changed: PathBuf,
    removed: PathBuf,
    render_all: PathBuf,
    seen: PathBuf,
    ir_changed: PathBuf,
    map_before: PathBuf,
    global_set: PathBuf,
    global_delta: PathBuf,
    global_timings: PathBuf,
    impact_set: PathBuf,
    render_set: PathBuf,
    render_timings: PathBuf,
    prune_json: PathBuf,
}

impl Work {
    fn new(root: &Path) -> Self {
        Self {
            changed: root.join("changed.txt"),
            removed: root.join("removed.txt"),
            render_all: root.join("render-all.txt"),
            seen: root.join("seen.txt"),
            ir_changed: root.join("ir-changed.txt"),
            map_before: root.join("name-map-before.json"),
            global_set: root.join("global-set.txt"),
            global_delta: root.join("global-delta.json"),
            global_timings: root.join("global-timings.json"),
            impact_set: root.join("impact-set.txt"),
            render_set: root.join("render-set.txt"),
            render_timings: root.join("render-timings.json"),
            prune_json: root.join("prune.json"),
        }
    }
}

/// What one incremental run did. Every field is a denominator.
pub(crate) struct Summary {
    pub rounds: usize,
    pub stale_found: usize,
    pub changed: usize,
    pub removed: usize,
    pub ir_changed: usize,
    pub global_stale: usize,
    /// Pages the renderer **wrote**, which is [`litedoc4_render::RenderSummary`]'s
    /// own count and the number the `render modules W/M` line prints.
    ///
    /// **Not the size of the render set** 【判断】. The two differ whenever the
    /// set names a module the IR does not hold, and the set is the larger of the
    /// two — so reporting it would be reporting work that was asked for rather
    /// than work that was done, under a name that says otherwise. The set's own
    /// size is on the `impact` line, where it belongs.
    pub pages_rendered: usize,
    /// Math spans the renderer could not convert to MathML and wrote back as
    /// `$…$` ([`litedoc4_render::RenderSummary::math_failures`]).
    ///
    /// Recorded rather than only printed because the fallback leaves a **valid
    /// page**: nothing downstream — not the byte count, not the page count, not
    /// the exit code — moves when a formula fails. A gate that wants to assert
    /// "the mathematics came out as mathematics" has nothing else to read.
    pub math_fallbacks: usize,
    /// The whole-package derivation's `contentHash` cache, as
    /// [`litedoc4_global::GlobalSummary`] counted it — the same values the
    /// `global  cache H hit / M miss` line prints.
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub mode: String,
}

/// One run's whole answer: the counts, the clock, and the ledger the run
/// *licenses* but does not write.
///
/// The third field is M3-d2's first debt, arriving (plan §7). `incremental`
/// drops it on the floor exactly as before — see this module's heading — and
/// [`crate::build`] is the command that writes it, after the last step that
/// could fail.
pub(crate) struct Run {
    pub summary: Summary,
    pub timings: Timings,
    /// [`litedoc4_incr::CheckSummary::fresh`]: the module hashes as `detect`
    /// read them, **before** the extraction they licensed.
    pub detected: Ledger,
}

/// One incremental round: a changed build tree in, an updated IR and updated
/// pages out.
pub(crate) fn run_incremental(
    options: &Incremental<'_>,
    extractor: &mut Extractor,
) -> Result<Run, Failure> {
    let started = Instant::now();
    let work = Work::new(options.work);
    create_dir(options.work)?;

    // The dependency map's identity **before the rounds**, so that the rewrite
    // one of them may perform can be seen (M5-b). `detect` below compares this
    // same value against the ledger's; what it cannot compare is a map that does
    // not exist yet, and from M5-b the ordinary case is that this run's own
    // extraction writes it.
    let map_before = link_index_digest(Some(options.link_index)).map_err(refused)?;

    // The global name -> module map **as it stands before this run**. Snapshotted
    // rather than recomputed, because step 6 overwrites it in place. See the
    // module heading: taking it later makes every delta empty.
    let live_map = options.pages.join("declarations").join("name-map.json");
    let _ = fs::remove_file(&work.map_before);
    let have_before = fs::metadata(&live_map).is_ok_and(|meta| meta.is_file());
    if have_before {
        create_dir(options.work)?;
        fs::copy(&live_map, &work.map_before)
            .map_err(|source| Failure::Failed(format!("{}: {source}", live_map.display())))?;
    }

    // 1 -- detect --------------------------------------------------------------
    // `--ir` is not optional: without it the ledger cannot see the IR schema or
    // the generator id, and a schema bump would leave every page stale with the
    // ledger reporting "0 changed". `--source-url` is not optional for the
    // mirror-image reason: it reaches the page bytes and it moves every commit,
    // so it is in the *render* key and a new revision re-renders without
    // starting Lean once.
    let check = check_ledger(&CheckOptions {
        ledger: options.ledger,
        // The ledger's own: two algorithms produce incomparable hashes, so
        // overriding here would report every module as changed.
        algorithm: None,
        modules: Some(&options.modules),
        ir: Some(options.ir),
        source_url: options.source_url,
        // M5-b. The half of "did the dependency map move" that can be answered
        // here: somebody handed this run a different `--link-index` than the one
        // the ledger records. The other half — the map this run's own extractor
        // is about to rewrite — is [`map_before`] / the check after the rounds.
        link_index: Some(options.link_index),
        // M7-c: the same map step 7 renders with, so "the pages were rendered
        // against this" is one value rather than two derivations of one.
        external_links: Some(&options.external_links.digest()),
        // The ledger's bytes do not depend on this (M3-a 【実測】); its speed
        // does — see [`hash_concurrency`].
        concurrency: hash_concurrency(),
        changed_out: Some(&work.changed),
        removed_out: Some(&work.removed),
        render_all_out: Some(&work.render_all),
        timings: None,
    })
    .map_err(refused)?;
    let detect_done = started.elapsed();
    println!(
        "detect  {} module(s): {} to re-extract, {} removed{}",
        check.modules,
        check.re_extract.len(),
        check.removed.len(),
        if check.render_all() {
            format!(
                " — render key moved ({})",
                check.render_key_changed.join(",")
            )
        } else {
            String::new()
        },
    );

    // 2/3/4 -- extract, ownership, merge, in rounds ----------------------------
    let mut seen: Vec<String> = check.re_extract.clone();
    let mut round_in: Vec<String> = check.re_extract.clone();
    let mut ir_changed: Vec<String> = Vec::new();
    let mut rounds = 0usize;
    let mut stale_found = 0usize;
    let (mut extract_seconds, mut ownership_seconds, mut merge_seconds) = (0.0, 0.0, 0.0);

    // The loop runs at least once when something was deleted, even with nothing
    // to re-extract: the deletion is folded into the first round's merge.
    while !round_in.is_empty() || (rounds == 0 && !check.removed.is_empty()) {
        rounds += 1;
        let round_in_file = options.work.join(format!("round-in-{rounds}.txt"));
        write_lines(&round_in_file, &round_in)?;
        let inc_ir = options.work.join(format!("inc-ir-{rounds}"));
        let _ = fs::remove_dir_all(&inc_ir);

        if !round_in.is_empty() {
            let at = Instant::now();
            extractor.run(
                &round_in_file,
                &inc_ir,
                &options.work.join(format!("extract-timings-{rounds}.json")),
            )?;
            extract_seconds += at.elapsed().as_secs_f64();
        }
        let inc = (!round_in.is_empty()).then_some(inc_ir.as_path());

        // gone from the IR, and asking again would be asking about nothing.
        // Deletions belong to the first round. The guard is **documentation
        // rather than protection**: both stages filter the list to modules the
        // base index still holds (`ownership.rs:157-163`, `merge.rs:278-285`),
        // so passing it again would be a no-op — which is what the prototype's
        // own comment says ("asking again would be asking about nothing"). The
        // mutation survey confirmed it: removing this condition changes no byte
        // and no count.
        let deletions =
            (rounds == 1 && !check.removed.is_empty()).then_some(work.removed.as_path());

        // 3 -- ownership (L3-1). Before the merge: it needs the IR's previous
        // idea of who owns each name, which the merge is about to overwrite.
        //
        // `--exclude` is the round's memory of what earlier rounds already took.
        // It is carried because the prototype carries it, **and it is currently
        // unobservable**: `ownership` excludes its own `--inc` set and its
        // `--removed` set on its own (`ownership.rs:241-243`), which covers
        // round 1 exactly, and a round after the first watches nothing — a
        // module reaches round 2 because its *references* went stale, so its
        // declaration names are still the base IR's and the lost/gained sets
        // come out empty. `tests/incremental.rs` asserts that rather than
        // assuming it, because the argument rests on "a module's declarations
        // come from its own olean", which is a fact about Lean and not about
        // this file.
        write_lines(&work.seen, &seen)?;
        let at = Instant::now();
        let owners = run_ownership(&OwnershipOptions {
            base: options.ir,
            inc,
            removed: deletions,
            exclude: Some(&work.seen),
            print_set: Some(&options.work.join(format!("stale-{rounds}.txt"))),
            json: Some(&options.work.join(format!("ownership-{rounds}.json"))),
        })
        .map_err(refused)?;
        ownership_seconds += at.elapsed().as_secs_f64();

        // 4 -- merge, in place. The removals are folded in here, so the IR is
        // never left in a state where a deleted module is still indexed.
        let at = Instant::now();
        let merged = run_merge(&MergeOptions {
            base: options.ir,
            inc,
            out: options.ir,
            removed: if deletions.is_some() {
                &check.removed
            } else {
                &[]
            },
            // The same list `detect` was given, and for the same reason: it is
            // what a from-scratch extraction would be handed, so it is the order
            // `index.json` comes out in (M3-d2b). Without it a round that adds a
            // module leaves an index a full run would have written differently —
            // same entries, different sequence — and no page byte says so.
            modules: Some(&options.modules),
            changed_out: Some(&options.work.join(format!("ir-changed-{rounds}.txt"))),
            timings: Some(&options.work.join(format!("merge-timings-{rounds}.json"))),
        })
        .map_err(refused)?;
        merge_seconds += at.elapsed().as_secs_f64();
        ir_changed.extend(merged.ir_changed.iter().cloned());

        println!(
            "round {rounds}  extracted {}, removed {}, IR moved for {}, stale {}",
            merged.updated.len(),
            merged.removed,
            merged.ir_changed.len(),
            owners.stale_modules.len(),
        );

        stale_found += owners.stale_modules.len();
        seen.extend(owners.stale_modules.iter().cloned());
        round_in = owners.stale_modules;
        if rounds >= options.max_rounds && !round_in.is_empty() {
            return Err(Failure::Refused {
                code: EXIT_ROUNDS,
                message: format!(
                    "still {} stale module(s) after {rounds} round(s): {}",
                    round_in.len(),
                    round_in.join(", "),
                ),
            });
        }
    }
    // **The loop is the only thing that can extract**, so the resident
    // environment is released here rather than at the end of the run: what
    // follows reads the whole IR, and holding 3 GB across it buys nothing. The
    // teardown stays inside `totalSeconds`, as the prototype's does
    // (`incremental.sh:378-381`) — only earlier.
    extractor.release();
    write_lines(&work.seen, &seen)?;
    write_lines(&work.ir_changed, &ir_changed)?;
    let rounds_done = started.elapsed();

    // 5 -- prune ---------------------------------------------------------------
    // The page third of the deletion path. The renderer only ever writes, so
    // without this a deleted module's page survives every later run and is
    // indistinguishable from a live one.
    if !check.removed.is_empty() {
        let pruned = prune_removed(options.pages, &work.removed, &work.prune_json)?;
        println!(
            "prune   deleted {}/{} page(s)",
            pruned.deleted.len(),
            pruned.requested,
        );
    }
    let prune_done = started.elapsed();

    // 6 -- global --------------------------------------------------------------
    // Constraint 2: this runs **before** the renderer because its map delta names
    // every declaration whose links can have changed anywhere on the site, which
    // is the half of the render set no changed module can produce.
    write_file(&work.global_set, "")?;
    let mut derive = GlobalOptions::new(options.ir, options.pages);
    derive.state = Some(options.state);
    derive.timings = Some(&work.global_timings);
    if have_before {
        derive.before = Some(&work.map_before);
        derive.print_set = Some(&work.global_set);
        derive.delta_json = Some(&work.global_delta);
    }
    let derived = build_global(&derive).map_err(|e| Failure::Failed(e.to_string()))?;
    let global_affected: &[String] = derived
        .delta
        .as_ref()
        .map_or(&[], |delta| delta.affected.as_slice());
    let global_done = started.elapsed();
    print_global_summary("global  ", &derived);

    // 7 -- impact, the union, render -------------------------------------------
    // Constraint 4: a moved render key is the one page set that does not follow
    // from any changed module — nothing was re-extracted, yet every page is
    // stale — so it overrides `--mode` rather than widening it.
    //
    // **And so is a dependency map this run rewrote** 【M5-b】. `detect` compared
    // the map as it stood at the head of the run; from M5-b the extractor writes
    // it, so the map the renderer is about to read may not be the one `detect`
    // saw. The comparison therefore happens twice, at the two moments its input
    // exists: once in `detect` (a map somebody else changed) and once here (a map
    // this run changed). Both answers are the same kind of answer — every page's
    // links can have moved — so both land on [`Mode::All`].
    //
    // Doing it only in `detect` would be worse than not doing it: the ledger
    // would record the *new* map, the next run would compare new against new and
    // find nothing, and the staleness would be permanent and silent.
    let map_after = link_index_digest(Some(options.link_index)).map_err(refused)?;
    let map_moved = map_after != map_before;
    if map_moved {
        eprintln!(
            "  render-all linkIndex: the dependency map moved during this run ({} -> {})",
            digest_or_none(map_before.as_deref()),
            digest_or_none(map_after.as_deref()),
        );
    }
    let mode = if check.render_all() || map_moved {
        for reason in &check.render_key_changed {
            eprintln!("  render-all renderKey:{reason}");
        }
        Mode::All
    } else {
        options.mode.clone()
    };
    let selected = run_impact(&ImpactOptions {
        ir: options.ir,
        changed: &seen,
        mode: &mode,
        census: None,
        print_set: Some(&work.impact_set),
        json: None,
    })
    .map_err(refused)?;
    // **The union, in memory.** See the module heading: the prototype does this
    // with `sort -u` over two files, one of which is absent in exactly the case
    // where the other one matters.
    let mut render_set: BTreeSet<String> = selected
        .summary
        .as_ref()
        .map(|summary| summary.selected.iter().cloned().collect())
        .unwrap_or_default();
    render_set.extend(global_affected.iter().cloned());
    let mut listed: Vec<String> = render_set.iter().cloned().collect();
    // Plan §7, U1 — and **this is not the prototype's order**. `sort -u`
    // (`incremental.sh:360`) collates in the caller's locale, and `en_US.UTF-8`
    // ignores the `.` separator, so it puts `…Shannon.ArithmeticCoding` before
    // `…Shannon.AWGN.Main` where code-unit order does the opposite. Same set,
    // different sequence: **163 of the 432 lines move** on a full render set,
    // 57 of 262 and 2 of 52 on the two narrower ones 【実測 2026-08-15,
    // tools/incremental-compare.sh】. Nothing generated depends on it — the
    // renderer is handed a [`ModuleSet::These`], which is a set, so the order
    // reaches `render-set.txt` (a diagnostic) and stops there. This is the order
    // the rest of the project's module lists are in.
    sort_utf16(&mut listed);
    write_lines(&work.render_set, &listed)?;
    let impact_done = started.elapsed();
    println!(
        "impact  mode {} -> {} page(s) ({} from the changed set, {} from the global map)",
        mode.name(),
        render_set.len(),
        selected
            .summary
            .as_ref()
            .map_or(0, |summary| summary.selected.len()),
        global_affected.len(),
    );

    // Constraint 5. `ModuleSet::These` of an empty set already renders nothing,
    // so this skip is an optimisation — it saves reading the whole IR to write no
    // file — and **not** the guard the prototype needed it to be.
    let mut pages_rendered = 0usize;
    let mut math_fallbacks = 0usize;
    if render_set.is_empty() {
        write_file(&work.render_timings, "{\"skipped\":\"empty render set\"}\n")?;
        println!("render  nothing to render");
    } else {
        let only = ModuleSet::These(render_set);
        let at = Instant::now();
        let rendered = render_site(&RenderOptions {
            ir: options.ir,
            pages: options.pages,
            source_url: options.source_url,
            external_links: options.external_links,
            link_index: Some(options.link_index),
            only: &only,
        })
        .map_err(|e| Failure::Failed(e.to_string()))?;
        // One value, three destinations: the log line below, `render-timings.json`
        // and the run's `work` record. Counting the render set instead would put a
        // second, larger number under the same word.
        pages_rendered = rendered.pages_written;
        math_fallbacks = rendered.math_failures;
        let record = serde_json::json!({
            "command": "render",
            "pagesWritten": rendered.pages_written,
            "modulesInIr": rendered.modules_in_ir,
            "declarationsRendered": rendered.declarations_rendered,
            "pageBytes": rendered.bytes_written,
            "mathFallbacks": rendered.math_failures,
            "renderSeconds": at.elapsed().as_secs_f64(),
        });
        write_file(
            &work.render_timings,
            &(serde_json::to_string(&record).expect("counts and durations serialise") + "\n"),
        )?;
        crate::print_render_summary("render  ", &rendered);
    }
    let render_done = started.elapsed();

    Ok(Run {
        summary: Summary {
            rounds,
            stale_found,
            changed: check.re_extract.len(),
            removed: check.removed.len(),
            ir_changed: ir_changed.len(),
            global_stale: global_affected.len(),
            pages_rendered,
            math_fallbacks,
            cache_hits: derived.cache_hits,
            cache_misses: derived.cache_misses,
            mode: mode.name().to_owned(),
        },
        timings: Timings {
            detect: detect_done.as_secs_f64(),
            extract: extract_seconds,
            ownership: ownership_seconds,
            merge: merge_seconds,
            rounds: rounds_done.saturating_sub(detect_done).as_secs_f64(),
            prune: prune_done.saturating_sub(rounds_done).as_secs_f64(),
            global: global_done.saturating_sub(prune_done).as_secs_f64(),
            impact: impact_done.saturating_sub(global_done).as_secs_f64(),
            render: render_done.saturating_sub(impact_done).as_secs_f64(),
            total: render_done.as_secs_f64(),
        },
        detected: check.fresh,
    })
}

/// The wall-clock split of one run, in the prototype's phases.
///
/// Kept out of [`Summary`] because the durations are **diagnostics**: nothing
/// may assert on them, and a summary without them is one a test can compare with
/// `==`.
pub(crate) struct Timings {
    pub detect: f64,
    pub extract: f64,
    pub ownership: f64,
    pub merge: f64,
    pub rounds: f64,
    pub prune: f64,
    pub global: f64,
    pub impact: f64,
    pub render: f64,
    pub total: f64,
}

/// `prune` over a deletion list, and **nothing else**.
///
/// The signature is the guard 【判断】. [`PruneOptions::ir`] turns on the orphan
/// rule, which calls every `.html` that is not a live module page an orphan —
/// pointed at a site that includes the whole-package artifacts, that is
/// `index.html`, `404.html`, `search.html` and `foundational_types.html` since
/// M8-d (`navbar.html`, `references.html` and `tactics.html` before it), and the
/// site goes **439 → 435** 【実測, plan §7 debt 4】. The prototype never passes
/// `--ir` either (`incremental.sh:304-305`), so nobody has walked into it yet;
/// here there is no parameter to pass it through — which matters more now that
/// the files at stake are the ones a reader lands on.
fn prune_removed(
    pages: &Path,
    remove: &Path,
    json: &Path,
) -> Result<litedoc4_incr::PruneSummary, Failure> {
    run_prune(&PruneOptions {
        pages,
        remove: Some(remove),
        ir: None,
        dry_run: false,
        json: Some(json),
    })
    .map_err(refused)
}

// ------------------------------------------------------------------- the CLI

/// `litedoc4 incremental`.
pub(crate) fn incremental(args: &[String]) -> Result<(), Failure> {
    let mut ir: Option<PathBuf> = None;
    let mut pages: Option<PathBuf> = None;
    let mut ledger: Option<PathBuf> = None;
    let mut work: Option<PathBuf> = None;
    let mut modules: Option<PathBuf> = None;
    let mut source_url: Option<String> = None;
    let mut link_index: Option<PathBuf> = None;
    let mut make_link_index = false;
    let mut state: Option<PathBuf> = None;
    let mut extractor: Option<String> = None;
    let mut extractor_args: Vec<String> = Vec::new();
    let mut mode: Option<String> = None;
    let mut max_rounds = DEFAULT_MAX_ROUNDS;
    let mut timings: Option<PathBuf> = None;
    let mut serve = false;
    let mut jobs: usize = 1;
    let mut extractor_bin: Option<PathBuf> = None;
    let mut target: Option<PathBuf> = None;
    let mut lake: Option<PathBuf> = None;
    let mut package: Option<PathBuf> = None;
    let mut deps_docs_map: Option<PathBuf> = None;

    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        let mut value = |flag: &str| -> Result<String, Failure> {
            match rest.next() {
                Some(value) => Ok(value.clone()),
                None => usage(format!("{flag} needs a value")),
            }
        };
        match arg.as_str() {
            "--ir" => ir = Some(value("--ir")?.into()),
            "--pages" => pages = Some(value("--pages")?.into()),
            "--ledger" => ledger = Some(value("--ledger")?.into()),
            "--work" => work = Some(value("--work")?.into()),
            "--modules" => modules = Some(value("--modules")?.into()),
            "--source-url" => source_url = Some(value("--source-url")?),
            "--link-index" => link_index = Some(value("--link-index")?.into()),
            // M5-b. Without it `--link-index` names an input; with it the
            // resident extractor **writes** that file out of the environment it
            // has imported anyway (M5-a), and the file is an output this run
            // produces before it renders against it.
            "--make-link-index" => make_link_index = true,
            "--state" => state = Some(value("--state")?.into()),
            "--extractor" => extractor = Some(value("--extractor")?),
            "--extractor-arg" => extractor_args.push(value("--extractor-arg")?),
            "--mode" => mode = Some(value("--mode")?),
            "--max-rounds" => {
                let raw = value("--max-rounds")?;
                max_rounds = raw.parse().map_err(|_| {
                    Failure::Usage(format!("--max-rounds wants a number, not {raw}"))
                })?;
            }
            "--timings" => timings = Some(value("--timings")?.into()),
            // M4-c. `--serve` takes no value: the prototype's `auto` existed only
            // to tell it from `--serve-dir`, which is refused below.
            "--serve" => serve = true,
            "--extractor-bin" => extractor_bin = Some(value("--extractor-bin")?.into()),
            "--target" => target = Some(value("--target")?.into()),
            "--lake" => lake = Some(value("--lake")?.into()),
            // M7-c. **A flag of neither extraction path**, and not `--target`:
            // `--target` is the package the resident extractor runs `lake env`
            // inside, and exists only on the `--serve` path; this is the package
            // whose lake-manifest.json and toolchain pin the dependencies, which
            // is a question about the *pages* and is asked however the extraction
            // is done. On a real package the two are the same directory. Left
            // out, dependency links stay relative and the run says so — and the
            // ledger that licenses those pages has to have been built the same
            // way, which is why `ledger` takes the same flag under the same name.
            "--root" => package = Some(value("--root")?.into()),
            // A-1. This command **renders**, and it is also the command that
            // compares the render key with the ledger's, so it needs the same
            // resolved documentation map `build` wrote for both: without it the
            // pages of the round lose the links the rest of the tree has, and
            // the key says every page has to be re-rendered — for ever, since
            // the run that re-renders them writes no map either. It is read, not
            // resolved: nothing here touches the network, and re-deriving it
            // would be the second way to attach it (`deps_docs.rs`, "why an
            // artifact and not the same flags on three commands").
            "--deps-docs-map" => deps_docs_map = Some(value("--deps-docs-map")?.into()),
            // **A flag of `--serve` only, and that is constraint 6 spelled out**
            // (plan §6): the resident server's job count is its start-up `cfg`
            // (`Extract.lean:2751`), so it is the pipeline's to choose exactly
            // when the pipeline is the thing that starts it. Behind `--extractor`
            // the parallelism belongs to the extraction program.
            "--jobs" => {
                let raw = value("--jobs")?;
                jobs = raw
                    .parse()
                    .map_err(|_| Failure::Usage(format!("--jobs wants a number, not {raw}")))?;
            }
            // Refused by name rather than as "unknown argument": each is a real
            // flag of `incremental.sh`, so what the caller needs to hear is why
            // it is gone, not that it was misspelled. See the module heading.
            "--l3-1" => {
                return usage(
                    "--l3-1 is not a pipeline flag: `off` was the ablation that measured L3-1's \
                     contribution and it produces a wrong site (a referring module keeps an IR \
                     naming the module a declaration used to live in). Ownership always runs",
                );
            }
            "--global" => {
                return usage(
                    "--global is not a pipeline flag: `old` was stage 5's two-process derivation, \
                     kept only as the control of stage 7h's A/B. The product is always the cached \
                     one, which is why --state is required",
                );
            }
            "--serve-dir" => {
                return usage(
                    "--serve-dir is not offered: `--serve` starts a server this run owns, and a \
                     server it does not own is one whose olean generation it cannot vouch for. \
                     Correctness comes from that generation and never from the round number — a \
                     server imported before the edit returns the pre-edit owner of every name that \
                     moved, and then no round is safe, including round 2 【実測, stage 6a】. See \
                     `crates/litedoc4/src/resident.rs`",
                );
            }
            "--serve-from" => {
                return usage(
                    "--serve-from is not offered: it chose which rounds a server the caller owns \
                     was allowed to answer, and stage 6a measured that the round number is not \
                     what makes a round safe. With `--serve` the server is started inside this \
                     run, so every round is served and every round is checked against the same \
                     olean generation",
                );
            }
            "--count-reads" => {
                return usage(
                    "--count-reads is a measurement tool, not a product flag: it wraps every stage \
                     to count IR reads and makes the timings meaningless",
                );
            }
            "--module" => {
                return usage(
                    "--module is not a pipeline flag: the prototype's is a label that goes \
                     straight into the timings record and is read by nothing. A harness that needs \
                     one adds it to the line it appends",
                );
            }
            "--no-link-index" => {
                return usage(format!(
                    "--no-link-index is not an incremental flag: a round re-renders a subset, so a \
                     page rendered without the map is indistinguishable from one that was not \
                     re-rendered at all — {LINK_INDEX_COST}",
                ));
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                return Ok(());
            }
            other => return usage(format!("unknown argument `{other}`")),
        }
    }

    let Some(ir) = ir else {
        return usage("--ir is required");
    };
    let Some(pages) = pages else {
        return usage("--pages is required");
    };
    let Some(ledger) = ledger else {
        return usage("--ledger is required");
    };
    let Some(work) = work else {
        return usage("--work is required");
    };
    let Some(modules) = modules else {
        return usage(
            "--modules is required: without the current module list `check` re-reads the ledger's \
             own and cannot see a module that appeared or vanished. `litedoc4 modules` writes it",
        );
    };
    let Some(source_url) = source_url.filter(|url| !url.is_empty()) else {
        return usage(
            "--source-url is required: doc-gen4 reads it from lake plus git, and it is not in the IR",
        );
    };
    check_source_url(&source_url)?;
    let Some(link_index) = link_index else {
        return usage(format!(
            "--link-index <file> is required, and there is no --no-link-index here: {LINK_INDEX_COST}",
        ));
    };
    let Some(state) = state else {
        return usage(
            "--state <dir> is required: the whole-package derivation is always the cached one, and \
             the map delta it feeds the renderer needs a cache to compare against. The previous \
             run — full generation with `litedoc4 site --state`, or the last incremental round — \
             is what leaves it behind",
        );
    };
    if serve && extractor.is_some() {
        return usage(
            "--serve and --extractor are exclusive: one names a program to run once per round, the \
             other says this run owns a Lean environment for all of them. `--serve` is the \
             resident path and it uses --extractor-bin, not a wrapper",
        );
    }
    // **A flag of `--serve` only** 【判断, M5-b】, and for the same reason
    // `--jobs` is: the map is written by the Lean extractor, and the only
    // extractor whose command line this command spells is the resident one.
    // `--extractor <program>`'s contract is three flags (`--modules --ir-dir
    // --timings`) and it is the seam the pipeline's tests hand a fake through;
    // adding a fourth would break `stage7g/extract-once.sh`, which is the
    // program the M3-d3 and M4-b recordings were taken with.
    if make_link_index && !serve {
        return usage(
            "--make-link-index is a flag of --serve: the dependency map is written by the Lean \
             extractor out of the environment it imported for the extraction (M5-a), and --serve \
             is the path where this command spells that command line. Behind --extractor, the \
             program is the one that decides — `litedoc4 extract --link-index <file>` writes it — \
             and --link-index here names the file it wrote",
        );
    }
    if !serve {
        for flag in ["--extractor-bin", "--target", "--lake"] {
            let given = match flag {
                "--extractor-bin" => extractor_bin.is_some(),
                "--target" => target.is_some(),
                _ => lake.is_some(),
            };
            if given {
                return usage(format!(
                    "{flag} is a flag of --serve: without it the extraction is whatever \
                     --extractor names, and how that program finds its binary is its own business \
                     (`litedoc4 extract` takes {flag} through --extractor-arg)",
                ));
            }
        }
        if jobs != 1 {
            return usage(
                "--jobs is a flag of --serve: parallelism is the extractor's, and a resident one \
                 fixes it at start-up (plan §6, constraint 6). Behind --extractor, pass it through \
                 with `--extractor-arg --jobs --extractor-arg <n>`",
            );
        }
    }
    if jobs == 0 {
        return usage("--jobs must be at least 1");
    }
    if !serve && extractor.is_none() {
        return usage(
            "one of --extractor <program> and --serve is required, and neither has a default: \
             --extractor is called as `<program> [<extractor-arg>…] --modules <list> --ir-dir \
             <dir> --timings <file>`, which is `stage7g/extract-once.sh`'s interface and \
             `litedoc4 extract`'s; --serve starts one resident Lean environment for the whole run \
             and needs --extractor-bin and --target",
        );
    }
    let mode = match mode {
        // The prototype's `MODE=self` (`incremental.sh:108`), which is what
        // every stage-7h measurement ran with. It is **not** the sound bound —
        // `impact`'s own default is `importers` — and choosing between them is
        // M4's, not a transcription's.
        None => Mode::SelfOnly,
        Some(text) => match Mode::parse(&text) {
            // Refused here rather than carried 【判断】. `impact.ts` only looks
            // at `--mode` when there is something to select, so the prototype
            // takes `--mode nonsens` with an empty changed set and **exits 0
            // having rendered nothing**. A pipeline that does that on a typo is
            // worse than one that stops.
            Mode::Unrecognised(text) => {
                return usage(format!(
                    "--mode takes self|referrers|importers|all, not `{text}`"
                ));
            }
            parsed => parsed,
        },
    };
    if max_rounds == 0 {
        return usage("--max-rounds must be at least 1: round 1 is where deletions are folded in");
    }

    let module_list = read_module_list(&modules).map_err(refused)?;
    // M7-c, once, before anything else runs: the same value `detect` hashes into
    // the render key and step 7 renders with.
    let external_links = crate::with_dependency_docs(
        crate::resolve_external_links(package.as_deref(), lake.as_deref()),
        deps_docs_map.as_deref(),
    )?;
    // **Built before the run starts, so the generation is the world `detect` is
    // about to look at.** [`Resident::new`] starts nothing; it records the
    // oleans, and every later check — before the spawn, after `ready`, around
    // every request — is against this one reading.
    let mut extractor = match extractor {
        Some(program) => Extractor::OneShot {
            program,
            args: extractor_args,
            requests: 0,
        },
        None => Extractor::Resident(Box::new(Resident::new(serve_options(ServeRequest {
            bin: extractor_bin,
            target,
            lake,
            jobs,
            modules_file: &modules,
            modules: &module_list,
            work: &work,
            link_index: make_link_index.then_some(link_index.as_path()),
        })?)?)),
    };
    let outcome = run_incremental(
        &Incremental {
            ir: &ir,
            pages: &pages,
            ledger: &ledger,
            work: &work,
            modules: module_list,
            source_url: &source_url,
            link_index: &link_index,
            external_links: &external_links,
            state: &state,
            mode,
            max_rounds,
        },
        &mut extractor,
    );
    // Not a `?` above 【判断】: the release has to happen on the failing path too,
    // and while dropping `extractor` would do it silently, doing it here is what
    // puts the stop **before** the error reaches the caller rather than after.
    // `Drop` remains the backstop for a panic (see `crate::resident`).
    extractor.release();
    let run = outcome?;

    if let Some(path) = timings {
        let serve = match &extractor {
            Extractor::Resident(resident) => Some((jobs, resident.generation().to_owned())),
            Extractor::OneShot { .. } => None,
        };
        write_timings(&path, &work, &run.summary, &run.timings, serve.as_ref())?;
    }
    Ok(())
}

/// `--serve`'s three paths, resolved the way `litedoc4 extract` resolves them.
///
/// **Flag, then environment, then nothing** — no default path for the binary and
/// none for the target, because both are absolute paths on somebody's machine and
/// a default would be the `defaultIrDir` mistake with a different name (M4-a).
/// `lake` does get one, and it is not an exception: it is a name looked up on
/// PATH, and elan's shim under that name is what picks the toolchain the target
/// pins, so `~/.elan/bin/lake` would be the more specific and the more fragile of
/// the two. The variable names are the prototype's (`serve-ctl.sh:48-50`).
pub(crate) struct ServeRequest<'a> {
    /// `--extractor-bin`, or `$EXTRACT_BIN`.
    pub bin: Option<PathBuf>,
    /// `--target`, or `$TARGET_REPO`.
    pub target: Option<PathBuf>,
    /// `--lake`, or `$LAKE`, or the name on PATH.
    pub lake: Option<PathBuf>,
    pub jobs: usize,
    pub modules_file: &'a Path,
    pub modules: &'a [String],
    pub work: &'a Path,
    /// Where the server writes the dependency map, or `None` to write none
    /// (M5-b). See [`crate::resident::Serve::link_index`].
    pub link_index: Option<&'a Path>,
}

pub(crate) fn serve_options(request: ServeRequest<'_>) -> Result<Serve, Failure> {
    let ServeRequest {
        bin,
        target,
        lake,
        jobs,
        modules_file,
        modules,
        work,
        link_index,
    } = request;
    let Some(bin) = crate::extract::or_env(bin, "EXTRACT_BIN") else {
        return usage(
            "--serve needs --extractor-bin <path> (or EXTRACT_BIN): the Lean extractor built by \
             `extractor/build.sh`, which is 171 MB and is therefore not committed. There is no \
             default — the binary is built against the target's toolchain, so a path baked in here \
             would be right on exactly one machine",
        );
    };
    let Some(target) = crate::extract::or_env(target, "TARGET_REPO") else {
        return usage(
            "--serve needs --target <repo> (or TARGET_REPO): the Lean package being documented. \
             `lake env` runs inside it, which is how the resident extractor gets the oleans and \
             the search path without litedoc4 owning a toolchain — and its oleans are the \
             generation every request is checked against",
        );
    };
    let target = fs::canonicalize(&target).map_err(|source| Failure::Refused {
        code: 3,
        message: format!("--target {}: {source}", target.display()),
    })?;
    // **Absolute, all three** 【実測 2026-08-15】. The server's working directory
    // is the target (`serve-ctl.sh:68-77`), so a relative path on its command
    // line resolves against the package being documented — the binary would be
    // looked for there, and the start-up events file would be *written* there.
    // `--lake` is the exception and stays as given: it is a name looked up on
    // PATH, not a path.
    // The map goes on the server's command line, so it is absolute for the same
    // reason the other three are — and it is refused inside the target for the
    // same reason `--ir-dir` is: the package being documented is opened
    // read-only, and 8.5 MB landing in somebody's working tree is a write.
    let link_index = match link_index {
        None => None,
        Some(path) => {
            let resolved = crate::extract::resolve(path);
            if resolved.starts_with(&target) {
                return Err(Failure::Refused {
                    code: 3,
                    message: format!(
                        "--link-index {} is inside --target {}: the package being documented is \
                         opened read-only and nothing is ever written into it",
                        resolved.display(),
                        target.display(),
                    ),
                });
            }
            Some(crate::extract::absolute(path))
        }
    };
    let modules_file = crate::extract::absolute(modules_file);
    // 段 D. Computed here, once, for both callers — `build` and `incremental`
    // resolve `--target` differently and neither should own a second spelling of
    // a key that has to compare equal across runs. `None` when there is no map:
    // the extractor is passed the token only beside `--link-index`.
    let link_index_key = match &link_index {
        None => None,
        Some(_) => Some(link_index_key(&target, &modules_file)?),
    };
    Ok(Serve {
        bin: crate::extract::absolute(&bin),
        lake: crate::extract::or_env(lake, "LAKE").unwrap_or_else(|| PathBuf::from("lake")),
        target,
        jobs,
        modules_file,
        modules: modules.to_vec(),
        work: crate::extract::absolute(work),
        link_index,
        link_index_key,
    })
}

/// How many modules' oleans are hashed at once when the ledger is built or
/// checked (段 F).
///
/// **The ledger's bytes do not depend on this and its speed does** (M3-a 【実測】),
/// which is why this can be a default rather than a decision: `detect` reads and
/// hashes every module's oleans — **228,448,584 B over 422 modules** on the
/// measurement target — and until 段 F it did so one at a time because the
/// pipeline had no opinion and 1 was the value that needed no argument.
///
/// It needed one. Measured with `litedoc4 ledger check --concurrency N
/// --timings`, read-only, on that target【実測 2026-08-17】:
///
/// ```text
/// concurrency  1   hashSeconds 0.500
/// concurrency  2   hashSeconds 0.068
/// concurrency  4   hashSeconds 0.037
/// concurrency  8   hashSeconds 0.029
/// ```
///
/// **7.4x from one to two**, which is more than two threads of CPU can explain:
/// the work is reading mmap'd oleans, so the second thread is hiding page-fault
/// latency rather than adding arithmetic. That also says the ceiling is low — 4
/// is already within 30% of 8 — so this clamps rather than taking every core:
/// the run's real parallelism belongs to the extractor (`--jobs`), and a hashing
/// pass that took the whole machine would be competing with it for nothing.
///
/// `available_parallelism` fails on a machine that will not say; 4 is the same
/// default `--jobs` uses, and one thread is never wrong, only slow.
#[must_use]
pub(crate) fn hash_concurrency() -> usize {
    std::thread::available_parallelism().map_or(4, |cores| cores.get().clamp(1, 8))
}

/// 段 D: the token the extractor checks the dependency map's sidecar against.
///
/// The map is a function of three inputs (`Extract.lean`'s `writeLinkIndex`, 段 D
/// heading): the imported module set, those modules' oleans, and the omit list.
/// The extractor checks the first itself, out of the environment it is holding;
/// this covers the other two.
///
/// **The oleans are covered by three of `extractKey`'s five values rather than
/// by hashing them**: `leanToolchain`, `manifestSha256` and `extractor`. The
/// first two are this pipeline's existing answer to "could the bytes Lean
/// produces have moved" — the one that invalidates every module's IR when it
/// changes — and the third is the constant that names the code doing the
/// writing, the one `docs/implementation-plan.md` §6 says to bump when the
/// extractor is reimplemented.
///
/// **The other two are deliberately left out**, and this is a correction of the
/// first version of this function rather than a shortcut. `irSchemaVersion` and
/// `irGenerator` are read out of `<ir>/index.json` and describe **the IR**: the
/// second is explicitly "who wrote the IR on disk", a fact §6 says *not* to
/// update when the implementation changes. The map is not the IR and does not
/// move with either. Including them cost real behaviour: `extract_key` reads
/// that file, a first-ever build has not written it yet, so the token of a
/// first-ever build differed from the token of every run after it and **the
/// first incremental build after a first-ever build rewrote the map for
/// nothing** — a symptom with no defensible cause【実測 2026-08-17】.
///
/// **The omit list goes in by its bytes, not its path.** It is the package's
/// module list, it changes when the package gains or loses a module, and a path
/// is not an identity — the same path holding a different list has to move the
/// token (品質ゲート: 入力の同一性をパスで判定しない).
///
/// What is *not* here: the format of the map itself. The extractor checks that
/// against its own `#lidx2` marker, where a format change and the check that
/// catches it sit in the same file.
fn link_index_key(target: &Path, omit: &Path) -> Result<String, Failure> {
    // `None`: the IR half of the key is not wanted here (see above), so there is
    // nothing to read `<ir>/index.json` for.
    let key = litedoc4_incr::extract_key(&target.to_string_lossy(), None).map_err(refused)?;
    let mut bytes = Vec::new();
    for name in ["leanToolchain", "manifestSha256", "extractor"] {
        let Some(value) = key.get(name) else {
            return Err(Failure::Refused {
                code: 3,
                message: format!("extractKey has no `{name}`: the reuse token cannot be built"),
            });
        };
        // `name=value\n`, one per line: neither half can contain a newline (a
        // toolchain string, a hex digest and a compile-time constant), so the
        // concatenation is unambiguous without escaping.
        bytes.extend_from_slice(name.as_bytes());
        bytes.push(b'=');
        bytes.extend_from_slice(value.as_bytes());
        bytes.push(b'\n');
    }
    // A blank line ends the key half, so that no rearrangement of its characters
    // can produce the same digest as a different omit list.
    bytes.push(b'\n');
    bytes.extend_from_slice(&fs::read(omit).map_err(|source| Failure::Refused {
        code: 3,
        message: format!("{}: {source}", omit.display()),
    })?);
    Ok(litedoc4_incr::sha256_hex(&bytes))
}

/// Plan 決定 1 / §7 debt 7: the revision in `--source-url` has to be 40 hex
/// digits.
///
/// **Checked here and nowhere else** 【判断】. `render` and `site` ask only for a
/// non-empty string and keep asking for one: tightening them would move bytes in
/// the middle of a migration, and they are called by hand and by harnesses that
/// pass placeholder URLs on purpose. The pipeline is the path that runs *every
/// commit*, and it is the only place a real revision enters, so the check
/// belongs on it.
pub(crate) fn check_source_url(url: &str) -> Result<(), Failure> {
    let broken = "the acceptance oracle normalises `/blob/[0-9a-f]{40}/` and nothing else \
                  (coverage.ts:512), so with a tag or a branch name here every page keeps its \
                  revision in the compared bytes and the score drops 3.1103 points with no \
                  diagnostic 【実測, plan 決定 1】";
    let Some((_, rest)) = url.split_once("/blob/") else {
        return usage(format!(
            "--source-url has no `/blob/` segment: {url}\n  {broken}",
        ));
    };
    let rev = rest.split('/').next().unwrap_or(rest);
    let hex = rev.len() == REV_HEX_DIGITS
        && rev
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    if hex {
        return Ok(());
    }
    usage(format!(
        "--source-url must carry a {REV_HEX_DIGITS}-digit lower-case hex revision after `/blob/`, \
         not `{rev}` ({} character(s))\n  {broken}",
        rev.chars().count(),
    ))
}

/// One JSON line, in `incremental.sh:393-424`'s field names.
///
/// The names are the contract: `benchmarks/tools/analyze.ts` and every JSONL
/// already in `benchmarks/results/` read them. **Two of the prototype's fields
/// are gone** — `global_impl` and `l3_1` — because the flags behind them are not
/// here (see the module heading); `module` is gone for the same reason and
/// `analyze.ts:121` already falls back when it is absent.
///
/// `serve` is written on both paths and `jobs` only on the resident one 【判断】:
/// a resident run and a fresh run are otherwise indistinguishable in the record,
/// which is what the prototype's comment says the field is for
/// (`incremental.sh:398-401`) — but behind `--extractor` the job count is inside
/// somebody else's argument list and this command does not know it. A number it
/// cannot see is left out rather than guessed at. `serveGeneration` is the olean
/// world every request of that run was checked against (see
/// [`crate::resident`]); it is the prototype's `--generation` tag with the tag
/// replaced by the thing itself.
///
/// The four nested records (`extract` / `merge` / `global` / `render`) are the
/// per-stage timings files, embedded as the prototype embeds them: one object
/// when there is one round, an array when there are more. They are read from
/// fixed paths rather than from a glob, so a directory listing's order cannot
/// reach the record.
fn write_timings(
    path: &Path,
    work: &Path,
    summary: &Summary,
    clocks: &Timings,
    serve: Option<&(usize, String)>,
) -> Result<(), Failure> {
    let mut record = serde_json::Map::new();
    record.insert("mode".to_owned(), summary.mode.clone().into());
    record.insert("serve".to_owned(), serve.is_some().into());
    if let Some((jobs, generation)) = serve {
        record.insert("jobs".to_owned(), serde_json::json!(jobs));
        record.insert(
            "serveGeneration".to_owned(),
            serde_json::json!(generation.clone()),
        );
    }
    for (name, value) in [
        ("detectSeconds", clocks.detect),
        ("extractSeconds", clocks.extract),
        ("ownershipSeconds", clocks.ownership),
        ("mergeSeconds", clocks.merge),
        ("roundsSeconds", clocks.rounds),
        ("pruneSeconds", clocks.prune),
        ("globalSeconds", clocks.global),
        ("impactSeconds", clocks.impact),
        ("renderSeconds", clocks.render),
        ("totalSeconds", clocks.total),
    ] {
        record.insert(name.to_owned(), serde_json::json!(value));
    }
    for (name, value) in [
        ("rounds", summary.rounds),
        ("staleFound", summary.stale_found),
        ("changed", summary.changed),
        ("removed", summary.removed),
        ("irChanged", summary.ir_changed),
        ("globalStale", summary.global_stale),
        ("pagesRendered", summary.pages_rendered),
        ("mathFallbacks", summary.math_fallbacks),
    ] {
        record.insert(name.to_owned(), serde_json::json!(value));
    }

    let rounds = summary.rounds;
    let per_round = |stem: &str| -> Vec<PathBuf> {
        (1..=rounds)
            .map(|round| work.join(format!("{stem}-{round}.json")))
            .filter(|path| path.is_file())
            .collect()
    };
    for (name, paths) in [
        ("extract", per_round("extract-timings")),
        ("merge", per_round("merge-timings")),
        ("global", vec![work.join("global-timings.json")]),
        ("render", vec![work.join("render-timings.json")]),
    ] {
        let loaded: Vec<serde_json::Value> = paths
            .iter()
            .filter(|path| path.is_file())
            .filter_map(|path| fs::read_to_string(path).ok())
            .filter_map(|text| serde_json::from_str(&text).ok())
            .collect();
        match loaded.len() {
            0 => {}
            1 => {
                record.insert(
                    name.to_owned(),
                    loaded.into_iter().next().expect("one element"),
                );
            }
            _ => {
                record.insert(name.to_owned(), serde_json::Value::Array(loaded));
            }
        }
    }

    let line = serde_json::to_string(&serde_json::Value::Object(record))
        .expect("counts and durations serialise");
    println!("{line}");
    write_file(path, &(line + "\n"))
}

// ---------------------------------------------------------------- the glob

/// `litedoc4 modules` — the package's module list, from a glob over the sources.
///
/// Ported from `stage7h/run.sh:82-86`'s `modlist()`:
///
/// ```text
/// find <Lib>.lean <Lib> -name '*.lean' | sort | sed 's/\.lean$//; s#/#.#g'
/// ```
///
/// **The sources, never `.lake/build`** (plan §5, M3-d). A walk of the build tree
/// picks up **659 orphan oleans** on the target 【実測】 — modules that were
/// deleted from the sources and whose compiled output Lake never removed — and
/// every one of them becomes a module the ledger watches and the extractor is
/// asked for.
///
/// **`--lib` now has an origin** (M4-d, M3-d2's fifth debt): with none given the
/// names are read out of the package's lakefile — [`crate::lakefile`], which
/// reads `lakefile.toml` and refuses `lakefile.lean` by name. The flag stays,
/// and it repeats, because a package may declare more than one library and
/// because a caller may want a subset of what the lakefile declares.
///
/// # The order is deliberately not the prototype's 【判断】
///
/// `sort` collates in the caller's locale. On this machine's default
/// (`en_US.UTF-8`) it puts `…Shannon.ArithmeticCoding` before
/// `…Shannon.AWGN.Main`; under `LC_ALL=C` it is the other way round. The same
/// 432 names in a different order 【実測 2026-08-12: same set, 22 lines moved】 —
/// and this list's order **is** the ledger's `modules` array order, so a ledger
/// built on two machines with different locales is two different files.
///
/// So the names are sorted here, in **UTF-16 code unit order** (plan §7, U1) —
/// the order every other module list in this project is in, and the order
/// `check`'s own re-extract set comes back in. **No generated byte depends on
/// it**: `check` sorts its re-extract set (`detect.rs:307`) and `impact` sorts
/// its selection, so the order reaches the ledger's array and the diagnostic
/// files and stops there.
pub(crate) fn modules(args: &[String]) -> Result<(), Failure> {
    let mut root: Option<PathBuf> = None;
    let mut libs: Vec<String> = Vec::new();
    let mut out: Option<PathBuf> = None;

    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        let mut value = |flag: &str| -> Result<String, Failure> {
            match rest.next() {
                Some(value) => Ok(value.clone()),
                None => usage(format!("{flag} needs a value")),
            }
        };
        match arg.as_str() {
            "--root" => root = Some(value("--root")?.into()),
            "--lib" => libs.push(value("--lib")?),
            "--out" => out = Some(value("--out")?.into()),
            "--help" | "-h" => {
                println!("{USAGE}");
                return Ok(());
            }
            other => return usage(format!("unknown argument `{other}`")),
        }
    }
    let Some(root) = root else {
        return usage("--root <repo> is required");
    };
    if libs.is_empty() {
        let declared = crate::lakefile::read_libraries(&root)?;
        // Which file answered, and what it said — **on stderr**, because this
        // command's stdout is the module list itself when `--out` is absent and
        // a caller redirecting it into a file would otherwise get a diagnostic
        // as its first module. A list that came out a library short is a site
        // that came out a library short, and the only way to notice is to see
        // the names.
        eprintln!(
            "lib     {} (from {})",
            declared.names.join(", "),
            declared.file.display(),
        );
        libs = declared.names;
    }
    let names = module_names(&root, &libs)?;

    match out {
        Some(path) => {
            write_lines(&path, &names)?;
            println!("{} modules -> {}", names.len(), path.display());
        }
        None => {
            for name in &names {
                println!("{name}");
            }
        }
    }
    Ok(())
}

/// The glob itself: every module of every named library, in UTF-16 order.
///
/// Split out of [`modules`] so that [`crate::build`] derives the list the same
/// way rather than shelling out to this command — **the same list has to reach
/// `detect`, the extractor and `merge`** (M3-d2b), and two derivations of "the
/// same" list is exactly how that stops being true.
pub(crate) fn module_names(root: &Path, libs: &[String]) -> Result<Vec<String>, Failure> {
    // Relative paths, as `find` prints them from inside the repository.
    let mut paths: Vec<String> = Vec::new();
    for lib in libs {
        let file = root.join(format!("{lib}.lean"));
        let dir = root.join(lib);
        let has_file = file.is_file();
        let has_dir = dir.is_dir();
        if !has_file && !has_dir {
            return Err(Failure::Refused {
                code: 3,
                message: format!(
                    "no {lib}.lean and no {lib}/ under {}: --lib names a library root, and an \
                     empty module list would look like a package whose every module was deleted",
                    root.display(),
                ),
            });
        }
        if has_file {
            paths.push(format!("{lib}.lean"));
        }
        if has_dir {
            collect_lean(&dir, lib, &mut paths)?;
        }
    }
    // **The path's components become a Lean *name*, and that is an escaping**
    // 【M5-b, 実測】. `Alpha/Odd-Name.lean` is the module Lean spells
    // `Alpha.«Odd-Name»`; written as `Alpha.Odd-Name` it does not parse — the
    // extractor's `String.toName` yields `Name.anonymous` and the run dies with
    // `import failed, trying to import module with anonymous name` before it has
    // imported anything. See [`litedoc4_ir::name`] for the whole mechanism. This
    // is the identity for every module name that is already an identifier, which
    // is all 432 of the measurement target's.
    let mut names: Vec<String> = paths
        .iter()
        .map(|path| {
            let stem = path.strip_suffix(".lean").unwrap_or(path);
            litedoc4_ir::escape_module(stem.split('/'))
        })
        .collect();
    // Plan §7, U1 — and see [`modules`]'s heading for why this is not the
    // prototype's `sort`. `dedup` after the sort: two `--lib` arguments that
    // overlap name the same module twice, and a ledger with a repeated module is
    // one whose `check` compares it against itself.
    sort_utf16(&mut names);
    names.dedup();
    Ok(names)
}

/// Every `*.lean` under `dir`, as a path relative to the repository root.
///
/// `find` follows no symlinks by default and neither does this: a symlinked
/// directory inside a library would otherwise let one module be listed twice
/// under two names, and the second one has no olean.
#[expect(
    clippy::case_sensitive_file_extension_comparisons,
    reason = "reproduces `find -name '*.lean'`, which decides what a module is"
)]
fn collect_lean(dir: &Path, prefix: &str, out: &mut Vec<String>) -> Result<(), Failure> {
    let listing = fs::read_dir(dir)
        .map_err(|source| Failure::Failed(format!("{}: {source}", dir.display())))?;
    for entry in listing {
        let entry =
            entry.map_err(|source| Failure::Failed(format!("{}: {source}", dir.display())))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let kind = entry
            .file_type()
            .map_err(|source| Failure::Failed(format!("{}: {source}", entry.path().display())))?;
        let relative = format!("{prefix}/{name}");
        if kind.is_dir() {
            collect_lean(&entry.path(), &relative, out)?;
        } else if kind.is_file() && name.ends_with(".lean") {
            out.push(relative);
        }
    }
    Ok(())
}

// ------------------------------------------------------------------ plumbing

/// The first 16 digits of a digest, or `none` — for the one diagnostic line
/// that has to say which of two maps this run read.
fn digest_or_none(digest: Option<&str>) -> String {
    digest.map_or_else(
        || "none".to_owned(),
        |digest| digest.chars().take(16).collect(),
    )
}

/// One name per line, and **no line at all** when there are no names.
///
/// The same spelling every stage uses (`detect::write_text`), for the same
/// reason: an empty set has to be an empty file rather than one blank line, or
/// `--only-from` and the round loop disagree about what "nothing" is.
pub(crate) fn write_lines(path: &Path, items: &[String]) -> Result<(), Failure> {
    let body = if items.is_empty() {
        String::new()
    } else {
        items.join("\n") + "\n"
    };
    write_file(path, &body)
}

fn write_file(path: &Path, body: &str) -> Result<(), Failure> {
    if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        create_dir(dir)?;
    }
    fs::write(path, body).map_err(|source| Failure::Failed(format!("{}: {source}", path.display())))
}

pub(crate) fn create_dir(path: &Path) -> Result<(), Failure> {
    fs::create_dir_all(path)
        .map_err(|source| Failure::Failed(format!("{}: {source}", path.display())))
}
