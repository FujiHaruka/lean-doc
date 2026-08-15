//! `lean-doc build` — a Lean package in, a documentation site out, in one
//! command.
//!
//! Milestone **M4-d**, and the gate of M4. Every stage this drives already
//! exists; what did not exist is **the thing that knows what to call them
//! with**. Before this command a caller had to know, and get right, all of:
//!
//! ```text
//!   lean-doc modules --root <repo> --lib <Name>       > modules.txt
//!   lean-doc extract --modules modules.txt --ir-dir <ir> --timings <t>
//!   lean-doc site    --ir <ir> --out <site> --source-url <url> --link-index …
//!   lean-doc ledger  build --modules modules.txt --target <repo> --ir <ir> …
//!   # …and on the second commit, a different five commands, with --state and
//!   # --ledger and --work threaded between them in the right order
//! ```
//!
//! — plus the two answers nothing in that list computes: **which library the
//! package declares** (M3-d2's debt 5, now [`crate::lakefile`]) and **what
//! `--source-url` is** (the prototype writes a 40-hex revision into a shell
//! variable by hand, `incremental.sh:106` / `run.sh:55`, which plan §6 lists as
//! one of the last hard-coded values). And it had to choose, correctly, between
//! the full path and the incremental one.
//!
//! `build` is those decisions, in this order:
//!
//! ```text
//!   1  --lib          the lakefile's [[lean_lib]] names, unless given
//!   2  the modules    the source glob (`lean-doc modules`), one list, reused
//!   3  --source-url   git HEAD + the origin remote, unless given
//!   4  the layout     what is under --out already, and whether it is complete
//!   5a first run      hash → extract everything → site → write the ledger
//!   5b later runs     hash → the incremental pipeline → write the ledger
//! ```
//!
//! # Where things go, and why there is no default for `--out` 【判断】
//!
//! `--out <dir>` is **required**, and the directory is the command's own:
//!
//! ```text
//!   <out>/site/                the site — 432 module pages + 6 whole-package
//!                              artifacts on the target. This is what ships.
//!   <out>/ir/                  the IR tree (16 MB), carried between runs
//!   <out>/state/               global-state.json, the contentHash cache
//!   <out>/ledger.json          which oleans the IR was built from
//!   <out>/work/                one run's diagnostics; read by nobody
//!   <out>/lean-doc-build.json  the marker below
//! ```
//!
//! **There is no default, and in particular the default is not
//! `<root>/.lake/build/doc`** — which is where doc-gen4 writes, and where this
//! project's target holds a 736 MB reference tree that took about nine hours to
//! produce. A default that overwrites another tool's output tree is a data-loss
//! bug with a friendly face; and this repository has already paid for one baked
//! path (`defaultIrDir`, M4-a) and for one relative path that resolved inside
//! the package being documented (M4-c). Naming the output is one word, and it is
//! the word that says where megabytes are about to land.
//!
//! **`--out` may not be inside `--root`.** `lean-doc extract` already refuses an
//! `--ir-dir` under its `--target`, because the package being documented is
//! opened read-only; since the IR lives under `--out`, an `--out` inside the
//! package would be refused one stage later and with a worse message. Stating it
//! here means the run stops before it has written anything at all. A caller who
//! wants the site *inside* the repository copies `<out>/site` there — one `cp
//! -R` of the thing that ships, without 16 MB of IR and a cache landing in
//! somebody's working tree.
//!
//! # The marker, and how a half-finished run is recognised
//!
//! `<out>/lean-doc-build.json` is written **twice** per run: `complete: false`
//! before anything else, `complete: true` after the ledger. It records the
//! layout version, the root it was built from and the libraries — and it is what
//! makes the choice in step 4 answerable rather than guessed:
//!
//! * `--out` absent or empty → **full generation**;
//! * marker present, `complete: true`, and the ledger, IR, state and site all
//!   there → **incremental**;
//! * marker present and `complete: false`, or a piece missing → **full
//!   generation**, and the site tree is removed first (see below);
//! * **`--out` not empty and no marker → exit 3.** This command deletes and
//!   overwrites inside `--out`, so it will only do that to a directory it can
//!   see it wrote. "It looked empty enough" is not a reason to remove somebody's
//!   files.
//! * marker present naming a **different `--root`** → exit 3. The ledger stores
//!   the target it hashed; carrying it over to another package would compare one
//!   package's oleans with another's hashes and re-extract everything, or worse,
//!   not.
//!
//! The site tree is removed before a full generation because **the renderer only
//! ever writes**: a module that vanished between two full runs would keep its
//! page for ever, and the page tree would carry a file no from-scratch run
//! produces. Deletion is confined to `<out>/site`, and only when the marker says
//! this command created it.
//!
//! # When the ledger is written — the one ordering that has a silent failure
//!
//! The ledger's claim is "**the IR was built from these oleans, and the pages
//! were rendered from that IR**". Two ways to get its timing wrong, and they are
//! not symmetric:
//!
//! * **Write it early** (before the pages) and a run that dies in the renderer
//!   leaves a ledger saying every module is up to date. The next run re-extracts
//!   nothing, re-renders nothing, and the site is permanently half-old **with no
//!   diagnostic anywhere**. This is the failure mode that matters.
//! * **Write it late but with hashes read late** and the same silence arrives by
//!   a different road: an olean rebuilt *while* this run was extracting gets
//!   recorded as the one its IR came from, and that module is never re-extracted
//!   again.
//!
//! So the rule is one sentence — **hash before extracting, write after
//! rendering** — and it is implemented as: the module hashes come from
//! [`lean_doc_incr::CheckSummary::fresh`] (or, on the first run, from a
//! `ledger build` that runs *before* the extractor), and the file is written by
//! [`write_ledger`] as the last thing a successful run does. Every failure
//! before that point — the extractor exiting non-zero (exit 4), the round loop
//! not converging (exit 5), a stage refusing (exit 3), the renderer failing
//! (exit 1) — leaves the previous ledger in place, and the next run redoes the
//! work. **Redoing work is the safe direction**; it is loud (the next run's
//! `changed` count says so) and it is finite.
//!
//! The two keys are the exception, and deliberately so: `extractKey` is
//! recomputed against the IR tree that now exists, not the one `detect` looked
//! at. They describe *the tree on disk*, and after a successful run that is the
//! merged tree. Writing back `detect`'s copy would leave a ledger claiming the
//! IR was written by whatever wrote the old one, and if the two differ every
//! later run re-extracts all 432 modules for ever.
//!
//! # What is deliberately *not* covered, and what to do about it 【実測】
//!
//! `renderKey` is `renderer` + `sourceUrl` (`ledger.rs`), so a run whose
//! **dependency map changed** and whose IR did not is not detected: the map
//! reaches 150 of the target's 432 pages' bytes 【実測, plan 決定 4】 and nothing
//! in the ledger names it. `--full` is the escape hatch, and the honest fix
//! belongs with M5, which is where the map stops being a file somebody passes
//! and starts being something the product derives (and therefore something with
//! an identity worth putting in the key).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use lean_doc_incr::{
    Algorithm, BuildOptions, Ledger, Mode, build_ledger, extract_key, link_index_digest, render_key,
};

use crate::extract::absolute;
use crate::pipeline::{Extractor, Incremental, check_source_url, module_names, write_lines};
use crate::resident::Resident;
use crate::{Failure, LINK_INDEX_COST, USAGE, refused, usage};

/// The layout under `--out`. Bumped when a directory written by an older
/// `build` can no longer be continued by this one.
const LAYOUT: u64 = 1;

/// The marker file. Not inside `<out>/site`, because the site's file count is a
/// denominator this project quotes (438 on the target) and a stray file in it
/// would change that number.
const MARKER: &str = "lean-doc-build.json";

/// `opt("--max-rounds", 5)`, the pipeline's own default.
const DEFAULT_MAX_ROUNDS: usize = 5;

/// Where every piece of one package's documentation state lives.
struct Layout {
    out: PathBuf,
    site: PathBuf,
    ir: PathBuf,
    state: PathBuf,
    work: PathBuf,
    ledger: PathBuf,
    marker: PathBuf,
    /// `<out>/link-index.lidx`, where the dependency map goes when this command
    /// derives it (M5-b). Overridden by `--link-index`, which names one
    /// somebody else made.
    link_index: PathBuf,
}

impl Layout {
    fn new(out: &Path) -> Self {
        Self {
            out: out.to_owned(),
            site: out.join("site"),
            ir: out.join("ir"),
            state: out.join("state"),
            work: out.join("work"),
            ledger: out.join("ledger.json"),
            marker: out.join(MARKER),
            link_index: out.join("link-index.lidx"),
        }
    }

    /// Whether every file a continuation reads is there.
    ///
    /// The name map is in the list because the incremental round's map delta
    /// compares against it (`pipeline.rs`), and a site tree without one runs
    /// with the delta off — which is a *correct* run that re-renders too little
    /// the first time the map moves.
    ///
    /// So is the dependency map (M5-b): an incremental run renders a *subset*,
    /// so a missing map does not fail — it produces pages whose links are gone,
    /// mixed into a tree of pages that still have theirs. A full generation
    /// extracts everything and therefore writes the map again, which is why a
    /// missing map is answered by taking that path rather than by refusing.
    fn carries_a_previous_run(&self, link_index: &Path) -> bool {
        self.ledger.is_file()
            && self.ir.join("index.json").is_file()
            && self.state.join("global-state.json").is_file()
            && link_index.is_file()
            && self
                .site
                .join("declarations")
                .join("name-map.json")
                .is_file()
    }
}

/// Which of the two paths this run takes, and why.
enum Plan {
    /// Nothing to continue from: extract everything, render everything.
    Full(&'static str),
    /// A previous run left a complete tree behind.
    Incremental,
}

// ------------------------------------------------------------------- the CLI

/// `lean-doc build`.
pub fn build(args: &[String]) -> Result<(), Failure> {
    let mut root: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut libs: Vec<String> = Vec::new();
    let mut link_index: Option<PathBuf> = None;
    let mut source_url: Option<String> = None;
    let mut extractor: Option<String> = None;
    let mut extractor_args: Vec<String> = Vec::new();
    let mut extractor_bin: Option<PathBuf> = None;
    let mut lake: Option<PathBuf> = None;
    let mut jobs: usize = 1;
    let mut mode: Option<String> = None;
    let mut max_rounds = DEFAULT_MAX_ROUNDS;
    let mut timings: Option<PathBuf> = None;
    let mut full = false;

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
            "--out" => out = Some(value("--out")?.into()),
            "--lib" => libs.push(value("--lib")?),
            "--link-index" => link_index = Some(value("--link-index")?.into()),
            "--source-url" => source_url = Some(value("--source-url")?),
            "--extractor" => extractor = Some(value("--extractor")?),
            "--extractor-arg" => extractor_args.push(value("--extractor-arg")?),
            "--extractor-bin" => extractor_bin = Some(value("--extractor-bin")?.into()),
            "--lake" => lake = Some(value("--lake")?.into()),
            "--jobs" => {
                let raw = value("--jobs")?;
                jobs = raw
                    .parse()
                    .map_err(|_| Failure::Usage(format!("--jobs wants a number, not {raw}")))?;
            }
            "--mode" => mode = Some(value("--mode")?),
            "--max-rounds" => {
                let raw = value("--max-rounds")?;
                max_rounds = raw.parse().map_err(|_| {
                    Failure::Usage(format!("--max-rounds wants a number, not {raw}"))
                })?;
            }
            "--timings" => timings = Some(value("--timings")?.into()),
            "--full" => full = true,
            // Refused by name rather than as "unknown argument": every one of
            // these is a real flag of a subcommand this one drives, so what the
            // caller needs to hear is which decision `build` has taken over.
            "--ir" | "--pages" | "--ledger" | "--work" | "--state" => {
                return usage(format!(
                    "{arg} is not a `build` flag: this command owns the layout under --out \
                     (<out>/ir, <out>/site, <out>/state, <out>/work, <out>/ledger.json) so that a \
                     second run can find what the first one left. Name the pieces yourself with \
                     `lean-doc incremental`",
                ));
            }
            "--modules" => {
                return usage(
                    "--modules is not a `build` flag: the list is the source glob over the \
                     libraries (`lean-doc modules`), and the same list has to reach detect, the \
                     extractor and merge or the merged index.json comes out in an order a \
                     from-scratch run would not have written. Choose the libraries with --lib",
                );
            }
            "--target" => {
                return usage(
                    "--target is not a `build` flag: the package being documented is --root, and \
                     it is the same repository the sources are globbed from, the oleans are hashed \
                     in and `lake env` runs inside",
                );
            }
            "--no-link-index" => {
                return usage(format!(
                    "--no-link-index is not a `build` flag: {LINK_INDEX_COST}, and a build command \
                     whose ordinary output is silently wrong on a third of its pages is not worth \
                     having. `lean-doc render --no-link-index` still says it on purpose",
                ));
            }
            "--serve" | "--serve-dir" | "--serve-from" => {
                return usage(format!(
                    "{arg} is not a `build` flag: with --extractor-bin this command *is* the \
                     resident path — one Lean environment for the whole run, started here and \
                     released after the last round (see crates/lean-doc/src/resident.rs). There is \
                     nothing to switch on, and a server this run did not start is one whose olean \
                     generation it cannot vouch for",
                ));
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                return Ok(());
            }
            other => return usage(format!("unknown argument `{other}`")),
        }
    }

    let Some(root) = root else {
        return usage("--root <repo> is required: the Lean package to document");
    };
    let Some(out) = out else {
        return usage(
            "--out <dir> is required and has no default: it is where the site, the IR, the cache \
             and the ledger go. The obvious default would be <root>/.lake/build/doc, which is \
             doc-gen4's own output tree — a default that overwrites another tool's output is a \
             data-loss bug with a friendly face",
        );
    };
    // **`--link-index` stopped being required in M5-b.** Left out, the map is
    // this command's own artefact: `<out>/link-index.lidx`, written by the
    // resident extractor out of the environment it imported for the extraction
    // (M5-a). Given, it is an input somebody else made and this command does not
    // touch it — which is what keeps every M4-d measurement re-runnable.
    //
    // The one shape that cannot work is `--extractor <program>` without
    // `--link-index`: that program's contract is three flags and it is the seam
    // the tests hand a fake through, so nothing here can make it write a map.
    if link_index.is_none() && extractor.is_some() {
        return usage(format!(
            "--extractor <program> needs --link-index <file>: the dependency map is written by the \
             Lean extractor out of the environment it imports, and --extractor names a program \
             whose interface is `--modules --ir-dir --timings` and nothing else. Either pass a map \
             (`lean-doc extract --link-index <file>` writes one) or use --extractor-bin, where \
             this command owns the extractor and derives the map itself. {LINK_INDEX_COST}",
        ));
    }
    if max_rounds == 0 {
        return usage("--max-rounds must be at least 1: round 1 is where deletions are folded in");
    }
    if jobs == 0 {
        return usage("--jobs must be at least 1");
    }
    if extractor.is_some() {
        for (flag, given) in [
            ("--extractor-bin", extractor_bin.is_some()),
            ("--lake", lake.is_some()),
        ] {
            if given {
                return usage(format!(
                    "{flag} and --extractor are exclusive: one names the Lean extractor this run \
                     keeps resident, the other names a program to call once per round. How that \
                     program finds its own binary is its own business",
                ));
            }
        }
        if jobs != 1 {
            return usage(
                "--jobs is a flag of the resident path: a resident extractor fixes its job count \
                 at start-up (plan §6, constraint 6). Behind --extractor, pass it through with \
                 `--extractor-arg --jobs --extractor-arg <n>`",
            );
        }
    }
    let mode = match mode {
        // See [`plan_of`]'s neighbours: `self` is the mode every measured run of
        // this pipeline used, and what makes it enough is that the render set is
        // a **union** with the whole-package map delta (approach.md §5.5). A
        // change that reaches another module's page without moving a name is a
        // change L3-1 already re-extracted the other module for.
        None => Mode::SelfOnly,
        Some(text) => match Mode::parse(&text) {
            Mode::Unrecognised(text) => {
                return usage(format!(
                    "--mode takes self|referrers|importers|all, not `{text}`"
                ));
            }
            parsed => parsed,
        },
    };

    // Canonicalised **before** anything is compared against it: `--out` under a
    // symlinked `--root` is still under `--root`, and the guard that says so is
    // the one M4-c found a real leak through.
    let root = fs::canonicalize(&root).map_err(|source| Failure::Refused {
        code: 3,
        message: format!("--root {}: {source}", root.display()),
    })?;
    let out = absolute(&out);
    if crate::extract::resolve(&out).starts_with(&root) {
        return Err(Failure::Refused {
            code: 3,
            message: format!(
                "--out {} is inside --root {}: the package being documented is opened read-only \
                 and nothing is ever written into it — `lean-doc extract` refuses an --ir-dir \
                 there for the same reason. Copy <out>/site into the repository afterwards if \
                 that is where the pages belong",
                out.display(),
                root.display(),
            ),
        });
    }

    let layout = Layout::new(&out);
    let derived = link_index.is_none();
    let link_index = absolute(&link_index.unwrap_or_else(|| layout.link_index.clone()));
    run(&Request {
        root,
        layout,
        libs,
        link_index,
        derived_link_index: derived,
        source_url,
        extractor,
        extractor_args,
        extractor_bin,
        lake,
        jobs,
        mode,
        max_rounds,
        timings,
        full,
    })
}

/// One `build` invocation, after the command line has been checked.
struct Request {
    root: PathBuf,
    layout: Layout,
    libs: Vec<String>,
    /// The dependency map the pages are rendered against, absolute.
    link_index: PathBuf,
    /// Whether this run *writes* that file (M5-b) or only reads it.
    derived_link_index: bool,
    source_url: Option<String>,
    extractor: Option<String>,
    extractor_args: Vec<String>,
    extractor_bin: Option<PathBuf>,
    lake: Option<PathBuf>,
    jobs: usize,
    mode: Mode,
    max_rounds: usize,
    timings: Option<PathBuf>,
    full: bool,
}

fn run(request: &Request) -> Result<(), Failure> {
    let started = Instant::now();
    let layout = &request.layout;

    // 1 -- the libraries ------------------------------------------------------
    let libs = if request.libs.is_empty() {
        let declared = crate::lakefile::read_libraries(&request.root)?;
        println!(
            "lib     {} (from {})",
            declared.names.join(", "),
            declared.file.display(),
        );
        declared.names
    } else {
        println!("lib     {} (--lib)", request.libs.join(", "));
        request.libs.clone()
    };

    // 2 -- the module list ----------------------------------------------------
    // One list, written down, and handed to every stage that takes one. M3-d2b:
    // its order is the ledger's `modules` array order and the merged
    // `index.json`'s, so two derivations of it are two different files.
    let modules = module_names(&request.root, &libs)?;
    if modules.is_empty() {
        return Err(Failure::Refused {
            code: 3,
            message: format!(
                "no modules under {} for {}: an empty list would build an empty site and report \
                 success",
                request.root.display(),
                libs.join(", "),
            ),
        });
    }
    println!("modules {}", modules.len());

    // 3 -- the source URL -----------------------------------------------------
    let source_url = match &request.source_url {
        Some(url) => url.clone(),
        None => derive_source_url(&request.root)?,
    };
    check_source_url(&source_url)?;
    println!("source  {source_url}");

    // 4 -- what is already there ----------------------------------------------
    // **Before anything is written**, and that ordering is load-bearing: the
    // question `plan_of` asks is "is `--out` empty, and if not, did this command
    // write it", and creating the work directory first would make every answer
    // "not empty, and yes".
    let plan = plan_of(request, &libs)?;
    match &plan {
        Plan::Full(why) => println!("plan    full generation ({why})"),
        Plan::Incremental => println!("plan    incremental (continuing {})", layout.out.display()),
    }
    write_marker(request, &libs, &source_url, modules.len(), false)?;
    crate::pipeline::create_dir(&layout.work)?;
    let modules_file = layout.work.join("modules.txt");
    write_lines(&modules_file, &modules)?;

    let mut extractor = open_extractor(request, &modules_file, &modules)?;
    let outcome = match plan {
        Plan::Full(_) => full_generation(
            request,
            &modules,
            &modules_file,
            &source_url,
            &mut extractor,
        ),
        Plan::Incremental => {
            incremental_generation(request, modules.clone(), &source_url, &mut extractor)
        }
    };
    // Not a `?` above 【判断】, exactly as `incremental` does it: the resident
    // environment is released on the failing path too, and doing it here puts
    // the stop *before* the error reaches the caller rather than after.
    extractor.release();
    let done = outcome?;

    // 5 -- the ledger, last ---------------------------------------------------
    // See this module's heading: everything that could have failed has now
    // succeeded, so the claim "the IR was built from these oleans and the pages
    // from that IR" is true when it is written and not before.
    let bytes = write_ledger(
        done.detected,
        &layout.ledger,
        &layout.ir,
        &source_url,
        &request.link_index,
    )?;
    println!(
        "ledger  {} module(s) -> {} ({bytes} B)",
        done.ledger_modules,
        layout.ledger.display(),
    );
    write_marker(request, &libs, &source_url, modules.len(), true)?;

    let total = started.elapsed().as_secs_f64();
    println!(
        "build   {} in {total:.4} s -> {}",
        done.what,
        layout.site.display(),
    );
    if let Some(path) = &request.timings {
        let record = serde_json::json!({
            "command": "build",
            "path": done.what,
            "modules": modules.len(),
            "extracted": done.extracted,
            "rounds": done.rounds,
            "pagesRendered": done.pages_rendered,
            "pagesInSite": done.pages_in_site,
            "ledgerModules": done.ledger_modules,
            "ledgerBytes": bytes,
            "extractSeconds": done.extract_seconds,
            "renderSeconds": done.render_seconds,
            "globalSeconds": done.global_seconds,
            "totalSeconds": total,
        });
        let line = serde_json::to_string(&record).expect("counts and durations serialise") + "\n";
        write_file(path, &line)?;
        println!("{}", line.trim_end());
    }
    Ok(())
}

/// What a run produced, in the shape the two paths have in common.
struct Done {
    what: &'static str,
    /// Modules handed to the extractor, summed over the rounds.
    extracted: usize,
    rounds: usize,
    pages_rendered: usize,
    pages_in_site: usize,
    ledger_modules: usize,
    extract_seconds: f64,
    render_seconds: f64,
    global_seconds: f64,
    /// The ledger this run licensed, with the hashes read **before** the
    /// extraction.
    detected: Ledger,
}

// -------------------------------------------------------------- the two paths

/// The first run: hash, extract everything, render everything.
///
/// The extraction is one request for the whole package — the same shape M4-b and
/// M4-c measured (436/436 byte-identical IR on both extraction paths) — and the
/// site is [`crate::generate_site`], which is `lean-doc site`'s own body, so the
/// tree this writes is the tree that command writes.
fn full_generation(
    request: &Request,
    modules: &[String],
    modules_file: &Path,
    source_url: &str,
    extractor: &mut Extractor,
) -> Result<Done, Failure> {
    let layout = &request.layout;
    // The hashes, **before** the extraction they license. Written into `--work`
    // as a diagnostic; the file that counts is written at the end of the run.
    let detected = build_ledger(&BuildOptions {
        modules,
        target: &request.root.to_string_lossy(),
        out: &layout.work.join("ledger-detect.json"),
        // No IR yet — the two IR keys are filled in from the tree this run is
        // about to write (see [`write_ledger`]).
        ir: None,
        source_url,
        // The map, if there is one yet. On a first run there is not: this
        // ledger is computed **before** the extraction that writes it, and the
        // key is filled in from the finished file by [`write_ledger`].
        link_index: Some(&request.link_index),
        algorithm: &Algorithm::sha256(),
        // The ledger's bytes do not depend on this (M3-a 【実測】); its speed
        // does. One is the pipeline's own choice too.
        concurrency: 1,
        timings: Some(&layout.work.join("ledger-timings.json")),
    })
    .map_err(refused)?
    .ledger;
    println!("detect  {} module(s) hashed", detected.modules.len());

    // The page tree is removed rather than written over: the renderer only ever
    // writes, so a module that vanished since the tree was made would keep its
    // page and the site would hold a file no from-scratch run produces. The
    // marker is what says this command made it (see the heading).
    if layout.site.exists() {
        fs::remove_dir_all(&layout.site)
            .map_err(|source| Failure::Failed(format!("{}: {source}", layout.site.display())))?;
    }
    // Same for the IR: a partial tree from an interrupted run would be merged
    // with, not replaced by, this extraction.
    if layout.ir.exists() {
        fs::remove_dir_all(&layout.ir)
            .map_err(|source| Failure::Failed(format!("{}: {source}", layout.ir.display())))?;
    }

    let at = Instant::now();
    extractor.run(
        modules_file,
        &layout.ir,
        &layout.work.join("extract-timings-1.json"),
    )?;
    let extract_seconds = at.elapsed().as_secs_f64();
    // The 3 GB environment is not held across the render: the loop is the only
    // thing that extracts, which on this path is one request.
    extractor.release();
    println!(
        "extract {} module(s) in {extract_seconds:.4} s",
        modules.len()
    );

    let site = crate::generate_site(
        &layout.ir,
        &layout.site,
        source_url,
        Some(&request.link_index),
        Some(&layout.state),
    )?;
    Ok(Done {
        what: "full",
        extracted: modules.len(),
        rounds: 1,
        pages_rendered: site.rendered.pages_written,
        pages_in_site: count_files(&layout.site),
        ledger_modules: detected.modules.len(),
        extract_seconds,
        render_seconds: site.render_seconds,
        global_seconds: site.global_seconds,
        detected,
    })
}

/// Every later run: the six-stage pipeline, over the tree the last one left.
fn incremental_generation(
    request: &Request,
    modules: Vec<String>,
    source_url: &str,
    extractor: &mut Extractor,
) -> Result<Done, Failure> {
    let layout = &request.layout;
    let run = crate::pipeline::run_incremental(
        &Incremental {
            ir: &layout.ir,
            pages: &layout.site,
            ledger: &layout.ledger,
            work: &layout.work,
            modules,
            source_url,
            link_index: &request.link_index,
            state: &layout.state,
            mode: request.mode.clone(),
            max_rounds: request.max_rounds,
        },
        extractor,
    )?;
    Ok(Done {
        what: "incremental",
        extracted: run.summary.changed + run.summary.stale_found,
        rounds: run.summary.rounds,
        pages_rendered: run.summary.pages_rendered,
        pages_in_site: count_files(&layout.site),
        ledger_modules: run.detected.modules.len(),
        extract_seconds: run.timings.extract,
        render_seconds: run.timings.render,
        global_seconds: run.timings.global,
        detected: run.detected,
    })
}

// ------------------------------------------------------------ the four answers

/// Step 4: full or incremental, and the reason, which is printed.
///
/// See the module heading for the whole table. The refusal in the middle is the
/// important one: this command removes and overwrites things under `--out`, so
/// it does that only to a directory whose marker says it made it.
fn plan_of(request: &Request, libs: &[String]) -> Result<Plan, Failure> {
    let layout = &request.layout;
    if !layout.out.exists() || is_empty_dir(&layout.out) {
        return Ok(Plan::Full("nothing there yet"));
    }
    // **`--full` is answered after the ownership checks, not before them**
    // 【判断】. A full generation *deletes* `<out>/site` and `<out>/ir`, so a
    // `--full` that short-circuited this would be the one way to make this
    // command remove a directory whose marker it never looked at — which is the
    // exact sentence the module heading promises it cannot do.
    let Some(marker) = read_marker(&layout.marker)? else {
        return Err(Failure::Refused {
            code: 3,
            message: format!(
                "{} is not empty and has no {MARKER}: this command deletes and overwrites inside \
                 --out, so it will only do that to a directory it can see it wrote. Name an empty \
                 directory, or remove this one yourself",
                layout.out.display(),
            ),
        });
    };
    let was = marker
        .get("root")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if was != request.root.to_string_lossy() {
        return Err(Failure::Refused {
            code: 3,
            message: format!(
                "{} was built from {was}, not from {}: the ledger under it stores the target whose \
                 oleans it hashed, and continuing here would compare one package's build tree with \
                 another package's hashes. Use a different --out",
                layout.out.display(),
                request.root.display(),
            ),
        });
    }
    if request.full {
        return Ok(Plan::Full("--full"));
    }
    let layout_version = marker.get("layout").and_then(serde_json::Value::as_u64);
    if layout_version != Some(LAYOUT) {
        return Ok(Plan::Full("the layout under --out is from another version"));
    }
    let declared: Vec<&str> = marker
        .get("libs")
        .and_then(serde_json::Value::as_array)
        .map(|libs| {
            libs.iter()
                .filter_map(serde_json::Value::as_str)
                .collect::<Vec<&str>>()
        })
        .unwrap_or_default();
    if declared != libs.iter().map(String::as_str).collect::<Vec<&str>>() {
        // Not a refusal: a package that gained a library has more modules, and
        // a full run is the correct answer to "the question changed".
        return Ok(Plan::Full("the libraries changed"));
    }
    if marker.get("complete") != Some(&serde_json::Value::Bool(true)) {
        return Ok(Plan::Full("the previous run did not finish"));
    }
    if !layout.carries_a_previous_run(&request.link_index) {
        return Ok(Plan::Full("the previous run's files are not all there"));
    }
    Ok(Plan::Incremental)
}

/// The extractor, in the same two shapes [`crate::pipeline`] offers.
///
/// **The resident one is the default** 【判断】, and `--extractor` is the seam.
/// The product's own extraction is a Lean environment this run owns; a caller
/// who wants something else — a wrapper, a recording, the frozen prototype's
/// `extract-once.sh`, a fake in a test that has no Lean at all — names a program
/// instead. Neither has a default *path*: `--extractor-bin` falls back to
/// `$EXTRACT_BIN` and then to a refusal, because the binary is 171 MB and built
/// against the target's own toolchain (M4-a).
fn open_extractor(
    request: &Request,
    modules_file: &Path,
    modules: &[String],
) -> Result<Extractor, Failure> {
    if let Some(program) = &request.extractor {
        return Ok(Extractor::OneShot {
            program: program.clone(),
            args: request.extractor_args.clone(),
        });
    }
    Ok(Extractor::Resident(Box::new(Resident::new(
        crate::pipeline::serve_options(crate::pipeline::ServeRequest {
            bin: request.extractor_bin.clone(),
            target: Some(request.root.clone()),
            lake: request.lake.clone(),
            jobs: request.jobs,
            modules_file,
            modules,
            work: &request.layout.work,
            // M5-b: the resident extractor writes the map when this command owns
            // it. With `--link-index` it is somebody else's file and is not
            // overwritten — the command line the M4-d gate ran is unchanged.
            link_index: request
                .derived_link_index
                .then_some(request.link_index.as_path()),
        })?,
    )?)))
}

/// `https://github.com/<owner>/<repo>/blob/<40-hex>`, from the checkout itself.
///
/// **Only `github.com` remotes are read** 【判断】. The `/blob/<rev>/<path>`
/// shape is GitHub's; GitLab spells the same thing `/-/blob/`, Gitea and sr.ht
/// differ again, and the acceptance oracle normalises `/blob/[0-9a-f]{40}/` and
/// nothing else (plan 決定 1). Guessing a host's URL scheme produces links that
/// are *plausible* and 404, on every declaration of every page. So anything else
/// is refused by name, with `--source-url` as the answer — the same shape as
/// [`crate::lakefile`]'s refusals.
///
/// The revision is `HEAD`, and an uncommitted working tree is reported rather
/// than refused: the pages will link to the last commit, which is a fact worth
/// one line of output and is not this command's to fix.
fn derive_source_url(root: &Path) -> Result<String, Failure> {
    let rev = git(root, &["rev-parse", "HEAD"])?;
    let remote = git(root, &["config", "--get", "remote.origin.url"])?;
    let Some(path) = github_path(&remote) else {
        return Err(Failure::Refused {
            code: 3,
            message: format!(
                "cannot derive --source-url from `{remote}`: only github.com remotes have a \
                 /blob/<rev>/<path> shape this can be sure of, and a guessed one 404s on every \
                 declaration of every page. Pass --source-url \
                 https://<host>/<owner>/<repo>/blob/{rev}",
            ),
        });
    };
    if let Ok(dirty) = git(root, &["status", "--porcelain"]) {
        let count = dirty.lines().filter(|line| !line.trim().is_empty()).count();
        if count > 0 {
            println!(
                "source  note: {count} uncommitted change(s) in {} — the pages will link to HEAD",
                root.display(),
            );
        }
    }
    Ok(format!("https://github.com/{path}/blob/{rev}"))
}

/// `<owner>/<repo>` when the remote is a github.com one, in any of the three
/// spellings git writes.
fn github_path(remote: &str) -> Option<String> {
    let remote = remote.trim();
    let rest = [
        "https://github.com/",
        "http://github.com/",
        "git@github.com:",
        "ssh://git@github.com/",
    ]
    .iter()
    .find_map(|prefix| remote.strip_prefix(prefix))?;
    let rest = rest.trim_end_matches('/');
    let rest = rest.strip_suffix(".git").unwrap_or(rest);
    let (owner, repo) = rest.split_once('/')?;
    if owner.is_empty() || repo.is_empty() || repo.contains('/') {
        return None;
    }
    Some(format!("{owner}/{repo}"))
}

/// One `git` command in the checkout, or a refusal naming it.
fn git(root: &Path, args: &[&str]) -> Result<String, Failure> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|source| Failure::Refused {
            code: 3,
            message: format!("git {}: {source}", args.join(" ")),
        })?;
    if !output.status.success() {
        return Err(Failure::Refused {
            code: 3,
            message: format!(
                "git {} in {} failed: {}. --source-url is a git question — pass it explicitly if \
                 the package is not a checkout",
                args.join(" "),
                root.display(),
                String::from_utf8_lossy(&output.stderr).trim(),
            ),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

// ------------------------------------------------------------------- the state

/// The ledger, with the keys taken from the tree that now exists.
///
/// See the module heading: the module hashes are `detect`'s (taken before the
/// extraction), the two keys are recomputed here (they describe the IR on disk,
/// which after a successful run is the merged tree). Returns the file's size,
/// which is the only number a caller can check the write by.
fn write_ledger(
    mut ledger: Ledger,
    path: &Path,
    ir: &Path,
    source_url: &str,
    link_index: &Path,
) -> Result<usize, Failure> {
    ledger.extract_key = extract_key(&ledger.target, Some(ir)).map_err(refused)?;
    // **The map as it is now, not as `detect` saw it** (M5-b), for exactly the
    // reason `extractKey` is recomputed here: both keys describe *the tree on
    // disk*, and after a successful run that is what this run produced. Writing
    // back the pre-run digest would leave a ledger claiming the pages were
    // rendered against a map that no longer exists, and the next run would
    // compare the new map with the old digest and re-render everything, for
    // ever.
    ledger.render_key = Some(render_key(
        source_url,
        link_index_digest(Some(link_index))
            .map_err(refused)?
            .as_deref(),
    ));
    let body = ledger.to_json();
    write_file(path, &body)?;
    Ok(body.len())
}

/// The marker, written twice per run — `complete: false` on the way in and
/// `complete: true` after the ledger.
///
/// It is a fixed set of keys in a fixed order and carries **no timestamp**: a
/// file that changes when nothing changed is one that cannot be compared across
/// two runs, and two runs of this command over an unchanged package have to be
/// able to produce identical trees.
fn write_marker(
    request: &Request,
    libs: &[String],
    source_url: &str,
    modules: usize,
    complete: bool,
) -> Result<(), Failure> {
    let record = serde_json::json!({
        "tool": "lean-doc build",
        "layout": LAYOUT,
        "root": request.root.to_string_lossy(),
        "libs": libs,
        "sourceUrl": source_url,
        "modules": modules,
        "complete": complete,
    });
    let body = serde_json::to_string(&record).expect("paths and counts serialise") + "\n";
    write_file(&request.layout.marker, &body)
}

/// The marker as JSON, or `None` when there is none.
///
/// A marker that will not parse is **not** treated as absent: it was written by
/// something, and deleting a site on the strength of a file this cannot read is
/// the failure the marker exists to prevent.
fn read_marker(path: &Path) -> Result<Option<serde_json::Value>, Failure> {
    let Ok(text) = fs::read_to_string(path) else {
        return Ok(None);
    };
    let record: serde_json::Value =
        serde_json::from_str(&text).map_err(|source| Failure::Refused {
            code: 3,
            message: format!(
                "{}: {source}. This file says which directory `lean-doc build` owns; one that will \
                 not parse is not one to overwrite a site on the strength of",
                path.display(),
            ),
        })?;
    Ok(Some(record))
}

// ------------------------------------------------------------------ plumbing

fn is_empty_dir(path: &Path) -> bool {
    fs::read_dir(path).is_ok_and(|mut entries| entries.next().is_none())
}

/// Every file under `root`, recursively. The site's denominator (438 on the
/// target) is a number this project quotes, so the run reports its own.
fn count_files(root: &Path) -> usize {
    let mut total = 0;
    let mut stack = vec![root.to_owned()];
    while let Some(dir) = stack.pop() {
        let Ok(listing) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in listing.flatten() {
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => stack.push(entry.path()),
                Ok(_) => total += 1,
                Err(_) => {}
            }
        }
    }
    total
}

fn write_file(path: &Path, body: &str) -> Result<(), Failure> {
    if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        crate::pipeline::create_dir(dir)?;
    }
    fs::write(path, body).map_err(|source| Failure::Failed(format!("{}: {source}", path.display())))
}
