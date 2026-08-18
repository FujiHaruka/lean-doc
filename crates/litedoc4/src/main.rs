//! The `litedoc4` command line tool.
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
//! **The product's command is [`build`]** (M4-d): a package in, a site out, in
//! one command — it reads the lakefile for the libraries, globs the sources for
//! the modules, derives `--source-url` from git, chooses between full generation
//! and the incremental pipeline, and writes the ledger back. Everything else
//! here is a **stage of it**, kept on the surface because each is separately
//! comparable against the frozen prototype and because the gates are stated
//! against them: `site` is full generation over an IR tree that already exists
//! (M3-d1), `incremental` is the six-stage round (M3-d2), `modules` is the glob
//! (M3-d2), `extract` is one extractor process (M4-b). A caller that wants to
//! name every path itself still can; a caller that wants documentation runs
//! `build`.
//!
//! Three flags are deliberately more awkward than the prototype's:
//!
//! - **`--only` and `--only-from` are the same option in two spellings, and
//!   `--only-from` an empty file renders nothing.** `render.ts` could not say
//!   "no modules": zero `--only` flags meant every module, so the incremental
//!   pipeline had to guard the call in shell (plan §5). A regeneration set is
//!   usually empty, so that hole is on the common path.
//! - **One of `--link-index` and `--no-link-index` is required.** The
//!   dependency map is not optional in the product (plan 決定 4); leaving it
//!   out costs 150 of 432 pages their correct bytes, and it does so silently.
//! - **`--root` on a stage command is the package, not the output** (M7-c). It
//!   is what the dependency link map is resolved from, it is optional there
//!   because a stage command may be pointed at an IR tree with no package next
//!   to it, and leaving it out is a *different site* — every link into a
//!   dependency stays a relative link to a page nothing writes. So the run says
//!   which of the two it did, on its own line, rather than letting the answer be
//!   read off the pages. [`build`] has a package by construction and never asks.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::ExitCode;

use std::time::Instant;

use litedoc4_global::{GlobalOptions, GlobalSummary, build_global};
use litedoc4_incr::{
    Algorithm, BuildOptions, CheckOptions, ImpactOptions, MergeOptions, Mode, ORPHANS_IN_LOG,
    OwnershipOptions, PruneOptions, TouchOptions, WITNESSES_IN_LOG, build_ledger, check_ledger,
    impact as run_impact, merge as run_merge, ownership as run_ownership, prune as run_prune,
    read_module_list, touch_ledger, verify as run_verify,
};
use litedoc4_render::{ModuleSet, RenderOptions, RenderSummary, render_site};

mod build;
mod extract;
mod lakefile;
mod packages;
mod pipeline;
mod resident;

const USAGE: &str = "\
usage: litedoc4 build  --root <repo> --out <dir> [--link-index <file>]
                       [--lib <Name>]... [--source-url <url>] [--full]
                       (--extractor-bin <path> [--lake <path>] [--jobs <n>]
                        | --extractor <program> [--extractor-arg <arg>]...)
                       [--mode self|referrers|importers|all] [--max-rounds <n>]
                       [--timings <file>]
       litedoc4 incremental --ir <dir> --pages <dir> --ledger <file> --work <dir>
                       --modules <file> --source-url <url> --link-index <file>
                       --state <dir> [--make-link-index] [--root <repo>]
                       (--extractor <program> [--extractor-arg <arg>]...
                        | --serve --extractor-bin <path> --target <repo>
                          [--lake <path>] [--jobs <n>])
                       [--mode self|referrers|importers|all] [--max-rounds <n>]
                       [--timings <file>]
       litedoc4 modules --root <repo> [--lib <Name>]... [--out <file>]
       litedoc4 extract --modules <file> --ir-dir <dir> --timings <file>
                       [--extractor-bin <path>] [--target <repo>] [--lake <path>]
                       [--events <file>] [--jobs <n>]
                       [--link-index <file> [--link-index-omit <file>]
                        [--link-index-key <token>]]
       litedoc4 site   --ir <dir> --out <dir> --source-url <url>
                       (--link-index <file> | --no-link-index)
                       [--root <repo>] [--lake <path>]
                       [--state <dir>] [--timings <file>]
       litedoc4 render --ir <dir> --pages <dir> --source-url <url>
                       (--link-index <file> | --no-link-index)
                       [--root <repo>] [--lake <path>]
                       [--only <Module>]... [--only-from <file>]
       litedoc4 global --ir <dir> --out <dir> [--state <dir>]
                       [--before <map.json>] [--print-set <file>]
                       [--delta-json <file>] [--timings <file>]
       litedoc4 ledger build --modules <file> --target <repo> --out <ledger.json>
                       [--algorithm sha256|lake] [--concurrency <n>]
                       [--ir <dir>] [--source-url <url>] [--link-index <file>]
                       [--root <repo>] [--lake <path>] [--timings <file>]
       litedoc4 ledger check --ledger <ledger.json> [--modules <file>]
                       [--algorithm sha256|lake] [--concurrency <n>] [--ir <dir>]
                       [--source-url <url>] [--link-index <file>]
                       [--root <repo>] [--lake <path>] [--changed-out <file>]
                       [--removed-out <file>] [--render-all-out <file>]
                       [--timings <file>]
       litedoc4 ledger touch --ledger <ledger.json> --module <Module> [--out <file>]
       litedoc4 ownership --base <ir> [--inc <ir>] [--removed <file>]
                       [--exclude <file>] [--print-set <file>] [--json <file>]
       litedoc4 merge --base <ir> [--inc <ir>] [--out <ir>] [--remove <file>]
                       [--modules <file>] [--changed-out <file>] [--timings <file>]
       litedoc4 merge --verify <ir> --against <ir>
       litedoc4 impact --ir <dir> [--changed <Module>]... [--changed-file <file>]
                       [--mode self|referrers|importers|all] [--census <file>]
                       [--print-set <file>] [--json <file>]
       litedoc4 prune --pages <dir> [--remove <file>] [--ir <dir>] [--dry-run]
                       [--json <file>]

  --root         (`build`, `modules`) the Lean package: the sources are globbed
                 under it, its oleans are hashed, `lake env` runs inside it, and
                 for `build` its git HEAD is where --source-url comes from.
                 (`site`, `render`, `incremental`, `ledger`) optional, and
                 only for the dependency link map: with it, every link into a
                 dependency is that package's version-pinned GitHub blob URL,
                 read out of its lake-manifest.json plus `lean --githash`
                 (M7-c); without it those links stay relative to pages this
                 site does not write. It is **not** --target: that names the
                 tree whose oleans are hashed. A ledger and the run it licenses
                 have to agree about this flag, or the render key moves and
                 every page is re-rendered. `build` has one by construction, so
                 its sites always carry the links.
  --out          (`build`) the directory this command owns: <out>/site is the
                 site, <out>/{ir,state,work} the caches, <out>/ledger.json the
                 ledger. Required, with no default — <root>/.lake/build/doc is
                 doc-gen4's own output tree — and it may not be inside --root
  --full         (`build`) regenerate everything, ignoring what is under --out.
                 The escape hatch for an input no ledger key covers. The
                 dependency map used to be one; since M5-b its bytes are in
                 renderKey, so a map that moved re-renders on its own.
  --ir           an IR tree written by the extractor (schema 4)
  --ir-dir       (`extract`) where the extractor writes that tree. Required and
                 with no default: the extractor's own default was one session's
                 scratchpad path and is gone (M4-a)
  --pages        where the pages go; directories are created
  --source-url   https://host/owner/repo/blob/<40-hex-rev>. `incremental`
                 checks the 40 hex digits; `render` and `site` do not.
  --link-index   the dependency closure's name -> module map (.lidx). Its
                 SHA-256 is part of renderKey: a map that moved re-renders every
                 page (150 of the target's 432 change bytes, plan 決定 4).
                 For `build` it is optional — left out, the map is this
                 command's own <out>/link-index.lidx, written by the extractor
                 from the environment it imported anyway (M5-a). Given, it is an
                 input and is never written to. For `extract` it asks the
                 extractor to write one.
  --link-index-omit  (`extract`, with --link-index) the modules whose own
                 declaration groups are left out of that map, one name per
                 line — normally the package's own module list. The renderer
                 answers those names out of the IR before it reads the map, so
                 the site is byte-identical; what changes is that the map stops
                 moving when the package is edited, and with it renderKey.
                 Module names still appear in the map's `@` section.
                 `incremental --serve` passes its own --modules here.
  --link-index-key  (`extract`, with --link-index) an opaque token standing for
                 everything about that map the extractor cannot see: the oleans
                 behind the imported modules, and the omit list's bytes. With
                 it, a map whose sidecar <file>.key holds the same token and
                 whose `@` section still matches the environment is left
                 untouched — no scan, no write (1.20-1.81 s of a 6.2 s one-module
                 incremental build on the measurement target). Anything less than
                 a full match rewrites both. `build` and `incremental --serve`
                 compute their own; here it is passed through verbatim.
  --make-link-index  (`incremental --serve`) the resident extractor writes
                 --link-index instead of reading it
  --work         (`incremental`) the round's scratch directory. Everything in
                 it is a diagnostic: the pipeline writes it and reads none of
                 it back.
  --extractor    (`incremental`) the extraction program, called as
                 `<program> [<extractor-arg>...] --modules <list>
                 --ir-dir <dir> --timings <file>`. No default, and the seam is
                 what lets the pipeline be tested without Lean. Exclusive with
                 --serve.
  --extractor-arg  one argument for it, before those three; repeatable
  --serve        (`incremental`) one resident Lean environment for the whole
                 run instead of one process per round: imported at the first
                 round that extracts, released on the way out of the loop.
                 There is no --serve-dir — a server this run did not start is
                 one whose olean generation it cannot vouch for.
  --max-rounds   how many extract/ownership/merge rounds may run (default 5).
                 Reaching it with modules still stale is exit 5.
  --root         (`modules`) the repository the sources are globbed under
  --lib          (`build`, `modules`) a library root: <Name>.lean and <Name>/;
                 repeatable. Left out, the names come from <root>/lakefile.toml's
                 [[lean_lib]] blocks; a lakefile.lean is refused by name, because
                 reading it honestly means elaborating it with Lake
  --extractor-bin  (`extract`, `incremental --serve`) the Lean extractor built
                 by extractor/build.sh, or $EXTRACT_BIN. No default: it is built
                 against the target's toolchain, so a baked-in path would be
                 right on one machine
  --target       (`extract`, `incremental --serve`) the Lean package to run
                 inside, or $TARGET_REPO. It is opened read-only: an --ir-dir
                 under it is refused, and its oleans are the generation every
                 resident request is checked against
  --lake         (`extract`, `incremental --serve`) the lake executable, or
                 $LAKE (default: `lake`). Also (`build`, `site`, `render`,
                 `ledger`) where the `lean` that answers `--githash` is looked
                 for: its **sibling**, so `--lake ~/.elan/bin/lake` means
                 `~/.elan/bin/lean`. That revision is Lean core's, the one
                 dependency the manifest does not pin
  --events       (`extract`) the extractor's phase events JSONL. Defaults to
                 <timings without .json>-events.jsonl, which is what
                 `incremental` relies on: it passes only --timings
  --jobs         (`extract`, `incremental --serve`) extractor threads
                 (default 1). It is the resident server's start-up
                 configuration, so there is no per-request job count
  --only         render only this module; repeatable
  --only-from    render only the modules named in this file, one per line.
                 An empty file renders nothing.
  --out          the site root the six whole-package artifacts go under — for
                 `site` the module pages go under it too — or the ledger file
                 `ledger build` writes
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
                 For `merge` it is the order the merged index.json's modules
                 come out in — a from-scratch extraction's, which is the order
                 the extractor was handed the list in. A list that names other
                 modules than the merged tree holds is exit 3, not a guess.
  --target       the repository whose .lake/build/lib/lean holds the oleans.
                 **Not** where the dependency link map comes from — that is
                 --root, even for `ledger`, where on a real package the two name
                 the same directory but on a hashed tree with no package behind
                 it only one of them exists
  --ledger       a ledger.json written by `ledger build`. `incremental` reads
                 it and never rewrites it; `build` writes it back, after the
                 last step that could fail.
  --algorithm  sha256 hashes the olean bytes; lake reads the <file>.hash Lake
                 already wrote. Defaults to sha256, and for `check` to the
                 ledger's own.
  --concurrency  olean reads in flight (default 1). The ledger's bytes do not
                 depend on it.
  --changed-out  the modules to re-extract, one per line
  --removed-out  the modules that no longer have an olean, one per line
  --render-all-out  why every page has to be re-rendered, one reason per line.
                 Empty means the render set follows from the IR diff as usual.
  --module       the module `ledger touch` invalidates
  --base         the IR as it was before this round
  --inc          the partial extraction's IR tree. Absent is a real case: a pure
                 deletion re-extracts nothing.
  --removed      modules that no longer exist, one per line (`ownership`)
  --remove       the same list, spelled as the prototype spells it for `merge`
  --exclude      modules already scheduled for re-extraction, one per line.
                 They are fresh by definition and are never reported.
  --verify       compare two IR trees; --against names the second
  --changed      a module that changed; repeatable (`impact`)
  --changed-file the same list in a file, one name per line
  --mode         which modules the change reaches: self, referrers (direct),
                 importers (the sound transitive bound, the default), or all —
                 which is valid with an empty changed set and is what a moved
                 render key selects
  --census       a per-module TSV of |IMPORTERS| / |REFERRERS| / declarations
  --pages        (`prune`) the page tree; nothing outside it is ever deleted
  --dry-run      report what would be deleted and delete nothing
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Usage(message)) => {
            eprintln!("litedoc4: {message}\n\n{USAGE}");
            ExitCode::from(2)
        }
        Err(Failure::Failed(message)) => {
            eprintln!("litedoc4: {message}");
            ExitCode::FAILURE
        }
        Err(Failure::Answered(code)) => ExitCode::from(code),
        Err(Failure::Refused { code, message }) => {
            eprintln!("litedoc4: {message}");
            ExitCode::from(code)
        }
    }
}

/// `Debug` so that a `Result<_, Failure>` can be `expect`ed in a test; nothing
/// prints one to a user, which is what the three arms of `main` are for.
#[derive(Debug)]
enum Failure {
    /// The command line is wrong. Exit 2, as the prototype does.
    Usage(String),
    Failed(String),
    /// The command ran to completion and answered "no" — `merge --verify` with
    /// differences, **exit 1**. The answer is already on stdout, so nothing goes
    /// to stderr: a caller that prints this as an error message would be
    /// reporting a working comparison as a broken one.
    Answered(u8),
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

/// The map that says where each **dependency's** source lives, resolved **once
/// per run** and then handed to both the renderer and the render key (M7-c).
///
/// `root` is the package being documented. `None` is a real case rather than an
/// oversight: `render` and `site` are pointed at an *IR tree*, which need not sit
/// next to any checkout — the harnesses under `tools/` run them against trees
/// extracted months ago — and with no package there is no manifest, no
/// revisions, and therefore nothing to link a dependency at. The map comes out
/// empty and every dependency link stays the relative page link v0.1 shipped
/// before M7. The run says which of the two happened, because the difference is
/// invisible in the exit code and visible on every page.
///
/// **Problems do not stop the run** (`docs/implementation-plan.md` §M7): a
/// package missing from disk, a manifest that will not parse, a `lake` that will
/// not run — each costs the roots it would have contributed and is printed. A
/// partial map renders a partial improvement; refusing would trade a site with
/// some dead links for no site at all.
fn resolve_external_links(
    root: Option<&std::path::Path>,
    lake: Option<&std::path::Path>,
) -> litedoc4_render::ExternalLinks {
    let Some(root) = root else {
        println!(
            "external  no package named (--root), so links into a dependency stay relative to \
             pages this site does not write"
        );
        return litedoc4_render::ExternalLinks::default();
    };
    let lake = lake.map_or_else(
        || extract::or_env(None, "LAKE").unwrap_or_else(|| PathBuf::from("lake")),
        std::path::Path::to_path_buf,
    );
    let resolved = packages::external_links(root, &lake);
    println!(
        "external  {} root(s) from {}/{} package(s) + core",
        resolved.links.len(),
        resolved.resolved,
        resolved.declared,
    );
    // The roots in that count that carry no URL: they are in the map so that the
    // pages stop linking into them, which is the opposite of what the line above
    // reads like on its own (M7, 2026-08-17). Printed only when there are any,
    // because on the measurement target there are none.
    if resolved.unpinned_roots > 0 {
        println!(
            "external  note: {} of those root(s) have no version-pinned URL, so names in them \
             render without a link rather than linking at a page this site does not write",
            resolved.unpinned_roots,
        );
    }
    // Counted and printed, not folded into the line above: a collision means the
    // map holds one of two candidates, which is a different answer from "a
    // package contributed nothing".
    for line in resolved.collisions.iter().chain(&resolved.problems) {
        println!("external  note: {line}");
    }
    resolved.links
}

fn run(args: &[String]) -> Result<(), Failure> {
    match args.first().map(String::as_str) {
        Some("build") => build::build(&args[1..]),
        Some("incremental") => pipeline::incremental(&args[1..]),
        Some("modules") => pipeline::modules(&args[1..]),
        Some("extract") => extract::extract(&args[1..]),
        Some("site") => site(&args[1..]),
        Some("render") => render(&args[1..]),
        Some("global") => global(&args[1..]),
        Some("ledger") => ledger(&args[1..]),
        Some("ownership") => ownership(&args[1..]),
        Some("merge") => merge(&args[1..]),
        Some("impact") => impact(&args[1..]),
        Some("prune") => prune(&args[1..]),
        Some("--help" | "-h") | None => {
            println!("{USAGE}");
            Ok(())
        }
        Some("--version") => {
            println!("litedoc4 {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some(other) => usage(format!("unknown subcommand `{other}`")),
    }
}

/// What leaving the dependency map out costs.
///
/// **One string, three call sites** (`render`, `site`, `incremental`), so that
/// they cannot drift apart on the one question this project has answered wrongly
/// twice: the prototype's `render()` (`stage7h/run.sh:78-80`) passed no
/// dependency map, and without it **150 of the target package's 432 pages change
/// bytes** 【実測, plan 決定 4】. It fails silently — a docstring name that did
/// not become a link looks exactly like a name that was never linkable — so the
/// guard is in the shape of the flags rather than in a default.
const LINK_INDEX_COST: &str = "without the dependency map 150 of the target package's 432 pages \
     change bytes (plan 決定 4)";

/// The refusal `render` and `site` print. Both offer `--no-link-index`, which is
/// how a caller says "no map, on purpose"; `incremental` does not (see
/// [`pipeline`]).
fn link_index_required() -> String {
    format!("pass --link-index <file>, or --no-link-index to say so on purpose: {LINK_INDEX_COST}")
}

/// Full generation: the module pages **and** the six whole-package artifacts,
/// into one tree, from one IR tree, in one command.
///
/// **The prototype has no script for this.** `stage7h/run.sh:78-80`'s `render()`
/// is three lines of shell — `render.ts` and then `global.ts build` over the
/// same IR and the same output directory — and every reference site this project
/// owns was made by it. The order is kept because the comparison is stated
/// against it, not because the stages talk: the cache `global` reads is keyed on
/// the IR, and the two write disjoint files 【実測, plan §7, M2】.
///
/// **The incremental round runs the same two stages the other way round**
/// (`incremental.sh` steps 6 and 7), and that is not an inconsistency to tidy
/// away: there, `global`'s map delta is half of the render set, so it has to
/// precede the renderer (plan §6, constraint 2). Here there is no delta and no
/// set, so nothing constrains the order — which is exactly why M3-d2 must not
/// read this function as saying render comes first.
///
/// Five of the two subcommands' flags are deliberately **not** accepted:
///
/// - **`--only` / `--only-from`.** Full generation is every module; that is what
///   the word means. A subset is `render`'s job, and the page set here is
///   [`ModuleSet::All`] with no way to say otherwise.
/// - **`--before` / `--print-set` / `--delta-json`.** The map delta answers
///   "which pages can have gone stale", which only an incremental round (M3-d2)
///   asks. A full run re-renders all of them, so a delta here would be a
///   diagnostic nobody reads — and one that quietly suggests the run was
///   partial.
fn site(args: &[String]) -> Result<(), Failure> {
    let mut ir: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut source_url: Option<String> = None;
    let mut link_index: Option<PathBuf> = None;
    let mut no_link_index = false;
    let mut root: Option<PathBuf> = None;
    let mut lake: Option<PathBuf> = None;
    let mut state: Option<PathBuf> = None;
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
            "--source-url" => source_url = Some(value("--source-url")?),
            "--link-index" => link_index = Some(value("--link-index")?.into()),
            "--no-link-index" => no_link_index = true,
            "--root" => root = Some(value("--root")?.into()),
            "--lake" => lake = Some(value("--lake")?.into()),
            "--state" => state = Some(value("--state")?.into()),
            "--timings" => timings = Some(value("--timings")?.into()),
            // Refused by name rather than as "unknown argument": each of these
            // is a real flag of the subcommand `site` calls, so the answer a
            // caller needs is *why it is not here*, not that it was misspelled.
            "--only" | "--only-from" => {
                return usage(format!(
                    "{arg} is not a `site` flag: full generation renders every module, which \
                     is what makes it full. Use `litedoc4 render {arg} ...` for a subset",
                ));
            }
            "--before" | "--print-set" | "--delta-json" => {
                return usage(format!(
                    "{arg} is not a `site` flag: the map delta names the pages an incremental \
                     round has to re-render, and this command re-renders all of them. Use \
                     `litedoc4 global {arg} ...`",
                ));
            }
            "--pages" => {
                return usage(
                    "`site` writes the pages and the six whole-package artifacts into one tree: \
                     name it with --out",
                );
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
    let Some(out) = out else {
        return usage("--out is required");
    };
    let Some(source_url) = source_url.filter(|url| !url.is_empty()) else {
        return usage(
            "--source-url is required: doc-gen4 reads it from lake plus git, and it is not in the IR",
        );
    };
    if link_index.is_some() == no_link_index {
        return usage(link_index_required());
    }

    let external = resolve_external_links(root.as_deref(), lake.as_deref());
    let site = generate_site(
        &ir,
        &out,
        &source_url,
        &external,
        link_index.as_deref(),
        state.as_deref(),
    )?;
    let (rendered, derived) = (&site.rendered, &site.derived);

    if let Some(path) = timings {
        // `renderSeconds` / `globalSeconds` / `totalSeconds` are
        // `incremental.sh:416-419`'s names for the same two phases, so a full
        // run and an incremental one subtract.
        let record = serde_json::json!({
            "command": "site",
            "pagesWritten": rendered.pages_written,
            "modulesInIr": rendered.modules_in_ir,
            "pageBytes": rendered.bytes_written,
            "cacheHits": derived.cache_hits,
            "cacheMisses": derived.cache_misses,
            "renderSeconds": site.render_seconds,
            "globalSeconds": site.global_seconds,
            "totalSeconds": site.render_seconds + site.global_seconds,
        });
        let line = serde_json::to_string(&record).expect("counts and durations serialise") + "\n";
        if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)
                .map_err(|e| Failure::Failed(format!("{}: {e}", dir.display())))?;
        }
        std::fs::write(&path, line)
            .map_err(|e| Failure::Failed(format!("{}: {e}", path.display())))?;
    }
    Ok(())
}

/// What full generation produced: both stages' counts and both stages' clocks.
struct Site {
    rendered: RenderSummary,
    derived: GlobalSummary,
    render_seconds: f64,
    global_seconds: f64,
}

/// Full generation, as a function.
///
/// **`site` and [`build`] call this, and that is the point** 【判断】: the M4-d
/// gate is "the tree `build` writes is byte-identical to the tree `litedoc4
/// site` writes", and a shared function turns that from a thing to measure into
/// a thing to state. It is still measured — a shared function can still be
/// called with different arguments — but the failure mode it removes is the one
/// where the two commands drift a flag apart and the comparison quietly becomes
/// a comparison of two different questions.
///
/// The order (render, then the whole-package derivation) is the prototype's
/// three lines of shell, and it is free here: the cache `global` reads is keyed
/// on the IR, and the two stages write disjoint files 【実測, plan §7, M2】. The
/// incremental round runs them the other way round because there the map delta
/// is half of the render set (plan §6, constraint 2).
fn generate_site(
    ir: &std::path::Path,
    out: &std::path::Path,
    source_url: &str,
    external_links: &litedoc4_render::ExternalLinks,
    link_index: Option<&std::path::Path>,
    state: Option<&std::path::Path>,
) -> Result<Site, Failure> {
    let started = Instant::now();
    let rendered = render_site(&RenderOptions {
        ir,
        pages: out,
        source_url,
        external_links,
        link_index,
        // Not a parameter. See `site`'s own documentation.
        only: &ModuleSet::All,
    })
    .map_err(|e| Failure::Failed(e.to_string()))?;
    let render_done = started.elapsed();

    let mut options = GlobalOptions::new(ir, out);
    options.state = state;
    let derived = build_global(&options).map_err(|e| Failure::Failed(e.to_string()))?;
    let total = started.elapsed();

    // Both stages' counts, each labelled with the stage that produced it. One
    // merged line would lose which half of the tree a number is about, and the
    // two stages count different things under the same word ("modules").
    print_render_summary("render  ", &rendered);
    print_global_summary("global  ", &derived);
    Ok(Site {
        rendered,
        derived,
        render_seconds: render_done.as_secs_f64(),
        global_seconds: total.saturating_sub(render_done).as_secs_f64(),
    })
}

/// The renderer's counts. Every number is a denominator something else is quoted
/// against, so a run that says "done" and nothing else is not comparable to the
/// prototype's numbers at all.
///
/// `lead` is what each line starts with: empty for `render`, which has one
/// stage, and the stage's name for `site`, which has two.
fn print_render_summary(lead: &str, summary: &RenderSummary) {
    println!(
        "{lead}modules {}/{}  declarations {}/{} ({} suppressed)  module docs {}  bytes {}",
        summary.pages_written,
        summary.modules_in_ir,
        summary.declarations_rendered,
        summary.declarations_in_ir,
        summary.declarations_suppressed,
        summary.module_docs_rendered,
        summary.bytes_written,
    );
    println!(
        "{lead}known {}  link index {}  known modules {}",
        summary.known_entries, summary.link_index_entries, summary.known_modules,
    );
}

/// The whole-package derivation's counts, including the delta when there is one.
fn print_global_summary(lead: &str, summary: &GlobalSummary) {
    println!(
        "{lead}modules {}  declarations {} + {} dependency names  instance classes {}  \
         instance types {}  tactic docs {}",
        summary.modules,
        summary.declarations,
        summary.dependency_names,
        summary.instance_classes,
        summary.instance_types,
        summary.tactic_docs,
    );
    println!(
        "{lead}name map {} B  module index {} B  search index {} B",
        summary.name_map_bytes, summary.modules_json_bytes, summary.search_index_bytes,
    );
    // The hit/miss counts are what the cache's oracle reads, and it reads them
    // twice: here and out of `--timings`. A cache that is silent about how often
    // it hit is one nobody notices has stopped hitting.
    println!(
        "{lead}cache {} hit / {} miss  state {} B",
        summary.cache_hits, summary.cache_misses, summary.state_bytes,
    );
    if let Some(delta) = &summary.delta {
        println!(
            "{lead}delta: {} name(s) moved in or out of the map ({} -> {}) -> {} page(s) to \
             re-render",
            delta.changed.len(),
            delta.before_names,
            delta.after_names,
            delta.affected.len(),
        );
        for witness in delta.witnesses.iter().take(10) {
            println!("{lead}  {}  (mentions `{}`)", witness.module, witness.name);
        }
    }
}

/// The whole-package artifacts, the `contentHash` cache and the map delta.
///
/// No `--only`: the derivation is over the whole package by construction, and
/// the cache makes it cheap rather than partial. No `--source-url` either —
/// none of the seven carries a source link (which is why `index.html` has no
/// "repository" anchor; see `litedoc4_global::entry`).
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

    print_global_summary("", &summary);
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
    let mut link_index: Option<PathBuf> = None;
    let mut package: Option<PathBuf> = None;
    let mut lake: Option<PathBuf> = None;
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
            // M5-b: the dependency map joins the render key, so `ledger build`
            // and `ledger check` have to be able to name it. Absent, and a path
            // that does not exist, both leave the key out.
            "--link-index" => link_index = Some(value("--link-index")?.into()),
            // M7-c. **Not `--target`**, even though on a real package the two
            // are the same directory: `--target` is the tree whose oleans are
            // hashed, and this is the package whose manifest and toolchain pin
            // the dependencies. Keeping them apart is what lets `ledger build`
            // over a hashed tree with no package behind it — every test in this
            // repository — go on producing the key it produced before M7.
            "--root" => package = Some(value("--root")?.into()),
            "--lake" => lake = Some(value("--lake")?.into()),
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
            let external = resolve_external_links(package.as_deref(), lake.as_deref());
            let summary = build_ledger(&BuildOptions {
                modules: &names,
                target: &target,
                out: &out,
                ir: ir.as_deref(),
                source_url: &source_url,
                link_index: link_index.as_deref(),
                external_links: Some(&external.digest()),
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
            let external = resolve_external_links(package.as_deref(), lake.as_deref());
            let summary = check_ledger(&CheckOptions {
                ledger: &path,
                algorithm: algorithm.as_ref(),
                modules: names.as_deref(),
                ir: ir.as_deref(),
                source_url: &source_url,
                link_index: link_index.as_deref(),
                external_links: Some(&external.digest()),
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

/// The `ownership` stage (L3-1): which modules point at a name that has moved.
///
/// Runs **before** `merge` in a round, and the reason is not a preference: merge
/// overwrites the base IR's idea of who owns each name (plan §6, constraint 1).
/// The pipeline that sequences them — and that bounds the rounds with
/// `--max-rounds`, leaving **exit 5** when the bound is hit with modules still
/// stale — is M3-d's; `incremental.sh:264-294` is what has to move.
fn ownership(args: &[String]) -> Result<(), Failure> {
    let mut base: Option<PathBuf> = None;
    let mut inc: Option<PathBuf> = None;
    let mut removed: Option<PathBuf> = None;
    let mut exclude: Option<PathBuf> = None;
    let mut print_set: Option<PathBuf> = None;
    let mut json: Option<PathBuf> = None;

    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        let mut value = |flag: &str| -> Result<String, Failure> {
            match rest.next() {
                Some(value) => Ok(value.clone()),
                None => usage(format!("{flag} needs a value")),
            }
        };
        match arg.as_str() {
            "--base" => base = Some(value("--base")?.into()),
            "--inc" => inc = Some(value("--inc")?.into()),
            "--removed" => removed = Some(value("--removed")?.into()),
            "--exclude" => exclude = Some(value("--exclude")?.into()),
            "--print-set" => print_set = Some(value("--print-set")?.into()),
            "--json" => json = Some(value("--json")?.into()),
            "--help" | "-h" => {
                println!("{USAGE}");
                return Ok(());
            }
            other => return usage(format!("unknown argument `{other}`")),
        }
    }

    // The prototype's own refusal: without a tree to diff against and without a
    // deletion list there is no question to answer.
    let Some(base) = base.filter(|_| inc.is_some() || removed.is_some()) else {
        return usage(
            "ownership needs --base <ir> and at least one of --inc <ir> / --removed <file>",
        );
    };
    let summary = run_ownership(&OwnershipOptions {
        base: &base,
        inc: inc.as_deref(),
        removed: removed.as_deref(),
        exclude: exclude.as_deref(),
        print_set: print_set.as_deref(),
        json: json.as_deref(),
    })
    .map_err(refused)?;

    println!(
        "ownership: {} name(s) lost, {} gained across {} re-extracted module(s) -> {} module(s) \
         need re-extraction — {:.4} s",
        summary.lost_names,
        summary.gained_names,
        summary.inc_modules,
        summary.stale_modules.len(),
        summary.total_seconds,
    );
    for witness in summary.witnesses.iter().take(WITNESSES_IN_LOG) {
        println!(
            "  {:<15} {}  (ref {} :: {})",
            witness.rule, witness.module, witness.reference[0], witness.reference[1],
        );
    }
    Ok(())
}

/// The `merge` stage: fold a partial extraction back into the package IR, and
/// the `--verify` that compares two trees.
///
/// **`--modules` is the prototype's unimplemented flag, implemented** (M3-d2b).
/// `merge-ir.ts` offers it in its usage and never reads it (`:29, :40`), so M3-b
/// did not reproduce it; it is here now because the merged `index.json`'s module
/// order has to be a from-scratch extraction's, and that is the order of the list
/// the extractor is handed. Left out, the order is the base index's with new
/// modules appended — the pre-M3-d2b behaviour, kept for callers that have no
/// list.
fn merge(args: &[String]) -> Result<(), Failure> {
    let mut base: Option<PathBuf> = None;
    let mut inc: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut modules: Option<PathBuf> = None;
    let mut remove: Option<PathBuf> = None;
    let mut changed_out: Option<PathBuf> = None;
    let mut timings: Option<PathBuf> = None;
    let mut verify_tree: Option<PathBuf> = None;
    let mut against: Option<PathBuf> = None;

    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        let mut value = |flag: &str| -> Result<String, Failure> {
            match rest.next() {
                Some(value) => Ok(value.clone()),
                None => usage(format!("{flag} needs a value")),
            }
        };
        match arg.as_str() {
            "--base" => base = Some(value("--base")?.into()),
            "--inc" => inc = Some(value("--inc")?.into()),
            "--out" => out = Some(value("--out")?.into()),
            "--modules" => modules = Some(value("--modules")?.into()),
            "--remove" => remove = Some(value("--remove")?.into()),
            "--changed-out" => changed_out = Some(value("--changed-out")?.into()),
            "--timings" => timings = Some(value("--timings")?.into()),
            "--verify" => verify_tree = Some(value("--verify")?.into()),
            "--against" => against = Some(value("--against")?.into()),
            "--help" | "-h" => {
                println!("{USAGE}");
                return Ok(());
            }
            other => return usage(format!("unknown argument `{other}`")),
        }
    }

    if let Some(tree) = verify_tree {
        let Some(against) = against else {
            return usage("merge --verify <ir> needs --against <ir>");
        };
        let report = run_verify(&tree, &against).map_err(refused)?;
        print!("{}", report.to_text());
        return if report.problems == 0 {
            Ok(())
        } else {
            Err(Failure::Answered(1))
        };
    }

    let Some(base) = base.filter(|_| inc.is_some() || remove.is_some()) else {
        return usage("merge needs --base <ir> and at least one of --inc <ir> / --remove <file>");
    };
    // `opt("--out", BASE + ".merged")`: the base tree is never written to unless
    // the caller asks for it by name.
    let out = out.unwrap_or_else(|| {
        let mut merged = base.clone().into_os_string();
        merged.push(".merged");
        PathBuf::from(merged)
    });
    let removed = match &remove {
        Some(path) => read_module_list(path).map_err(refused)?,
        None => Vec::new(),
    };
    let listed = match &modules {
        Some(path) => Some(read_module_list(path).map_err(refused)?),
        None => None,
    };
    let summary = run_merge(&MergeOptions {
        base: &base,
        inc: inc.as_deref(),
        out: &out,
        removed: &removed,
        modules: listed.as_deref(),
        changed_out: changed_out.as_deref(),
        timings: timings.as_deref(),
    })
    .map_err(refused)?;

    println!(
        "merged {} module(s){} into {}: modules {:.4} s, deps+index {:.4} s, total {:.4} s -> {}",
        summary.updated.len(),
        if summary.removed > 0 {
            format!(", removed {}", summary.removed)
        } else {
            String::new()
        },
        summary.modules,
        summary.copy_seconds,
        summary.deps_seconds,
        summary.total_seconds,
        out.display(),
    );
    println!(
        "IR content hash moved for {} of {} re-extracted module(s){}",
        summary.ir_changed.len(),
        summary.updated.len(),
        if summary.ir_changed.is_empty() {
            String::new()
        } else {
            format!(": {}", summary.ir_changed.join(", "))
        },
    );
    Ok(())
}

/// The `impact` stage (L3-2): a changed module set in, the modules to re-render
/// out.
///
/// **`global` runs before this** (plan §6, constraint 2) — but not into it. The
/// whole-package map's delta is the other half of the render set and it reaches
/// the renderer by being *unioned* with this stage's `--print-set`, which is the
/// pipeline's job (M3-d, `incremental.sh:354-360`). Two things M3-d inherits:
/// a delta with no changes is a **0-byte file, not a blank line**, and this
/// command writes **no `--print-set` at all** when the changed set is empty and
/// the mode is not `all` — a missing file is the empty set.
fn impact(args: &[String]) -> Result<(), Failure> {
    let mut ir: Option<PathBuf> = None;
    let mut changed: Vec<String> = Vec::new();
    let mut changed_file: Option<PathBuf> = None;
    let mut mode: Option<String> = None;
    let mut census: Option<PathBuf> = None;
    let mut print_set: Option<PathBuf> = None;
    let mut json: Option<PathBuf> = None;

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
            "--changed" => changed.push(value("--changed")?),
            "--changed-file" => changed_file = Some(value("--changed-file")?.into()),
            "--mode" => mode = Some(value("--mode")?),
            "--census" => census = Some(value("--census")?.into()),
            "--print-set" => print_set = Some(value("--print-set")?.into()),
            "--json" => json = Some(value("--json")?.into()),
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
    // The flags first, then the file's lines: the order reaches the summary's
    // `changed` array, and repeats are kept rather than folded.
    if let Some(path) = &changed_file {
        changed.extend(read_module_list(path).map_err(refused)?);
    }
    let mode = mode.as_deref().map_or_else(Mode::default, Mode::parse);
    let run = run_impact(&ImpactOptions {
        ir: &ir,
        changed: &changed,
        mode: &mode,
        census: census.as_deref(),
        print_set: print_set.as_deref(),
        json: json.as_deref(),
    })
    .map_err(refused)?;

    if let (Some(modules), Some(path)) = (run.census_modules, &census) {
        println!("census -> {} ({modules} modules)", path.display());
    }
    // The whole summary, as the prototype prints it: every count in it is a
    // denominator, and `selected` is the one the renderer is about to be given.
    if let Some(summary) = &run.summary {
        println!("{}", summary.to_json());
    }
    Ok(())
}

/// The `prune` stage: the deletion path's page third.
///
/// **The one subcommand that deletes.** Two guards are in the library
/// (containment, and paths built by concatenation rather than
/// [`std::path::Path::join`]);
/// the third is here, in the shape of the flag: `--dry-run` computes the whole
/// answer and writes nothing, so "what would this remove" is a question that can
/// be asked of a tree nobody is willing to lose.
fn prune(args: &[String]) -> Result<(), Failure> {
    let mut pages: Option<PathBuf> = None;
    let mut remove: Option<PathBuf> = None;
    let mut ir: Option<PathBuf> = None;
    let mut json: Option<PathBuf> = None;
    let mut dry_run = false;

    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        let mut value = |flag: &str| -> Result<String, Failure> {
            match rest.next() {
                Some(value) => Ok(value.clone()),
                None => usage(format!("{flag} needs a value")),
            }
        };
        match arg.as_str() {
            "--pages" => pages = Some(value("--pages")?.into()),
            "--remove" => remove = Some(value("--remove")?.into()),
            "--ir" => ir = Some(value("--ir")?.into()),
            "--json" => json = Some(value("--json")?.into()),
            "--dry-run" => dry_run = true,
            "--help" | "-h" => {
                println!("{USAGE}");
                return Ok(());
            }
            other => return usage(format!("unknown argument `{other}`")),
        }
    }

    // The prototype's own refusal: a page tree with neither a deletion list nor
    // an IR to call orphans against has nothing to do, and doing nothing quietly
    // is how a deleted module's page survives.
    let Some(pages) = pages.filter(|_| remove.is_some() || ir.is_some()) else {
        return usage("prune needs --pages <dir> and at least one of --remove <file> / --ir <dir>");
    };
    let summary = run_prune(&PruneOptions {
        pages: &pages,
        remove: remove.as_deref(),
        ir: ir.as_deref(),
        dry_run,
        json: json.as_deref(),
    })
    .map_err(refused)?;

    println!(
        "prune-pages{}: deleted {}/{} requested, {} orphan(s), {} empty dir(s) — {:.4} s",
        if summary.dry_run { " (dry run)" } else { "" },
        summary.deleted.len(),
        summary.requested,
        summary.orphans.len(),
        summary.emptied.len(),
        summary.total_seconds,
    );
    for orphan in summary.orphans.iter().take(ORPHANS_IN_LOG) {
        println!("  orphan  {orphan}");
    }
    Ok(())
}

/// Carries the library's exit code out to the process, so that "the ledger is
/// too old" (3) stays distinguishable from "the file would not read" (1).
#[expect(
    clippy::needless_pass_by_value,
    reason = "it is a `map_err` argument, which is handed the error by value"
)]
fn refused(error: litedoc4_incr::Error) -> Failure {
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
    let mut root: Option<PathBuf> = None;
    let mut lake: Option<PathBuf> = None;
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
            "--root" => root = Some(value("--root")?.into()),
            "--lake" => lake = Some(value("--lake")?.into()),
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
        return usage(link_index_required());
    }

    let only = match only {
        Some(names) => ModuleSet::These(names),
        None => ModuleSet::All,
    };
    let external = resolve_external_links(root.as_deref(), lake.as_deref());
    let summary = render_site(&RenderOptions {
        ir: &ir,
        pages: &pages,
        source_url: &source_url,
        external_links: &external,
        link_index: link_index.as_deref(),
        only: &only,
    })
    .map_err(|e| Failure::Failed(e.to_string()))?;

    print_render_summary("", &summary);
    Ok(())
}
