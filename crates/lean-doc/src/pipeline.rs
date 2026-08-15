//! `lean-doc incremental` — the pipeline that sequences the six Rust stages, and
//! `lean-doc modules` — the source glob its `--modules` list comes from.
//!
//! Milestone **M3-d2**. Ported from `experiments/stage7h/incremental.sh` (441
//! lines, frozen). This is the last piece of the incremental path: every stage
//! it drives already lives in a library, so what moves here is **the ordering,
//! the round loop and the union of the two render-set derivations** — the three
//! things that belong to no stage.
//!
//! ```text
//!  1 detect     lean_doc_incr::check_ledger      changed / removed / render-all
//!  2 extract    --extractor <program>            the only external process
//!  3 ownership  lean_doc_incr::ownership   ┐ L3-1: who points at a name that moved
//!  4 merge      lean_doc_incr::merge       ┘ rounds, bounded by --max-rounds
//!  5 prune      lean_doc_incr::prune             the deleted modules' pages
//!  6 global     lean_doc_global::build_global    six artifacts + the map delta
//!  7 render     lean_doc_render::render_site     the union of the two sets
//! ```
//!
//! **The stages are library calls, not subprocesses.** `lean-doc` never
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
//! 6. **`--jobs` is the resident extractor's start-time configuration**, so it is
//!    not a flag of this command. It reaches the extractor through
//!    `--extractor-arg --jobs --extractor-arg 4` like any other of its settings.
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
//! `--work`, under the prototype's names, **as diagnostics** — a differential
//! harness (M3-d3) compares them. `impact-set.txt` is written by `impact` itself
//! and is therefore still absent exactly when the prototype's is absent; that
//! absence is now inert, because nothing reads it back.
//!
//! # What a round is allowed to touch
//!
//! - **`prune` is called without an IR tree.** Pointed at a site, its orphan rule
//!   deletes every `.html` that is not a live module page, which on the target is
//!   the three whole-package HTML artifacts — **438 → 435** 【実測, plan §7 debt
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
//!   them (`lean-doc ledger build`), or the second run re-extracts the first
//!   run's changed set again — wasteful, not wrong. **Who owns that is M4's**,
//!   together with the resident extractor.
//! - **It does not start or stop a resident extractor** (`--serve`,
//!   `--serve-dir`, `--serve-from`): that is on the far side of `--extractor` and
//!   arrives with M4.
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

use lean_doc_global::{GlobalOptions, build_global};
use lean_doc_incr::{
    CheckOptions, ImpactOptions, MergeOptions, Mode, OwnershipOptions, PruneOptions, check_ledger,
    impact as run_impact, merge as run_merge, ownership as run_ownership, prune as run_prune,
    read_module_list,
};
use lean_doc_ir::sort_utf16;
use lean_doc_render::{ModuleSet, RenderOptions, render_site};

use crate::{Failure, LINK_INDEX_COST, USAGE, print_global_summary, refused, usage};

/// `--max-rounds` reached with modules still stale. The prototype's exit code
/// (`incremental.sh:293`), and the one number a caller may branch on: it means
/// "the loop did not converge", not "something is broken".
pub const EXIT_ROUNDS: u8 = 5;

/// The extractor exited non-zero.
///
/// **Deliberately not the child's own code** 【判断】. `set -euo pipefail` gave
/// the prototype whatever the extractor exited with, which can be 5 — and 5
/// already means "the round loop did not converge" here. A caller that has to
/// tell those apart cannot, so the child's code is put in the message instead of
/// on the process.
pub const EXIT_EXTRACTOR: u8 = 4;

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
struct Incremental<'a> {
    ir: &'a Path,
    pages: &'a Path,
    ledger: &'a Path,
    work: &'a Path,
    /// The current module list, from a glob over the sources — `lean-doc
    /// modules`. **Required**, unlike the prototype's, where it is optional
    /// (`${MODULES:+--modules …}`): without it `check` re-reads the ledger's own
    /// list and **cannot see a module that appeared or vanished**, which are two
    /// of the seven states this pipeline is judged on.
    modules: Vec<String>,
    source_url: &'a str,
    link_index: &'a Path,
    state: &'a Path,
    extractor: Extractor,
    mode: Mode,
    max_rounds: usize,
}

/// The extractor, as a program and the arguments that configure it.
///
/// **There is no default, and that is the design** 【判断】. Two jobs:
///
/// 1. **It is M4's boundary.** Productising the extractor — moving `Extract.lean`
///    into the tree, starting and stopping the resident server, deciding
///    `--jobs` — is M4. A default here would make the shipped binary depend on a
///    path inside `experiments/`, which is frozen; and M3-d3 can hand the
///    prototype's own `stage7g/extract-once.sh` straight to `--extractor`,
///    because the three flags below are exactly its required arguments
///    (`extract-once.sh:24, 47`).
/// 2. **It is what lets the pipeline be tested without Lean.** The tests pass a
///    fake extractor that copies a baked partial IR tree into `--ir-dir`. Without
///    a seam here every test of this file would need a built Lean toolchain and a
///    30-second extraction, which in practice means the pipeline is not tested at
///    all.
struct Extractor {
    program: String,
    /// `--extractor-arg`, in order, placed **before** the three flags below so
    /// that a wrapper script sees its own configuration first.
    args: Vec<String>,
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
    /// Lean side reports its phase timers.
    fn run(&self, modules: &Path, ir_dir: &Path, timings: &Path) -> Result<(), Failure> {
        let mut command = Command::new(&self.program);
        command
            .args(&self.args)
            .arg("--modules")
            .arg(modules)
            .arg("--ir-dir")
            .arg(ir_dir)
            .arg("--timings")
            .arg(timings);
        let status = command.status().map_err(|source| Failure::Refused {
            code: EXIT_EXTRACTOR,
            message: format!("--extractor {}: {source}", self.program),
        })?;
        if status.success() {
            return Ok(());
        }
        Err(Failure::Refused {
            code: EXIT_EXTRACTOR,
            message: format!(
                "--extractor {} exited {} for {}; the IR was not updated and nothing was rendered",
                self.program,
                status
                    .code()
                    .map_or_else(|| "on a signal".to_owned(), |code| code.to_string()),
                modules.display(),
            ),
        })
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
struct Summary {
    rounds: usize,
    stale_found: usize,
    changed: usize,
    removed: usize,
    ir_changed: usize,
    global_stale: usize,
    pages_rendered: usize,
    mode: String,
}

/// One incremental round: a changed build tree in, an updated IR and updated
/// pages out.
fn run_incremental(options: &Incremental<'_>) -> Result<(Summary, Timings), Failure> {
    let started = Instant::now();
    let work = Work::new(options.work);
    create_dir(options.work)?;

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
        // The ledger's bytes do not depend on this (M3-a 【実測】); its speed
        // does, and a flag for it belongs with the rest of M4's tuning.
        concurrency: 1,
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
            options.extractor.run(
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
    let mode = if check.render_all() {
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
    // Plan §7, U1. The prototype's `sort -u` is the shell's collation; the two
    // agree on every name Lean has produced on the target (all ASCII), and this
    // one is the order the rest of the project's module lists are in.
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
    let pages_rendered = render_set.len();
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
            link_index: Some(options.link_index),
            only: &only,
        })
        .map_err(|e| Failure::Failed(e.to_string()))?;
        let record = serde_json::json!({
            "command": "render",
            "pagesWritten": rendered.pages_written,
            "modulesInIr": rendered.modules_in_ir,
            "declarationsRendered": rendered.declarations_rendered,
            "pageBytes": rendered.bytes_written,
            "renderSeconds": at.elapsed().as_secs_f64(),
        });
        write_file(
            &work.render_timings,
            &(serde_json::to_string(&record).expect("counts and durations serialise") + "\n"),
        )?;
        crate::print_render_summary("render  ", &rendered);
    }
    let render_done = started.elapsed();

    Ok((
        Summary {
            rounds,
            stale_found,
            changed: check.re_extract.len(),
            removed: check.removed.len(),
            ir_changed: ir_changed.len(),
            global_stale: global_affected.len(),
            pages_rendered,
            mode: mode.name().to_owned(),
        },
        Timings {
            detect: detect_done.as_secs_f64(),
            extract: extract_seconds,
            ownership: ownership_seconds,
            merge: merge_seconds,
            rounds: (rounds_done - detect_done).as_secs_f64(),
            prune: (prune_done - rounds_done).as_secs_f64(),
            global: (global_done - prune_done).as_secs_f64(),
            impact: (impact_done - global_done).as_secs_f64(),
            render: (render_done - impact_done).as_secs_f64(),
            total: render_done.as_secs_f64(),
        },
    ))
}

/// The wall-clock split of one run, in the prototype's phases.
///
/// Kept out of [`Summary`] because the durations are **diagnostics**: nothing
/// may assert on them, and a summary without them is one a test can compare with
/// `==`.
struct Timings {
    detect: f64,
    extract: f64,
    ownership: f64,
    merge: f64,
    rounds: f64,
    prune: f64,
    global: f64,
    impact: f64,
    render: f64,
    total: f64,
}

/// `prune` over a deletion list, and **nothing else**.
///
/// The signature is the guard 【判断】. [`PruneOptions::ir`] turns on the orphan
/// rule, which calls every `.html` that is not a live module page an orphan —
/// pointed at a site that includes the whole-package artifacts, that is
/// `navbar.html`, `references.html` and `tactics.html`, and the site goes
/// **438 → 435** 【実測, plan §7 debt 4】. The prototype never passes `--ir`
/// either (`incremental.sh:304-305`), so nobody has walked into it yet; here
/// there is no parameter to pass it through.
fn prune_removed(
    pages: &Path,
    remove: &Path,
    json: &Path,
) -> Result<lean_doc_incr::PruneSummary, Failure> {
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

/// `lean-doc incremental`.
pub fn incremental(args: &[String]) -> Result<(), Failure> {
    let mut ir: Option<PathBuf> = None;
    let mut pages: Option<PathBuf> = None;
    let mut ledger: Option<PathBuf> = None;
    let mut work: Option<PathBuf> = None;
    let mut modules: Option<PathBuf> = None;
    let mut source_url: Option<String> = None;
    let mut link_index: Option<PathBuf> = None;
    let mut state: Option<PathBuf> = None;
    let mut extractor: Option<String> = None;
    let mut extractor_args: Vec<String> = Vec::new();
    let mut mode: Option<String> = None;
    let mut max_rounds = DEFAULT_MAX_ROUNDS;
    let mut timings: Option<PathBuf> = None;

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
            // Refused by name rather than as "unknown argument": each is a real
            // flag of `incremental.sh`, so what the caller needs to hear is why
            // it is gone, not that it was misspelled. See the module heading.
            "--jobs" => {
                return usage(
                    "--jobs is not a pipeline flag: parallelism is the extractor's, and a resident \
                     one fixes it at start-up (plan §6, constraint 6). Pass it through with \
                     `--extractor-arg --jobs --extractor-arg <n>`",
                );
            }
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
            "--serve" | "--serve-dir" | "--serve-from" => {
                return usage(format!(
                    "{arg} is not a pipeline flag yet: the resident extractor lives on the far \
                     side of --extractor and arrives with M4",
                ));
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
             own and cannot see a module that appeared or vanished. `lean-doc modules` writes it",
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
             run — full generation with `lean-doc site --state`, or the last incremental round — \
             is what leaves it behind",
        );
    };
    let Some(program) = extractor else {
        return usage(
            "--extractor <program> is required and has no default: it is called as `<program> \
             [<extractor-arg>…] --modules <list> --ir-dir <dir> --timings <file>`, which is \
             `stage7g/extract-once.sh`'s interface. Productising the extractor is M4",
        );
    };
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
    let (summary, clocks) = run_incremental(&Incremental {
        ir: &ir,
        pages: &pages,
        ledger: &ledger,
        work: &work,
        modules: module_list,
        source_url: &source_url,
        link_index: &link_index,
        state: &state,
        extractor: Extractor {
            program,
            args: extractor_args,
        },
        mode,
        max_rounds,
    })?;

    if let Some(path) = timings {
        write_timings(&path, &work, &summary, &clocks)?;
    }
    Ok(())
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
fn check_source_url(url: &str) -> Result<(), Failure> {
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
/// already in `benchmarks/results/` read them. **Four of the prototype's fields
/// are gone** — `global_impl`, `l3_1`, `jobs` and the `serve*` group — because
/// the flags behind them are not here (see the module heading); `module` is gone
/// for the same reason and `analyze.ts:121` already falls back when it is
/// absent.
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
) -> Result<(), Failure> {
    let mut record = serde_json::Map::new();
    record.insert("mode".to_owned(), summary.mode.clone().into());
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

/// `lean-doc modules` — the package's module list, from a glob over the sources.
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
/// **Where `--lib` comes from is M4's.** A `lakefile` declares it; reading one is
/// a dependency on Lake's format that this milestone does not need, so the name
/// is an argument here. It repeats, because a package may declare more than one
/// library.
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
pub fn modules(args: &[String]) -> Result<(), Failure> {
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
        return usage(
            "--lib <Name> is required: it names `<Name>.lean` and `<Name>/` under --root. Which \
             libraries a package declares is in its lakefile, and reading one is M4's",
        );
    }

    // Relative paths, as `find` prints them from inside the repository.
    let mut paths: Vec<String> = Vec::new();
    for lib in &libs {
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
    let mut names: Vec<String> = paths
        .iter()
        .map(|path| path.strip_suffix(".lean").unwrap_or(path).replace('/', "."))
        .collect();
    // Plan §7, U1 — and see this function's heading for why this is not the
    // prototype's `sort`. `dedup` after the sort: two `--lib` arguments that
    // overlap name the same module twice, and a ledger with a repeated module is
    // one whose `check` compares it against itself.
    sort_utf16(&mut names);
    names.dedup();

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

/// Every `*.lean` under `dir`, as a path relative to the repository root.
///
/// `find` follows no symlinks by default and neither does this: a symlinked
/// directory inside a library would otherwise let one module be listed twice
/// under two names, and the second one has no olean.
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

/// One name per line, and **no line at all** when there are no names.
///
/// The same spelling every stage uses (`detect::write_text`), for the same
/// reason: an empty set has to be an empty file rather than one blank line, or
/// `--only-from` and the round loop disagree about what "nothing" is.
fn write_lines(path: &Path, items: &[String]) -> Result<(), Failure> {
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

fn create_dir(path: &Path) -> Result<(), Failure> {
    fs::create_dir_all(path)
        .map_err(|source| Failure::Failed(format!("{}: {source}", path.display())))
}
