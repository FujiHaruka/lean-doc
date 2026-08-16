//! Milestone **M3-a**: the `detect` stage — the olean hash ledger.
//!
//! # The oracle this file used to have, and why it is gone
//!
//! Until 2026-08-16 the exercises here were scored against **the frozen
//! prototype's own files**: `tools/ledger-reference.sh --impl ts` ran
//! `experiments/stage5/ledger.ts` over the measurement target, and
//! `tests/data/ledger-expected.json` held the sizes and digests of everything it
//! wrote. That oracle is **retired**, not lost: the port it was there to check is
//! finished, the prototype was removed with `experiments/` (tag
//! `experiments-frozen`), and the fixture compared a run over **today's** target
//! with bytes taken from an **older build of it** — so when the target moved (9
//! of its 432 modules rebuilt) the comparison failed for a reason that was never
//! about this code. **The comparison was deleted rather than re-frozen**: a
//! fixture regenerated from the port's own output is not an oracle, it is a
//! photograph.
//!
//! What the exercises still have is their own input — a synthetic package built
//! by [`FakeRepo`], including **the dependency shape the target package does not
//! have** (its own modules carry one `.olean` each; the three-file form of
//! Lean's module system only appears in its dependencies').
//!
//! # Branch coverage is counted, not believed (plan §7)
//!
//! Of the **53** [`BRANCHES`] this milestone added:
//!
//! | exercise | reaches |
//! |---|---:|
//! | one `build` over a package shaped like the target ([`LEDGER_BYTE_COMPARISON`]) | **7** |
//! | everything the twelve scenarios reach ([`HARNESS_SCENARIOS`]) | **34** |
//! | curated cases only ([`NO_REAL_DATA_REACHES`]) | **19** |
//!
//! The 34 are **measured** by
//! [`the_harness_scenarios_are_measured_on_a_synthetic_package`] and the
//! accounting is asserted rather than commented by
//! [`the_curated_cases_cover_what_the_package_does_not`].

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use lean_doc_incr::{
    Algorithm, BuildOptions, BuildSummary, CheckOptions, CheckSummary, Error, KeySet,
    LEDGER_SCHEMA, Ledger, TouchOptions, build_ledger, check_ledger, extract_key, hash_module,
    module_paths, render_key, touch_ledger,
};
use serde_json::{Value, json};

/// The same URLs `tools/ledger-reference.sh` passes. A rev is configuration:
/// the first only has to be 40 hex, the second only has to differ.
const URL: &str = "https://github.com/FujiHaruka/information-theory/blob/573793b243fb1343636088eb62d1789ab2b14cec";
const URL2: &str = "https://github.com/FujiHaruka/information-theory/blob/0000000000000000000000000000000000000000";

/// U+1D49C MATHEMATICAL SCRIPT CAPITAL A and U+FB00 LATIN SMALL LIGATURE FF:
/// the pair that separates UTF-16 order from code point order. `𝒜` sorts
/// *before* `ﬀ` in UTF-16 and after it by code point (plan §7, U1).
const ASTRAL: &str = "\u{1D49C}";
const LIGATURE: &str = "\u{FB00}";

// --------------------------------------------------------------- the branches

/// Every branch M3-a added, named by an event of the run rather than by a line
/// of the code.
///
/// Named by events on purpose (plan §7's rule from M1): a counter that mirrors
/// the branch structure becomes a second definition of it and drifts. Each of
/// these is decided by [`observe`] from the inputs a run was given and the files
/// it produced — never by asking the code under test what it decided.
const BRANCHES: [&str; 53] = [
    // Hashing one module's files.
    "fileHashedFromBytes",
    "fileHashReadFromLake",
    "algorithmForeignHashesBytes",
    "moduleOneOleanFile",
    "moduleThreeOleanFiles",
    "moduleNoOleanFile",
    "lakeHashFileMissing",
    "oleanUnreadable",
    "pathOutsideTarget",
    // The read pool.
    "poolSequential",
    "poolConcurrent",
    // The two keys.
    "extractKeyWithIr",
    "extractKeyWithoutIr",
    "irIndexFieldMissing",
    "extractKeyFileMissing",
    "renderKeyWithSourceUrl",
    "renderKeyWithoutSourceUrl",
    "sourceUrlTrailingSlashStripped",
    // Comparing them — the union rule, from both sides.
    "keysEqual",
    "keyValueDiffers",
    "keyOnlyInLedger",
    "keyOnlyInCurrent",
    "keyNameAboveBmp",
    // Shapes of the file itself.
    "ledgerRenderKeyAbsent",
    "ledgerSchemaAbsent",
    "ledgerDuplicateEntry",
    // build.
    "buildWrote",
    "buildRefusedMissingOlean",
    // check: what happened to a module.
    "moduleUnchanged",
    "moduleChanged",
    "moduleAdded",
    "moduleRemovedNoOlean",
    "moduleRemovedNotInList",
    "moduleListFromArgument",
    "moduleListFromLedger",
    "moduleListEmpty",
    // check: the two sets.
    "reExtractFromKeyChange",
    "reExtractFromHashes",
    "reExtractSortAboveBmp",
    "renderAllOn",
    "renderAllOff",
    "checkRefusedOldSchema",
    // What a run writes.
    "changedOutWritten",
    "changedOutOmitted",
    "removedOutWritten",
    "removedOutOmitted",
    "renderAllOutWritten",
    "renderAllOutOmitted",
    "emptySetWroteEmptyFile",
    "timingsWritten",
    "timingsOmitted",
    // touch.
    "touchInvalidated",
    "touchRefusedNoSuchModule",
];

/// What **one `build`** of a package shaped like the target reaches — the
/// artifact `tools/ledger-compare.sh` used to put side by side with the
/// prototype's.
///
/// Seven of the fifty-three. Every question `check` asks is invisible to it,
/// and so is every shape of file the target does not have.
const LEDGER_BYTE_COMPARISON: [&str; 7] = [
    "buildWrote",
    "extractKeyWithIr",
    "fileHashedFromBytes",
    "moduleOneOleanFile",
    "poolSequential",
    "renderKeyWithSourceUrl",
    "timingsWritten",
];

/// What the whole harness reaches — the seven ledgers, the two touches and the
/// twelve check scenarios of `tools/ledger-reference.sh`, replayed in process by
/// [`the_harness_scenarios_are_measured_on_a_synthetic_package`].
const HARNESS_SCENARIOS: [&str; 34] = [
    "buildWrote",
    "changedOutWritten",
    "emptySetWroteEmptyFile",
    "extractKeyWithIr",
    "extractKeyWithoutIr",
    "fileHashReadFromLake",
    "fileHashedFromBytes",
    "keyOnlyInCurrent",
    "keyOnlyInLedger",
    "keyValueDiffers",
    "keysEqual",
    "moduleAdded",
    "moduleChanged",
    "moduleListFromArgument",
    "moduleListFromLedger",
    "moduleNoOleanFile",
    "moduleOneOleanFile",
    "moduleRemovedNoOlean",
    "moduleRemovedNotInList",
    "moduleThreeOleanFiles",
    "moduleUnchanged",
    "poolConcurrent",
    "poolSequential",
    "reExtractFromHashes",
    "reExtractFromKeyChange",
    "removedOutWritten",
    "renderAllOff",
    "renderAllOn",
    "renderAllOutWritten",
    "renderKeyWithSourceUrl",
    "renderKeyWithoutSourceUrl",
    "sourceUrlTrailingSlashStripped",
    "timingsWritten",
    "touchInvalidated",
];

/// The branches **no exercise over the real target reaches at all**, whatever
/// the scenario.
///
/// Nineteen of fifty-three. Five are shapes of a ledger file only a hand edit or
/// an older version produces; five are failures of the disk or of the caller;
/// four are flags the pipeline always passes; two are the UTF-16 traps, which
/// need a name above the BMP and the package has none; and three are properties
/// the target package does not have (a foreign algorithm string, an IR index
/// missing a field, an empty module list).
const NO_REAL_DATA_REACHES: [&str; 19] = [
    "algorithmForeignHashesBytes",
    "buildRefusedMissingOlean",
    "changedOutOmitted",
    "checkRefusedOldSchema",
    "extractKeyFileMissing",
    "irIndexFieldMissing",
    "keyNameAboveBmp",
    "lakeHashFileMissing",
    "ledgerDuplicateEntry",
    "ledgerRenderKeyAbsent",
    "ledgerSchemaAbsent",
    "moduleListEmpty",
    "oleanUnreadable",
    "pathOutsideTarget",
    "reExtractSortAboveBmp",
    "removedOutOmitted",
    "renderAllOutOmitted",
    "timingsOmitted",
    "touchRefusedNoSuchModule",
];

// ------------------------------------------------------------- the observer

/// One run of one command, with everything needed to say what it reached.
enum Run<'a> {
    Build {
        options: &'a BuildOptions<'a>,
        result: &'a Result<BuildSummary, Error>,
    },
    Check {
        options: &'a CheckOptions<'a>,
        /// The ledger file as it was **before** the run: its text, and its
        /// parse when it parses.
        text: &'a str,
        ledger: Option<&'a Ledger>,
        result: &'a Result<CheckSummary, Error>,
    },
    Touch {
        before: Option<&'a Ledger>,
        result: &'a Result<usize, Error>,
    },
}

/// Which branches a run reached.
///
/// Everything here is read off the run's inputs (the options, the files on
/// disk, the ledger as it was) and its outputs (the summary, the files it
/// wrote). Nothing re-derives what the code under test decided: where a
/// decision is needed — did this module lose its olean? did the sort reorder
/// anything? — it is made here from the disk and from `std`.
fn observe(run: &Run<'_>) -> BTreeSet<&'static str> {
    let mut fired: BTreeSet<&'static str> = BTreeSet::new();
    let mut fire = |branch: &'static str| {
        assert!(
            BRANCHES.contains(&branch),
            "{branch} is not a counted branch"
        );
        fired.insert(branch);
    };

    match run {
        Run::Build { options, result } => {
            let target = options.target.trim_end_matches('/');
            let lib_dir = format!("{target}/.lake/build/lib/lean");
            observe_modules(options.modules, &lib_dir, options.algorithm, &mut fire);
            observe_pool(options.concurrency, &mut fire);
            observe_keys(options.ir, options.source_url, &mut fire);
            fire(if options.timings.is_some() {
                "timingsWritten"
            } else {
                "timingsOmitted"
            });
            match result {
                Ok(summary) => {
                    fire("buildWrote");
                    if summary
                        .ledger
                        .modules
                        .iter()
                        .flat_map(|entry| &entry.files)
                        .any(|file| file.path.starts_with('/'))
                    {
                        fire("pathOutsideTarget");
                    }
                }
                Err(error) => observe_failure(error, &mut fire),
            }
        }
        Run::Check {
            options,
            text,
            ledger,
            result,
        } => {
            if !text.contains("\"ledgerSchema\"") {
                fire("ledgerSchemaAbsent");
            }
            if let Some(ledger) = ledger {
                let algorithm = options.algorithm.unwrap_or(&ledger.algorithm);
                let current: Vec<String> = match options.modules {
                    Some(modules) => modules.to_vec(),
                    None => ledger.modules.iter().map(|e| e.module.clone()).collect(),
                };
                observe_modules(&current, &ledger.lib_dir, algorithm, &mut fire);
                if !ledger.lib_dir.starts_with(&ledger.target) {
                    fire("pathOutsideTarget");
                }
                if ledger.render_key.is_none() {
                    fire("ledgerRenderKeyAbsent");
                }
                let names: BTreeSet<&str> =
                    ledger.modules.iter().map(|e| e.module.as_str()).collect();
                if names.len() != ledger.modules.len() {
                    fire("ledgerDuplicateEntry");
                }
                // The key sets are the *inputs*; the comparison of them is made
                // here with a plain map union rather than with KeySet::diff.
                let want_extract = extract_key(&ledger.target, options.ir).ok();
                let want_render = render_key(options.source_url, None, options.external_links);
                if let Some(want) = &want_extract {
                    observe_key_diff(&ledger.extract_key, want, &mut fire);
                }
                observe_key_diff(&ledger.render_key_or_empty(), &want_render, &mut fire);
                for keys in [Some(&ledger.extract_key), ledger.render_key.as_ref()]
                    .into_iter()
                    .flatten()
                    .chain(want_extract.as_ref())
                    .chain([&want_render])
                {
                    if keys.iter().any(|(name, _)| !name.is_ascii()) {
                        fire("keyNameAboveBmp");
                    }
                }
            }
            observe_pool(options.concurrency, &mut fire);
            observe_keys(options.ir, options.source_url, &mut fire);
            fire(if options.modules.is_some() {
                "moduleListFromArgument"
            } else {
                "moduleListFromLedger"
            });
            if options.modules.is_some_and(<[String]>::is_empty) {
                fire("moduleListEmpty");
            }
            for (path, written, omitted) in [
                (
                    options.changed_out,
                    "changedOutWritten",
                    "changedOutOmitted",
                ),
                (
                    options.removed_out,
                    "removedOutWritten",
                    "removedOutOmitted",
                ),
                (
                    options.render_all_out,
                    "renderAllOutWritten",
                    "renderAllOutOmitted",
                ),
            ] {
                match path {
                    None => fire(omitted),
                    Some(path) => {
                        fire(written);
                        if result.is_ok()
                            && fs::read(path)
                                .expect("the output file was written")
                                .is_empty()
                        {
                            fire("emptySetWroteEmptyFile");
                        }
                    }
                }
            }
            fire(if options.timings.is_some() {
                "timingsWritten"
            } else {
                "timingsOmitted"
            });
            match result {
                Ok(summary) => {
                    let current: Vec<String> = match (options.modules, ledger) {
                        (Some(modules), _) => modules.to_vec(),
                        (None, Some(ledger)) => {
                            ledger.modules.iter().map(|e| e.module.clone()).collect()
                        }
                        (None, None) => Vec::new(),
                    };
                    if summary.modules > summary.changed.len() + summary.added.len() {
                        fire("moduleUnchanged");
                    }
                    if !summary.changed.is_empty() {
                        fire("moduleChanged");
                    }
                    if !summary.added.is_empty() {
                        fire("moduleAdded");
                    }
                    let lib_dir = ledger.map(|l| l.lib_dir.clone()).unwrap_or_default();
                    for module in &summary.removed {
                        if module_paths(&lib_dir, module).is_empty() && current.contains(module) {
                            fire("moduleRemovedNoOlean");
                        }
                        if !current.contains(module) {
                            fire("moduleRemovedNotInList");
                        }
                    }
                    if summary.extract_invalidated() {
                        fire("reExtractFromKeyChange");
                    } else {
                        fire("reExtractFromHashes");
                        // The union the prototype sorts, in code point order:
                        // when the two orders differ the UTF-16 sort is doing
                        // work no byte comparison of an ASCII package can see.
                        let mut by_code_point: Vec<&String> =
                            summary.changed.iter().chain(&summary.added).collect();
                        by_code_point.sort();
                        if by_code_point.len() > 1
                            && by_code_point
                                .iter()
                                .zip(&summary.re_extract)
                                .any(|(a, b)| a.as_str() != b.as_str())
                        {
                            fire("reExtractSortAboveBmp");
                        }
                    }
                    fire(if summary.render_all() {
                        "renderAllOn"
                    } else {
                        "renderAllOff"
                    });
                }
                Err(error) => observe_failure(error, &mut fire),
            }
        }
        Run::Touch { before, result } => {
            if let Some(before) = before {
                let names: BTreeSet<&str> =
                    before.modules.iter().map(|e| e.module.as_str()).collect();
                if names.len() != before.modules.len() {
                    fire("ledgerDuplicateEntry");
                }
            }
            match result {
                Ok(_) => fire("touchInvalidated"),
                Err(error) => observe_failure(error, &mut fire),
            }
        }
    }
    fired
}

fn observe_modules(
    modules: &[String],
    lib_dir: &str,
    algorithm: &Algorithm,
    fire: &mut impl FnMut(&'static str),
) {
    let mut files = 0usize;
    for module in modules {
        let paths = module_paths(lib_dir, module);
        files += paths.len();
        match paths.len() {
            0 => fire("moduleNoOleanFile"),
            1 => fire("moduleOneOleanFile"),
            3 => fire("moduleThreeOleanFiles"),
            other => panic!("{module} has {other} olean files; only 0, 1 and 3 are counted"),
        }
    }
    if files > 0 {
        if algorithm.hashes_bytes() {
            fire("fileHashedFromBytes");
            if algorithm.name() != Algorithm::SHA256 {
                fire("algorithmForeignHashesBytes");
            }
        } else {
            fire("fileHashReadFromLake");
        }
    }
}

fn observe_pool(concurrency: usize, fire: &mut impl FnMut(&'static str)) {
    fire(if concurrency <= 1 {
        "poolSequential"
    } else {
        "poolConcurrent"
    });
}

fn observe_keys(ir: Option<&Path>, source_url: &str, fire: &mut impl FnMut(&'static str)) {
    match ir {
        None => fire("extractKeyWithoutIr"),
        Some(ir) => {
            fire("extractKeyWithIr");
            let index: Option<Value> = fs::read_to_string(ir.join("index.json"))
                .ok()
                .and_then(|text| serde_json::from_str(&text).ok());
            let has = |key: &str| index.as_ref().is_some_and(|index| index.get(key).is_some());
            if !has("schemaVersion") || !has("generator") {
                fire("irIndexFieldMissing");
            }
        }
    }
    if source_url.is_empty() {
        fire("renderKeyWithoutSourceUrl");
    } else {
        fire("renderKeyWithSourceUrl");
        if source_url.ends_with('/') {
            fire("sourceUrlTrailingSlashStripped");
        }
    }
}

/// The union comparison, written out with a `BTreeMap` so that it is a second
/// definition of the rule rather than a call to the one under test.
fn observe_key_diff(was: &KeySet, now: &KeySet, fire: &mut impl FnMut(&'static str)) {
    let left: BTreeMap<&str, &str> = was.iter().collect();
    let right: BTreeMap<&str, &str> = now.iter().collect();
    let mut equal = true;
    for name in left.keys().chain(right.keys()) {
        match (left.get(name), right.get(name)) {
            (Some(a), Some(b)) if a == b => {}
            (Some(_), Some(_)) => {
                equal = false;
                fire("keyValueDiffers");
            }
            (Some(_), None) => {
                equal = false;
                fire("keyOnlyInLedger");
            }
            (None, Some(_)) => {
                equal = false;
                fire("keyOnlyInCurrent");
            }
            (None, None) => unreachable!("the name came from one of the two maps"),
        }
    }
    if equal {
        fire("keysEqual");
    }
}

fn observe_failure(error: &Error, fire: &mut impl FnMut(&'static str)) {
    match error {
        Error::NoOlean { .. } => fire("buildRefusedMissingOlean"),
        Error::LedgerSchema { .. } => fire("checkRefusedOldSchema"),
        Error::NoSuchModule { .. } => fire("touchRefusedNoSuchModule"),
        Error::Io { path, .. } => {
            let path = path.to_string_lossy();
            if path.ends_with(".hash") {
                fire("lakeHashFileMissing");
            } else if path.contains(".olean") {
                fire("oleanUnreadable");
            } else {
                fire("extractKeyFileMissing");
            }
        }
        Error::Json { .. } => panic!("no exercise here hands the ledger unparseable JSON"),
        // The ones the IR and page stages raise (M3-b, M3-c). `detect` never
        // reads an IR tree beyond `index.json`'s two key fields and never
        // touches the page tree, so none of them can arrive here.
        Error::Ir(_)
        | Error::IndexShape { .. }
        | Error::ModuleListMismatch { .. }
        | Error::NotAModule { .. }
        | Error::UnknownMode { .. }
        | Error::OutsidePageRoot { .. } => {
            panic!("the ledger stage does not read the IR or the pages")
        }
    }
}

// ------------------------------------------------------------ running things

fn run_build(options: &BuildOptions<'_>) -> (Result<BuildSummary, Error>, BTreeSet<&'static str>) {
    let result = build_ledger(options);
    let fired = observe(&Run::Build {
        options,
        result: &result,
    });
    (result, fired)
}

fn run_check(options: &CheckOptions<'_>) -> (Result<CheckSummary, Error>, BTreeSet<&'static str>) {
    let text = fs::read_to_string(options.ledger).unwrap_or_default();
    let ledger: Option<Ledger> = serde_json::from_str(&text).ok();
    let result = check_ledger(options);
    let fired = observe(&Run::Check {
        options,
        text: &text,
        ledger: ledger.as_ref(),
        result: &result,
    });
    (result, fired)
}

fn run_touch(options: &TouchOptions<'_>) -> (Result<usize, Error>, BTreeSet<&'static str>) {
    let before: Option<Ledger> = fs::read_to_string(options.ledger)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok());
    let result = touch_ledger(options);
    let fired = observe(&Run::Touch {
        before: before.as_ref(),
        result: &result,
    });
    (result, fired)
}

/// The same twelve scenarios, over a package this test owns.
///
/// **Why this exists.** The twelve used to run only inside
/// `the_corpus_matches_the_prototype` (deleted 2026-08-16), which compared every file it produced
/// with the frozen prototype's bytes over the measurement target. That
/// comparison asked "does the port write what the prototype wrote" — a question
/// the port answered when it was ported, and one nobody can ask again: the
/// prototype was removed on 2026-08-16 (tag `experiments-frozen`) and the target
/// keeps moving, so the fixture and the package it was taken against drift apart
/// and the drift is reported as a failure of code that did not change.
///
/// **What it also did, and nothing else does, is *measure* [`HARNESS_SCENARIOS`]
/// rather than assume it.** [`the_curated_cases_cover_what_the_package_does_not`]
/// reads the 34 as a constant and checks only that the other 19 have a
/// written-down case; if nothing measures the 34, the branch accounting is a
/// claim about the code with no exercise behind it. That is the half kept here,
/// and it is kept **without a corpus**, so it runs on CI and needs no gate.
#[test]
fn the_harness_scenarios_are_measured_on_a_synthetic_package() {
    let package = FakeRepo::new(
        "harness-package",
        &[
            FakeModule::one("Pkg.A"),
            FakeModule::one("Pkg.B"),
            FakeModule::one("Pkg.C"),
            FakeModule::one("Pkg.D"),
        ],
    );
    // The dependency shape — all three files of Lean's module system — which the
    // target package does not have and only its dependencies do.
    let dependency = FakeRepo::new(
        "harness-dependency",
        &[FakeModule::three("Dep.One"), FakeModule::three("Dep.Two")],
    );
    let work = TempDir::new("harness");
    let mut covered: BTreeSet<&'static str> = BTreeSet::new();

    let modules = package.module_names();
    let dep_modules = dependency.module_names();
    // The two lists the drift scenario needs, sliced the way
    // `tools/ledger-reference.sh` sliced the target's: the ledger is missing the
    // first two modules, and the list drops the third while naming one module
    // that has no olean at all.
    let minus_ab: Vec<String> = modules[2..].to_vec();
    let mut minus_c_plus_ghost: Vec<String> = modules.clone();
    minus_c_plus_ghost.remove(2);
    minus_c_plus_ghost.push("Pkg.Ghost".to_owned());

    let sha256 = Algorithm::sha256();
    let lake = Algorithm::lake();
    let package_target = package.target();
    let dependency_target = dependency.target();
    let ir = package.ir();

    // --- the seven ledgers
    let mut build = |name: &str,
                     modules: &[String],
                     target: &str,
                     ir: Option<&Path>,
                     algorithm: &Algorithm,
                     concurrency: usize| {
        let out = work.path.join(format!("ledger-{name}.json"));
        let timings = work.path.join(format!("ledger-{name}.timings.json"));
        let (result, fired) = run_build(&BuildOptions {
            link_index: None,
            external_links: None,
            modules,
            target,
            out: &out,
            ir,
            source_url: URL,
            algorithm,
            concurrency,
            timings: Some(&timings),
        });
        let summary = result.expect("the synthetic package builds");
        covered.extend(fired);
        (out, summary)
    };
    let (sha_path, sha_summary) = build("sha256", &modules, &package_target, Some(&ir), &sha256, 1);
    assert_eq!(sha_summary.modules, modules.len());
    assert_eq!(sha_summary.files, modules.len(), "one olean per module");
    let (lake_path, lake_summary) = build("lake", &modules, &package_target, Some(&ir), &lake, 1);
    assert_eq!(
        lake_summary.hashed_bytes, 0,
        "the lake algorithm reads no olean at all"
    );
    let (minus_ab_path, _) = build(
        "minus-ab",
        &minus_ab,
        &package_target,
        Some(&ir),
        &sha256,
        1,
    );
    let (conc8_path, _) = build("conc8", &modules, &package_target, Some(&ir), &sha256, 8);
    let (noir_path, _) = build("noir", &modules, &package_target, None, &sha256, 1);
    let (dep_path, dep_summary) = build(
        "dep-sha256",
        &dep_modules,
        &dependency_target,
        Some(&ir),
        &sha256,
        1,
    );
    assert_eq!(
        dep_summary.files,
        dep_modules.len() * 3,
        "three olean files per dependency module"
    );
    let (dep_lake_path, _) = build(
        "dep-lake",
        &dep_modules,
        &dependency_target,
        Some(&ir),
        &lake,
        1,
    );
    assert_eq!(
        fs::read(&conc8_path).expect("reads"),
        fs::read(&sha_path).expect("reads"),
        "the ledger's bytes depend on the scheduling of the read pool"
    );

    // --- touch, twice, onto one file
    let touched = work.path.join("ledger-touched.json");
    for module in &modules[..2] {
        let source = if touched.exists() {
            &touched
        } else {
            &sha_path
        };
        let (result, fired) = run_touch(&TouchOptions {
            ledger: source,
            module,
            out: &touched,
        });
        result.expect("the module is in the ledger");
        covered.extend(fired);
    }

    // --- the twelve questions
    let mut scenario = |name: &str,
                        ledger: &Path,
                        modules: Option<&[String]>,
                        ir: Option<&Path>,
                        source_url: &str| {
        let changed = work.path.join(format!("{name}-changed.txt"));
        let removed = work.path.join(format!("{name}-removed.txt"));
        let render_all = work.path.join(format!("{name}-render-all.txt"));
        let timings = work.path.join(format!("{name}-timings.json"));
        let (result, fired) = run_check(&CheckOptions {
            link_index: None,
            external_links: None,
            ledger,
            algorithm: None,
            modules,
            ir,
            source_url,
            concurrency: 1,
            changed_out: Some(&changed),
            removed_out: Some(&removed),
            render_all_out: Some(&render_all),
            timings: Some(&timings),
        });
        let summary = result.expect("the synthetic package checks");
        covered.extend(fired);
        summary
    };
    let all = Some(modules.as_slice());
    let clean = scenario("clean", &sha_path, all, Some(&ir), URL);
    assert_eq!(
        (clean.changed.len(), clean.added.len(), clean.removed.len()),
        (0, 0, 0)
    );
    let two = scenario("touched", &touched, all, Some(&ir), URL);
    assert_eq!(two.changed, modules[..2].to_vec());
    let drift = scenario(
        "drift",
        &minus_ab_path,
        Some(&minus_c_plus_ghost),
        Some(&ir),
        URL,
    );
    assert_eq!(drift.added, modules[..2].to_vec());
    assert_eq!(
        drift.removed,
        vec!["Pkg.Ghost".to_owned(), modules[2].clone()],
        "the module with no olean comes first, in list order, then the one the list dropped"
    );
    let no_ir = scenario("extractkey", &sha_path, all, None, URL);
    assert_eq!(
        no_ir.extract_key_changed,
        ["irGenerator", "irSchemaVersion"]
    );
    assert_eq!(
        no_ir.re_extract.len(),
        modules.len(),
        "every module is re-extracted"
    );
    let other_rev = scenario("rendervalue", &sha_path, all, Some(&ir), URL2);
    assert_eq!(other_rev.render_key_changed, ["sourceUrl"]);
    assert!(
        other_rev.re_extract.is_empty(),
        "a render key changes no IR"
    );
    let no_url = scenario("renderless", &sha_path, all, Some(&ir), "");
    assert_eq!(no_url.render_key_changed, ["sourceUrl"]);
    scenario("lake", &lake_path, all, Some(&ir), URL);
    let from_ledger = scenario("fromledger", &sha_path, None, Some(&ir), URL);
    assert_eq!(from_ledger.modules, modules.len());
    scenario("dependency", &dep_path, Some(&dep_modules), Some(&ir), URL);
    scenario(
        "dependencylake",
        &dep_lake_path,
        Some(&dep_modules),
        Some(&ir),
        URL,
    );
    let into_ir = scenario("intoir", &noir_path, all, Some(&ir), URL);
    assert_eq!(
        into_ir.extract_key_changed,
        ["irGenerator", "irSchemaVersion"]
    );
    let slash = scenario("slash", &sha_path, all, Some(&ir), &format!("{URL}/"));
    assert!(
        slash.render_key_changed.is_empty(),
        "a trailing slash is not a different source URL"
    );

    assert_eq!(
        covered,
        BTreeSet::from(HARNESS_SCENARIOS),
        "which branches the twelve scenarios reach has changed"
    );
}

/// The identity strings are the cache's version key (plan §3, §6).
#[test]
fn the_identity_strings_are_not_the_prototypes() {
    assert_ne!(lean_doc_incr::EXTRACTOR_ID, "lean-doc/experiments/stage4b");
    assert_ne!(lean_doc_incr::RENDERER_ID, "lean-doc/experiments/stage4c");
    let repo = FakeRepo::new("identity", &[FakeModule::one("Pkg.A")]);
    let ledger = repo.build(&[], &Algorithm::sha256(), URL);
    assert_eq!(
        ledger.extract_key.get("extractor"),
        Some(lean_doc_incr::EXTRACTOR_ID)
    );
    assert_eq!(
        ledger.render_key_or_empty().get("renderer"),
        Some(lean_doc_incr::RENDERER_ID)
    );
    // …and the IR's own generator is *not* renamed with them: it names what
    // wrote the tree on disk, which the port does not claim to be.
    assert_eq!(
        ledger.extract_key.get("irGenerator"),
        Some("lean-doc/experiments/stage4b")
    );
}

/// `renderKey.externalLinks` — M7-b's key.
///
/// Where each **dependency's** source lives reaches every page that links into
/// one, and it moves on exactly the occasion an incremental build runs (a bumped
/// dependency is a new `rev`). So it is a render key, it sits after the two the
/// renderer already had — the ledger's bytes are the insertion order — and all
/// three of appearing, vanishing and moving count as a change.
#[test]
fn the_dependency_link_maps_digest_is_a_render_key_of_its_own() {
    let without = render_key(URL, None, None);
    let with = render_key(URL, None, Some("d1"));
    let moved = render_key(URL, None, Some("d2"));

    assert_eq!(without.get("externalLinks"), None);
    assert_eq!(with.get("externalLinks"), Some("d1"));
    assert_eq!(
        with.iter().map(|(name, _)| name).collect::<Vec<_>>(),
        ["renderer", "sourceUrl", "externalLinks"],
    );

    assert_eq!(without.diff(&with), ["externalLinks"], "it appeared");
    assert_eq!(with.diff(&without), ["externalLinks"], "it vanished");
    assert_eq!(with.diff(&moved), ["externalLinks"], "a dependency moved");
    assert!(with.diff(&with).is_empty());

    // The key does not disturb the other two, and it is not the `.lidx`'s: the
    // two maps are different things and either can move without the other.
    let both = render_key(URL, Some("lidx"), Some("d1"));
    assert_eq!(
        both.iter().collect::<Vec<_>>(),
        [
            ("renderer", lean_doc_incr::RENDERER_ID),
            ("sourceUrl", URL),
            ("linkIndex", "lidx"),
            ("externalLinks", "d1"),
        ],
    );
    assert_eq!(
        both.diff(&render_key(URL, Some("lidx"), None)),
        ["externalLinks"]
    );
}

// ------------------------------------------------------------ the dependency

/// The dependency this milestone's coverage rests on, stated so that it fails
/// when it stops being true (plan §7: 「全件バイト一致」は分岐被覆の証明ではない).
///
/// Four claims, all counted rather than believed:
///
/// 1. The ledger byte comparison — one `build` over the target — reaches **7 of
///    the 53**. Everything `check` decides is invisible to it.
/// 2. The whole harness reaches **34 of 53** ([`HARNESS_SCENARIOS`], measured in
///    [`the_harness_scenarios_are_measured_on_a_synthetic_package`]).
/// 3. The other **19** are reachable only by a written-down case, so every one
///    of them is one.
/// 4. Everything together is all 53.
#[test]
fn the_curated_cases_cover_what_the_package_does_not() {
    // (1) A build of a package shaped like the target: one olean per module,
    // both keys, sha256, one read at a time.
    let repo = FakeRepo::new(
        "byte-comparison",
        &[FakeModule::one("Pkg.A"), FakeModule::one("Pkg.B")],
    );
    let out = repo.dir.path.join("ledger.json");
    let timings = repo.dir.path.join("timings.json");
    let modules = repo.module_names();
    let ir = repo.ir();
    let (result, fired) = run_build(&BuildOptions {
        link_index: None,
        external_links: None,
        modules: &modules,
        target: &repo.target(),
        out: &out,
        ir: Some(&ir),
        source_url: URL,
        algorithm: &Algorithm::sha256(),
        concurrency: 1,
        timings: Some(&timings),
    });
    result.expect("the synthetic package builds");
    assert_eq!(
        fired,
        BTreeSet::from(LEDGER_BYTE_COMPARISON),
        "the ledger byte comparison reaches a different set of branches than it did"
    );
    assert_eq!(fired.len(), 7);

    // (2) and (3): what the harness leaves for the curated cases.
    let harness = BTreeSet::from(HARNESS_SCENARIOS);
    assert!(fired.is_subset(&harness));
    assert_eq!(harness.len(), 34);
    let only_curated: BTreeSet<&str> = BRANCHES
        .iter()
        .copied()
        .filter(|branch| !harness.contains(branch))
        .collect();
    assert_eq!(
        only_curated,
        BTreeSet::from(NO_REAL_DATA_REACHES),
        "which branches no real-data exercise reaches has changed"
    );

    // (4) Everything, together. The curated cases are what stands between the
    // nineteen branches above and nothing at all testing them.
    let curated = {
        let mut curated = curated_hash_branches();
        curated.extend(curated_key_branches());
        curated.extend(curated_module_branches());
        curated
    };
    for branch in NO_REAL_DATA_REACHES {
        assert!(
            curated.contains(branch),
            "{branch} is reached by no real data and by no curated case either"
        );
    }
    let mut all = harness;
    all.extend(curated);
    let missing: Vec<&str> = BRANCHES
        .iter()
        .copied()
        .filter(|branch| !all.contains(branch))
        .collect();
    assert!(
        missing.is_empty(),
        "these branches fire nowhere at all: {missing:?}"
    );
    assert_eq!(all.len(), BRANCHES.len());
}

// -------------------------------------------------------- the curated cases

/// The three-file module, the algorithms and the two ways reading a hash can
/// fail.
fn curated_hash_branches() -> BTreeSet<&'static str> {
    let mut covered = BTreeSet::new();

    // All three olean files, on both algorithms — the dependency packages'
    // shape, which the target package does not have.
    let repo = FakeRepo::new("three-oleans", &[FakeModule::three("Pkg.Split")]);
    let sha = repo.build(&[], &Algorithm::sha256(), URL);
    let entry = &sha.modules[0];
    assert_eq!(entry.files.len(), 3, "all three files are hashed");
    assert_eq!(
        entry
            .files
            .iter()
            .map(|f| f.path.as_str())
            .collect::<Vec<_>>(),
        [
            ".lake/build/lib/lean/Pkg/Split.olean",
            ".lake/build/lib/lean/Pkg/Split.olean.server",
            ".lake/build/lib/lean/Pkg/Split.olean.private",
        ],
        "in suffix order, relative to the target"
    );
    let lake = repo.build(&[], &Algorithm::lake(), URL);
    assert!(
        lake.modules[0].files.iter().all(|f| f.bytes == -1),
        "the lake path reads no bytes, so it reports none"
    );
    assert_ne!(
        lake.modules[0].hash, entry.hash,
        "two algorithms, two hashes: a ledger's hashes are only comparable with its own"
    );
    // The per-module hash is over the per-file hashes, so dropping one file
    // moves it even when every file that stayed is identical.
    let one = FakeRepo::new("one-olean", &[FakeModule::one("Pkg.Split")]);
    assert_ne!(
        one.build(&[], &Algorithm::sha256(), URL).modules[0].hash,
        entry.hash,
        "a module that lost two olean files has to look changed"
    );

    // An algorithm nobody defined hashes the bytes: the prototype tests for
    // `lake` and treats everything else as the reference.
    let foreign = repo.build(&[], &Algorithm::new("md5"), URL);
    assert_eq!(foreign.modules[0].hash, entry.hash);
    assert_eq!(foreign.algorithm.name(), "md5", "recorded verbatim");
    covered.extend(repo.observed_build(&Algorithm::new("md5")));

    // `--algorithm lake` with no `<file>.hash` on disk: an error, not a removed
    // module. Reporting it as removed would delete the module's pages.
    let no_hash = FakeRepo::new("no-hash-file", &[FakeModule::one("Pkg.A").without_hashes()]);
    let (result, fired) = no_hash.run_build(&Algorithm::lake(), URL);
    let error = result.expect_err("a missing hash file stops the run");
    assert!(
        error
            .to_string()
            .ends_with(".olean.hash: No such file or directory (os error 2)"),
        "{error}"
    );
    assert_eq!(
        error.exit_code(),
        1,
        "a file that would not read is not a refusal"
    );
    covered.extend(fired);

    // An olean that cannot be read at all.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let locked = FakeRepo::new("unreadable", &[FakeModule::one("Pkg.A")]);
        let path = format!("{}/Pkg/A.olean", locked.lib_dir());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).expect("chmod");
        let (result, fired) = locked.run_build(&Algorithm::sha256(), URL);
        assert!(result.is_err(), "an unreadable olean stops the run");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("chmod back");
        covered.extend(fired);
    }

    // A `libDir` that is not under the target: the prototype cuts the path at
    // an arbitrary offset, this keeps it whole. No `build` can produce it.
    let outside = FakeRepo::new("outside", &[FakeModule::one("Pkg.A")]);
    let entry = hash_module(
        "/somewhere/else",
        &outside.lib_dir(),
        "Pkg.A",
        &Algorithm::sha256(),
    )
    .expect("the olean reads")
    .expect("the module has an olean");
    assert_eq!(
        entry.files[0].path,
        format!("{}/Pkg/A.olean", outside.lib_dir())
    );
    covered.extend(outside.observed_check_with_lib_dir("/somewhere/else"));

    covered
}

/// The keys: the shapes of the two files they are read from, and the union rule
/// with a key name above the BMP.
fn curated_key_branches() -> BTreeSet<&'static str> {
    let mut covered = BTreeSet::new();

    // An IR index with neither field: the two keys become the string
    // "undefined", which is what the prototype writes and what makes two
    // IR trees without a schema version compare equal.
    let repo = FakeRepo::new("empty-index", &[FakeModule::one("Pkg.A")]);
    fs::write(repo.ir().join("index.json"), "{}").expect("the index is writable");
    let ledger = repo.build(&[], &Algorithm::sha256(), URL);
    assert_eq!(ledger.extract_key.get("irSchemaVersion"), Some("undefined"));
    assert_eq!(ledger.extract_key.get("irGenerator"), Some("undefined"));
    covered.extend(repo.observed_build(&Algorithm::sha256()));

    // A target without `lean-toolchain`: the key cannot be built at all.
    let bare = FakeRepo::new("bare-target", &[FakeModule::one("Pkg.A")]);
    fs::remove_file(bare.dir.path.join("repo/lean-toolchain")).expect("the file is removable");
    let (result, fired) = bare.run_build(&Algorithm::sha256(), URL);
    let error = result.expect_err("no toolchain, no extract key");
    assert!(error.to_string().contains("lean-toolchain"), "{error}");
    covered.extend(fired);

    // A ledger whose render key holds two names above the BMP, both changed.
    // The reasons reach `--render-all-out`, so the UTF-16 sort is in the file's
    // bytes: `𝒜` before `ﬀ`, which code point order reverses.
    let repo = FakeRepo::new("astral-keys", &[FakeModule::one("Pkg.A")]);
    let mut ledger = repo.build(&[], &Algorithm::sha256(), URL);
    let mut keys = KeySet::new();
    keys.insert(LIGATURE, "was");
    keys.insert(ASTRAL, "was");
    ledger.render_key = Some(keys);
    let path = repo.dir.path.join("astral.json");
    fs::write(&path, ledger.to_json()).expect("the ledger is writable");
    let render_all = repo.dir.path.join("astral-render-all.txt");
    let (result, fired) = run_check(&repo.check_options(&path, &render_all));
    let summary = result.expect("the check runs");
    assert_eq!(
        summary.render_key_changed,
        ["renderer", "sourceUrl", ASTRAL, LIGATURE]
    );
    assert_eq!(
        fs::read_to_string(&render_all).expect("written"),
        format!(
            "renderKey:renderer\nrenderKey:sourceUrl\nrenderKey:{ASTRAL}\nrenderKey:{LIGATURE}\n"
        ),
        "UTF-16 order: str::cmp would put the ligature before the script capital"
    );
    covered.extend(fired);

    // A schema-2 ledger with no `renderKey` at all: every current render key is
    // a change, because the comparison is over the union.
    let mut without = repo.build(&[], &Algorithm::sha256(), URL);
    without.render_key = None;
    let path = repo.dir.path.join("no-render-key.json");
    fs::write(&path, without.to_json()).expect("writable");
    assert!(
        !fs::read_to_string(&path).unwrap().contains("renderKey"),
        "a ledger that had no render key does not grow one"
    );
    let render_all = repo.dir.path.join("no-render-key-out.txt");
    let (result, fired) = run_check(&repo.check_options(&path, &render_all));
    assert_eq!(
        result.expect("the check runs").render_key_changed,
        ["renderer", "sourceUrl"]
    );
    covered.extend(fired);

    // Ledgers older than the split, both spellings.
    for (what, damage) in [
        ("an explicit 1", json!(1)),
        ("no field at all", Value::Null),
    ] {
        let mut value: Value =
            serde_json::from_str(&repo.build(&[], &Algorithm::sha256(), URL).to_json())
                .expect("the ledger is JSON");
        match damage {
            Value::Null => {
                value
                    .as_object_mut()
                    .expect("an object")
                    .remove("ledgerSchema");
            }
            other => value["ledgerSchema"] = other,
        }
        let path = repo.dir.path.join("old.json");
        fs::write(&path, value.to_string()).expect("writable");
        let out = repo.dir.path.join("old-out.txt");
        let (result, fired) = run_check(&repo.check_options(&path, &out));
        let error = result.expect_err("a ledger older than the split is refused");
        assert_eq!(error.exit_code(), 3, "{what}: a refusal, not an IO failure");
        assert!(
            error
                .to_string()
                .contains(&format!("needs {LEDGER_SCHEMA}")),
            "{error}"
        );
        assert!(!out.exists(), "{what}: a refused check writes nothing");
        covered.extend(fired);
    }

    covered
}

/// The module list, the sets, the output files and `touch`.
fn curated_module_branches() -> BTreeSet<&'static str> {
    let mut covered = BTreeSet::new();
    let repo = FakeRepo::new(
        "modules",
        &[FakeModule::one("Pkg.A"), FakeModule::one("Pkg.B")],
    );
    let ledger_path = repo.dir.path.join("ledger.json");
    fs::write(
        &ledger_path,
        repo.build(&[], &Algorithm::sha256(), URL).to_json(),
    )
    .expect("writable");

    // An empty `--modules` list is a list, not a missing one: every module the
    // ledger knows is gone. Nothing else in the CLI can say that.
    let removed = repo.dir.path.join("empty-removed.txt");
    let (result, fired) = run_check(&CheckOptions {
        link_index: None,
        external_links: None,
        ledger: &ledger_path,
        algorithm: None,
        modules: Some(&[]),
        ir: Some(&repo.ir()),
        source_url: URL,
        concurrency: 1,
        changed_out: None,
        removed_out: Some(&removed),
        render_all_out: None,
        timings: None,
    });
    let summary = result.expect("the check runs");
    assert_eq!(summary.modules, 0);
    assert_eq!(summary.removed, ["Pkg.A", "Pkg.B"]);
    assert_eq!(
        fs::read_to_string(&removed).expect("written"),
        "Pkg.A\nPkg.B\n"
    );
    covered.extend(fired);

    // A ledger naming the same module twice: the first entry answers, as
    // `Array.prototype.find` and `new Map(...)` both do.
    let mut twice = repo.build(&[], &Algorithm::sha256(), URL);
    let mut first = twice.modules[0].clone();
    first.hash = "different".to_owned();
    twice.modules.insert(0, first);
    let path = repo.dir.path.join("twice.json");
    fs::write(&path, twice.to_json()).expect("writable");
    let (result, fired) = run_touch(&TouchOptions {
        ledger: &path,
        module: "Pkg.A",
        out: &path,
    });
    result.expect("the module is there twice");
    let after: Ledger =
        serde_json::from_str(&fs::read_to_string(&path).expect("reads")).expect("parses");
    assert_eq!(after.modules[0].hash, "injected-change:different");
    assert_eq!(
        after.modules[1].hash, twice.modules[1].hash,
        "only the first entry is invalidated"
    );
    covered.extend(fired);

    // `touch` on a module the ledger does not have.
    let (result, fired) = run_touch(&TouchOptions {
        ledger: &ledger_path,
        module: "Pkg.Nowhere",
        out: &repo.dir.path.join("untouched.json"),
    });
    let error = result.expect_err("there is no such module");
    assert_eq!(error.exit_code(), 3);
    assert!(error.to_string().contains("Pkg.Nowhere"), "{error}");
    covered.extend(fired);

    // A check that asks for none of the four files: the sets are still
    // computed, and nothing is written.
    let (result, fired) = run_check(&CheckOptions {
        link_index: None,
        external_links: None,
        ledger: &ledger_path,
        algorithm: None,
        modules: None,
        ir: Some(&repo.ir()),
        source_url: URL,
        concurrency: 1,
        changed_out: None,
        removed_out: None,
        render_all_out: None,
        timings: None,
    });
    assert_eq!(result.expect("the check runs").modules, 2);
    assert_eq!(
        fs::read_dir(&repo.dir.path)
            .expect("reads")
            .filter(|e| e.as_ref().is_ok_and(|e| e.file_name() == "timings.json"))
            .count(),
        0,
        "no timings file was asked for and none was written"
    );
    covered.extend(fired);

    // A build whose list names a module with no olean: refused, because at
    // build time the list and the build tree are supposed to agree.
    let mut names = repo.module_names();
    names.push("Pkg.Ghost".to_owned());
    let out = repo.dir.path.join("refused.json");
    let (result, fired) = run_build(&BuildOptions {
        link_index: None,
        external_links: None,
        modules: &names,
        target: &repo.target(),
        out: &out,
        ir: Some(&repo.ir()),
        source_url: URL,
        algorithm: &Algorithm::sha256(),
        concurrency: 1,
        timings: None,
    });
    let error = result.expect_err("a module with no olean stops the build");
    assert_eq!(error.exit_code(), 3);
    assert!(error.to_string().ends_with("for: Pkg.Ghost"), "{error}");
    assert!(!out.exists(), "a refused build writes no ledger");
    covered.extend(fired);

    // Two changed modules whose names are the UTF-16 pair: the re-extract set
    // is sorted `𝒜` first, which code point order reverses. The set is what
    // the next stage re-extracts, so its order is in a file.
    let astral: String = format!("Pkg.{ASTRAL}");
    let ligature: String = format!("Pkg.{LIGATURE}");
    let repo = FakeRepo::new(
        "astral-modules",
        &[
            FakeModule::one(Box::leak(ligature.clone().into_boxed_str())),
            FakeModule::one(Box::leak(astral.clone().into_boxed_str())),
        ],
    );
    let mut ledger = repo.build(&[], &Algorithm::sha256(), URL);
    for entry in &mut ledger.modules {
        entry.hash = format!("injected-change:{}", entry.hash);
    }
    let path = repo.dir.path.join("astral.json");
    fs::write(&path, ledger.to_json()).expect("writable");
    let changed = repo.dir.path.join("astral-changed.txt");
    let modules = repo.module_names();
    let (result, fired) = run_check(&CheckOptions {
        link_index: None,
        external_links: None,
        ledger: &path,
        algorithm: None,
        modules: Some(&modules),
        ir: Some(&repo.ir()),
        source_url: URL,
        concurrency: 1,
        changed_out: Some(&changed),
        removed_out: None,
        render_all_out: None,
        timings: None,
    });
    let summary = result.expect("the check runs");
    assert_eq!(summary.re_extract, [astral.clone(), ligature.clone()]);
    let mut by_code_point = vec![astral.clone(), ligature.clone()];
    by_code_point.sort();
    assert_eq!(
        by_code_point,
        [ligature.clone(), astral.clone()],
        "the two orders really do disagree about this pair"
    );
    assert_eq!(
        fs::read_to_string(&changed).expect("written"),
        format!("{astral}\n{ligature}\n")
    );
    covered.extend(fired);

    covered
}

// ------------------------------------------------------------ the fake repo

/// A module of a synthetic repository.
struct FakeModule {
    module: &'static str,
    suffixes: &'static [&'static str],
    hashes: bool,
}

impl FakeModule {
    /// The target package's shape: one `.olean`.
    fn one(module: &'static str) -> Self {
        Self {
            module,
            suffixes: &[".olean"],
            hashes: true,
        }
    }

    /// The dependency packages' shape: all three files of Lean's module system.
    fn three(module: &'static str) -> Self {
        Self {
            module,
            suffixes: &[".olean", ".olean.server", ".olean.private"],
            hashes: true,
        }
    }

    fn without_hashes(mut self) -> Self {
        self.hashes = false;
        self
    }
}

/// A repository the ledger can be built over: the two files `extractKey` reads,
/// an IR index, and a `.lake/build/lib/lean` holding exactly the olean files
/// asked for.
struct FakeRepo {
    dir: TempDir,
    modules: Vec<String>,
}

impl FakeRepo {
    fn new(what: &str, modules: &[FakeModule]) -> Self {
        let dir = TempDir::new(what);
        let repo = dir.path.join("repo");
        fs::create_dir_all(&repo).expect("the repository is creatable");
        fs::write(repo.join("lean-toolchain"), "leanprover/lean4:v4.31.0\n").expect("writable");
        fs::write(repo.join("lake-manifest.json"), "{\"version\":\"1.1.0\"}").expect("writable");
        let ir = dir.path.join("ir");
        fs::create_dir_all(&ir).expect("creatable");
        fs::write(
            ir.join("index.json"),
            json!({"schemaVersion": 4, "generator": "lean-doc/experiments/stage4b"}).to_string(),
        )
        .expect("writable");
        for module in modules {
            for suffix in module.suffixes {
                let path = PathBuf::from(format!(
                    "{}/.lake/build/lib/lean/{}{suffix}",
                    repo.display(),
                    module.module.replace('.', "/")
                ));
                fs::create_dir_all(path.parent().expect("a parent")).expect("creatable");
                // Deterministic, and different for every file, so that a hash
                // that mixed two files up would show.
                fs::write(&path, format!("olean {} {suffix}", module.module)).expect("writable");
                if module.hashes {
                    fs::write(
                        format!("{}.hash", path.display()),
                        format!("{:016x}\n", fnv_of(&path)),
                    )
                    .expect("writable");
                }
            }
        }
        Self {
            dir,
            modules: modules.iter().map(|m| m.module.to_owned()).collect(),
        }
    }

    fn target(&self) -> String {
        self.dir.path.join("repo").display().to_string()
    }

    fn lib_dir(&self) -> String {
        format!("{}/.lake/build/lib/lean", self.target())
    }

    fn ir(&self) -> PathBuf {
        self.dir.path.join("ir")
    }

    fn module_names(&self) -> Vec<String> {
        self.modules.clone()
    }

    /// Builds a ledger and returns it, for the cases that only care about what
    /// is in it.
    fn build(&self, extra: &[String], algorithm: &Algorithm, source_url: &str) -> Ledger {
        let mut modules = self.module_names();
        modules.extend_from_slice(extra);
        let out = self.dir.path.join("built.json");
        build_ledger(&BuildOptions {
            link_index: None,
            external_links: None,
            modules: &modules,
            target: &self.target(),
            out: &out,
            ir: Some(&self.ir()),
            source_url,
            algorithm,
            concurrency: 1,
            timings: None,
        })
        .expect("the synthetic package builds")
        .ledger
    }

    fn run_build(
        &self,
        algorithm: &Algorithm,
        source_url: &str,
    ) -> (Result<BuildSummary, Error>, BTreeSet<&'static str>) {
        let modules = self.module_names();
        let out = self.dir.path.join("run.json");
        run_build(&BuildOptions {
            link_index: None,
            external_links: None,
            modules: &modules,
            target: &self.target(),
            out: &out,
            ir: Some(&self.ir()),
            source_url,
            algorithm,
            concurrency: 1,
            timings: None,
        })
    }

    fn observed_build(&self, algorithm: &Algorithm) -> BTreeSet<&'static str> {
        let (result, fired) = self.run_build(algorithm, URL);
        result.expect("the synthetic package builds");
        fired
    }

    /// A check against a ledger whose `libDir` was moved out from under the
    /// target — the one way `pathOutsideTarget` is reachable from a command.
    fn observed_check_with_lib_dir(&self, lib_dir: &str) -> BTreeSet<&'static str> {
        let mut ledger = self.build(&[], &Algorithm::sha256(), URL);
        ledger.lib_dir = lib_dir.to_owned();
        ledger.modules.clear();
        let path = self.dir.path.join("moved.json");
        fs::write(&path, ledger.to_json()).expect("writable");
        let out = self.dir.path.join("moved-out.txt");
        let (result, fired) = run_check(&CheckOptions {
            link_index: None,
            external_links: None,
            ledger: &path,
            algorithm: None,
            modules: Some(&[]),
            ir: Some(&self.ir()),
            source_url: URL,
            concurrency: 1,
            changed_out: None,
            removed_out: Some(&out),
            render_all_out: None,
            timings: None,
        });
        result.expect("the check runs");
        fired
    }

    fn check_options<'a>(&'a self, ledger: &'a Path, render_all: &'a Path) -> CheckOptions<'a> {
        CheckOptions {
            link_index: None,
            external_links: None,
            ledger,
            algorithm: None,
            modules: None,
            ir: None,
            source_url: URL,
            concurrency: 1,
            changed_out: None,
            removed_out: None,
            render_all_out: Some(render_all),
            timings: None,
        }
    }
}

fn fnv_of(path: &Path) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in path.display().to_string().as_bytes() {
        hash = (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(what: &str) -> Self {
        use std::sync::atomic::{AtomicU32, Ordering};
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let slug: String = what
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .take(40)
            .collect();
        let path = std::env::temp_dir().join(format!(
            "lean-doc-incr-{}-{}-{slug}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("the temporary directory is creatable");
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
