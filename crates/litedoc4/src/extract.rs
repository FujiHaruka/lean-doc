//! `litedoc4 extract` — one extractor process over a module list, and its phase
//! timers folded into one JSON object.
//!
//! Milestone **M4-b**. Ported from `experiments/stage7g/extract-once.sh` (87
//! lines, frozen), which did three things; this does two of them:
//!
//! 1. start the Lean extractor **inside the target package** through `lake env`,
//!    with the six flags that spell "IR schema 5";
//! 2. fold the extractor's events JSONL into a single timings object;
//! 3. ~~send the request to a resident extractor instead (`--serve-dir`)~~ —
//!    residency arrived in **M4-c** and it is **`litedoc4 incremental --serve`**,
//!    not a flag here. See [`crate::resident`]: a server that answers one request
//!    and stops is a one-shot process with a protocol in front of it, and the
//!    only caller that extracts more than once from one environment is the round
//!    loop. Every `--serve*` spelling is still refused by name below, now with
//!    that as the reason.
//!
//! This file's [`fold_timings`], [`FIXED_FLAGS`] and [`resolve`] are the resident
//! path's too: the two paths differ in **who owns the process**, and in nothing
//! else that reaches a byte.
//!
//! # Why this is a subcommand and not a library call 【判断】
//!
//! Every other stage of the pipeline is a library call ([`crate::pipeline`]'s
//! heading says why). This one is a process on purpose, and stays one:
//!
//! - **`litedoc4 incremental --extractor` already names a program**, and its
//!   contract is `<program> [<extractor-arg>…] --modules <list> --ir-dir <dir>
//!   --timings <file>` (`pipeline.rs`'s [`crate::pipeline`] `Extractor`). Making
//!   extraction a subcommand means the product can be its own extractor —
//!   `--extractor <litedoc4> --extractor-arg extract` — **without closing the
//!   seam**: the pipeline's tests still hand it a fake, so testing the pipeline
//!   still does not need Lean and a 20-second extraction.
//! - **The Lean binary cannot be linked in.** It is 171 MB, it is built by
//!   `extractor/build.sh` against the *target's* toolchain, and it has to run
//!   with that target as its working directory. `lake env` is the thing that
//!   sets that environment up, so a process boundary exists whatever this file
//!   does; what this file decides is only who spells the command line.
//!
//! # What is fixed here rather than exposed
//!
//! The six extraction flags — `--equations --refs --write-ir --tagged-code
//! --jobs N --ir-dir <dir>` (`extract-once.sh:63-64`) — are not configurable
//! except for `--jobs` and `--ir-dir`. Four of them *are* the IR the rest of the
//! product reads: `--tagged-code` is what makes it schema 5, `--refs` is what
//! fills the reference arrays that the link resolution and `ownership` both
//! consume, and `--write-ir` is the point. A run with any of them off produces a
//! tree that parses and renders wrongly, which is the worst failure shape this
//! project has. They are refused by name, with the reason, rather than silently
//! accepted or silently ignored.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::{Failure, USAGE, usage};

/// The extractor exited non-zero.
///
/// The same number [`crate::pipeline`] reports when a child extractor fails, and
/// deliberately so: when this command *is* that child, the code a caller sees is
/// the same whichever of the two produced it.
pub(crate) const EXIT_EXTRACTOR: u8 = 4;

/// The six flags that spell "IR schema 5", minus the two the caller chooses.
pub(crate) const FIXED_FLAGS: [&str; 4] = ["--equations", "--refs", "--write-ir", "--tagged-code"];

/// `litedoc4 extract`.
pub(crate) fn extract(args: &[String]) -> Result<(), Failure> {
    let mut modules: Option<PathBuf> = None;
    let mut ir_dir: Option<PathBuf> = None;
    let mut timings: Option<PathBuf> = None;
    let mut events: Option<PathBuf> = None;
    let mut link_index: Option<PathBuf> = None;
    let mut link_index_omit: Option<PathBuf> = None;
    let mut link_index_key: Option<String> = None;
    let mut jobs: usize = 1;
    let mut bin: Option<PathBuf> = None;
    let mut target: Option<PathBuf> = None;
    let mut lake: Option<PathBuf> = None;

    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        let mut value = |flag: &str| -> Result<String, Failure> {
            match rest.next() {
                Some(value) => Ok(value.clone()),
                None => usage(format!("{flag} needs a value")),
            }
        };
        match arg.as_str() {
            "--modules" => modules = Some(value("--modules")?.into()),
            "--ir-dir" => ir_dir = Some(value("--ir-dir")?.into()),
            "--timings" => timings = Some(value("--timings")?.into()),
            "--events" => events = Some(value("--events")?.into()),
            "--jobs" => {
                let raw = value("--jobs")?;
                jobs = raw
                    .parse()
                    .map_err(|_| Failure::Usage(format!("--jobs wants a number, not {raw}")))?;
            }
            // M5-b. `Extract.lean`'s own flag (M5-a): the dependency closure's
            // `name -> module` map, written out of the environment this process
            // has imported for the extraction anyway. Optional here — the map is
            // a whole-package artefact and an extraction is often a subset — but
            // when it is asked for it costs 0.9 s warm on top of the run, not a
            // second 15-second environment load.
            "--link-index" => link_index = Some(value("--link-index")?.into()),
            // 段 C. Handed straight to the extractor: the modules whose own
            // declaration groups the map leaves out. The renderer answers those
            // names out of the IR-derived index before it reads the `.lidx` at
            // all, so leaving them out renders the same bytes — and it is the
            // only part of the map an edit to the package moves, which is what
            // makes the map stop invalidating `renderKey`. See
            // `Extract.lean`'s `writeLinkIndex` heading for the measurement.
            //
            // A path, not a derivation from `--modules`: this command extracts a
            // *subset* as often as not, and the omit set has to be the package,
            // not this round's slice of it. The caller that knows the difference
            // is the one that writes both files.
            "--link-index-omit" => link_index_omit = Some(value("--link-index-omit")?.into()),
            // 段 D. An opaque token, passed through verbatim — this command does
            // not compute one and does not interpret one. The extractor uses it
            // as the caller's promise about the two inputs it cannot see (the
            // oleans behind the imported modules, and the omit list's bytes) and
            // leaves the map alone when the token still matches the sidecar
            // `<map>.key` *and* the map's `@` section still matches the
            // environment; see `Extract.lean`'s `writeLinkIndex` heading.
            //
            // Not derived here from `--target`, unlike `litedoc4 build`'s: this
            // command is the seam, and a caller driving it in a loop is the one
            // that knows what its own runs have in common. `litedoc4 build`
            // computes the token in `pipeline::serve_options` and that is the
            // only place in the product that does.
            "--link-index-key" => link_index_key = Some(value("--link-index-key")?),
            "--extractor-bin" => bin = Some(value("--extractor-bin")?.into()),
            "--target" => target = Some(value("--target")?.into()),
            "--lake" => lake = Some(value("--lake")?.into()),
            // Refused by name rather than as "unknown argument": each is a real
            // flag of the program behind this one, so what a caller needs to
            // hear is why it is not offered. Same rule as `incremental`'s.
            "--serve" | "--serve-dir" | "--serve-from" => {
                return usage(format!(
                    "{arg} is not an `extract` flag: residency is `litedoc4 incremental --serve` \
                     (M4-c). A server that answers one request and stops is this command with a \
                     protocol in front of it — the environment is still imported once per \
                     extraction — so the only caller it can pay off for is the round loop, which \
                     owns the server for the whole run. `--serve-dir` is not offered anywhere: a \
                     server this process did not start is one whose olean generation it cannot \
                     vouch for, and that is where correctness comes from 【実測, stage 6a】",
                ));
            }
            flag if FIXED_FLAGS.contains(&flag) => {
                return usage(format!(
                    "{arg} is not a flag here: it is always on. Those four are what \"IR schema \
                     4\" means, and an IR written without one of them parses and renders wrongly \
                     rather than failing. See this file's heading",
                ));
            }
            "--no-attrs" | "--no-inst-index" | "--no-member-extra" => {
                return usage(format!(
                    "{arg} is an ablation, not a product flag: it subtracts one of the three \
                     stage-7b additions so its cost can be measured, and the resulting index.json \
                     carries an `ablations` list precisely because the tree is not renderable",
                ));
            }
            "--decl-profile" | "--pp-breakdown" | "--dump" | "--dump-modules" | "--dump-refs"
            | "--dump-tactics" | "--only" | "--open" | "--tag" | "--skip-analyze"
            | "--tactics-emulate" | "--tactics-probe" => {
                return usage(format!(
                    "{arg} is a measurement or inspection flag of the extractor, not a product \
                     one. Run `extractor/build/extract` directly for it — the command line is in \
                     `Extract.lean`'s header",
                ));
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                return Ok(());
            }
            other => return usage(format!("unknown argument `{other}`")),
        }
    }

    let Some(modules) = modules else {
        return usage(
            "--modules <file> is required: the module list to extract, one name per line",
        );
    };
    let Some(ir_dir) = ir_dir else {
        return usage(
            "--ir-dir <dir> is required and has no default: the extractor's own default was one \
             session's scratchpad path and is gone (M4-a). An IR tree written somewhere the \
             caller did not name is worse than none",
        );
    };
    let Some(timings) = timings else {
        return usage(
            "--timings <file> is required: it is the extractor's phase timers folded into one \
             JSON object, and `litedoc4 incremental` merges it into the run's record",
        );
    };
    if jobs == 0 {
        return usage("--jobs must be at least 1");
    }
    // 段 C. Refused rather than ignored. The extractor itself tolerates the
    // combination — it is a low-level tool and a script that passes the omit
    // list unconditionally is not making a mistake — but here the flag would do
    // nothing at all, and a flag that does nothing is the shape of bug this
    // project keeps finding: the run looks right and the artefact is not the one
    // that was asked for.
    if link_index_omit.is_some() && link_index.is_none() {
        return usage(
            "--link-index-omit without --link-index does nothing: it names the modules whose \
             declaration groups are left out of the map, and no map is being written",
        );
    }
    // 段 D, and the same rule for the same reason: with no map there is nothing
    // to reuse and nothing to write a `.key` sidecar beside, so the token would
    // be accepted and dropped.
    if link_index_key.is_some() && link_index.is_none() {
        return usage(
            "--link-index-key without --link-index does nothing: it is the token that lets the \
             extractor leave an already-correct map alone, and no map is being written or read",
        );
    }
    // Flag, then environment, then nothing. **No default path** — both of these
    // are absolute paths on somebody's machine, and a default would be the
    // `defaultIrDir` mistake with a different name (M4-a). The two variable
    // names are the prototype's (`extract-once.sh:32`, `benchmarks/tools/env.sh`)
    // so a shell that already exports them keeps working.
    let bin = or_env(bin, "EXTRACT_BIN");
    let Some(bin) = bin else {
        return usage(
            "--extractor-bin <path> is required (or EXTRACT_BIN): the Lean extractor built by \
             `extractor/build.sh`, which is 171 MB and is therefore not committed. There is no \
             default — the binary is built against the target's toolchain, so a path baked in \
             here would be right on exactly one machine",
        );
    };
    let target = or_env(target, "TARGET_REPO");
    let Some(target) = target else {
        return usage(
            "--target <repo> is required (or TARGET_REPO): the Lean package being documented. \
             `lake env` runs inside it, which is how the extractor gets the oleans and the \
             search path without litedoc4 owning a toolchain",
        );
    };
    // `lake` **does** get a default, and it is not an exception to the rule
    // above: `lake` is a name looked up on PATH, not a path. elan installs a
    // shim under that name and the shim is what picks the toolchain the target
    // pins, so hard-coding `~/.elan/bin/lake` (the prototype's default) would be
    // the more specific and the more fragile of the two.
    let lake = or_env(lake, "LAKE").unwrap_or_else(|| PathBuf::from("lake"));

    let target = fs::canonicalize(&target).map_err(|source| Failure::Refused {
        code: 3,
        message: format!("--target {}: {source}", target.display()),
    })?;
    // The child's own path, for the same reason as the three below: `lake env
    // ./extract` inside the target would look for it in the target.
    let bin = absolute(&bin);
    // `extract-once.sh:48-50` refuses an `--ir-dir` under the measurement target,
    // with the target's path written out. Here the target is a parameter, so the
    // rule is stated against it instead of against one repository — the reason
    // it exists (CLAUDE.md: never write into the target) does not depend on
    // which target it is.
    let resolved = resolve(&ir_dir);
    if resolved.starts_with(&target) {
        return Err(Failure::Refused {
            code: 3,
            message: format!(
                "--ir-dir {} is inside --target {}: the package being documented is opened \
                 read-only and nothing is ever written into it",
                resolved.display(),
                target.display(),
            ),
        });
    }
    // The same rule for the map, which is 8.5 MB on the measurement target and
    // is written by the same child with the same working directory.
    if let Some(path) = &link_index {
        let resolved = resolve(path);
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
    }

    // The prototype's default (`extract-once.sh:52`), kept: `--timings` and its
    // events file travel together, and `incremental` deliberately passes only
    // the first (`pipeline.rs`'s `Extractor::run`).
    let events = events.unwrap_or_else(|| {
        let text = timings.to_string_lossy();
        PathBuf::from(format!(
            "{}-events.jsonl",
            text.strip_suffix(".json").unwrap_or(&text)
        ))
    });
    // **Every path handed to the child is made absolute first, and the guard
    // above is why** 【実測 2026-08-15, M4-c】. `lake env` runs inside `--target`,
    // so a relative path on that command line resolves against the package being
    // documented: `--ir-dir out` passes the guard (it resolves against *this*
    // process's directory, which is not under the target) and then the extractor
    // writes the IR tree into `<target>/out`. That is a write into the
    // measurement target arriving through the one command whose heading says it
    // never happens. A relative `--modules` fails loudly instead — the extractor
    // exits 1 with "no such file or directory" — which is how this was found.
    let ir_dir = absolute(&ir_dir);
    let modules = absolute(&modules);
    let events = absolute(&events);
    // Removed rather than truncated on open: the extractor appends, so a stale
    // file from an earlier round would be folded into this round's timings.
    let _ = fs::remove_file(&events);
    fs::create_dir_all(&ir_dir)
        .map_err(|source| Failure::Failed(format!("{}: {source}", ir_dir.display())))?;

    let link_index = link_index.as_deref().map(absolute);
    // 段 C. Made absolute for the reason in the block above — the child's working
    // directory is the target — but **not** guarded against being inside the
    // target, and the difference is the direction of the I/O: `--ir-dir` and
    // `--link-index` are written and the target is opened read-only, while this
    // one is read. A module list that lives inside the package being documented
    // is an odd place to keep it, not a write into it.
    let link_index_omit = link_index_omit.as_deref().map(absolute);
    let mut command = Command::new(&lake);
    command
        .current_dir(&target)
        .arg("env")
        .arg(&bin)
        .arg(&modules)
        .arg(&events)
        .args(FIXED_FLAGS)
        .arg("--jobs")
        .arg(jobs.to_string())
        .arg("--ir-dir")
        .arg(&ir_dir);
    if let Some(path) = &link_index {
        if let Some(parent) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
            fs::create_dir_all(parent)
                .map_err(|source| Failure::Failed(format!("{}: {source}", parent.display())))?;
        }
        command.arg("--link-index").arg(path);
        if let Some(omit) = &link_index_omit {
            command.arg("--link-index-omit").arg(omit);
        }
        if let Some(key) = &link_index_key {
            command.arg("--link-index-key").arg(key);
        }
    }
    command
        // `> /dev/null`, as the prototype does. The extractor's stdout is a
        // human-readable phase report; the machine-readable copy of the same
        // numbers is the events file, which is what the timings are folded from.
        // stderr is inherited, so a Lean error still reaches the caller.
        .stdout(Stdio::null());
    let status = command.status().map_err(|source| Failure::Refused {
        code: EXIT_EXTRACTOR,
        message: format!("{} env {}: {source}", lake.display(), bin.display()),
    })?;
    if !status.success() {
        return Err(Failure::Refused {
            code: EXIT_EXTRACTOR,
            message: format!(
                "the extractor exited {} for {}; the IR tree at {} is incomplete",
                status
                    .code()
                    .map_or_else(|| "on a signal".to_owned(), |code| code.to_string()),
                modules.display(),
                ir_dir.display(),
            ),
        });
    }

    let counted = fold_timings(&events, &modules, jobs, &timings)?;
    println!(
        "extract {counted} module(s) -> {} (timings {})",
        ir_dir.display(),
        timings.display(),
    );
    Ok(())
}

/// A flag's value, or the environment variable's, or nothing.
///
/// An empty variable counts as unset: `TARGET_REPO=` in a wrapper script is how
/// a shell spells "I did not set this", and taking it literally would make the
/// target the filesystem root.
pub(crate) fn or_env(flag: Option<PathBuf>, name: &str) -> Option<PathBuf> {
    flag.or_else(|| {
        std::env::var_os(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    })
}

/// `path`, made absolute and with every existing component's symlinks resolved.
///
/// Not [`fs::canonicalize`], which needs the whole path to exist: the guard runs
/// **before** the IR directory is created, because creating it is already a write
/// and the one thing the guard exists to prevent is a write inside the target.
/// So the deepest existing ancestor is canonicalised and the rest is appended —
/// which is what makes `/tmp/x` and `/private/tmp/x` the same path here.
/// `path`, made absolute **without touching what it spells**.
///
/// The one for a command line, where [`resolve`] is the one for a comparison:
/// resolving symlinks would hand the child a path the caller did not write
/// (`/tmp/x` becomes `/private/tmp/x` on this platform), and the paths on this
/// command line are recorded in two projects' measurement logs. What has to
/// change is only the *relative* case, because the child's working directory is
/// the target and a relative path there means somewhere inside the package being
/// documented.
pub(crate) fn absolute(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    std::env::current_dir().map_or_else(|_| path.to_path_buf(), |cwd| cwd.join(path))
}

pub(crate) fn resolve(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().map_or_else(|_| path.to_path_buf(), |cwd| cwd.join(path))
    };
    let mut prefix = absolute.as_path();
    let mut tail: Vec<&std::ffi::OsStr> = Vec::new();
    loop {
        if let Ok(real) = fs::canonicalize(prefix) {
            let mut out = real;
            for part in tail.iter().rev() {
                out.push(part);
            }
            return out;
        }
        match (prefix.file_name(), prefix.parent()) {
            (Some(name), Some(parent)) => {
                tail.push(name);
                prefix = parent;
            }
            _ => return absolute,
        }
    }
}

/// The extractor's events JSONL, folded into the one object the prototype's
/// inline Python wrote (`extract-once.sh:67-87`).
///
/// ```text
/// {"phase":"stage4b.importModules","pid":83359,"us":2498376,"directImports":432}
///   -> {"importModules": 2.498376, "importModules:directImports": 432}
/// ```
///
/// Three rules, all transcribed rather than improved:
///
/// * the `stage4b.` prefix is dropped — it names the stage the counter was
///   introduced in, and the consumer (`benchmarks/tools/analyze.ts`) has never
///   seen it;
/// * `us` becomes seconds under the phase's own name, so every duration in this
///   project's records ends up in the same unit;
/// * every other key of the event except `pid` becomes `<phase>:<key>` **with
///   its JSON value carried through untouched**. The extractor emits a mixture
///   of numbers, booleans and strings there; re-typing any of them would make
///   two records of the same run disagree over what is, by construction, the
///   same measurement.
///
/// Note that a `<phase>:<name>Us` key is a *sub-phase duration*, not a count —
/// `analyze:ppUs`, `writeIR:writeUs`. The M4-b gate compares this record against
/// the prototype's with every duration dropped, and those are durations.
///
/// Returns the module count, which is also written as `targetModules`.
pub(crate) fn fold_timings(
    events: &Path,
    modules: &Path,
    jobs: usize,
    out: &Path,
) -> Result<usize, Failure> {
    let text = fs::read_to_string(events)
        .map_err(|source| Failure::Failed(format!("{}: {source}", events.display())))?;
    // `preserve_order` is on for the workspace, so this comes out in the order
    // the extractor emitted the phases — which is the order the prototype's
    // `dict` kept too.
    let mut record = serde_json::Map::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let event: serde_json::Value = serde_json::from_str(line)
            .map_err(|source| Failure::Failed(format!("{}: {source}", events.display())))?;
        let Some(object) = event.as_object() else {
            return Err(Failure::Failed(format!(
                "{}: an event that is not a JSON object",
                events.display()
            )));
        };
        let phase = object
            .get("phase")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .replace("stage4b.", "");
        let micros = object.get("us").and_then(serde_json::Value::as_f64);
        record.insert(
            phase.clone(),
            serde_json::json!(micros.unwrap_or(0.0) / 1e6),
        );
        for (key, value) in object {
            if key != "phase" && key != "pid" && key != "us" {
                record.insert(format!("{phase}:{key}"), value.clone());
            }
        }
    }

    let listed = fs::read_to_string(modules)
        .map_err(|source| Failure::Failed(format!("{}: {source}", modules.display())))?;
    // The prototype's own predicate (`extract-once.sh:81-84`): a line that is
    // blank once trimmed, or that *starts* with `#` untrimmed. Kept exactly,
    // including the asymmetry — this number is compared across the two.
    let counted = listed
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .count();
    record.insert("targetModules".to_owned(), serde_json::json!(counted));
    record.insert("jobsRequested".to_owned(), serde_json::json!(jobs));

    // No trailing newline, as `json.dump` writes none. The separators differ
    // (`{"a":1}` here, `{"a": 1}` there) and nothing reads these files as bytes:
    // both sides are parsed — by `analyze.ts`, by `pipeline::write_timings` and
    // by the M4-b gate.
    let encoded = serde_json::to_string(&record)
        .map_err(|source| Failure::Failed(format!("{}: {source}", out.display())))?;
    if let Some(parent) = out.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .map_err(|source| Failure::Failed(format!("{}: {source}", parent.display())))?;
    }
    fs::write(out, encoded)
        .map_err(|source| Failure::Failed(format!("{}: {source}", out.display())))?;
    Ok(counted)
}
