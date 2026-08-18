//! Milestone **M3-c**: the `impact` and `prune` stages.
//!
//! Three oracles, none of which is this file's own opinion:
//!
//! - **The frozen prototype's own answers.** `tools/impact-reference.sh --impl
//!   ts` ran `experiments/stage5/{impact,prune-pages}.ts` over the base IR, the
//!   432-page reference tree and the whole 438-file site;
//!   `tests/oracle/gen-impact-expected.ts` reduced that tree to
//!   `tests/data/impact-expected.json`, which [`the_corpus_matches_the_prototype`]
//!   compares against scenario by scenario.
//!   **The fixture is a frozen value: HEAD has no way to regenerate it.** The
//!   generator, the prototype and the `--impl ts` half of the harness were
//!   removed with `experiments/` on 2026-08-16 and exist only at tag
//!   `experiments-frozen`.
//! - **A second reader, here.** [`SecondTree`] rebuilds the import and reference
//!   graphs out of `serde_json::Value` and recomputes both closures, so the
//!   counts in every summary are checked against a rule written twice rather
//!   than against the code that produced them.
//! - **The page tree itself.** `prune` is judged by what is left as much as by
//!   what went: [`the_surviving_pages_are_the_bytes_they_were_copied_from`]
//!   compares every survivor against the tree it came from, so deleting one page
//!   too many fails even when the summary says the right number.
//!
//! # Byte equality is not branch coverage (plan §7)
//!
//! Of the [`BRANCHES`] this milestone added:
//!
//! | exercise | reaches |
//! |---|---:|
//! | one changed module, `--mode importers`, one page deleted ([`ONE_RUN`]) | **24** |
//! | everything the 29 scenarios reach ([`HARNESS`]) | **53** |
//! | curated cases only ([`NO_REAL_DATA_REACHES`]) | **16** |
//!
//! Of 69. **Mutation testing found the sixteenth**: thirteen plausible mistakes
//! were applied to the two stages, the whole-corpus byte comparison caught
//! seven, and of the six that escaped it one — `fs::metadata` losing its
//! symlink-following — escaped these tests too, until
//! `pageIsDanglingSymlink` and its case were added【実測 2026-08-12】.
//!
//! The dependency is asserted rather than commented:
//! [`the_curated_cases_cover_what_the_package_does_not`].

#![expect(
    clippy::case_sensitive_file_extension_comparisons,
    reason = "reproduces the prototype's `endsWith`, which is the thing being checked"
)]
#![expect(
    clippy::cast_possible_truncation,
    reason = "counts read back out of the frozen fixture's JSON"
)]

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use litedoc4_incr::{
    Error, ImpactOptions, ImpactRun, Mode, PageRoot, PruneOptions, PruneSummary, impact, page_of,
    prune,
};
use serde_json::{Value, json};

/// The from-scratch IR the milestone is measured against. Read only.
const DEFAULT_BASE_IR: &str = "/private/tmp/lean-doc-relay/w7h/base-ir";
/// The 432 module pages, without the whole-package artifacts. Read only —
/// every scenario copies it first.
const DEFAULT_PAGES: &str = "/private/tmp/lean-doc-relay/m1/ref-pages";
/// The whole site: 432 pages **+ 6 whole-package artifacts** (plan §6). Read
/// only. Three of the six are `.html`, which is what makes the orphan rule
/// interesting.
const DEFAULT_SITE: &str = "/private/tmp/lean-doc-relay/m2/gate/ref-site";

/// U+1D49C MATHEMATICAL SCRIPT CAPITAL A and U+FB00 LATIN SMALL LIGATURE FF:
/// the pair that separates UTF-16 order from code point order. `𝒜` sorts
/// *before* `ﬀ` in UTF-16 and after it by code point (plan §7, U1).
const ASTRAL: &str = "\u{1D49C}";
const LIGATURE: &str = "\u{FB00}";

// --------------------------------------------------------------- the branches

/// Every branch M3-c added, named by an event of the run rather than by a line
/// of the code.
///
/// Each is decided by [`observe`] from the inputs a run was given and the files
/// it produced — never by asking the code under test what it decided. Where the
/// decision needs a rule (which modules import which? what does the closure
/// reach?) the rule is written out a second time in [`SecondTree`].
const BRANCHES: [&str; 69] = [
    // impact: what it was given.
    "impactChangedGiven",
    "impactChangedEmpty",
    "impactCensusWritten",
    "impactCensusOmitted",
    "impactPrintSetWritten",
    "impactPrintSetSkipped",
    "impactPrintSetOmitted",
    "impactPrintSetOmittedWithSelection",
    "impactJsonWritten",
    "impactJsonOmitted",
    // impact: the graph the IR carries.
    "importEdgeOwn",
    "importEdgeForeign",
    "moduleImportsNothingOwn",
    "moduleImportedByNobody",
    "refEdgeOwn",
    "refEdgeSelf",
    "refEdgeForeign",
    "refEdgeRepeated",
    // impact: the closures.
    "closureEmpty",
    "closureOneLevel",
    "closureMultiLevel",
    "closureExcludesEverySeed",
    "closureContainsASeed",
    "closureSeedReachesItself",
    "closureRepeatedSeed",
    // impact: the modes.
    "modeSelf",
    "modeReferrers",
    "modeImporters",
    "modeAll",
    "modeAllWithEmptyChanged",
    "modeUnrecognisedRefused",
    "modeUnrecognisedUnreached",
    // impact: the selection.
    "changedRepeated",
    "changedNotAModule",
    "referrersDirectBelowTransitive",
    "referrersDirectEqualsTransitive",
    "selectionSortAboveBmp",
    "selectedIrBytesRepeatedEntry",
    "selectionEmpty",
    // prune: what it was given.
    "pruneRemoveGiven",
    "pruneRemoveAbsent",
    "pruneIrGiven",
    "pruneIrAbsent",
    "pruneDryRun",
    "pruneWet",
    "pruneJsonWritten",
    "pruneJsonOmitted",
    "removeListEmpty",
    "removeListRepeated",
    // prune: the deletions.
    "pagePresentDeleted",
    "pageAlreadyAbsent",
    "pageIsDanglingSymlink",
    // prune: the orphans.
    "orphanFound",
    "orphanNoneFound",
    "orphanAtTreeRoot",
    "orphanInSubdirectory",
    "orphanReportedNotDeleted",
    "orphansTruncatedInSummary",
    "walkDescendsDirectory",
    "walkSkipsNonHtml",
    "walkSkipsSymlinkedDirectory",
    // prune: the empty directories.
    "directoryEmptied",
    "directoryEmptiedByOrphans",
    "directoryCascaded",
    "directoryKept",
    "emptyPassSkippedByDryRun",
    // prune: the guards.
    "pathEscapeRefused",
    "pathOutsideRootRefused",
    "pruneIndexRefusedShape",
];

/// What the commonest single run reaches: one changed module, `--mode
/// importers`, `--print-set` and `--json` written, and one page deleted from a
/// tree with an orphan-free IR.
///
/// Twenty-four of the sixty-nine. Every mode but one, both deletion guards, the
/// whole orphan half and the whole empty-directory cascade are invisible to it —
/// and so is every refusal.
const ONE_RUN: [&str; 24] = [
    "closureMultiLevel",
    "closureOneLevel",
    "closureExcludesEverySeed",
    "directoryKept",
    "impactCensusOmitted",
    "impactChangedGiven",
    "impactJsonWritten",
    "impactPrintSetWritten",
    "importEdgeForeign",
    "importEdgeOwn",
    "modeImporters",
    "moduleImportedByNobody",
    "moduleImportsNothingOwn",
    "pagePresentDeleted",
    "pruneIrAbsent",
    "pruneJsonWritten",
    "pruneRemoveGiven",
    "pruneWet",
    "refEdgeForeign",
    "refEdgeOwn",
    "refEdgeRepeated",
    "refEdgeSelf",
    "referrersDirectBelowTransitive",
    "walkDescendsDirectory",
];

/// Fifty-three of sixty-nine. What the whole harness reaches — the 18 `impact`
/// and 11 `prune` scenarios of
/// `tools/impact-reference.sh`, replayed in process by
/// [`the_corpus_matches_the_prototype`]. **Measured there, not assumed.**
const HARNESS: [&str; 53] = [
    "changedNotAModule",
    "changedRepeated",
    "closureContainsASeed",
    "closureEmpty",
    "closureExcludesEverySeed",
    "closureMultiLevel",
    "closureOneLevel",
    "closureRepeatedSeed",
    "directoryCascaded",
    "directoryEmptied",
    "directoryKept",
    "emptyPassSkippedByDryRun",
    "impactCensusOmitted",
    "impactCensusWritten",
    "impactChangedEmpty",
    "impactChangedGiven",
    "impactJsonOmitted",
    "impactJsonWritten",
    "impactPrintSetOmitted",
    "impactPrintSetSkipped",
    "impactPrintSetWritten",
    "importEdgeForeign",
    "importEdgeOwn",
    "modeAll",
    "modeAllWithEmptyChanged",
    "modeImporters",
    "modeReferrers",
    "modeSelf",
    "modeUnrecognisedRefused",
    "modeUnrecognisedUnreached",
    "moduleImportedByNobody",
    "moduleImportsNothingOwn",
    "orphanAtTreeRoot",
    "orphanFound",
    "orphanNoneFound",
    "orphanReportedNotDeleted",
    "pageAlreadyAbsent",
    "pagePresentDeleted",
    "pruneDryRun",
    "pruneIrAbsent",
    "pruneIrGiven",
    "pruneJsonWritten",
    "pruneRemoveAbsent",
    "pruneRemoveGiven",
    "pruneWet",
    "refEdgeForeign",
    "refEdgeOwn",
    "refEdgeRepeated",
    "refEdgeSelf",
    "referrersDirectBelowTransitive",
    "referrersDirectEqualsTransitive",
    "walkDescendsDirectory",
    "walkSkipsNonHtml",
];

/// The branches **no exercise over the real corpus reaches at all**, whatever
/// the scenario.
///
/// Sixteen of sixty-nine.
///
/// - **Two are the deletion guards** (`pathEscapeRefused`,
///   `pathOutsideRootRefused`). No module name can reach either, because
///   [`page_of`] turns every dot into a separator and a `..` is made of dots —
///   which is the argument, and the guards are the check. `pathEscapeRefused` is
///   counted by hand below, because there is no *run* that reaches it.
/// - **Two are the UTF-16 / code-point traps** (`selectionSortAboveBmp`,
///   `selectionEmpty`). The package has no name above the BMP — 0 of 4,750
///   declaration names, 0 module names【実測 2026-08-12】— and no package with
///   modules can select nothing.
/// - **Four are flags the pipeline always passes** (`pruneJsonOmitted`,
///   `impactPrintSetOmittedWithSelection`, `directoryEmptiedByOrphans`,
///   `orphanInSubdirectory`): `incremental.sh` asks for every output file and
///   never passes `--ir` to `prune`, so the orphan rule's own consequences are
///   off the pipeline's path entirely.
/// - **Four are input shapes only a hand edit produces** (`removeListEmpty`,
///   `removeListRepeated`, `selectedIrBytesRepeatedEntry`,
///   `pruneIndexRefusedShape`).
/// - **Four are corpus shapes the target does not have**: an import cycle
///   (`closureSeedReachesItself` — Lean cannot produce one), more than twenty
///   orphans (`orphansTruncatedInSummary` — the target has three), a symlinked
///   directory in the page tree (`walkSkipsSymlinkedDirectory`), and a page that
///   is a dangling symlink (`pageIsDanglingSymlink` — the only shape that tells
///   `metadata` from `symlink_metadata`, and the one mutation testing caught
///   this file missing).
///
/// **The neighbouring-package trick does not help here**【判断】. M3-a borrowed
/// Mathlib's oleans for a file shape the target lacked; what is missing here is a
/// *graph* shape (a cycle) and a *page tree* shape (a symlink), neither of which
/// any package supplies. So these stay curated.
const NO_REAL_DATA_REACHES: [&str; 16] = [
    "closureSeedReachesItself",
    "directoryEmptiedByOrphans",
    "impactPrintSetOmittedWithSelection",
    "orphanInSubdirectory",
    "orphansTruncatedInSummary",
    "pageIsDanglingSymlink",
    "pathEscapeRefused",
    "pathOutsideRootRefused",
    "pruneIndexRefusedShape",
    "pruneJsonOmitted",
    "removeListEmpty",
    "removeListRepeated",
    "selectedIrBytesRepeatedEntry",
    "selectionEmpty",
    "selectionSortAboveBmp",
    "walkSkipsSymlinkedDirectory",
];

// ------------------------------------------------------------- the observer

/// One run of one command, with everything needed to say what it reached.
enum Run<'a> {
    Impact {
        options: &'a ImpactOptions<'a>,
        /// The IR as a *second* reader sees it.
        tree: &'a SecondTree,
        result: &'a Result<ImpactRun, Error>,
    },
    Prune {
        options: &'a PruneOptions<'a>,
        /// The page tree **before** the run: `prune` deletes, so reading it
        /// afterwards would be reading the answer.
        before: &'a Snapshot,
        result: &'a Result<PruneSummary, Error>,
    },
}

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
        Run::Impact {
            options,
            tree,
            result,
        } => observe_impact(options, tree, result, &mut fire),
        Run::Prune {
            options,
            before,
            result,
        } => observe_prune(options, before, result, &mut fire),
    }
    fired
}

/// An IR tree as this file reads it: the index and every module file, as plain
/// JSON.
///
/// Deliberately **not** `litedoc4_ir`: the observer has to be a second reader,
/// or a bug in the one under test would hide itself here too. Built once per
/// directory ([`SecondTree::open`] is the expensive call in this file) and
/// handed to every run over that tree.
struct SecondTree {
    /// Module names in index order — repeats kept, because the index may have
    /// them and the byte total counts them twice.
    order: Vec<String>,
    /// `index.modules[].bytes`, parallel to `order`.
    bytes: Vec<u64>,
    own: HashSet<String>,
    /// Every import, unfiltered.
    imports: HashMap<String, Vec<String>>,
    /// Every `(defining module, name)` pair, in file order with repeats.
    refs: HashMap<String, Vec<(String, String)>>,
    declarations: HashMap<String, usize>,
}

impl SecondTree {
    fn open(root: &Path) -> Self {
        let index: Value =
            serde_json::from_str(&fs::read_to_string(root.join("index.json")).expect("an index"))
                .expect("the index is JSON");
        let entries = index["modules"].as_array().expect("an array").clone();
        let mut tree = Self {
            order: Vec::new(),
            bytes: Vec::new(),
            own: HashSet::new(),
            imports: HashMap::new(),
            refs: HashMap::new(),
            declarations: HashMap::new(),
        };
        for entry in &entries {
            let module = entry["module"].as_str().expect("a name").to_owned();
            let file = entry["file"].as_str().expect("a path").to_owned();
            tree.order.push(module.clone());
            tree.bytes.push(entry["bytes"].as_u64().unwrap_or(0));
            tree.own.insert(module.clone());
            let body: Value =
                serde_json::from_str(&fs::read_to_string(root.join(&file)).expect("a module file"))
                    .expect("the module file is JSON");
            tree.imports.insert(
                module.clone(),
                body["imports"]
                    .as_array()
                    .expect("an array")
                    .iter()
                    .map(|i| i.as_str().expect("a name").to_owned())
                    .collect(),
            );
            let declarations = body["declarations"].as_array().expect("an array");
            tree.declarations.insert(module.clone(), declarations.len());
            let mut pairs: Vec<(String, String)> = Vec::new();
            for decl in declarations {
                for pair in decl["refs"].as_array().expect("an array") {
                    let pair = pair.as_array().expect("a two-element array");
                    pairs.push((
                        pair[0].as_str().expect("a module").to_owned(),
                        pair[1].as_str().expect("a name").to_owned(),
                    ));
                }
            }
            tree.refs.insert(module, pairs);
        }
        tree
    }

    /// Own-package imports, the rule written a second time.
    fn own_imports(&self, module: &str) -> Vec<&str> {
        self.imports
            .get(module)
            .into_iter()
            .flatten()
            .map(String::as_str)
            .filter(|import| self.own.contains(*import))
            .collect()
    }

    /// Own-package modules this one's printed text names, deduplicated.
    fn named(&self, module: &str) -> BTreeSet<&str> {
        self.refs
            .get(module)
            .into_iter()
            .flatten()
            .map(|(owner, _)| owner.as_str())
            .filter(|owner| self.own.contains(*owner) && *owner != module)
            .collect()
    }

    fn reverse(&self, references: bool) -> HashMap<&str, Vec<&str>> {
        let mut out: HashMap<&str, Vec<&str>> =
            self.own.iter().map(|m| (m.as_str(), Vec::new())).collect();
        for module in &self.own {
            let targets: Vec<&str> = if references {
                self.named(module).into_iter().collect()
            } else {
                self.own_imports(module)
            };
            for target in targets {
                out.get_mut(target).expect("own-package").push(module);
            }
        }
        out
    }

    /// The closure, written out a second time — seeds excluded unless something
    /// leads back to them.
    fn closure<'a>(seeds: &[&str], edges: &HashMap<&'a str, Vec<&'a str>>) -> BTreeSet<&'a str> {
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        let mut stack: Vec<&str> = Vec::new();
        for seed in seeds {
            if let Some((key, _)) = edges.get_key_value(seed) {
                stack.push(key);
            }
        }
        while let Some(current) = stack.pop() {
            for next in edges.get(current).into_iter().flatten() {
                if seen.insert(next) {
                    stack.push(next);
                }
            }
        }
        seen
    }
}

fn observe_impact(
    options: &ImpactOptions<'_>,
    tree: &SecondTree,
    result: &Result<ImpactRun, Error>,
    fire: &mut impl FnMut(&'static str),
) {
    // 1. What it was given.
    if options.changed.is_empty() {
        fire("impactChangedEmpty");
    } else {
        fire("impactChangedGiven");
    }
    fire(if options.census.is_some() {
        "impactCensusWritten"
    } else {
        "impactCensusOmitted"
    });
    fire(if options.json.is_some() {
        "impactJsonWritten"
    } else {
        "impactJsonOmitted"
    });
    match (options.print_set, result) {
        (None, run) => {
            fire("impactPrintSetOmitted");
            // Asked for on every run of the pipeline, so a selection that
            // nobody wanted written is a shape only a curated case produces.
            if matches!(run, Ok(run) if run.summary.is_some()) {
                fire("impactPrintSetOmittedWithSelection");
            }
        }
        (Some(path), Ok(run)) => {
            if run.summary.is_some() {
                assert!(
                    path.exists(),
                    "a selection was made and nothing was written"
                );
                fire("impactPrintSetWritten");
            } else {
                assert!(!path.exists(), "nothing was selected and a file appeared");
                fire("impactPrintSetSkipped");
            }
        }
        (Some(_), Err(_)) => fire("impactPrintSetSkipped"),
    }

    // 2. The graph, decided from the IR and not from the run.
    let mut some_module_imports_nothing_own = false;
    for module in &tree.own {
        let mut own = 0usize;
        for import in tree.imports.get(module).into_iter().flatten() {
            if tree.own.contains(import) {
                own += 1;
                fire("importEdgeOwn");
            } else {
                fire("importEdgeForeign");
            }
        }
        if own == 0 {
            some_module_imports_nothing_own = true;
        }
        let mut seen: HashSet<&str> = HashSet::new();
        for (owner, _) in tree.refs.get(module).into_iter().flatten() {
            if owner == module {
                fire("refEdgeSelf");
            } else if tree.own.contains(owner) {
                fire("refEdgeOwn");
                if !seen.insert(owner.as_str()) {
                    fire("refEdgeRepeated");
                }
            } else {
                fire("refEdgeForeign");
            }
        }
    }
    if some_module_imports_nothing_own {
        fire("moduleImportsNothingOwn");
    }
    let imported_by = tree.reverse(false);
    if imported_by.values().any(Vec::is_empty) {
        fire("moduleImportedByNobody");
    }
    if tree.order.len() != tree.own.len() {
        fire("selectedIrBytesRepeatedEntry");
    }

    // 3. The modes and the refusals.
    match (options.mode, result) {
        (Mode::Unrecognised(_), Err(Error::UnknownMode { .. })) => fire("modeUnrecognisedRefused"),
        (Mode::Unrecognised(_), Ok(run)) => {
            assert!(run.summary.is_none(), "an unrecognised mode selected");
            fire("modeUnrecognisedUnreached");
        }
        (Mode::SelfOnly, _) => fire("modeSelf"),
        (Mode::Referrers, _) => fire("modeReferrers"),
        (Mode::Importers, _) => fire("modeImporters"),
        (Mode::All, _) => {
            fire("modeAll");
            if options.changed.is_empty() {
                fire("modeAllWithEmptyChanged");
            }
        }
        (Mode::Unrecognised(_), Err(_)) => {}
    }
    if let Err(Error::NotAModule { .. }) = result {
        fire("changedNotAModule");
        assert!(
            options
                .changed
                .iter()
                .any(|module| !tree.own.contains(module)),
            "a name the IR has was refused"
        );
    }
    let mut seen: HashSet<&str> = HashSet::new();
    if options
        .changed
        .iter()
        .any(|module| !seen.insert(module.as_str()))
    {
        fire("changedRepeated");
        fire("closureRepeatedSeed");
    }

    // 4. The selection, held against the rule written out again above.
    let Ok(run) = result else { return };
    let Some(summary) = &run.summary else { return };
    let seeds: Vec<&str> = options.changed.iter().map(String::as_str).collect();
    let referred_by = tree.reverse(true);
    let importers = SecondTree::closure(&seeds, &imported_by);
    let referrers = SecondTree::closure(&seeds, &referred_by);
    let mut direct: BTreeSet<&str> = BTreeSet::new();
    for seed in &seeds {
        for referrer in referred_by.get(seed).into_iter().flatten() {
            direct.insert(referrer);
        }
    }
    assert_eq!(summary.importers_transitive, importers.len());
    assert_eq!(summary.referrers_transitive, referrers.len());
    assert_eq!(summary.referrers_direct, direct.len());
    assert_eq!(summary.own_modules, tree.own.len());
    assert_eq!(
        summary.self_modules,
        seeds.iter().copied().collect::<BTreeSet<_>>().len()
    );

    if importers.is_empty() {
        fire("closureEmpty");
    } else {
        let one_hop: BTreeSet<&str> = seeds
            .iter()
            .flat_map(|seed| imported_by.get(seed).into_iter().flatten().copied())
            .collect();
        if !one_hop.is_empty() {
            fire("closureOneLevel");
        }
        if importers.iter().any(|module| !one_hop.contains(module)) {
            fire("closureMultiLevel");
        }
    }
    // **Not** "a cycle": with several seeds, one of them importing another is
    // enough. The cycle is the branch below, and it needs one seed to reach
    // itself on its own.
    if seeds.iter().any(|seed| importers.contains(seed)) {
        fire("closureContainsASeed");
    } else if !seeds.is_empty() {
        fire("closureExcludesEverySeed");
    }
    if seeds
        .iter()
        .any(|seed| SecondTree::closure(&[*seed], &imported_by).contains(seed))
    {
        fire("closureSeedReachesItself");
    }
    fire(if summary.referrers_direct < summary.referrers_transitive {
        "referrersDirectBelowTransitive"
    } else {
        "referrersDirectEqualsTransitive"
    });
    if summary.selected.is_empty() {
        fire("selectionEmpty");
    }
    if summary
        .selected
        .iter()
        .any(|name| name.chars().any(|c| c as u32 > 0xFFFF))
    {
        fire("selectionSortAboveBmp");
    }

    // The selection itself, recomputed.
    let expected: BTreeSet<String> = match options.mode {
        Mode::SelfOnly => seeds.iter().map(|m| (*m).to_owned()).collect(),
        Mode::Referrers => seeds
            .iter()
            .copied()
            .chain(direct.iter().copied())
            .map(str::to_owned)
            .collect(),
        Mode::Importers => seeds
            .iter()
            .copied()
            .chain(importers.iter().copied())
            .map(str::to_owned)
            .collect(),
        Mode::All => tree.own.iter().cloned().collect(),
        Mode::Unrecognised(_) => unreachable!("an unrecognised mode selected nothing"),
    };
    assert_eq!(
        summary.selected.iter().cloned().collect::<BTreeSet<_>>(),
        expected,
        "the selection is not what the rule written out again says"
    );
    assert_eq!(
        summary.selected_ir_bytes,
        tree.order
            .iter()
            .zip(&tree.bytes)
            .filter(|(module, _)| expected.contains(*module))
            .map(|(_, bytes)| *bytes)
            .sum::<u64>()
    );
    assert_eq!(
        summary.selected_declarations,
        expected
            .iter()
            .map(|module| tree.declarations.get(module).copied().unwrap_or(0))
            .sum::<usize>()
    );
}

/// A page tree as a listing: relative file and directory paths.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Snapshot {
    files: BTreeSet<String>,
    dirs: BTreeSet<String>,
    /// Entries whose `file_type()` says symlink — neither a directory to walk
    /// into nor a file to keep.
    symlinks: BTreeSet<String>,
    /// Symlinks whose target is not there. **The one shape that separates
    /// `metadata` from `symlink_metadata`**: for a live link both succeed, so
    /// only a dangling one says which call was made.
    dangling: BTreeSet<String>,
}

impl Snapshot {
    fn take(root: &Path) -> Self {
        let mut out = Self::default();
        fn walk(root: &Path, relative: &str, out: &mut Snapshot) {
            let dir = if relative.is_empty() {
                root.to_owned()
            } else {
                root.join(relative)
            };
            let Ok(listing) = fs::read_dir(&dir) else {
                return;
            };
            for entry in listing.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                let child = if relative.is_empty() {
                    name
                } else {
                    format!("{relative}/{name}")
                };
                let kind = entry.file_type().expect("a file type");
                if kind.is_symlink() {
                    if fs::metadata(dir.join(entry.file_name())).is_err() {
                        out.dangling.insert(child.clone());
                    }
                    out.symlinks.insert(child);
                } else if kind.is_dir() {
                    out.dirs.insert(child.clone());
                    walk(root, &child, out);
                } else {
                    out.files.insert(child);
                }
            }
        }
        walk(root, "", &mut out);
        out
    }
}

/// The page path as a directory listing shows it. `${root}/${page}` is a
/// concatenation, so a module name that starts with a separator makes `//` —
/// which the filesystem collapses and a relative listing never shows.
fn as_listed(page: &str) -> String {
    page.trim_start_matches('/').to_owned()
}

fn observe_prune(
    options: &PruneOptions<'_>,
    before: &Snapshot,
    result: &Result<PruneSummary, Error>,
    fire: &mut impl FnMut(&'static str),
) {
    fire(if options.remove.is_some() {
        "pruneRemoveGiven"
    } else {
        "pruneRemoveAbsent"
    });
    fire(if options.ir.is_some() {
        "pruneIrGiven"
    } else {
        "pruneIrAbsent"
    });
    fire(if options.dry_run {
        "pruneDryRun"
    } else {
        "pruneWet"
    });
    fire(if options.json.is_some() {
        "pruneJsonWritten"
    } else {
        "pruneJsonOmitted"
    });
    if let Some(path) = options.remove {
        let lines: Vec<String> = fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(str::to_owned)
            .collect();
        if lines.is_empty() {
            fire("removeListEmpty");
        }
        let mut seen: HashSet<&str> = HashSet::new();
        if lines.iter().any(|module| !seen.insert(module.as_str())) {
            fire("removeListRepeated");
        }
        if lines
            .iter()
            .any(|module| before.dangling.contains(&as_listed(&page_of(module))))
        {
            fire("pageIsDanglingSymlink");
        }
    }
    if before.dirs.iter().any(|dir| !dir.contains('/')) {
        fire("walkDescendsDirectory");
    }
    if before.files.iter().any(|file| !file.ends_with(".html")) {
        fire("walkSkipsNonHtml");
    }
    if !before.symlinks.is_empty() {
        fire("walkSkipsSymlinkedDirectory");
    }

    match result {
        Err(Error::OutsidePageRoot { path, .. }) => {
            if path.is_relative() {
                fire("pathEscapeRefused");
            } else {
                fire("pathOutsideRootRefused");
            }
            return;
        }
        Err(Error::IndexShape { .. }) => {
            fire("pruneIndexRefusedShape");
            return;
        }
        Err(_) => return,
        Ok(_) => {}
    }
    let Ok(summary) = result else { unreachable!() };

    if summary.deleted.is_empty() {
        // Nothing was there to delete; only meaningful when something was asked
        // for, which `pageAlreadyAbsent` below covers.
    } else {
        fire("pagePresentDeleted");
        // The rule, written again: every deleted module's page was in the tree.
        for module in &summary.deleted {
            assert!(
                before.files.contains(&as_listed(&page_of(module))),
                "{module} was reported deleted and its page was never there"
            );
        }
    }
    if !summary.already_absent.is_empty() {
        fire("pageAlreadyAbsent");
        for module in &summary.already_absent {
            // …or an earlier line of the same list already took it: the second
            // mention of a repeated module finds the page gone.
            assert!(
                !before.files.contains(&as_listed(&page_of(module)))
                    || summary.deleted.contains(module),
                "{module} was reported absent and its page was there"
            );
        }
    }

    if options.ir.is_some() {
        if summary.orphans.is_empty() {
            fire("orphanNoneFound");
        } else {
            fire("orphanFound");
            if summary.orphans.iter().any(|page| !page.contains('/')) {
                fire("orphanAtTreeRoot");
            }
            if summary.orphans.iter().any(|page| page.contains('/')) {
                fire("orphanInSubdirectory");
            }
            if summary.orphans.len() > 20 {
                fire("orphansTruncatedInSummary");
            }
            if options.dry_run {
                fire("orphanReportedNotDeleted");
            }
        }
    }

    if options.dry_run {
        fire("emptyPassSkippedByDryRun");
        assert!(summary.emptied.is_empty());
    } else if summary.emptied.is_empty() {
        fire("directoryKept");
    } else {
        fire("directoryEmptied");
        if options.remove.is_none() {
            // Only the orphan pass can have emptied it. The pipeline never
            // passes `--ir`, so this is a curated shape.
            fire("directoryEmptiedByOrphans");
        }
        if summary.emptied.iter().any(|dir| {
            summary
                .emptied
                .iter()
                .any(|other| other != dir && other.starts_with(&format!("{dir}/")))
        }) {
            fire("directoryCascaded");
        }
        if before.dirs.len() > summary.emptied.len() {
            fire("directoryKept");
        }
    }
}

// ------------------------------------------------------------ running things

fn run_impact(
    options: &ImpactOptions<'_>,
    tree: &SecondTree,
) -> (Result<ImpactRun, Error>, BTreeSet<&'static str>) {
    let result = impact(options);
    let fired = observe(&Run::Impact {
        options,
        tree,
        result: &result,
    });
    (result, fired)
}

fn run_prune(options: &PruneOptions<'_>) -> (Result<PruneSummary, Error>, BTreeSet<&'static str>) {
    let before = Snapshot::take(options.pages);
    let result = prune(options);
    let fired = observe(&Run::Prune {
        options,
        before: &before,
        result: &result,
    });
    (result, fired)
}

// ------------------------------------------------------------ the fixture

struct Expected {
    value: Value,
    seen: std::cell::RefCell<BTreeSet<String>>,
}

fn expected() -> Expected {
    Expected {
        value: serde_json::from_str(include_str!("data/impact-expected.json"))
            .expect("the fixture is JSON"),
        seen: std::cell::RefCell::new(BTreeSet::new()),
    }
}

impl Expected {
    fn scenario(&self, stage: &str, name: &str) -> &Value {
        self.seen.borrow_mut().insert(format!("{stage}/{name}"));
        self.value[stage]
            .get(name)
            .unwrap_or_else(|| panic!("{stage}/{name} is not in the fixture; regenerate it"))
    }
}

fn fnv1a64(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash = (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// The base IR, the reference pages and the reference site, or a panic naming
/// what to set.
///
/// The only caller is `#[ignore]`d, so reaching this function at all means the
/// corpus gate asked for the test by name. Returning "not here, never mind"
/// there would be a green result for a comparison that never ran.
fn corpus() -> (PathBuf, PathBuf, PathBuf) {
    let base =
        PathBuf::from(std::env::var("LITEDOC4_BASE_IR").unwrap_or_else(|_| DEFAULT_BASE_IR.into()));
    let pages =
        PathBuf::from(std::env::var("LITEDOC4_PAGES").unwrap_or_else(|_| DEFAULT_PAGES.into()));
    let site =
        PathBuf::from(std::env::var("LITEDOC4_SITE").unwrap_or_else(|_| DEFAULT_SITE.into()));
    // Presence is counted in files, not directories. These trees live under
    // `/private/tmp`, which is swept: an emptied `ref-pages` left its directory
    // behind, `is_dir()` said yes, and the run reported the corpus's own pages
    // as "already absent" — a green-looking scenario failing for an
    // environmental reason.
    for (var, dir) in [
        ("LITEDOC4_BASE_IR", &base),
        ("LITEDOC4_PAGES", &pages),
        ("LITEDOC4_SITE", &site),
    ] {
        assert!(
            file_count(dir) != 0,
            "{} is empty or missing: set {var}, or run this test through tools/corpus-gate.sh, \
             which is the only thing that should be asking for it",
            dir.display()
        );
    }
    (base, pages, site)
}

/// Regular files under `dir`, recursively. Zero for a missing directory.
fn file_count(dir: &Path) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| match entry.file_type() {
            Ok(kind) if kind.is_dir() => file_count(&entry.path()),
            Ok(kind) if kind.is_file() => 1,
            _ => 0,
        })
        .sum()
}

// ------------------------------------------------------- the corpus test

/// Every scenario `tools/impact-reference.sh` defines, replayed in process and
/// compared with the frozen prototype's answers.
#[test]
#[ignore = "corpus: needs LITEDOC4_BASE_IR + LITEDOC4_PAGES + LITEDOC4_SITE (tools/corpus-gate.sh)"]
fn the_corpus_matches_the_prototype() {
    let (base_ir, pages_src, site_src) = corpus();
    let e = expected();
    assert_eq!(
        e.value["corpus"]["modules"].as_u64(),
        Some(432),
        "the fixture was taken against a different package"
    );
    let tree = SecondTree::open(&base_ir);
    assert_eq!(tree.own.len(), 432);
    let work = TempDir::new("impact-corpus");
    let mut covered: BTreeSet<&'static str> = BTreeSet::new();

    let hub = "InformationTheory.Shannon.Bridge".to_owned();
    let leaf = "InformationTheory.Meta.EntryPoint".to_owned();
    let other = "InformationTheory.Polymatroid.Basic".to_owned();
    let ghost = "InformationTheory.Nonexistent.Module".to_owned();

    // The 18 `impact` scenarios, in the harness's order.
    let one = vec![hub.clone()];
    let leaf_only = vec![leaf.clone()];
    let ghost_only = vec![ghost.clone()];
    let none: Vec<String> = Vec::new();
    // `--changed OTHER --changed HUB` then the file's `HUB / LEAF / HUB`.
    let multi = vec![
        other.clone(),
        hub.clone(),
        hub.clone(),
        leaf.clone(),
        hub.clone(),
    ];
    let dup = vec![hub.clone(), hub];
    let unrecognised = Mode::parse("nonsense");
    let m_self = Mode::SelfOnly;
    let m_referrers = Mode::Referrers;
    let m_importers = Mode::Importers;
    let m_all = Mode::All;
    /// One `impact` scenario: the name the fixture files it under, the changed
    /// set, the mode, and which of the three output files it asks for.
    struct Scenario<'a> {
        name: &'a str,
        changed: &'a [String],
        mode: &'a Mode,
        census: bool,
        print_set: bool,
        json: bool,
    }
    let scenarios: Vec<Scenario<'_>> = vec![
        Scenario {
            name: "census",
            changed: &none,
            mode: &m_importers,
            census: true,
            print_set: false,
            json: false,
        },
        Scenario {
            name: "self",
            changed: &one,
            mode: &m_self,
            census: false,
            print_set: true,
            json: true,
        },
        Scenario {
            name: "referrers",
            changed: &one,
            mode: &m_referrers,
            census: false,
            print_set: true,
            json: true,
        },
        Scenario {
            name: "importers",
            changed: &one,
            mode: &m_importers,
            census: false,
            print_set: true,
            json: true,
        },
        Scenario {
            name: "default-mode",
            changed: &one,
            mode: &m_importers,
            census: false,
            print_set: true,
            json: true,
        },
        Scenario {
            name: "all-empty",
            changed: &none,
            mode: &m_all,
            census: false,
            print_set: true,
            json: true,
        },
        Scenario {
            name: "all-changed",
            changed: &one,
            mode: &m_all,
            census: false,
            print_set: true,
            json: true,
        },
        Scenario {
            name: "leaf",
            changed: &leaf_only,
            mode: &m_importers,
            census: false,
            print_set: true,
            json: true,
        },
        Scenario {
            name: "multi",
            changed: &multi,
            mode: &m_referrers,
            census: false,
            print_set: true,
            json: true,
        },
        Scenario {
            name: "file-only",
            changed: &leaf_only,
            mode: &m_importers,
            census: false,
            print_set: true,
            json: true,
        },
        Scenario {
            name: "empty-file",
            changed: &none,
            mode: &m_importers,
            census: false,
            print_set: true,
            json: true,
        },
        Scenario {
            name: "empty-file-all",
            changed: &none,
            mode: &m_all,
            census: false,
            print_set: true,
            json: true,
        },
        Scenario {
            name: "unknown-mode-quiet",
            changed: &none,
            mode: &unrecognised,
            census: false,
            print_set: true,
            json: false,
        },
        Scenario {
            name: "unknown-mode",
            changed: &one,
            mode: &unrecognised,
            census: false,
            print_set: true,
            json: false,
        },
        Scenario {
            name: "not-a-module",
            changed: &ghost_only,
            mode: &m_importers,
            census: false,
            print_set: true,
            json: true,
        },
        Scenario {
            name: "census-and-set",
            changed: &leaf_only,
            mode: &m_referrers,
            census: true,
            print_set: true,
            json: true,
        },
        Scenario {
            name: "dup-changed",
            changed: &dup,
            mode: &m_self,
            census: false,
            print_set: true,
            json: true,
        },
    ];
    for Scenario {
        name,
        changed,
        mode,
        census,
        print_set,
        json,
    } in scenarios
    {
        let dir = work.path.join(name);
        fs::create_dir_all(&dir).expect("creatable");
        let census_path = dir.join("census.tsv");
        let set_path = dir.join("set.txt");
        let json_path = dir.join("impact.json");
        let options = ImpactOptions {
            ir: &base_ir,
            changed,
            mode,
            census: census.then_some(census_path.as_path()),
            print_set: print_set.then_some(set_path.as_path()),
            json: json.then_some(json_path.as_path()),
        };
        let (result, fired) = run_impact(&options, &tree);
        covered.extend(fired);
        let want = e.scenario("impact", name);
        match &result {
            Ok(run) => {
                assert_eq!(
                    want["status"].as_u64(),
                    Some(0),
                    "{name} was expected to fail"
                );
                check_file(
                    want,
                    "census",
                    census.then_some(census_path.as_path()),
                    name,
                );
                check_file(
                    want,
                    "printSet",
                    print_set.then_some(set_path.as_path()),
                    name,
                );
                match (&run.summary, want["summary"].as_object()) {
                    (None, None) => {}
                    (Some(summary), Some(_)) => {
                        let produced: Value =
                            serde_json::from_str(&summary.to_json()).expect("the summary is JSON");
                        let mut masked = produced.clone();
                        masked["ir"] = json!("<IR>");
                        assert_eq!(&masked, &want["summary"], "{name}: the summary differs");
                        // The `--json` file is the same text plus a newline.
                        assert_eq!(
                            fs::read_to_string(&json_path).expect("the summary was written"),
                            summary.to_json() + "\n",
                            "{name}: --json is not what was printed"
                        );
                    }
                    (summary, _) => panic!(
                        "{name}: summary {} in one and not the other",
                        summary.is_some()
                    ),
                }
            }
            Err(error) => {
                assert_eq!(
                    u64::from(error.exit_code()),
                    want["status"].as_u64().expect("a status"),
                    "{name}: {error}"
                );
                assert!(want["complained"].as_bool().unwrap_or(false));
            }
        }
    }

    // The 9 `prune` scenarios that touch a page tree.
    let fixtures = TempDir::new("impact-lists");
    let write_list = |name: &str, modules: &[&str]| -> PathBuf {
        let path = fixtures.path.join(name);
        fs::write(
            &path,
            modules.iter().flat_map(|m| [*m, "\n"]).collect::<String>(),
        )
        .expect("writable");
        path
    };
    let leaf_and_ghost = write_list("remove-leaf-and-ghost.txt", &[&leaf, &ghost]);
    let other_only = write_list("remove-other.txt", &[&other]);
    let ghost_list = write_list("remove-ghost-only.txt", &[&ghost]);
    let cascade = write_list(
        "remove-cascade.txt",
        &[
            "InformationTheory.Shannon.ConditionalMethodOfTypes.Mass.Concentration",
            "InformationTheory.Shannon.ConditionalMethodOfTypes.Mass.SliceMass",
            "InformationTheory.Shannon.ConditionalMethodOfTypes.Core",
            "InformationTheory.Shannon.ConditionalMethodOfTypes.Mass",
        ],
    );

    let prune_scenarios: Vec<(&str, &Path, Option<&Path>, bool, bool)> = vec![
        // name, source tree, remove list, --ir, --dry-run
        ("dry-remove", &pages_src, Some(&leaf_and_ghost), false, true),
        (
            "real-remove",
            &pages_src,
            Some(&leaf_and_ghost),
            false,
            false,
        ),
        ("orphans-site", &site_src, Some(&leaf_and_ghost), true, true),
        ("orphans-pages", &pages_src, Some(&other_only), true, false),
        ("orphans-only", &site_src, None, true, false),
        ("cascade", &pages_src, Some(&cascade), false, false),
        (
            "already-absent",
            &pages_src,
            Some(&ghost_list),
            false,
            false,
        ),
    ];
    let mut survivors_checked = 0usize;
    for (name, source, remove, with_ir, dry_run) in prune_scenarios {
        let dir = work.path.join(name);
        fs::create_dir_all(&dir).expect("creatable");
        let tree_path = dir.join("pages");
        copy_tree(source, &tree_path);
        let json_path = dir.join("prune.json");
        let (result, fired) = run_prune(&PruneOptions {
            pages: &tree_path,
            remove,
            ir: with_ir.then_some(base_ir.as_path()),
            dry_run,
            json: Some(&json_path),
        });
        covered.extend(fired);
        let summary = result.expect("the prune runs");
        let want = e.scenario("prune", name);
        check_prune(want, &summary, &json_path, &tree_path, name);
        survivors_checked += compare_survivors(source, &tree_path, name);
    }

    // The two-run scenario: the same deletion twice on one tree.
    let rerun = work.path.join("rerun");
    fs::create_dir_all(&rerun).expect("creatable");
    let rerun_pages = rerun.join("pages");
    copy_tree(&pages_src, &rerun_pages);
    for name in ["rerun-1", "rerun-2"] {
        let json_path = rerun.join(format!("{name}.json"));
        let (result, fired) = run_prune(&PruneOptions {
            pages: &rerun_pages,
            remove: Some(&other_only),
            ir: None,
            dry_run: false,
            json: Some(&json_path),
        });
        covered.extend(fired);
        let summary = result.expect("the prune runs");
        check_prune(
            e.scenario("prune", name),
            &summary,
            &json_path,
            &rerun_pages,
            name,
        );
    }
    survivors_checked += compare_survivors(&pages_src, &rerun_pages, "rerun");

    // The two usage refusals are the CLI's, not the library's: the library is
    // never called without a page tree. Recorded in the fixture as the exit
    // codes the shell harness compares.
    for name in ["no-list", "no-pages"] {
        assert_eq!(
            e.scenario("prune", name)["status"].as_u64(),
            Some(2),
            "{name} is a usage refusal"
        );
    }

    // Every scenario of the fixture, and no more.
    let compared = e.seen.borrow().clone();
    let mut in_fixture: BTreeSet<String> = BTreeSet::new();
    for stage in ["impact", "prune"] {
        for name in e.value[stage].as_object().expect("a map").keys() {
            in_fixture.insert(format!("{stage}/{name}"));
        }
    }
    // `default-mode` and `no-ir` are the CLI's own: one is the default `--mode`
    // resolved before the library is called, the other a missing `--ir`.
    let cli_only: BTreeSet<String> = ["impact/no-ir".to_owned()].into_iter().collect();
    assert_eq!(
        &(&in_fixture - &compared),
        &cli_only,
        "the fixture and this test disagree about which scenarios exist"
    );
    assert_eq!(compared.len(), 28);
    assert_eq!(survivors_checked, 3_458);

    // How [`HARNESS`] was written: measured, then transcribed.
    // `LITEDOC4_DUMP_BRANCHES=1 cargo test -p litedoc4-incr --test impact` prints
    // it again when a scenario is added, so the constant stays a record of a
    // measurement rather than a guess that has to be reverse-engineered.
    if std::env::var("LITEDOC4_DUMP_BRANCHES").is_ok() {
        eprintln!("HARNESS = {covered:#?}");
    }
    assert_eq!(
        covered,
        BTreeSet::from(HARNESS),
        "which branches the harness reaches has changed"
    );
}

fn check_file(want: &Value, key: &str, path: Option<&Path>, name: &str) {
    match (path, want[key].as_object()) {
        (None, None) => {}
        (Some(path), Some(_)) => {
            let body = fs::read(path).unwrap_or_default();
            assert_eq!(
                body.len() as u64,
                want[key]["bytes"].as_u64().expect("a size"),
                "{name}/{key}: size"
            );
            assert_eq!(
                fnv1a64(&body),
                want[key]["fnv1a64"].as_str().expect("a digest"),
                "{name}/{key}: digest"
            );
        }
        (Some(path), None) => assert!(
            !path.exists(),
            "{name}/{key}: written, and the prototype wrote nothing"
        ),
        (None, Some(_)) => panic!("{name}/{key}: the prototype wrote one and this run did not ask"),
    }
}

fn check_prune(want: &Value, summary: &PruneSummary, json: &Path, tree: &Path, name: &str) {
    let produced: Value =
        serde_json::from_str(&fs::read_to_string(json).expect("the summary was written"))
            .expect("the summary is JSON");
    let mut masked = serde_json::Map::new();
    for (key, value) in produced.as_object().expect("an object") {
        if key.ends_with("Seconds") {
            continue;
        }
        masked.insert(
            key.clone(),
            if key == "pages" {
                json!("<OUT>")
            } else {
                value.clone()
            },
        );
    }
    assert_eq!(
        &Value::Object(masked),
        &want["summary"],
        "{name}: the summary differs from the prototype's"
    );
    // **What is left, counted independently of what went.** A stage that deletes
    // is as wrong when it takes too much as when it takes too little, and the
    // summary above only ever names what it took.
    let after = Snapshot::take(tree);
    assert_eq!(
        after.files.len() as u64,
        want["files"]["count"].as_u64().expect("a count"),
        "{name}: {} files survived, the prototype left {}",
        after.files.len(),
        want["files"]["count"]
    );
    assert_eq!(
        after.dirs.len() as u64 + 1,
        want["dirs"]["count"].as_u64().expect("a count"),
        "{name}: directory count (the harness counts the root, this does not)"
    );
    let listing: String = after
        .files
        .iter()
        .flat_map(|file| [file.as_str(), "\n"])
        .collect();
    assert_eq!(
        fnv1a64(listing.as_bytes()),
        want["files"]["fnv1a64"].as_str().expect("a digest"),
        "{name}: the survivors are not the prototype's survivors"
    );
    assert_eq!(
        summary.orphans.len(),
        want["summary"]["orphans"].as_u64().expect("a count") as usize
    );
}

/// Every file still in the pruned tree is byte for byte the one it was copied
/// from. Stronger than a digest of the listing: it says the stage deleted and
/// did not rewrite.
fn compare_survivors(source: &Path, tree: &Path, name: &str) -> usize {
    let after = Snapshot::take(tree);
    for file in &after.files {
        assert_eq!(
            fs::read(tree.join(file)).expect("a survivor"),
            fs::read(source.join(file)).expect("its source"),
            "{name}: {file} is not the bytes it was copied from"
        );
    }
    after.files.len()
}

// ------------------------------------------------------- the curated cases

/// The dependency this milestone's coverage rests on, stated so that it fails
/// when it stops being true (plan §7: 全件バイト一致は分岐被覆の証明ではない).
///
/// Four claims, all counted rather than believed:
///
/// 1. The commonest single run reaches [`ONE_RUN`] — 21 of the 64.
/// 2. The whole harness reaches [`HARNESS`] — 44 of 64, measured in
///    [`the_corpus_matches_the_prototype`].
/// 3. The other 19 are reachable only by a written-down case, so every one of
///    them is one.
/// 4. Everything together is all 64.
#[test]
fn the_curated_cases_cover_what_the_package_does_not() {
    // (1) One changed module, the default mode, one page deleted.
    let repo = FakeIr::target_shaped("one-run");
    let fired = repo.one_run();
    if std::env::var("LITEDOC4_DUMP_BRANCHES").is_ok() {
        eprintln!("ONE_RUN = {fired:#?}");
    }
    assert_eq!(
        fired,
        BTreeSet::from(ONE_RUN),
        "the commonest single run reaches a different set of branches than it did"
    );
    assert_eq!(fired.len(), 24);

    // (2) and (3): what the harness leaves for the curated cases.
    let harness = BTreeSet::from(HARNESS);
    assert_eq!(harness.len(), 53);
    let only_curated: BTreeSet<&str> = BRANCHES
        .iter()
        .copied()
        .filter(|branch| !harness.contains(branch))
        .collect();
    if std::env::var("LITEDOC4_DUMP_BRANCHES").is_ok() {
        eprintln!("NO_REAL_DATA_REACHES = {only_curated:#?}");
    }
    assert_eq!(
        only_curated,
        BTreeSet::from(NO_REAL_DATA_REACHES),
        "which branches no real-data exercise reaches has changed"
    );

    // (4) Everything, together.
    let curated = {
        let mut curated = curated_impact_branches();
        curated.extend(curated_prune_branches());
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

/// The two sorts, the shapes of an index only a hand edit produces, and the
/// graphs the target does not have.
fn curated_impact_branches() -> BTreeSet<&'static str> {
    let mut covered = BTreeSet::new();

    // A name above the BMP: `𝒜` sorts before `ﬀ` in UTF-16 and after it by code
    // point, so the selection's order says which comparator ran.
    let repo = FakeIr::new("astral");
    let astral = format!("Pkg.{ASTRAL}Mod");
    let ligature = format!("Pkg.{LIGATURE}Mod");
    repo.module("Pkg.Root", &[], &[]);
    repo.module(&astral, &["Pkg.Root"], &[]);
    repo.module(&ligature, &["Pkg.Root"], &[]);
    repo.finish();
    let tree = SecondTree::open(&repo.dir.path);
    let changed = vec!["Pkg.Root".to_owned()];
    let set = repo.dir.path.join("set.txt");
    let (result, fired) = run_impact(
        &ImpactOptions {
            ir: &repo.dir.path,
            changed: &changed,
            mode: &Mode::Importers,
            census: None,
            print_set: Some(&set),
            json: None,
        },
        &tree,
    );
    let summary = result
        .expect("the impact runs")
        .summary
        .expect("a selection");
    assert_eq!(
        summary.selected,
        vec!["Pkg.Root".to_owned(), astral, ligature],
        "the selection is not in UTF-16 order (plan §7, U1)"
    );
    // The same list by code point would put the ligature first.
    let mut by_code_point = summary.selected.clone();
    by_code_point.sort();
    assert_ne!(by_code_point, summary.selected, "the two orders agree here");
    covered.extend(fired);

    // An import cycle: the seed is in its own closure. Lean cannot produce one,
    // and the closure's shape depends on it (the seed is otherwise absent).
    let repo = FakeIr::new("cycle");
    repo.module("Pkg.A", &["Pkg.B"], &[]);
    repo.module("Pkg.B", &["Pkg.A"], &[]);
    repo.finish();
    let tree = SecondTree::open(&repo.dir.path);
    let changed = vec!["Pkg.A".to_owned()];
    let (result, fired) = run_impact(
        &ImpactOptions {
            ir: &repo.dir.path,
            changed: &changed,
            mode: &Mode::Importers,
            census: None,
            print_set: None,
            json: None,
        },
        &tree,
    );
    let summary = result
        .expect("the impact runs")
        .summary
        .expect("a selection");
    assert_eq!(summary.importers_transitive, 2, "the cycle brings A back");
    covered.extend(fired);

    // A package with no modules at all: `--mode all` selects nothing, and the
    // `--print-set` is **one blank line** rather than an empty file — the
    // prototype writes `list.join("\n") + "\n"`. Unreachable with any non-empty
    // IR, and harmless because `--only-from` drops blank lines.
    let repo = FakeIr::new("empty");
    repo.finish();
    let tree = SecondTree::open(&repo.dir.path);
    let set = repo.dir.path.join("set.txt");
    let (result, fired) = run_impact(
        &ImpactOptions {
            ir: &repo.dir.path,
            changed: &[],
            mode: &Mode::All,
            census: None,
            print_set: Some(&set),
            json: None,
        },
        &tree,
    );
    let summary = result
        .expect("the impact runs")
        .summary
        .expect("a selection");
    assert!(summary.selected.is_empty());
    assert_eq!(fs::read_to_string(&set).expect("written"), "\n");
    covered.extend(fired);

    // Two index entries for one module: the byte total counts both, as the
    // prototype's `filter(...).reduce(...)` over `index.modules` does.
    let repo = FakeIr::new("repeated-entry");
    repo.module("Pkg.A", &[], &[]);
    repo.repeat_index_entry("Pkg.A");
    repo.finish();
    let tree = SecondTree::open(&repo.dir.path);
    let changed = vec!["Pkg.A".to_owned()];
    let (result, fired) = run_impact(
        &ImpactOptions {
            ir: &repo.dir.path,
            changed: &changed,
            mode: &Mode::SelfOnly,
            census: None,
            print_set: None,
            json: None,
        },
        &tree,
    );
    let summary = result
        .expect("the impact runs")
        .summary
        .expect("a selection");
    assert_eq!(summary.own_modules, 1, "the set folds the repeat");
    assert_eq!(
        summary.selected_ir_bytes,
        tree.bytes.iter().sum::<u64>(),
        "the byte total does not"
    );
    covered.extend(fired);

    // Referrers direct == transitive: a package where the reference graph is one
    // hop deep. The target's is not.
    let repo = FakeIr::new("flat-refs");
    repo.module("Pkg.A", &[], &[]);
    repo.module("Pkg.B", &["Pkg.A"], &[("Pkg.A", "Pkg.A.thing")]);
    repo.finish();
    let tree = SecondTree::open(&repo.dir.path);
    let changed = vec!["Pkg.A".to_owned()];
    let (result, fired) = run_impact(
        &ImpactOptions {
            ir: &repo.dir.path,
            changed: &changed,
            mode: &Mode::Referrers,
            census: None,
            print_set: None,
            json: None,
        },
        &tree,
    );
    let summary = result
        .expect("the impact runs")
        .summary
        .expect("a selection");
    assert_eq!(summary.referrers_direct, summary.referrers_transitive);
    covered.extend(fired);

    // `--print-set` not asked for while there **is** a selection: the pipeline
    // always asks, so no scenario over the corpus reaches it.
    let repo = FakeIr::target_shaped("no-print-set");
    let tree = SecondTree::open(&repo.dir.path);
    let changed = vec!["Pkg.Leaf".to_owned()];
    let (result, fired) = run_impact(
        &ImpactOptions {
            ir: &repo.dir.path,
            changed: &changed,
            mode: &Mode::Importers,
            census: None,
            print_set: None,
            json: None,
        },
        &tree,
    );
    result
        .expect("the impact runs")
        .summary
        .expect("a selection");
    covered.extend(fired);

    covered
}

/// The deletion guards, the page-tree shapes the target does not have, and the
/// two flags the pipeline always passes.
fn curated_prune_branches() -> BTreeSet<&'static str> {
    let mut covered = BTreeSet::new();

    // A `--remove` list with a repeat, an empty one, and no `--json`: three
    // shapes the pipeline never produces.
    let work = TempDir::new("prune-lists");
    let pages = work.path.join("pages");
    fs::create_dir_all(pages.join("Pkg")).expect("creatable");
    fs::write(pages.join("Pkg/A.html"), b"a").expect("writable");
    fs::write(pages.join("Pkg/B.html"), b"b").expect("writable");
    let repeated = work.path.join("repeated.txt");
    fs::write(&repeated, "Pkg.A\nPkg.A\n").expect("writable");
    let (result, fired) = run_prune(&PruneOptions {
        pages: &pages,
        remove: Some(&repeated),
        ir: None,
        dry_run: false,
        json: None,
    });
    let summary = result.expect("the prune runs");
    assert_eq!(summary.requested, 2);
    assert_eq!(summary.deleted.len(), 1, "the first delete takes the page");
    assert_eq!(summary.already_absent.len(), 1, "the second finds it gone");
    covered.extend(fired);

    let empty = work.path.join("empty.txt");
    fs::write(&empty, "# nothing\n").expect("writable");
    let (result, fired) = run_prune(&PruneOptions {
        pages: &pages,
        remove: Some(&empty),
        ir: None,
        dry_run: false,
        json: None,
    });
    result.expect("the prune runs");
    covered.extend(fired);

    // An orphan in a subdirectory, and more than twenty of them: the summary
    // keeps the first twenty. The target's orphans are the three whole-package
    // artifacts, all at the root and all fewer than twenty.
    let work = TempDir::new("prune-orphans");
    let pages = work.path.join("pages");
    fs::create_dir_all(pages.join("Pkg/Deep")).expect("creatable");
    for i in 0..25 {
        fs::write(pages.join(format!("Pkg/Deep/M{i}.html")), b"x").expect("writable");
    }
    fs::write(pages.join("Pkg/Deep/keep.css"), b"not html").expect("writable");
    let ir = FakeIr::new("orphan-ir");
    ir.module("Pkg.Deep.M0", &[], &[]);
    ir.finish();
    let json = work.path.join("prune.json");
    let (result, fired) = run_prune(&PruneOptions {
        pages: &pages,
        remove: None,
        ir: Some(&ir.dir.path),
        dry_run: false,
        json: Some(&json),
    });
    let summary = result.expect("the prune runs");
    assert_eq!(
        summary.orphans.len(),
        24,
        "M0 is live, the other 24 are not"
    );
    let written: Value =
        serde_json::from_str(&fs::read_to_string(&json).expect("written")).expect("JSON");
    assert_eq!(
        written["orphanPages"].as_array().expect("an array").len(),
        20,
        "the summary keeps the first twenty"
    );
    assert!(
        pages.join("Pkg/Deep/keep.css").exists(),
        "a file that is not .html is not an orphan"
    );
    covered.extend(fired);

    // A directory emptied by the **orphan** pass rather than by `--remove`. The
    // pipeline never passes `--ir`, so nothing on the target can reach it.
    let work = TempDir::new("prune-orphan-empties");
    let pages = work.path.join("pages");
    fs::create_dir_all(pages.join("Pkg/Gone")).expect("creatable");
    fs::write(pages.join("Pkg/Gone/only.html"), b"x").expect("writable");
    let ir = FakeIr::new("empty-ir");
    ir.finish();
    let (result, fired) = run_prune(&PruneOptions {
        pages: &pages,
        remove: None,
        ir: Some(&ir.dir.path),
        dry_run: false,
        json: None,
    });
    let summary = result.expect("the prune runs");
    assert_eq!(summary.orphans, vec!["Pkg/Gone/only.html".to_owned()]);
    assert_eq!(
        summary.emptied,
        vec!["Pkg/Gone".to_owned(), "Pkg".to_owned()],
        "the orphan pass emptied the directory and then its parent"
    );
    assert!(pages.exists(), "the page root itself went");
    covered.extend(fired);

    // A symlinked directory: neither walked into nor counted as a file, so the
    // directory holding it survives the empty-directory pass.
    let work = TempDir::new("prune-symlink");
    let pages = work.path.join("pages");
    let outside = work.path.join("outside");
    fs::create_dir_all(&pages).expect("creatable");
    fs::create_dir_all(&outside).expect("creatable");
    fs::write(outside.join("Escaped.html"), b"outside").expect("writable");
    symlink(&outside, &pages.join("Link"));
    // …and one **inside a subdirectory**, so that a symlink wrongly counted as
    // "nothing" would take the directory holding it with it.
    fs::create_dir_all(pages.join("Sub")).expect("creatable");
    symlink(&outside, &pages.join("Sub/Link"));
    let ir = FakeIr::new("symlink-ir");
    ir.finish();
    let (result, fired) = run_prune(&PruneOptions {
        pages: &pages,
        remove: None,
        ir: Some(&ir.dir.path),
        dry_run: false,
        json: None,
    });
    let summary = result.expect("the prune runs");
    assert!(
        summary.orphans.is_empty(),
        "the walk did not descend the link"
    );
    assert!(
        outside.join("Escaped.html").exists(),
        "a file outside the root was deleted through a symlink"
    );
    assert!(
        summary.emptied.is_empty(),
        "a symlink was counted as nothing"
    );
    assert!(
        pages.join("Sub").is_dir(),
        "the directory holding a symlink was removed as empty"
    );
    covered.extend(fired);

    // **The guards.** No module name reaches either, because `page_of` turns
    // every dot into a separator — which is the argument, and these are the
    // check.
    let root = PageRoot::new(&pages);
    let escaped = root.resolve("../outside/Escaped.html");
    assert!(
        matches!(escaped, Err(Error::OutsidePageRoot { .. })),
        "a relative path with `..` was resolved"
    );
    assert!(
        !page_of("../../etc/passwd").contains(".."),
        "page_of let a `..` through"
    );
    // Counted by hand rather than by [`observe`]: **no run reaches it**, which
    // is the claim. The guard is called directly because there is no input that
    // calls it — that is what makes it worth having.
    covered.insert("pathEscapeRefused");
    // …and the physical half: a symlinked directory whose *parent* resolves
    // outside the root. `--remove Link.Escaped` names `pages/Link/Escaped.html`,
    // which exists, and whose parent is the outside directory.
    let remove = work.path.join("remove-through-link.txt");
    fs::write(&remove, "Link.Escaped\n").expect("writable");
    let (result, fired) = run_prune(&PruneOptions {
        pages: &pages,
        remove: Some(&remove),
        ir: None,
        dry_run: false,
        json: None,
    });
    assert!(
        matches!(result, Err(Error::OutsidePageRoot { .. })),
        "a deletion through a symlinked directory was allowed: {result:?}"
    );
    assert!(
        outside.join("Escaped.html").exists(),
        "the file outside the root went anyway"
    );
    covered.extend(fired);

    // An absolute-looking module name stays **inside** the root: the path is
    // concatenated, never joined. `Path::join` would have made this
    // `/…/evil.html`.
    let work = TempDir::new("prune-absolute");
    let pages = work.path.join("pages");
    fs::create_dir_all(pages.join("tmp")).expect("creatable");
    fs::write(pages.join("tmp/evil.html"), b"inside").expect("writable");
    let outside_file = work.path.join("evil.html");
    fs::write(&outside_file, b"outside").expect("writable");
    let remove = work.path.join("remove-absolute.txt");
    fs::write(&remove, "/tmp/evil\n").expect("writable");
    let (result, fired) = run_prune(&PruneOptions {
        pages: &pages,
        remove: Some(&remove),
        ir: None,
        dry_run: false,
        json: None,
    });
    let summary = result.expect("the prune runs");
    assert_eq!(summary.deleted, vec!["/tmp/evil".to_owned()]);
    assert!(
        !pages.join("tmp/evil.html").exists(),
        "the page inside went"
    );
    assert!(outside_file.exists(), "a file outside the root went");
    covered.extend(fired);

    // The root itself is never removed, even when the tree ends up empty.
    assert!(pages.exists());
    let root = PageRoot::new(&pages);
    assert!(
        matches!(
            root.allow_remove_dir(&pages),
            Err(Error::OutsidePageRoot { .. })
        ),
        "the page root can be removed"
    );

    // A page that is a **dangling symlink**. `Deno.statSync` follows links, so
    // the prototype calls it absent and leaves the link alone; `symlink_metadata`
    // would call it present and unlink it. Nothing in the corpus has the shape,
    // and mutation testing found that nothing else here did either.
    let work = TempDir::new("prune-dangling");
    let pages = work.path.join("pages");
    fs::create_dir_all(pages.join("Pkg")).expect("creatable");
    symlink(Path::new("../nowhere.html"), &pages.join("Pkg/A.html"));
    let remove = work.path.join("remove.txt");
    fs::write(&remove, "Pkg.A\n").expect("writable");
    let (result, fired) = run_prune(&PruneOptions {
        pages: &pages,
        remove: Some(&remove),
        ir: None,
        dry_run: false,
        json: None,
    });
    let summary = result.expect("the prune runs");
    assert_eq!(
        summary.already_absent,
        vec!["Pkg.A".to_owned()],
        "a dangling symlink is a page that is not there"
    );
    assert!(summary.deleted.is_empty());
    assert!(
        pages.join("Pkg/A.html").symlink_metadata().is_ok(),
        "the dangling link itself was unlinked"
    );
    covered.extend(fired);

    // An index that parses and is not an index.
    let work = TempDir::new("prune-index");
    let pages = work.path.join("pages");
    fs::create_dir_all(&pages).expect("creatable");
    let ir = work.path.join("ir");
    fs::create_dir_all(&ir).expect("creatable");
    fs::write(ir.join("index.json"), br#"{"modules":"not an array"}"#).expect("writable");
    let (result, fired) = run_prune(&PruneOptions {
        pages: &pages,
        remove: None,
        ir: Some(&ir),
        dry_run: false,
        json: None,
    });
    assert!(matches!(result, Err(Error::IndexShape { .. })));
    covered.extend(fired);

    covered
}

// ----------------------------------------------------------------- fixtures

/// A hand-built IR tree.
struct FakeIr {
    dir: TempDir,
    entries: std::cell::RefCell<Vec<Value>>,
}

impl FakeIr {
    fn new(what: &str) -> Self {
        let dir = TempDir::new(what);
        fs::create_dir_all(dir.path.join("modules")).expect("creatable");
        fs::create_dir_all(dir.path.join("deps")).expect("creatable");
        Self {
            dir,
            entries: std::cell::RefCell::new(Vec::new()),
        }
    }

    /// A package shaped like the target's commonest run: a root, a hub two hops
    /// below it, and a leaf nobody imports.
    fn target_shaped(what: &str) -> Self {
        let repo = Self::new(what);
        repo.module("Pkg.Root", &[], &[("Mathlib.Order.Basic", "Nat")]);
        repo.module("Pkg.Mid", &["Pkg.Root"], &[("Pkg.Root", "Pkg.Root.thing")]);
        repo.module(
            "Pkg.Top",
            &["Pkg.Mid", "Mathlib.Order.Basic"],
            &[
                ("Pkg.Mid", "Pkg.Mid.a"),
                ("Pkg.Mid", "Pkg.Mid.b"),
                ("Pkg.Top", "Pkg.Top.self"),
            ],
        );
        repo.module("Pkg.Leaf", &["Pkg.Root"], &[]);
        repo.finish();
        repo
    }

    fn module(&self, name: &str, imports: &[&str], refs: &[(&str, &str)]) {
        let body = json!({
            "schemaVersion": 4,
            "module": name,
            "imports": imports,
            "moduleDocs": [],
            "tactics": [],
            "declarations": [decl(&format!("{name}.thing"), refs)],
        });
        let text = serde_json::to_string(&body).expect("serialises");
        let file = format!("modules/{name}.json");
        fs::write(self.dir.path.join(&file), &text).expect("writable");
        self.entries.borrow_mut().push(json!({
            "module": name,
            "file": file,
            "bytes": text.len(),
            "declarations": 1,
            "contentHash": fnv1a64(text.as_bytes()),
        }));
    }

    /// A second index entry for a module that already has one.
    fn repeat_index_entry(&self, name: &str) {
        let found = self
            .entries
            .borrow()
            .iter()
            .find(|entry| entry["module"] == json!(name))
            .cloned()
            .expect("the module is there");
        self.entries.borrow_mut().push(found);
    }

    fn finish(&self) {
        let entries = self.entries.borrow().clone();
        let index = json!({
            "schemaVersion": 4,
            "generator": "litedoc4/tests",
            "leanVersion": "v4.31.0",
            "hashAlgorithm": "lean-string-hash-64/hex16",
            "moduleCount": entries.len(),
            "declarationCount": entries.len(),
            "modules": entries,
            "dependencyMaps": [],
        });
        fs::write(
            self.dir.path.join("index.json"),
            serde_json::to_string(&index).expect("serialises"),
        )
        .expect("writable");
    }

    /// The commonest single run: one changed module, `--mode importers`, both
    /// output files, and one page deleted from a tree the IR still matches.
    fn one_run(&self) -> BTreeSet<&'static str> {
        let mut covered = BTreeSet::new();
        let tree = SecondTree::open(&self.dir.path);
        let changed = vec!["Pkg.Root".to_owned()];
        let set = self.dir.path.join("set.txt");
        let summary_json = self.dir.path.join("impact.json");
        let (result, fired) = run_impact(
            &ImpactOptions {
                ir: &self.dir.path,
                changed: &changed,
                mode: &Mode::Importers,
                census: None,
                print_set: Some(&set),
                json: Some(&summary_json),
            },
            &tree,
        );
        result.expect("the impact runs");
        covered.extend(fired);

        let pages = self.dir.path.join("pages");
        fs::create_dir_all(pages.join("Pkg")).expect("creatable");
        for name in ["Root", "Mid", "Top", "Leaf"] {
            fs::write(pages.join(format!("Pkg/{name}.html")), name).expect("writable");
        }
        let remove = self.dir.path.join("remove.txt");
        fs::write(&remove, "Pkg.Leaf\n").expect("writable");
        let (result, fired) = run_prune(&PruneOptions {
            pages: &pages,
            remove: Some(&remove),
            ir: None,
            dry_run: false,
            json: Some(&self.dir.path.join("prune.json")),
        });
        result.expect("the prune runs");
        covered.extend(fired);
        covered
    }
}

/// One declaration with every key the schema-4 reader requires.
fn decl(name: &str, refs: &[(&str, &str)]) -> Value {
    json!({
        "name": name,
        "kind": "theorem",
        "modifiers": [],
        "binders": [],
        "implicits": [],
        "binderCode": [],
        "type": "True",
        "typeCode": [],
        "line": 1,
        "col": 0,
        "endLine": 1,
        "endCol": 10,
        "index": 0,
        "members": [],
        "doc": Value::Null,
        "equations": [],
        "equationCode": [],
        "refs": refs.iter().map(|(m, n)| json!([m, n])).collect::<Vec<_>>(),
    })
}

fn symlink(target: &Path, link: &Path) {
    std::os::unix::fs::symlink(target, link).expect("the symlink is creatable");
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("creatable");
    for entry in fs::read_dir(from).expect("the source tree reads") {
        let entry = entry.expect("a directory entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("a file type").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("copyable");
        }
    }
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
            "litedoc4-impact-{}-{}-{slug}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("the temporary directory is creatable");
        Self { path }
    }
}
