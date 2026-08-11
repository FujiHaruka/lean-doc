//! The `lean-doc` command line tool.
//!
//! **The subcommand surface is milestone M4's.** What is here is the
//! subcommands M1, M2 and M3 need to be judged: `render`, which turns an IR tree
//! into a tree of module pages, `global`, which turns the same tree into the
//! six whole-package artifacts, and `ledger`, which answers "which modules must
//! be re-extracted" without starting Lean. `tools/render-compare.sh`,
//! `tools/global-compare.sh` and `tools/ledger-compare.sh` run them against the
//! frozen prototype's output. Everything else — extraction, the rest of the
//! incremental pipeline, the resident server — arrives with its own milestone,
//! and guessing at its flags now would only have to be undone.
//!
//! Two flags are deliberately more awkward than the prototype's:
//!
//! - **`--only` and `--only-from` are the same option in two spellings, and
//!   `--only-from` an empty file renders nothing.** `render.ts` could not say
//!   "no modules": zero `--only` flags meant every module, so the incremental
//!   pipeline had to guard the call in shell (plan §5). A regeneration set is
//!   usually empty, so that hole is on the common path.
//! - **One of `--link-index` and `--no-link-index` is required.** The
//!   dependency map is not optional in the product (plan 決定 4); leaving it
//!   out costs 150 of 432 pages their correct bytes, and it does so silently.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lean_doc_global::{GlobalOptions, build_global};
use lean_doc_incr::{
    Algorithm, BuildOptions, CheckOptions, TouchOptions, build_ledger, check_ledger,
    read_module_list, touch_ledger,
};
use lean_doc_render::{ModuleSet, RenderOptions, render_site};

const USAGE: &str = "\
usage: lean-doc render --ir <dir> --pages <dir> --source-url <url>
                       (--link-index <file> | --no-link-index)
                       [--only <Module>]... [--only-from <file>]
       lean-doc global --ir <dir> --out <dir> [--state <dir>]
                       [--before <map.json>] [--print-set <file>]
                       [--delta-json <file>] [--timings <file>]
       lean-doc ledger build --modules <file> --target <repo> --out <ledger.json>
                       [--algorithm sha256|lake] [--concurrency <n>]
                       [--ir <dir>] [--source-url <url>] [--timings <file>]
       lean-doc ledger check --ledger <ledger.json> [--modules <file>]
                       [--algorithm sha256|lake] [--concurrency <n>] [--ir <dir>]
                       [--source-url <url>] [--changed-out <file>]
                       [--removed-out <file>] [--render-all-out <file>]
                       [--timings <file>]
       lean-doc ledger touch --ledger <ledger.json> --module <Module> [--out <file>]

  --ir           an IR tree written by the extractor (schema 4)
  --pages        where the pages go; directories are created
  --source-url   https://host/owner/repo/blob/<40-hex-rev>
  --link-index   the dependency closure's name -> module map (.lidx)
  --only         render only this module; repeatable
  --only-from    render only the modules named in this file, one per line.
                 An empty file renders nothing.
  --out          the site root the six whole-package artifacts go under, or the
                 ledger file `ledger build` writes
  --state        directory holding the contentHash cache (global-state.json).
                 Without it every module is read: the from-scratch build.
  --before       a previous declarations/name-map.json. Turns the delta on.
  --print-set    the modules to re-render, one per line. An affected set that
                 came out empty is an empty file, not a blank line.
  --delta-json   the delta's diagnostic summary
  --timings      one JSON line of counts and durations
  --modules      the module list, one name per line; # comments are skipped.
                 `ledger check` without it re-reads the ledger's own list and
                 cannot see a module that appeared or vanished since `build`.
  --target       the repository whose .lake/build/lib/lean holds the oleans
  --ledger       a ledger.json written by `ledger build`
  --algorithm    sha256 hashes the olean bytes; lake reads the <file>.hash Lake
                 already wrote. Defaults to sha256, and for `check` to the
                 ledger's own.
  --concurrency  olean reads in flight (default 1). The ledger's bytes do not
                 depend on it.
  --changed-out  the modules to re-extract, one per line
  --removed-out  the modules that no longer have an olean, one per line
  --render-all-out  why every page has to be re-rendered, one reason per line.
                 Empty means the render set follows from the IR diff as usual.
  --module       the module `ledger touch` invalidates
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Usage(message)) => {
            eprintln!("lean-doc: {message}\n\n{USAGE}");
            ExitCode::from(2)
        }
        Err(Failure::Failed(message)) => {
            eprintln!("lean-doc: {message}");
            ExitCode::FAILURE
        }
        Err(Failure::Refused { code, message }) => {
            eprintln!("lean-doc: {message}");
            ExitCode::from(code)
        }
    }
}

enum Failure {
    /// The command line is wrong. Exit 2, as the prototype does.
    Usage(String),
    Failed(String),
    /// The run stopped because the world and the files disagree — a ledger too
    /// old, a module with no olean. **Exit 3**, as the prototype does: a
    /// pipeline that treats "the ledger is stale" the same as "the disk is
    /// full" retries the wrong thing.
    Refused {
        code: u8,
        message: String,
    },
}

fn usage<T>(message: impl Into<String>) -> Result<T, Failure> {
    Err(Failure::Usage(message.into()))
}

fn run(args: &[String]) -> Result<(), Failure> {
    match args.first().map(String::as_str) {
        Some("render") => render(&args[1..]),
        Some("global") => global(&args[1..]),
        Some("ledger") => ledger(&args[1..]),
        Some("--help" | "-h") | None => {
            println!("{USAGE}");
            Ok(())
        }
        Some("--version") => {
            println!("lean-doc {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some(other) => usage(format!("unknown subcommand `{other}`")),
    }
}

/// The six whole-package artifacts, the `contentHash` cache and the map delta.
///
/// No `--only`: the derivation is over the whole package by construction, and
/// the cache makes it cheap rather than partial. No `--source-url` either —
/// none of the six carries a source link.
///
/// `--print-set` / `--delta-json` do nothing without `--before`, exactly as in
/// the prototype: the delta is off unless there is a map to compare against.
fn global(args: &[String]) -> Result<(), Failure> {
    let mut ir: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut state: Option<PathBuf> = None;
    let mut before: Option<PathBuf> = None;
    let mut print_set: Option<PathBuf> = None;
    let mut delta_json: Option<PathBuf> = None;
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
            "--out" => out = Some(value("--out")?.into()),
            "--state" => state = Some(value("--state")?.into()),
            "--before" => before = Some(value("--before")?.into()),
            "--print-set" => print_set = Some(value("--print-set")?.into()),
            "--delta-json" => delta_json = Some(value("--delta-json")?.into()),
            "--timings" => timings = Some(value("--timings")?.into()),
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
    let Some(out) = out else {
        return usage("--out is required");
    };
    let mut options = GlobalOptions::new(&ir, &out);
    options.state = state.as_deref();
    options.before = before.as_deref();
    options.print_set = print_set.as_deref();
    options.delta_json = delta_json.as_deref();
    options.timings = timings.as_deref();
    let summary = build_global(&options).map_err(|e| Failure::Failed(e.to_string()))?;

    println!(
        "modules {}  declarations {} + {} dependency names  instance classes {}  tactic docs {}",
        summary.modules,
        summary.declarations,
        summary.dependency_names,
        summary.instance_classes,
        summary.tactic_docs,
    );
    println!(
        "declaration data {} B  name map {} B",
        summary.bmp_bytes, summary.name_map_bytes,
    );
    // The hit/miss counts are what the cache's oracle reads, and it reads them
    // twice: here and out of `--timings`. A cache that is silent about how often
    // it hit is one nobody notices has stopped hitting.
    println!(
        "cache {} hit / {} miss  state {} B",
        summary.cache_hits, summary.cache_misses, summary.state_bytes,
    );
    if let Some(delta) = &summary.delta {
        println!(
            "delta: {} name(s) moved in or out of the map ({} -> {}) -> {} page(s) to re-render",
            delta.changed.len(),
            delta.before_names,
            delta.after_names,
            delta.affected.len(),
        );
        for witness in delta.witnesses.iter().take(10) {
            println!("  {}  (mentions `{}`)", witness.module, witness.name);
        }
    }
    Ok(())
}

/// The `detect` stage: the olean hash ledger (plan §6, milestone M3-a).
///
/// Three subcommands rather than three top-level ones, because they share the
/// ledger file and nothing else in the CLI does. `touch` is here for the same
/// reason it is in the library: the measurement target must not be modified, so
/// "module M changed" is injected into the ledger instead.
fn ledger(args: &[String]) -> Result<(), Failure> {
    let mut modules: Option<PathBuf> = None;
    let mut target: Option<String> = None;
    let mut out: Option<PathBuf> = None;
    let mut ledger: Option<PathBuf> = None;
    let mut ir: Option<PathBuf> = None;
    let mut source_url = String::new();
    let mut algorithm: Option<Algorithm> = None;
    let mut concurrency: usize = 1;
    let mut module: Option<String> = None;
    let mut changed_out: Option<PathBuf> = None;
    let mut removed_out: Option<PathBuf> = None;
    let mut render_all_out: Option<PathBuf> = None;
    let mut timings: Option<PathBuf> = None;

    let Some(command) = args.first().map(String::as_str) else {
        return usage("ledger needs a subcommand: build, check or touch");
    };
    let mut rest = args[1..].iter();
    while let Some(arg) = rest.next() {
        let mut value = |flag: &str| -> Result<String, Failure> {
            match rest.next() {
                Some(value) => Ok(value.clone()),
                None => usage(format!("{flag} needs a value")),
            }
        };
        match arg.as_str() {
            "--modules" => modules = Some(value("--modules")?.into()),
            "--target" => target = Some(value("--target")?),
            "--out" => out = Some(value("--out")?.into()),
            "--ledger" => ledger = Some(value("--ledger")?.into()),
            "--ir" => ir = Some(value("--ir")?.into()),
            "--source-url" => source_url = value("--source-url")?,
            "--algorithm" => algorithm = Some(Algorithm::new(value("--algorithm")?)),
            "--concurrency" => {
                let raw = value("--concurrency")?;
                concurrency = raw.parse().map_err(|_| {
                    Failure::Usage(format!("--concurrency wants a number, not {raw}"))
                })?;
            }
            "--module" => module = Some(value("--module")?),
            "--changed-out" => changed_out = Some(value("--changed-out")?.into()),
            "--removed-out" => removed_out = Some(value("--removed-out")?.into()),
            "--render-all-out" => render_all_out = Some(value("--render-all-out")?.into()),
            "--timings" => timings = Some(value("--timings")?.into()),
            "--help" | "-h" => {
                println!("{USAGE}");
                return Ok(());
            }
            other => return usage(format!("unknown argument `{other}`")),
        }
    }

    match command {
        "build" => {
            let (Some(modules), Some(target), Some(out)) = (modules, target, out) else {
                return usage(
                    "ledger build needs --modules <file>, --target <repo> and --out <ledger.json>",
                );
            };
            let names = read_module_list(&modules).map_err(refused)?;
            let algorithm = algorithm.unwrap_or_else(Algorithm::sha256);
            let summary = build_ledger(&BuildOptions {
                modules: &names,
                target: &target,
                out: &out,
                ir: ir.as_deref(),
                source_url: &source_url,
                algorithm: &algorithm,
                concurrency,
                timings: timings.as_deref(),
            })
            .map_err(refused)?;
            println!(
                "build {} modules, {} olean file(s), {} B hashed in {:.4} s -> {} ({} B)",
                summary.modules,
                summary.files,
                grouped(summary.hashed_bytes),
                summary.hash_seconds,
                out.display(),
                summary.ledger_bytes,
            );
        }
        "check" => {
            let Some(path) = ledger else {
                return usage("ledger check needs --ledger <ledger.json>");
            };
            let names = match modules {
                Some(list) => Some(read_module_list(&list).map_err(refused)?),
                None => None,
            };
            let summary = check_ledger(&CheckOptions {
                ledger: &path,
                algorithm: algorithm.as_ref(),
                modules: names.as_deref(),
                ir: ir.as_deref(),
                source_url: &source_url,
                concurrency,
                changed_out: changed_out.as_deref(),
                removed_out: removed_out.as_deref(),
                render_all_out: render_all_out.as_deref(),
                timings: timings.as_deref(),
            })
            .map_err(refused)?;
            // The counts first, then the reasons, then the names: a run that
            // re-extracts everything has to say which key did it.
            println!(
                "check {} modules ({}, concurrency {}): {} changed, {} added, {} removed",
                summary.modules,
                summary.algorithm.name(),
                concurrency,
                summary.changed.len(),
                summary.added.len(),
                summary.removed.len(),
            );
            if summary.extract_invalidated() {
                println!(
                    "  extract key changed ({}) -> all {} re-extracted",
                    summary.extract_key_changed.join(","),
                    summary.re_extract.len(),
                );
            }
            if summary.render_all() {
                println!(
                    "  render key changed ({}) -> re-render all, re-extract {}",
                    summary.render_key_changed.join(","),
                    summary.re_extract.len(),
                );
            }
            for module in &summary.changed {
                println!("  changed  {module}");
            }
            for module in &summary.added {
                println!("  added    {module}");
            }
            for module in &summary.removed {
                println!("  removed  {module}");
            }
        }
        "touch" => {
            let (Some(path), Some(module)) = (ledger, module) else {
                return usage("ledger touch needs --ledger <ledger.json> and --module <Module>");
            };
            let out = out.unwrap_or_else(|| path.clone());
            let bytes = touch_ledger(&TouchOptions {
                ledger: &path,
                module: &module,
                out: &out,
            })
            .map_err(refused)?;
            println!(
                "touched {module} in {} ({bytes} B; injected change, the olean is untouched)",
                out.display(),
            );
        }
        other => return usage(format!("unknown ledger subcommand `{other}`")),
    }
    Ok(())
}

/// Carries the library's exit code out to the process, so that "the ledger is
/// too old" (3) stays distinguishable from "the file would not read" (1).
fn refused(error: lean_doc_incr::Error) -> Failure {
    Failure::Refused {
        code: error.exit_code(),
        message: error.to_string(),
    }
}

/// `Number.prototype.toLocaleString("en-US")` for the one place the prototype
/// prints a byte count to a human.
fn grouped(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

fn render(args: &[String]) -> Result<(), Failure> {
    let mut ir: Option<PathBuf> = None;
    let mut pages: Option<PathBuf> = None;
    let mut source_url: Option<String> = None;
    let mut link_index: Option<PathBuf> = None;
    let mut no_link_index = false;
    // `None` until an `--only` of either spelling appears: the distinction
    // between "no subset asked for" and "a subset that came out empty" is the
    // whole point (plan §5).
    let mut only: Option<BTreeSet<String>> = None;

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
            "--source-url" => source_url = Some(value("--source-url")?),
            "--link-index" => link_index = Some(value("--link-index")?.into()),
            "--no-link-index" => no_link_index = true,
            "--only" => {
                only.get_or_insert_with(BTreeSet::new)
                    .insert(value("--only")?);
            }
            "--only-from" => {
                let path = PathBuf::from(value("--only-from")?);
                let text = std::fs::read_to_string(&path)
                    .map_err(|e| Failure::Failed(format!("{}: {e}", path.display())))?;
                let ModuleSet::These(names) = ModuleSet::from_lines(&text) else {
                    unreachable!("from_lines always names a set")
                };
                only.get_or_insert_with(BTreeSet::new).extend(names);
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
    // The prototype refuses too: the source URL is configuration that no IR
    // carries, and a page written without it links every declaration to `/`.
    let Some(source_url) = source_url.filter(|url| !url.is_empty()) else {
        return usage(
            "--source-url is required: doc-gen4 reads it from lake plus git, and it is not in the IR",
        );
    };
    if link_index.is_some() == no_link_index {
        return usage(
            "pass --link-index <file>, or --no-link-index to say so on purpose: without the \
             dependency map 150 of the target package's 432 pages change bytes (plan 決定 4)",
        );
    }

    let only = match only {
        Some(names) => ModuleSet::These(names),
        None => ModuleSet::All,
    };
    let summary = render_site(&RenderOptions {
        ir: &ir,
        pages: &pages,
        source_url: &source_url,
        link_index: link_index.as_deref().map(Path::new),
        only: &only,
    })
    .map_err(|e| Failure::Failed(e.to_string()))?;

    // Counts, not a checkmark: every number here is a denominator something
    // else gets quoted against.
    println!(
        "modules {}/{}  declarations {}/{} ({} suppressed)  module docs {}  bytes {}",
        summary.pages_written,
        summary.modules_in_ir,
        summary.declarations_rendered,
        summary.declarations_in_ir,
        summary.declarations_suppressed,
        summary.module_docs_rendered,
        summary.bytes_written,
    );
    println!(
        "known {}  link index {}  known modules {}",
        summary.known_entries, summary.link_index_entries, summary.known_modules,
    );
    Ok(())
}
