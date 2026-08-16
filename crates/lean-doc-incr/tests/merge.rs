//! Milestone **M3-b**: the `ownership` and `merge` stages.
//!
//! Three oracles, none of which is this file's own opinion:
//!
//! - **The frozen prototype's own files.** `tools/merge-reference.sh --impl ts`
//!   ran `experiments/stage5/{ownership,merge-ir}.ts` over the base IR and wrote
//!   nine rounds and three verifications;
//!   `tests/oracle/gen-merge-expected.ts` reduced that tree to
//!   `tests/data/merge-expected.json`, which [`the_corpus_matches_the_prototype`]
//!   compares against file by file.
//!   **The fixture is a frozen value: HEAD has no way to regenerate it.** The
//!   generator, the prototype and the `--impl ts` half of the harness were
//!   removed with `experiments/` on 2026-08-16 and exist only at tag
//!   `experiments-frozen`.
//! - **The from-scratch IR itself.** The port writes `deps/<Root>.json` in
//!   Lean's order rather than the prototype's insertion order (plan §7's "already
//!   not byte-identical" note, decided in M3), so the merged slice can be held
//!   against the one `Extract.lean` wrote:
//!   [`the_dependency_slices_are_the_from_scratch_bytes`].
//! - **A second writer, here.** [`lean_order_slice`] is the sorting rule written
//!   out again, so the invariant above is checked even for a mapping the
//!   from-scratch tree does not contain.
//!
//! # Byte equality is not branch coverage (plan §7)
//!
//! Of the [`BRANCHES`] this milestone added:
//!
//! | exercise | reaches |
//! |---|---:|
//! | the merged tree's byte comparison — one round over the base IR ([`ONE_ROUND`]) | **19** |
//! | everything the nine rounds and three verifications reach ([`HARNESS_AND_INDEX`]) | **48** |
//! | curated cases only ([`NO_REAL_DATA_REACHES`]) | **23** |
//!
//! The dependency is asserted rather than commented:
//! [`the_curated_cases_cover_what_the_package_does_not`].

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use lean_doc_incr::{
    Error, MergeOptions, MergeSummary, OwnershipOptions, OwnershipSummary, VerifyReport, merge,
    ownership, verify,
};
use lean_doc_ir::cmp_utf16;
use serde_json::{Value, json};

/// The from-scratch IR the whole milestone is measured against. Read only.
const DEFAULT_BASE_IR: &str = "/private/tmp/lean-doc-relay/w7h/base-ir";
/// The tree `tools/merge-reference.sh --impl ts` writes. Its `fixtures/`
/// subdirectory holds the partial extractions **both** implementations are fed —
/// the scenarios are defined once, in the harness, and consumed here.
const DEFAULT_REFERENCE: &str = "/private/tmp/lean-doc-relay/m3b/ref";

/// U+1D49C MATHEMATICAL SCRIPT CAPITAL A and U+FB00 LATIN SMALL LIGATURE FF:
/// the pair that separates UTF-16 order from code point order. `𝒜` sorts
/// *before* `ﬀ` in UTF-16 and after it by code point (plan §7, U1).
const ASTRAL: &str = "\u{1D49C}";
const LIGATURE: &str = "\u{FB00}";

// --------------------------------------------------------------- the branches

/// Every branch M3-b added, named by an event of the run rather than by a line
/// of the code.
///
/// Each is decided by [`observe`] from the inputs a run was given and the files
/// it produced — never by asking the code under test what it decided. Where the
/// decision needs a rule (which names moved? which references went stale?) the
/// rule is written out a second time there, over `serde_json::Value` rather than
/// over the IR reader.
const BRANCHES: [&str; 71] = [
    // ownership: what it was given.
    "ownershipIncGiven",
    "ownershipIncAbsent",
    "ownershipRemovedGiven",
    "ownershipRemovedAbsent",
    "ownershipRemovedNotInBase",
    "ownershipExcludeGiven",
    "ownershipExcludeAbsent",
    // ownership: the diff.
    "incModuleKnownToBase",
    "incModuleNewToBase",
    "nameLost",
    "nameGained",
    "nameKept",
    "removedModuleNamesLost",
    "declarationNameRepeated",
    // ownership: the scan.
    "scanSkippedNothingMoved",
    "scanRan",
    "scanSkippedExcluded",
    "ruleLostOwner",
    "ruleMovedElsewhere",
    "refPointsSomewhereLive",
    "witnessFirstForRule",
    "witnessRepeatSuppressed",
    "witnessesTruncatedInSummary",
    "staleSortAboveBmp",
    "scannedBaseModulesNegative",
    // ownership: what it writes.
    "printSetWritten",
    "printSetOmitted",
    "printSetEmpty",
    "ownershipJsonWritten",
    "ownershipJsonOmitted",
    // merge: what it was given.
    "mergeIncGiven",
    "mergeIncAbsent",
    "mergeOutIsBase",
    "mergeOutIsCopy",
    "mergeRemoveHitIndex",
    "mergeRemoveMissedIndex",
    "mergeRemoveEmpty",
    "mergeRemoveRepeated",
    // merge: the package's module list (M3-d2b).
    "mergeModulesGiven",
    "mergeModulesAbsent",
    "mergeModulesReordersIndex",
    "mergeModulesRepeated",
    "mergeModulesMissingFromTree",
    "mergeModulesExtraInTree",
    // merge: folding.
    "incModuleReplaced",
    "incModuleAppended",
    "contentHashMoved",
    "contentHashUnchanged",
    "baseModuleCopied",
    "baseModuleDeletedInPlace",
    // merge: the dependency slice.
    "depSliceWritten",
    "depSliceStaleRemoved",
    "depSliceNone",
    "depNameLastWriterWins",
    "depNamesAboveBmp",
    "depRootsAboveBmp",
    "depSchemaVersionAbsent",
    // merge: the index.
    "indexCarriesAblations",
    "indexRefusedShape",
    // merge: what it writes.
    "changedOutWritten",
    "changedOutOmitted",
    "changedOutEmpty",
    "mergeTimingsWritten",
    "mergeTimingsOmitted",
    // verify.
    "verifyAgreed",
    "verifyModuleCountDiffers",
    "verifyMissingInB",
    "verifyIndexFieldDiffers",
    "verifyModuleBytesDiffer",
    "verifyDepValueDiffers",
    "verifyDepOnlyInB",
];

/// What a byte comparison of **one merged tree** reaches: one round that folds
/// one re-extracted module back in place, which is what a green
/// `tools/merge-compare.sh` on the commonest scenario means.
///
/// Nineteen of the seventy-one. Every shape of edit the pipeline is *for* — a
/// move, a deletion, a module that never existed — is invisible to it, and so is
/// every question `verify` asks.
const ONE_ROUND: [&str; 19] = [
    "changedOutWritten",
    "contentHashMoved",
    "depSliceWritten",
    "incModuleKnownToBase",
    "incModuleReplaced",
    "mergeIncGiven",
    "mergeModulesAbsent",
    "mergeOutIsBase",
    "mergeRemoveEmpty",
    "mergeTimingsWritten",
    "nameKept",
    "ownershipExcludeAbsent",
    "ownershipIncGiven",
    "ownershipJsonWritten",
    "ownershipRemovedAbsent",
    "printSetEmpty",
    "printSetWritten",
    "scanSkippedExcluded",
    "scanSkippedNothingMoved",
];

/// What the whole harness reaches — the nine rounds and three verifications of
/// `tools/merge-reference.sh`, replayed in process by
/// [`the_corpus_matches_the_prototype`]. **Measured there, not assumed.**
const HARNESS_AND_INDEX: [&str; 48] = [
    "baseModuleCopied",
    "baseModuleDeletedInPlace",
    "changedOutEmpty",
    "changedOutWritten",
    "contentHashMoved",
    "contentHashUnchanged",
    "depSliceStaleRemoved",
    "depSliceWritten",
    "incModuleAppended",
    "incModuleKnownToBase",
    "incModuleNewToBase",
    "incModuleReplaced",
    "mergeIncAbsent",
    "mergeIncGiven",
    "mergeModulesAbsent",
    "mergeOutIsBase",
    "mergeOutIsCopy",
    "mergeRemoveEmpty",
    "mergeRemoveHitIndex",
    "mergeTimingsWritten",
    "nameGained",
    "nameKept",
    "nameLost",
    "ownershipExcludeAbsent",
    "ownershipExcludeGiven",
    "ownershipIncAbsent",
    "ownershipIncGiven",
    "ownershipJsonWritten",
    "ownershipRemovedAbsent",
    "ownershipRemovedGiven",
    "printSetEmpty",
    "printSetWritten",
    "refPointsSomewhereLive",
    "removedModuleNamesLost",
    "ruleLostOwner",
    "ruleMovedElsewhere",
    "scanRan",
    "scanSkippedExcluded",
    "scanSkippedNothingMoved",
    "verifyAgreed",
    "verifyDepOnlyInB",
    "verifyIndexFieldDiffers",
    "verifyMissingInB",
    "verifyModuleBytesDiffer",
    "verifyModuleCountDiffers",
    "witnessFirstForRule",
    "witnessRepeatSuppressed",
    "witnessesTruncatedInSummary",
];

/// The branches **no exercise over the real base IR reaches at all**, whatever
/// the scenario.
///
/// Twenty-three of seventy-one. Four are flags the pipeline always passes; four
/// are shapes of an index or a remove list only a hand edit produces; three are
/// the UTF-16 / code-point traps, which need a name above the BMP and the
/// package has none — one of them, the order of the *roots*, is the one place
/// where keeping `index.json` the prototype's and each slice Lean's pull apart;
/// four are dependency shapes the package cannot have (no external references at
/// all, a name owned by two modules at once, a base index with no schema
/// version, an ablation marker); two are answers that need a tree the harness
/// does not build; and **five are `--modules`** (M3-d2b), which the harness
/// never passes because it replays the prototype, and the prototype never reads
/// the flag it offers.
///
/// The three UTF-16 ones are **not hypothetical, and the neighbouring packages
/// cannot supply them cheaply**. The target's IR has no supplementary scalar at
/// all — 0 of 4,750 declaration names, 0 reference names, 0 module names — while
/// the dependency closure's link index has **37 of 264,535** declaration lines
/// carrying one (`Topology.term𝓝`, `Cardinal.term𝔠`, …)【実測 2026-08-12】. M3-a
/// could borrow Mathlib's *oleans* for the shape the target lacked; here the
/// shape has to be inside an IR tree's `refs`, and no dependency IR tree exists
/// without running the extractor over Mathlib. So these stay curated.
const NO_REAL_DATA_REACHES: [&str; 23] = [
    "changedOutOmitted",
    "declarationNameRepeated",
    "depNameLastWriterWins",
    "depNamesAboveBmp",
    "depRootsAboveBmp",
    "depSchemaVersionAbsent",
    "depSliceNone",
    "indexCarriesAblations",
    "indexRefusedShape",
    "mergeModulesExtraInTree",
    "mergeModulesGiven",
    "mergeModulesMissingFromTree",
    "mergeModulesReordersIndex",
    "mergeModulesRepeated",
    "mergeRemoveMissedIndex",
    "mergeRemoveRepeated",
    "mergeTimingsOmitted",
    "ownershipJsonOmitted",
    "ownershipRemovedNotInBase",
    "printSetOmitted",
    "scannedBaseModulesNegative",
    "staleSortAboveBmp",
    "verifyDepValueDiffers",
];

// ------------------------------------------------------------- the observer

/// One run of one command, with everything needed to say what it reached.
enum Run<'a> {
    Ownership {
        options: &'a OwnershipOptions<'a>,
        result: &'a Result<OwnershipSummary, Error>,
    },
    Merge {
        options: &'a MergeOptions<'a>,
        /// The base `index.json` **before** the run. An in-place merge rewrites
        /// it, so reading it afterwards would be reading the answer.
        base_before: Option<&'a Value>,
        /// The `deps/` files that were in `out` **before** the run.
        deps_before: &'a BTreeSet<String>,
        result: &'a Result<MergeSummary, Error>,
    },
    Verify {
        a: &'a Path,
        b: &'a Path,
        result: &'a Result<VerifyReport, Error>,
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
        Run::Ownership { options, result } => observe_ownership(options, result, &mut fire),
        Run::Merge {
            options,
            base_before,
            deps_before,
            result,
        } => observe_merge(options, *base_before, deps_before, result, &mut fire),
        Run::Verify { a, b, result } => observe_verify(a, b, result, &mut fire),
    }
    fired
}

/// A tree as this file reads it: the index and the module files, as plain JSON.
///
/// Deliberately **not** `lean_doc_ir`: the observer has to be a second reader,
/// or a bug in the one under test would hide itself here too.
struct Tree {
    root: PathBuf,
    index: Value,
}

impl Tree {
    fn open(root: &Path) -> Option<Self> {
        let text = fs::read_to_string(root.join("index.json")).ok()?;
        Some(Self {
            root: root.to_owned(),
            index: serde_json::from_str(&text).ok()?,
        })
    }

    fn entries(&self) -> Vec<&Value> {
        self.index
            .get("modules")
            .and_then(Value::as_array)
            .map(|entries| entries.iter().collect())
            .unwrap_or_default()
    }

    fn module_names(&self) -> Vec<String> {
        self.entries()
            .iter()
            .filter_map(|entry| entry.get("module")?.as_str().map(str::to_owned))
            .collect()
    }

    fn entry(&self, module: &str) -> Option<&Value> {
        self.entries()
            .into_iter()
            .find(|entry| entry.get("module").and_then(Value::as_str) == Some(module))
    }

    /// The module file, as JSON.
    fn body(&self, module: &str) -> Option<Value> {
        let file = self.entry(module)?.get("file")?.as_str()?.to_owned();
        serde_json::from_str(&fs::read_to_string(self.root.join(file)).ok()?).ok()
    }

    /// Declaration names, **as the array holds them**: a repeat is two entries.
    fn declaration_names(&self, module: &str) -> Vec<String> {
        self.body(module)
            .and_then(|body| {
                Some(
                    body.get("declarations")?
                        .as_array()?
                        .iter()
                        .filter_map(|decl| decl.get("name")?.as_str().map(str::to_owned))
                        .collect(),
                )
            })
            .unwrap_or_default()
    }

    /// `(defining module, name)` references, per module.
    fn references(&self, module: &str) -> Vec<(String, String)> {
        self.body(module)
            .and_then(|body| {
                Some(
                    body.get("declarations")?
                        .as_array()?
                        .iter()
                        .filter_map(|decl| decl.get("refs")?.as_array())
                        .flatten()
                        .filter_map(|pair| {
                            let pair = pair.as_array()?;
                            Some((
                                pair.first()?.as_str()?.to_owned(),
                                pair.get(1)?.as_str()?.to_owned(),
                            ))
                        })
                        .collect(),
                )
            })
            .unwrap_or_default()
    }

    /// `name -> defining module` over every dependency slice, in file order.
    fn dep_mapping(&self) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = Vec::new();
        let slices = self
            .index
            .get("dependencyMaps")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for slice in slices {
            let Some(file) = slice.get("file").and_then(Value::as_str) else {
                continue;
            };
            let Ok(text) = fs::read_to_string(self.root.join(file)) else {
                continue;
            };
            let Ok(body) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            let Some(declarations) = body.get("declarations").and_then(Value::as_object) else {
                continue;
            };
            for (name, module) in declarations {
                let module = module.as_str().unwrap_or_default().to_owned();
                match out.iter_mut().find(|(other, _)| other == name) {
                    Some(slot) => slot.1 = module,
                    None => out.push((name.clone(), module)),
                }
            }
        }
        out
    }
}

/// A module list file, as the stages read it.
fn read_lines(path: Option<&Path>) -> Vec<String> {
    let Some(path) = path else { return Vec::new() };
    fs::read_to_string(path)
        .unwrap_or_default()
        .split('\n')
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

fn observe_ownership(
    options: &OwnershipOptions<'_>,
    result: &Result<OwnershipSummary, Error>,
    fire: &mut impl FnMut(&'static str),
) {
    fire(if options.inc.is_some() {
        "ownershipIncGiven"
    } else {
        "ownershipIncAbsent"
    });
    fire(if options.removed.is_some() {
        "ownershipRemovedGiven"
    } else {
        "ownershipRemovedAbsent"
    });
    fire(if options.exclude.is_some() {
        "ownershipExcludeGiven"
    } else {
        "ownershipExcludeAbsent"
    });
    fire(if options.print_set.is_some() {
        "printSetWritten"
    } else {
        "printSetOmitted"
    });
    fire(if options.json.is_some() {
        "ownershipJsonWritten"
    } else {
        "ownershipJsonOmitted"
    });
    if let Some(path) = options.print_set
        && fs::read(path).is_ok_and(|body| body.is_empty())
    {
        fire("printSetEmpty");
    }

    let Some(base) = Tree::open(options.base) else {
        return;
    };
    let base_modules: BTreeSet<String> = base.module_names().into_iter().collect();
    let inc = options.inc.and_then(Tree::open);
    let removed = read_lines(options.removed);
    if removed.iter().any(|m| !base_modules.contains(m)) {
        fire("ownershipRemovedNotInBase");
    }

    // The diff, written out again: name -> the modules that lost / gained it.
    let mut lost: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut gained: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    if let Some(inc) = &inc {
        for module in inc.module_names() {
            fire(if base_modules.contains(&module) {
                "incModuleKnownToBase"
            } else {
                "incModuleNewToBase"
            });
            let now: BTreeSet<String> = inc.declaration_names(&module).into_iter().collect();
            let was: BTreeSet<String> = if base_modules.contains(&module) {
                base.declaration_names(&module).into_iter().collect()
            } else {
                BTreeSet::new()
            };
            for name in was.difference(&now) {
                lost.entry(name.clone()).or_default().insert(module.clone());
                fire("nameLost");
            }
            for name in now.difference(&was) {
                gained
                    .entry(name.clone())
                    .or_default()
                    .insert(module.clone());
                fire("nameGained");
            }
            if was.intersection(&now).next().is_some() {
                fire("nameKept");
            }
        }
    }
    for module in &removed {
        if !base_modules.contains(module) {
            continue;
        }
        let names = base.declaration_names(module);
        let distinct: BTreeSet<&String> = names.iter().collect();
        if distinct.len() != names.len() {
            fire("declarationNameRepeated");
        }
        for name in names {
            lost.entry(name).or_default().insert(module.clone());
            fire("removedModuleNamesLost");
        }
    }

    let mut exclude: BTreeSet<String> = read_lines(options.exclude).into_iter().collect();
    if let Some(inc) = &inc {
        exclude.extend(inc.module_names());
    }
    exclude.extend(removed.iter().cloned());
    if exclude.iter().any(|m| base_modules.contains(m)) {
        fire("scanSkippedExcluded");
    }

    let watching = !lost.is_empty() || !gained.is_empty();
    fire(if watching {
        "scanRan"
    } else {
        "scanSkippedNothingMoved"
    });
    if watching {
        // The scan, written out again — and counted per reference so that the
        // witness rules can be observed rather than asked about.
        let mut firing: BTreeMap<(String, &str), usize> = BTreeMap::new();
        let mut stale: BTreeSet<String> = BTreeSet::new();
        for module in base.module_names() {
            if exclude.contains(&module) {
                continue;
            }
            for (owner, name) in base.references(&module) {
                let rule = if lost.get(&name).is_some_and(|o| o.contains(&owner)) {
                    "ruleLostOwner"
                } else if gained.get(&name).is_some_and(|o| !o.contains(&owner)) {
                    "ruleMovedElsewhere"
                } else {
                    "refPointsSomewhereLive"
                };
                fire(rule);
                if rule != "refPointsSomewhereLive" {
                    *firing.entry((module.clone(), rule)).or_default() += 1;
                    stale.insert(module.clone());
                }
            }
        }
        if !firing.is_empty() {
            fire("witnessFirstForRule");
        }
        if firing.values().any(|count| *count > 1) {
            fire("witnessRepeatSuppressed");
        }
        if firing.len() > lean_doc_incr::WITNESSES_IN_SUMMARY {
            fire("witnessesTruncatedInSummary");
        }
        let mut by_utf16: Vec<&String> = stale.iter().collect();
        by_utf16.sort_by(|a, b| cmp_utf16(a, b));
        let by_code_point: Vec<&String> = stale.iter().collect();
        if by_utf16 != by_code_point {
            fire("staleSortAboveBmp");
        }
    }
    if result
        .as_ref()
        .is_ok_and(|summary| summary.scanned_base_modules < 0)
    {
        fire("scannedBaseModulesNegative");
    }
}

fn observe_merge(
    options: &MergeOptions<'_>,
    base_before: Option<&Value>,
    deps_before: &BTreeSet<String>,
    result: &Result<MergeSummary, Error>,
    fire: &mut impl FnMut(&'static str),
) {
    fire(if options.inc.is_some() {
        "mergeIncGiven"
    } else {
        "mergeIncAbsent"
    });
    fire(if options.modules.is_some() {
        "mergeModulesGiven"
    } else {
        "mergeModulesAbsent"
    });
    fire(if options.out == options.base {
        "mergeOutIsBase"
    } else {
        "mergeOutIsCopy"
    });
    fire(if options.changed_out.is_some() {
        "changedOutWritten"
    } else {
        "changedOutOmitted"
    });
    fire(if options.timings.is_some() {
        "mergeTimingsWritten"
    } else {
        "mergeTimingsOmitted"
    });
    if let Some(path) = options.changed_out
        && fs::read(path).is_ok_and(|body| body.is_empty())
    {
        fire("changedOutEmpty");
    }
    // Whether the package's module list describes the tree the merge was about
    // to write, derived here from the same three inputs `merge` has.
    let list_agrees = observe_module_list(options, base_before, fire);
    if result.is_err() {
        // A refusal with a list that agrees is the other refusal there is: an
        // `index.json` that parses and is not an index.
        if list_agrees {
            fire("indexRefusedShape");
        }
        return;
    }

    let Some(index) = base_before else { return };
    let base = Tree {
        root: options.base.to_owned(),
        index: index.clone(),
    };
    let base_modules: BTreeSet<String> = base.module_names().into_iter().collect();
    if options.removed.is_empty() {
        fire("mergeRemoveEmpty");
    }
    let mut seen: BTreeSet<&String> = BTreeSet::new();
    for module in options.removed {
        if !seen.insert(module) {
            fire("mergeRemoveRepeated");
        }
        fire(if base_modules.contains(module) {
            "mergeRemoveHitIndex"
        } else {
            "mergeRemoveMissedIndex"
        });
    }
    let gone: BTreeSet<&String> = options
        .removed
        .iter()
        .filter(|m| base_modules.contains(*m))
        .collect();

    let inc = options.inc.and_then(Tree::open);
    if let Some(inc) = &inc {
        for module in inc.module_names() {
            fire(if base_modules.contains(&module) {
                "incModuleReplaced"
            } else {
                "incModuleAppended"
            });
            let was = base
                .entry(&module)
                .and_then(|entry| entry.get("contentHash").cloned());
            let now = inc
                .entry(&module)
                .and_then(|entry| entry.get("contentHash").cloned());
            fire(if was.is_some() && was == now {
                "contentHashUnchanged"
            } else {
                "contentHashMoved"
            });
        }
    }
    let in_inc: BTreeSet<String> = inc
        .as_ref()
        .map(Tree::module_names)
        .unwrap_or_default()
        .into_iter()
        .collect();
    if options.out != options.base
        && base_modules
            .iter()
            .any(|m| !gone.contains(m) && !in_inc.contains(m))
    {
        fire("baseModuleCopied");
    }
    if options.out == options.base && !gone.is_empty() {
        fire("baseModuleDeletedInPlace");
    }
    if base.index.get("ablations").is_some() {
        fire("indexCarriesAblations");
    }
    if base.index.get("schemaVersion").is_none() {
        fire("depSchemaVersionAbsent");
    }

    // The dependency slice, recomputed here from the merged tree.
    let Some(merged) = Tree::open(options.out) else {
        return;
    };
    // **The whole of what `--modules` buys**, asserted wherever it is passed: the
    // merged index is in the list's order, which is the order a from-scratch
    // extraction over the same list writes.
    if let Some(list) = options.modules {
        assert_eq!(
            merged.module_names(),
            deduplicated(list),
            "the merged index is not in --modules' order",
        );
    }
    let own: BTreeSet<String> = merged.module_names().into_iter().collect();
    let mut owners: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for module in merged.module_names() {
        for (defining, name) in merged.references(&module) {
            if !own.contains(&defining) {
                owners.entry(name).or_default().insert(defining);
            }
        }
    }
    if owners.values().any(|modules| modules.len() > 1) {
        fire("depNameLastWriterWins");
    }
    if owners
        .keys()
        .any(|name| name.chars().any(|c| c > '\u{FFFF}'))
    {
        fire("depNamesAboveBmp");
    }
    if owners
        .values()
        .flatten()
        .filter_map(|module| module.split('.').next())
        .any(|root| root.chars().any(|c| c > '\u{FFFF}'))
    {
        fire("depRootsAboveBmp");
    }
    let deps_after: BTreeSet<String> = list_dir(&options.out.join("deps"));
    if deps_after.is_empty() {
        fire("depSliceNone");
    } else {
        fire("depSliceWritten");
    }
    if deps_before.difference(&deps_after).next().is_some() {
        fire("depSliceStaleRemoved");
    }
}

/// A module list with repeats collapsed, each name keeping its first position —
/// the rule an index and `lean-doc modules`' own output follow, written out here
/// a second time.
fn deduplicated(list: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(list.len());
    for module in list {
        if !out.contains(module) {
            out.push(module.clone());
        }
    }
    out
}

/// The `--modules` branches (M3-d2b), and whether the list describes the tree
/// the merge was about to write.
///
/// The tree is derived here from the three inputs — the base index **as it was**,
/// the remove list and the partial extraction — rather than from what `merge`
/// answered, so a merge that folded the wrong set together cannot make its own
/// list look right.
fn observe_module_list(
    options: &MergeOptions<'_>,
    base_before: Option<&Value>,
    fire: &mut impl FnMut(&'static str),
) -> bool {
    let (Some(list), Some(index)) = (options.modules, base_before) else {
        return true;
    };
    let base = Tree {
        root: options.base.to_owned(),
        index: index.clone(),
    };
    let base_names = base.module_names();
    let in_base: BTreeSet<&String> = base_names.iter().collect();
    let gone: BTreeSet<&str> = options
        .removed
        .iter()
        .filter(|module| in_base.contains(*module))
        .map(String::as_str)
        .collect();

    // Base order minus the deletions, then whatever the partial extraction adds:
    // the order `merge` produces with no list at all.
    let mut appended: Vec<String> = base_names
        .iter()
        .filter(|module| !gone.contains(module.as_str()))
        .cloned()
        .collect();
    for module in options
        .inc
        .and_then(Tree::open)
        .map(|inc| inc.module_names())
        .unwrap_or_default()
    {
        if !appended.contains(&module) {
            appended.push(module);
        }
    }

    let wanted = deduplicated(list);
    if wanted.len() != list.len() {
        fire("mergeModulesRepeated");
    }
    let tree: BTreeSet<&String> = appended.iter().collect();
    let listed: BTreeSet<&String> = wanted.iter().collect();
    let missing = listed.difference(&tree).next().is_some();
    let extra = tree.difference(&listed).next().is_some();
    if missing {
        fire("mergeModulesMissingFromTree");
    }
    if extra {
        fire("mergeModulesExtraInTree");
    }
    if missing || extra {
        return false;
    }
    if wanted != appended {
        fire("mergeModulesReordersIndex");
    }
    true
}

fn observe_verify(
    a: &Path,
    b: &Path,
    result: &Result<VerifyReport, Error>,
    fire: &mut impl FnMut(&'static str),
) {
    let (Some(ta), Some(tb)) = (Tree::open(a), Tree::open(b)) else {
        return;
    };
    let names_a: Vec<String> = ta.module_names();
    let names_b: BTreeSet<String> = tb.module_names().into_iter().collect();
    let mut trouble = false;
    if BTreeSet::from_iter(names_a.iter().cloned()).len() != names_b.len() {
        fire("verifyModuleCountDiffers");
        trouble = true;
    }
    for module in &names_a {
        if !names_b.contains(module) {
            fire("verifyMissingInB");
            trouble = true;
            continue;
        }
        let (Some(ea), Some(eb)) = (ta.entry(module), tb.entry(module)) else {
            continue;
        };
        for key in ["file", "bytes", "declarations", "contentHash"] {
            if ea.get(key) != eb.get(key) {
                fire("verifyIndexFieldDiffers");
                trouble = true;
            }
        }
        let file_a = ea.get("file").and_then(Value::as_str).unwrap_or_default();
        let file_b = eb.get("file").and_then(Value::as_str).unwrap_or_default();
        if fs::read(ta.root.join(file_a)).ok() != fs::read(tb.root.join(file_b)).ok() {
            fire("verifyModuleBytesDiffer");
            trouble = true;
        }
    }
    let dep_a = ta.dep_mapping();
    let dep_b: HashMap<String, String> = tb.dep_mapping().into_iter().collect();
    let mut mismatches = 0usize;
    for (name, module) in &dep_a {
        if dep_b.get(name) != Some(module) {
            fire("verifyDepValueDiffers");
            trouble = true;
            mismatches += 1;
        }
    }
    let in_a: BTreeSet<&String> = dep_a.iter().map(|(name, _)| name).collect();
    for name in dep_b.keys() {
        if !in_a.contains(name) {
            fire("verifyDepOnlyInB");
            trouble = true;
            mismatches += 1;
        }
    }
    let _ = mismatches;
    if !trouble {
        fire("verifyAgreed");
    }
    assert!(
        result.is_ok(),
        "no exercise here hands verify a tree it cannot read"
    );
}

fn list_dir(dir: &Path) -> BTreeSet<String> {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|entry| Some(entry.ok()?.file_name().to_string_lossy().into_owned()))
                .collect()
        })
        .unwrap_or_default()
}

// ------------------------------------------------------------ running things

fn run_ownership(
    options: &OwnershipOptions<'_>,
) -> (Result<OwnershipSummary, Error>, BTreeSet<&'static str>) {
    let result = ownership(options);
    let fired = observe(&Run::Ownership {
        options,
        result: &result,
    });
    (result, fired)
}

fn run_merge(options: &MergeOptions<'_>) -> (Result<MergeSummary, Error>, BTreeSet<&'static str>) {
    let deps_before = list_dir(&options.out.join("deps"));
    // Snapshotted before the run: an in-place merge overwrites the base index,
    // and an observer that read it afterwards would be reading the answer it is
    // supposed to be checking.
    let base_before: Option<Value> = fs::read_to_string(options.base.join("index.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok());
    let result = merge(options);
    let fired = observe(&Run::Merge {
        options,
        base_before: base_before.as_ref(),
        deps_before: &deps_before,
        result: &result,
    });
    (result, fired)
}

fn run_verify(a: &Path, b: &Path) -> (Result<VerifyReport, Error>, BTreeSet<&'static str>) {
    let result = verify(a, b);
    let fired = observe(&Run::Verify {
        a,
        b,
        result: &result,
    });
    (result, fired)
}

// --------------------------------------------------------------- the fixture

struct Expected {
    value: Value,
    seen: std::cell::RefCell<BTreeSet<String>>,
    /// The files the port is expected **not** to match, because it writes them
    /// in Lean's order on purpose. Filled in as the comparison runs and asserted
    /// against [`Expected::diverged_by_design`] at the end.
    diverged: std::cell::RefCell<BTreeSet<String>>,
}

fn expected() -> Expected {
    Expected {
        value: serde_json::from_str(include_str!("data/merge-expected.json"))
            .expect("the fixture is JSON"),
        seen: std::cell::RefCell::new(BTreeSet::new()),
        diverged: std::cell::RefCell::new(BTreeSet::new()),
    }
}

impl Expected {
    fn file(&self, name: &str) -> (usize, String) {
        let entry = self.value["files"]
            .get(name)
            .unwrap_or_else(|| panic!("{name} is not in the fixture; regenerate it"));
        (
            entry["bytes"].as_u64().expect("a size") as usize,
            entry["fnv1a64"].as_str().expect("a digest").to_owned(),
        )
    }

    /// Compares one produced file with the prototype's, by size and digest.
    fn check(&self, name: &str, body: &[u8]) {
        let (bytes, digest) = self.file(name);
        self.seen.borrow_mut().insert(name.to_owned());
        assert_eq!(
            body.len(),
            bytes,
            "{name}: {} bytes against the prototype's {bytes}",
            body.len()
        );
        assert_eq!(
            fnv1a64(body),
            digest,
            "{name} differs from the prototype's somewhere in {} bytes",
            body.len()
        );
    }

    /// A `deps/*.json`, which the port writes in Lean's order rather than the
    /// prototype's. Asserts **both** halves of the decision: it is not the
    /// prototype's bytes, and it is the sorted writer's.
    fn check_diverged(&self, name: &str, body: &[u8], lean_order: &[u8]) {
        let (bytes, digest) = self.file(name);
        self.seen.borrow_mut().insert(name.to_owned());
        self.diverged.borrow_mut().insert(name.to_owned());
        assert_eq!(
            body, lean_order,
            "{name} is not what a from-scratch writer would have written"
        );
        assert_eq!(
            body.len(),
            bytes,
            "{name}: reordering a JSON object changed its length"
        );
        assert_ne!(
            fnv1a64(body),
            digest,
            "{name} matches the prototype byte for byte; the divergence M3-b \
             decided on is gone and this test is now lying about it"
        );
    }

    /// The merged `index.json`, which diverges from the prototype in exactly one
    /// place: every `dependencyMaps` element is written in Lean's alphabetical
    /// key order rather than the prototype's object literal.
    ///
    /// Stated independently of the code under test — this reads the produced
    /// bytes back with an order-preserving parser and asserts the order it finds,
    /// rather than comparing against a second copy of the writer.
    fn check_diverged_index(&self, name: &str, body: &[u8]) {
        let (bytes, digest) = self.file(name);
        self.seen.borrow_mut().insert(name.to_owned());
        self.diverged.borrow_mut().insert(name.to_owned());
        let text = std::str::from_utf8(body).expect("the index is UTF-8");
        let parsed: Value = serde_json::from_str(text).expect("the index is JSON");
        let maps = parsed["dependencyMaps"]
            .as_array()
            .expect("dependencyMaps is an array");
        for element in maps {
            let keys: Vec<&String> = element
                .as_object()
                .expect("a dependencyMaps element is an object")
                .keys()
                .collect();
            let mut alphabetical = keys.clone();
            alphabetical.sort();
            assert_eq!(
                keys, alphabetical,
                "{name}: a dependencyMaps element is not in Lean's key order"
            );
        }
        assert_eq!(
            body.len(),
            bytes,
            "{name}: reordering a JSON object changed its length"
        );
        // With no dependency slices at all there is nothing to reorder and the
        // two writers agree; every round in this harness has some.
        assert!(!maps.is_empty(), "{name}: nothing to diverge about");
        assert_ne!(
            fnv1a64(body),
            digest,
            "{name} matches the prototype byte for byte; the divergence M3-b \
             decided on is gone and this test is now lying about it"
        );
    }
}

fn fnv1a64(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash = (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// The dependency slice as **Lean** writes it, written out here a second time:
/// alphabetical top-level keys, declaration names in code point order, compact.
///
/// This is the whole content of M3-b's deliberate divergence, stated
/// independently of the code that implements it.
fn lean_order_slice(mapping: &BTreeMap<String, String>, package: &str, schema: &Value) -> Vec<u8> {
    let mut body = String::from("{\"declarations\":{");
    for (i, (name, module)) in mapping.iter().enumerate() {
        if i > 0 {
            body.push(',');
        }
        body.push_str(&serde_json::to_string(name).expect("a name is a string"));
        body.push(':');
        body.push_str(&serde_json::to_string(module).expect("a module is a string"));
    }
    body.push_str("},\"package\":");
    body.push_str(&serde_json::to_string(package).expect("a package is a string"));
    if !schema.is_null() {
        body.push_str(",\"schemaVersion\":");
        body.push_str(&schema.to_string());
    }
    body.push('}');
    body.into_bytes()
}

/// The `name -> defining module` mapping a tree's `deps/` should hold,
/// recomputed from the merged module files.
fn dep_mapping_of(tree: &Tree) -> BTreeMap<String, BTreeMap<String, String>> {
    let own: BTreeSet<String> = tree.module_names().into_iter().collect();
    let mut flat: BTreeMap<String, String> = BTreeMap::new();
    for module in tree.module_names() {
        for (defining, name) in tree.references(&module) {
            if !own.contains(&defining) {
                flat.insert(name, defining);
            }
        }
    }
    let mut by_root: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for (name, module) in flat {
        let root = module.split('.').next().unwrap_or("").to_owned();
        by_root.entry(root).or_default().insert(name, module);
    }
    by_root
}

// --------------------------------------------------------------- the corpus

fn corpus() -> Option<(PathBuf, PathBuf)> {
    let base =
        PathBuf::from(std::env::var("LEAN_DOC_BASE_IR").unwrap_or_else(|_| DEFAULT_BASE_IR.into()));
    let fixtures = PathBuf::from(
        std::env::var("LEAN_DOC_MERGE_FIXTURES")
            .unwrap_or_else(|_| format!("{DEFAULT_REFERENCE}/fixtures")),
    );
    if !base.is_dir() || !fixtures.is_dir() {
        eprintln!(
            "skipping: no base IR at {} or no fixtures at {} — run tools/merge-reference.sh --impl ts",
            base.display(),
            fixtures.display()
        );
        return None;
    }
    Some((base, fixtures))
}

/// One round of the pipeline: `ownership`, then `merge`, over one edit.
struct Round<'a> {
    name: &'a str,
    /// The tree ownership diffs against and merge folds into.
    ir: PathBuf,
    /// Where merge writes. Equal to `ir` for an in-place round.
    out: PathBuf,
    inc: Option<PathBuf>,
    removed: Option<PathBuf>,
    exclude: Option<PathBuf>,
}

/// The whole harness, in process: nine rounds and three verifications, each file
/// compared with the size and digest the prototype produced.
///
/// This is `tools/merge-compare.sh` without the shell, and it is where
/// [`HARNESS_SCENARIOS`] is measured rather than assumed. The partial
/// extractions come from the harness's own `fixtures/` directory, so the
/// scenarios have exactly one definition.
#[test]
fn the_corpus_matches_the_prototype() {
    let e = expected();
    // Pinned whether or not the corpus is on this machine, so the numbers in the
    // report have a home.
    assert_eq!(e.value["baseModules"], 432);
    assert_eq!(e.value["baseDeclarations"], 4750);
    assert_eq!(e.value["files"].as_object().expect("a map").len(), 80);
    assert_eq!(e.value["trees"].as_object().expect("a map").len(), 9);

    let Some((base_ir, fixtures)) = corpus() else {
        return;
    };
    let work = TempDir::new("corpus");
    let mut covered: BTreeSet<&'static str> = BTreeSet::new();

    // The fixtures both implementations were fed, checked before they are used:
    // a fixture builder that drifted would otherwise look like a port bug.
    for (name, _) in e.value["files"].as_object().expect("a map") {
        if let Some(rest) = name.strip_prefix("fixtures/") {
            e.check(
                name,
                &fs::read(fixtures.join(rest)).expect("the fixture reads"),
            );
        }
    }

    let copy_of = |what: &str| -> PathBuf {
        let dir = work.path.join(what);
        copy_tree(&base_ir, &dir);
        dir
    };
    let removed_leaf = fixtures.join("removed-leaf.txt");
    let exclude_three = fixtures.join("exclude-three.txt");
    let restored = copy_of("restored");
    let copyout_base = copy_of("copyout-base");
    let copyout_merged = work.path.join("copyout-merged");
    let rounds = vec![
        Round {
            name: "rerun",
            ir: copy_of("rerun"),
            out: copy_of("rerun"),
            inc: Some(fixtures.join("inc-rerun")),
            removed: None,
            exclude: None,
        },
        Round {
            name: "modified",
            ir: copy_of("modified"),
            out: copy_of("modified"),
            inc: Some(fixtures.join("inc-modified")),
            removed: None,
            exclude: None,
        },
        Round {
            name: "moved",
            ir: copy_of("moved"),
            out: copy_of("moved"),
            inc: Some(fixtures.join("inc-moved")),
            removed: None,
            exclude: Some(exclude_three.clone()),
        },
        Round {
            name: "gained",
            ir: copy_of("gained"),
            out: copy_of("gained"),
            inc: Some(fixtures.join("inc-gained")),
            removed: None,
            exclude: None,
        },
        Round {
            name: "copyout",
            ir: copyout_base.clone(),
            out: copyout_merged.clone(),
            inc: Some(fixtures.join("inc-moved")),
            removed: None,
            exclude: None,
        },
        Round {
            name: "added",
            ir: copy_of("added"),
            out: copy_of("added"),
            inc: Some(fixtures.join("inc-added")),
            removed: None,
            exclude: None,
        },
        Round {
            name: "removed",
            ir: copy_of("removed"),
            out: copy_of("removed"),
            inc: None,
            removed: Some(removed_leaf.clone()),
            exclude: None,
        },
        Round {
            name: "restored-1",
            ir: restored.clone(),
            out: restored.clone(),
            inc: None,
            removed: Some(removed_leaf.clone()),
            exclude: None,
        },
        Round {
            name: "restored-2",
            ir: restored.clone(),
            out: restored.clone(),
            inc: Some(fixtures.join("inc-restored")),
            removed: None,
            exclude: None,
        },
    ];

    let mut module_files_checked = 0usize;
    let mut computed_files_checked = 0usize;
    for round in &rounds {
        let stale = work.path.join(format!("{}-stale.txt", round.name));
        let json = work.path.join(format!("{}-ownership.json", round.name));
        let changed = work.path.join(format!("{}-changed.txt", round.name));
        let timings = work.path.join(format!("{}-merge-timings.json", round.name));

        // ownership *before* merge: merge is about to overwrite the base IR's
        // idea of who owns each name.
        let (result, fired) = run_ownership(&OwnershipOptions {
            base: &round.ir,
            inc: round.inc.as_deref(),
            removed: round.removed.as_deref(),
            exclude: round.exclude.as_deref(),
            print_set: Some(&stale),
            json: Some(&json),
        });
        result.expect("the corpus round runs");
        covered.extend(fired);
        e.check(
            &format!("{}-stale.txt", round.name),
            &fs::read(&stale).expect("the stale set was written"),
        );
        check_normalised(&e, "ownership", round.name, &json, &["base", "inc"]);
        computed_files_checked += 2;

        let removed = read_lines(round.removed.as_deref());
        let (result, fired) = run_merge(&MergeOptions {
            base: &round.ir,
            inc: round.inc.as_deref(),
            out: &round.out,
            removed: &removed,
            modules: None,
            changed_out: Some(&changed),
            timings: Some(&timings),
        });
        result.expect("the corpus round merges");
        covered.extend(fired);
        e.check(
            &format!("{}-changed.txt", round.name),
            &fs::read(&changed).expect("the changed set was written"),
        );
        check_normalised(&e, "timings", round.name, &timings, &[]);
        computed_files_checked += 2;

        // The merged tree, snapshotted now: two rounds share one tree and the
        // second overwrites the first. Neither `index.json` nor `deps/*.json` is
        // the prototype's bytes: both are the from-scratch writer's, which is
        // what makes a merged tree a from-scratch one.
        let tree = Tree::open(&round.out).expect("the merged tree opens");
        e.check_diverged_index(
            &format!("{}-index.json", round.name),
            &fs::read(round.out.join("index.json")).expect("the index was written"),
        );
        computed_files_checked += 1;
        let by_root = dep_mapping_of(&tree);
        let schema = tree
            .index
            .get("schemaVersion")
            .cloned()
            .unwrap_or(Value::Null);
        assert_eq!(
            list_dir(&round.out.join("deps")),
            by_root
                .keys()
                .map(|root| format!("{root}.json"))
                .collect::<BTreeSet<String>>(),
            "{}: the dependency slices on disk are not the ones the refs ask for",
            round.name
        );
        for (root, mapping) in &by_root {
            let name = format!("{}-deps/{root}.json", round.name);
            e.check_diverged(
                &name,
                &fs::read(round.out.join(format!("deps/{root}.json"))).expect("written"),
                &lean_order_slice(mapping, root, &schema),
            );
            computed_files_checked += 1;
        }

        // Every module file, against the bytes it was copied from. A digest
        // would say less: this says which source each byte came from.
        let inc = round.inc.as_deref().and_then(Tree::open);
        for module in tree.module_names() {
            let file = tree
                .entry(&module)
                .and_then(|entry| entry.get("file")?.as_str().map(str::to_owned))
                .expect("every entry names a file");
            let source = match &inc {
                Some(inc) if inc.entry(&module).is_some() => inc.root.join(&file),
                _ => base_ir.join(&file),
            };
            assert_eq!(
                fs::read(round.out.join(&file)).expect("the module file is there"),
                fs::read(&source).expect("the source module file is there"),
                "{}: {module} is not the bytes it was copied from",
                round.name
            );
            module_files_checked += 1;
        }
        let expected_modules = e.value["trees"][round.name]["modules"]
            .as_u64()
            .expect("a module count") as usize;
        assert_eq!(tree.module_names().len(), expected_modules);
        assert_eq!(
            list_dir(&round.out.join("modules")).len(),
            expected_modules,
            "{}: an orphan module file survived in the merged tree",
            round.name
        );
    }

    // The three verifications.
    for (name, a) in [
        ("same", work.path.join("rerun")),
        ("moved", work.path.join("moved")),
        ("deleted", work.path.join("removed")),
    ] {
        let (result, fired) = run_verify(&a, &base_ir);
        let report = result.expect("verify reads both trees");
        covered.extend(fired);
        e.check(&format!("{name}-verify.txt"), report.to_text().as_bytes());
        e.check(
            &format!("{name}-verify-status.txt"),
            format!("{}\n", usize::from(report.problems > 0)).as_bytes(),
        );
        computed_files_checked += 2;
    }

    // Every file of the reference tree, and no more.
    let compared = e.seen.borrow().clone();
    let in_fixture: BTreeSet<String> = e.value["files"]
        .as_object()
        .expect("a map")
        .keys()
        .cloned()
        .collect();
    assert_eq!(
        compared, in_fixture,
        "the fixture and this test disagree about which files exist"
    );
    assert_eq!(compared.len(), 80);
    assert_eq!(module_files_checked, 3_890);
    assert_eq!(computed_files_checked, 76);

    // The divergence, pinned: the port differs from the prototype on these files
    // and on nothing else.
    let diverged = e.diverged.borrow().clone();
    assert_eq!(diverged.len(), 34);
    assert!(
        diverged.iter().all(
            |name| (name.contains("-deps/") || name.ends_with("-index.json"))
                && name.ends_with(".json")
        ),
        "the deliberate divergence has escaped deps/*.json and index.json: {diverged:?}"
    );
    assert_eq!(
        diverged
            .iter()
            .filter(|name| name.ends_with("-index.json"))
            .count(),
        9,
        "one merged index per round, and no more"
    );

    // How [`HARNESS_AND_INDEX`] was written: measured, then transcribed.
    // `LEAN_DOC_DUMP_BRANCHES=1 cargo test -p lean-doc-incr --test merge` prints
    // it again when a scenario is added, so the constant stays a record of a
    // measurement rather than a guess that has to be reverse-engineered.
    if std::env::var("LEAN_DOC_DUMP_BRANCHES").is_ok() {
        eprintln!("HARNESS_AND_INDEX = {covered:#?}");
    }
    assert_eq!(
        covered,
        BTreeSet::from(HARNESS_AND_INDEX),
        "which branches the harness reaches has changed"
    );
}

/// `<what>-<round>` in the fixture's `ownership` / `timings` sections: the
/// record with the fields that differ between two runs by construction dropped.
fn check_normalised(e: &Expected, section: &str, round: &str, path: &Path, drop: &[&str]) {
    let produced: Value =
        serde_json::from_str(&fs::read_to_string(path).expect("the record was written"))
            .expect("the record is JSON");
    let want = &e.value[section][round];
    let object = produced.as_object().expect("a JSON object");
    let mut got = serde_json::Map::new();
    for (key, value) in object {
        if drop.contains(&key.as_str()) || key.ends_with("Seconds") {
            continue;
        }
        got.insert(key.clone(), value.clone());
    }
    assert_eq!(
        &Value::Object(got.clone()),
        want,
        "{section}/{round} differs from the prototype's record"
    );
    // The dropped keys are dropped, not missing: a port that stopped writing
    // them would pass the comparison above.
    for key in drop {
        assert!(object.contains_key(*key), "{section}/{round} has no {key}");
    }
    assert!(
        object.keys().any(|key| key.ends_with("Seconds")),
        "{section}/{round} reports no durations at all"
    );
}

/// **The invariant M3-b bought**: an incremental tree's dependency slices are
/// byte for byte the ones a from-scratch extraction wrote.
///
/// Stated against the real `Extract.lean` output rather than against the writer
/// in [`lean_order_slice`], so it is the Lean side that is being matched and not
/// this file's idea of it. Only the rounds that do not change the dependency
/// *mapping* can be held to it — a round that adds a reference to a new
/// dependency name legitimately produces different bytes, and those are the ones
/// [`the_corpus_matches_the_prototype`] holds against the second writer instead.
#[test]
fn the_dependency_slices_are_the_from_scratch_bytes() {
    let Some((base_ir, fixtures)) = corpus() else {
        return;
    };
    let work = TempDir::new("from-scratch");
    let mut checked = 0usize;
    for (what, inc) in [
        ("rerun", "inc-rerun"),
        ("modified", "inc-modified"),
        ("moved", "inc-moved"),
        ("gained", "inc-gained"),
    ] {
        let ir = work.path.join(what);
        copy_tree(&base_ir, &ir);
        merge(&MergeOptions {
            base: &ir,
            inc: Some(&fixtures.join(inc)),
            out: &ir,
            removed: &[],
            modules: None,
            changed_out: None,
            timings: None,
        })
        .expect("the merge runs");
        for name in list_dir(&ir.join("deps")) {
            assert_eq!(
                fs::read(ir.join("deps").join(&name)).expect("the slice was written"),
                fs::read(base_ir.join("deps").join(&name)).expect("the from-scratch slice"),
                "{what}: deps/{name} is not what the extractor wrote"
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 12, "three slices in each of four trees");
}

/// The nested key order of a `serde_json::Value` survives a round trip.
///
/// [`lean_doc_incr::JsonObject`] controls the merged index's **top-level** key
/// order itself, but the entries inside `modules` are `Value`s carried through
/// verbatim, and that depends on the workspace's `preserve_order` feature. A
/// build that lost the feature would sort them and rewrite every index entry.
#[test]
fn nested_json_keeps_its_key_order() {
    let text = r#"{"module":"A","file":"modules/A.json","bytes":1,"contentHash":"z"}"#;
    let value: Value = serde_json::from_str(text).expect("parses");
    assert_eq!(serde_json::to_string(&value).expect("serialises"), text);
}

// ------------------------------------------------------- the curated cases

/// The dependency this milestone's coverage rests on, stated so that it fails
/// when it stops being true (plan §7: 全件バイト一致は分岐被覆の証明ではない).
///
/// Four claims, all counted rather than believed:
///
/// 1. A byte comparison of **one merged tree** reaches [`ONE_ROUND`] — 19 of the
///    71. Every edit the pipeline exists for is invisible to it.
/// 2. The whole harness reaches [`HARNESS_AND_INDEX`] — 48 of 71, measured in
///    [`the_corpus_matches_the_prototype`].
/// 3. The other 23 are reachable only by a written-down case, so every one of
///    them is one.
/// 4. Everything together is all 71.
#[test]
fn the_curated_cases_cover_what_the_package_does_not() {
    // (1) One round over a package shaped like the target: one module
    // re-extracted, folded in place, nothing removed.
    let repo = FakeIr::target_shaped("one-round");
    let fired = repo.one_round();
    // As above: `LEAN_DOC_DUMP_BRANCHES=1` reprints the measured set.
    if std::env::var("LEAN_DOC_DUMP_BRANCHES").is_ok() {
        eprintln!("ONE_ROUND = {fired:#?}");
    }
    assert_eq!(
        fired,
        BTreeSet::from(ONE_ROUND),
        "a byte comparison of one merged tree reaches a different set of branches than it did"
    );
    assert_eq!(fired.len(), 19);

    // (2) and (3): what the harness leaves for the curated cases.
    let harness = BTreeSet::from(HARNESS_AND_INDEX);
    assert_eq!(harness.len(), 48);
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

    // (4) Everything, together.
    let curated = {
        let mut curated = curated_ownership_branches();
        curated.extend(curated_merge_branches());
        curated.extend(curated_module_list_branches());
        curated.extend(curated_verify_branches());
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

/// The flags the pipeline always passes, the two sorts, and the shapes of a
/// module list only a hand edit produces.
fn curated_ownership_branches() -> BTreeSet<&'static str> {
    let mut covered = BTreeSet::new();

    // Neither output file: the sets are still computed and nothing is written.
    let repo = FakeIr::target_shaped("ownership-silent");
    let (result, fired) = run_ownership(&OwnershipOptions {
        base: &repo.base,
        inc: Some(&repo.inc),
        removed: None,
        exclude: None,
        print_set: None,
        json: None,
    });
    result.expect("the ownership runs");
    assert!(
        list_dir(&repo.dir.path)
            .iter()
            .all(|name| name != "stale.txt"),
        "nothing was asked for and nothing was written"
    );
    covered.extend(fired);

    // A `--removed` naming a module the base IR never had: dropped, because a
    // module with no history cannot have lost anything.
    let removed = repo.dir.path.join("ghosts.txt");
    fs::write(&removed, "Pkg.Ghost\n# a comment\n\n").expect("writable");
    let (result, fired) = run_ownership(&OwnershipOptions {
        base: &repo.base,
        inc: None,
        removed: Some(&removed),
        exclude: None,
        print_set: None,
        json: None,
    });
    let summary = result.expect("the ownership runs");
    assert_eq!(summary.removed_modules, 0);
    assert_eq!(summary.lost_names, 0);
    assert_eq!(
        summary.scanned_base_modules, 0,
        "nothing moved, so the base IR is not read at all"
    );
    covered.extend(fired);

    // A module that declares the same name twice: `lostNames` counts the array,
    // `lostNamesDistinct` the set. The two numbers are different on purpose.
    let twice = FakeIr::new("ownership-twice");
    twice.write_module(
        &twice.base,
        "Pkg.Twice",
        &[decl("Pkg.twin", &[]), decl("Pkg.twin", &[])],
    );
    twice.write_index(&twice.base, &["Pkg.Twice"], false);
    let removed = twice.dir.path.join("removed.txt");
    fs::write(&removed, "Pkg.Twice\n").expect("writable");
    let json = twice.dir.path.join("ownership.json");
    let (result, fired) = run_ownership(&OwnershipOptions {
        base: &twice.base,
        inc: None,
        removed: Some(&removed),
        exclude: None,
        print_set: None,
        json: Some(&json),
    });
    let summary = result.expect("the ownership runs");
    assert_eq!(summary.lost_names, 2, "the array, not the set");
    assert_eq!(summary.lost_names_distinct, 1);
    covered.extend(fired);

    // An exclude list longer than the base IR: `scannedBaseModules` goes
    // negative, as the prototype's subtraction of two unnested counts does.
    let exclude = twice.dir.path.join("exclude.txt");
    fs::write(&exclude, "A\nB\nC\nD\nE\n").expect("writable");
    let (result, fired) = run_ownership(&OwnershipOptions {
        base: &twice.base,
        inc: None,
        removed: Some(&removed),
        exclude: Some(&exclude),
        print_set: None,
        json: None,
    });
    assert_eq!(
        result.expect("the ownership runs").scanned_base_modules,
        1 - 6,
        "one module in the base, six names excluded"
    );
    covered.extend(fired);

    // Two stale modules whose names are the UTF-16 pair. The set reaches
    // `--print-set`, which the next round re-extracts, so its order is in a file
    // (plan §7, U1) — and `𝒜` comes first, which code point order reverses.
    let astral = format!("Pkg.{ASTRAL}");
    let ligature = format!("Pkg.{LIGATURE}");
    let moved = FakeIr::new("ownership-astral");
    // Two referrers, and one module that is about to lose the name they point at.
    moved.write_module(&moved.base, "Pkg.Owner", &[decl("Pkg.moved", &[])]);
    moved.write_module(
        &moved.base,
        &astral,
        &[decl("Pkg.a", &[("Pkg.Owner", "Pkg.moved")])],
    );
    moved.write_module(
        &moved.base,
        &ligature,
        &[decl("Pkg.b", &[("Pkg.Owner", "Pkg.moved")])],
    );
    moved.write_index(&moved.base, &["Pkg.Owner", &astral, &ligature], false);
    moved.write_module(&moved.inc, "Pkg.Owner", &[]);
    moved.write_index(&moved.inc, &["Pkg.Owner"], false);
    let stale = moved.dir.path.join("stale.txt");
    let (result, fired) = run_ownership(&OwnershipOptions {
        base: &moved.base,
        inc: Some(&moved.inc),
        removed: None,
        exclude: None,
        print_set: Some(&stale),
        json: None,
    });
    let summary = result.expect("the ownership runs");
    assert_eq!(summary.stale_modules, [astral.clone(), ligature.clone()]);
    let mut by_code_point = vec![astral.clone(), ligature.clone()];
    by_code_point.sort();
    assert_eq!(
        by_code_point,
        [ligature.clone(), astral.clone()],
        "the two orders really do disagree about this pair"
    );
    assert_eq!(
        fs::read_to_string(&stale).expect("written"),
        format!("{astral}\n{ligature}\n")
    );
    covered.extend(fired);

    // More than twenty witnesses: the summary keeps the first twenty, the set
    // keeps all of them.
    let many = FakeIr::new("ownership-many");
    many.write_module(&many.base, "Pkg.Owner", &[decl("Pkg.moved", &[])]);
    let mut names: Vec<String> = vec!["Pkg.Owner".to_owned()];
    for i in 0..25 {
        let module = format!("Pkg.Referrer{i:02}");
        many.write_module(
            &many.base,
            &module,
            &[decl(
                &format!("Pkg.r{i:02}"),
                &[("Pkg.Owner", "Pkg.moved"), ("Pkg.Owner", "Pkg.moved")],
            )],
        );
        names.push(module);
    }
    many.write_index(
        &many.base,
        &names.iter().map(String::as_str).collect::<Vec<_>>(),
        false,
    );
    many.write_module(&many.inc, "Pkg.Owner", &[]);
    many.write_index(&many.inc, &["Pkg.Owner"], false);
    let json = many.dir.path.join("ownership.json");
    let (result, fired) = run_ownership(&OwnershipOptions {
        base: &many.base,
        inc: Some(&many.inc),
        removed: None,
        exclude: None,
        print_set: None,
        json: Some(&json),
    });
    let summary = result.expect("the ownership runs");
    assert_eq!(summary.stale_modules.len(), 25);
    assert_eq!(
        summary.witnesses.len(),
        25,
        "one witness per module per rule, however many references fired it"
    );
    let written: Value =
        serde_json::from_str(&fs::read_to_string(&json).expect("written")).expect("JSON");
    assert_eq!(
        written["witnesses"].as_array().expect("an array").len(),
        lean_doc_incr::WITNESSES_IN_SUMMARY
    );
    assert_eq!(written["stale"], 25);
    covered.extend(fired);

    covered
}

/// The dependency slice's shapes, the index's, and the flags the pipeline always
/// passes.
fn curated_merge_branches() -> BTreeSet<&'static str> {
    let mut covered = BTreeSet::new();

    // A package that refers to nothing outside itself: no dependency slice at
    // all, and `deps/` stays empty rather than holding an empty file.
    let alone = FakeIr::new("merge-alone");
    alone.write_module(
        &alone.base,
        "Pkg.A",
        &[decl("Pkg.a", &[("Pkg.A", "Pkg.a")])],
    );
    alone.write_index(&alone.base, &["Pkg.A"], false);
    alone.write_module(&alone.inc, "Pkg.A", &[decl("Pkg.a", &[])]);
    alone.write_index(&alone.inc, &["Pkg.A"], false);
    let (result, fired) = run_merge(&MergeOptions {
        base: &alone.base,
        inc: Some(&alone.inc),
        out: &alone.base,
        removed: &[],
        modules: None,
        changed_out: None,
        timings: None,
    });
    let summary = result.expect("the merge runs");
    assert!(summary.dep_maps.is_empty());
    assert!(list_dir(&alone.base.join("deps")).is_empty());
    assert_eq!(
        Tree::open(&alone.base).expect("opens").index["dependencyMaps"],
        json!([]),
        "an empty dependency list is an empty array, not a missing key"
    );
    covered.extend(fired);

    // A base index with no `schemaVersion`: the slice is written without one,
    // which is what `JSON.stringify` does with an undefined property.
    let bare = FakeIr::new("merge-bare-index");
    bare.write_module(&bare.base, "Pkg.A", &[decl("Pkg.a", &[("Dep.M", "Dep.x")])]);
    bare.write_index_without_schema(&bare.base, &["Pkg.A"]);
    bare.write_module(&bare.inc, "Pkg.A", &[decl("Pkg.a", &[("Dep.M", "Dep.x")])]);
    bare.write_index(&bare.inc, &["Pkg.A"], false);
    let (result, fired) = run_merge(&MergeOptions {
        base: &bare.base,
        inc: Some(&bare.inc),
        out: &bare.base,
        removed: &[],
        modules: None,
        changed_out: None,
        timings: None,
    });
    result.expect("the merge runs");
    assert_eq!(
        fs::read_to_string(bare.base.join("deps/Dep.json")).expect("written"),
        r#"{"declarations":{"Dep.x":"Dep.M"},"package":"Dep"}"#
    );
    covered.extend(fired);

    // Dependency names above the BMP. Lean's writer sorts by code point, so the
    // ligature comes **first** — the opposite of the prototype's UTF-16 order,
    // and the whole visible content of M3-b's divergence.
    let astral = FakeIr::new("merge-astral-deps");
    let refs: Vec<(String, String)> = vec![
        ("Dep.M".to_owned(), format!("Dep.{ASTRAL}")),
        ("Dep.M".to_owned(), format!("Dep.{LIGATURE}")),
    ];
    let borrowed: Vec<(&str, &str)> = refs.iter().map(|(m, n)| (m.as_str(), n.as_str())).collect();
    astral.write_module(&astral.base, "Pkg.A", &[decl("Pkg.a", &borrowed)]);
    astral.write_index(&astral.base, &["Pkg.A"], false);
    astral.write_module(&astral.inc, "Pkg.A", &[decl("Pkg.a", &borrowed)]);
    astral.write_index(&astral.inc, &["Pkg.A"], false);
    let (result, fired) = run_merge(&MergeOptions {
        base: &astral.base,
        inc: Some(&astral.inc),
        out: &astral.base,
        removed: &[],
        modules: None,
        changed_out: None,
        timings: None,
    });
    result.expect("the merge runs");
    let written = fs::read_to_string(astral.base.join("deps/Dep.json")).expect("written");
    assert_eq!(
        written,
        format!(
            "{{\"declarations\":{{\"Dep.{LIGATURE}\":\"Dep.M\",\"Dep.{ASTRAL}\":\"Dep.M\"}},\
             \"package\":\"Dep\",\"schemaVersion\":4}}"
        ),
        "code point order: the prototype's UTF-16 sort would put the astral name first"
    );
    let mut utf16 = vec![format!("Dep.{ASTRAL}"), format!("Dep.{LIGATURE}")];
    utf16.sort_by(|a, b| cmp_utf16(a, b));
    assert_eq!(
        utf16,
        [format!("Dep.{ASTRAL}"), format!("Dep.{LIGATURE}")],
        "the two orders really do disagree about this pair"
    );
    covered.extend(fired);

    // Dependency **roots** above the BMP. The `dependencyMaps` array order is
    // part of `index.json`, which this milestone writes the from-scratch way, so
    // it follows `Extract.lean:2050` (code point order) and not the prototype's
    // `.sort()` (UTF-16, plan §7 U1). The real roots (`Init` / `Lean` /
    // `Mathlib`) are ASCII, where the two agree, so only a built pair shows it.
    let roots = FakeIr::new("merge-astral-roots");
    let refs: Vec<(String, String)> = vec![
        (format!("{ASTRAL}.M"), format!("{ASTRAL}.x")),
        (format!("{LIGATURE}.M"), format!("{LIGATURE}.y")),
    ];
    let borrowed: Vec<(&str, &str)> = refs.iter().map(|(m, n)| (m.as_str(), n.as_str())).collect();
    roots.write_module(&roots.base, "Pkg.A", &[decl("Pkg.a", &borrowed)]);
    roots.write_index(&roots.base, &["Pkg.A"], false);
    roots.write_module(&roots.inc, "Pkg.A", &[decl("Pkg.a", &borrowed)]);
    roots.write_index(&roots.inc, &["Pkg.A"], false);
    let (result, fired) = run_merge(&MergeOptions {
        base: &roots.base,
        inc: Some(&roots.inc),
        out: &roots.base,
        removed: &[],
        modules: None,
        changed_out: None,
        timings: None,
    });
    let summary = result.expect("the merge runs");
    assert_eq!(
        summary
            .dep_maps
            .iter()
            .map(|record| record.package.as_str())
            .collect::<Vec<_>>(),
        [LIGATURE, ASTRAL],
        "code point order: the prototype's UTF-16 sort would put the astral root first"
    );
    let mut by_utf16 = vec![LIGATURE, ASTRAL];
    by_utf16.sort_by(|a, b| cmp_utf16(a, b));
    assert_eq!(
        by_utf16,
        [ASTRAL, LIGATURE],
        "the two orders really do disagree about this pair"
    );
    covered.extend(fired);

    // One name owned by two modules at once: the last writer wins, as
    // `Map.prototype.set` does. The modules are visited in index order.
    let clash = FakeIr::new("merge-clash");
    clash.write_module(
        &clash.base,
        "Pkg.A",
        &[decl("Pkg.a", &[("Dep.First", "Dep.x")])],
    );
    clash.write_module(
        &clash.base,
        "Pkg.B",
        &[decl("Pkg.b", &[("Dep.Second", "Dep.x")])],
    );
    clash.write_index(&clash.base, &["Pkg.A", "Pkg.B"], false);
    clash.write_module(
        &clash.inc,
        "Pkg.B",
        &[decl("Pkg.b", &[("Dep.Second", "Dep.x")])],
    );
    clash.write_index(&clash.inc, &["Pkg.B"], false);
    let (result, fired) = run_merge(&MergeOptions {
        base: &clash.base,
        inc: Some(&clash.inc),
        out: &clash.base,
        removed: &[],
        modules: None,
        changed_out: None,
        timings: None,
    });
    result.expect("the merge runs");
    assert_eq!(
        fs::read_to_string(clash.base.join("deps/Dep.json")).expect("written"),
        r#"{"declarations":{"Dep.x":"Dep.Second"},"package":"Dep","schemaVersion":4}"#,
        "the later module in index order owns the name"
    );
    covered.extend(fired);

    // The same module named twice in `--remove`, and one that is not in the
    // index at all. Neither is an error: the list is the caller's idea of what
    // vanished, and the index is the authority on what was there.
    let twice = FakeIr::target_shaped("merge-remove-twice");
    let (result, fired) = run_merge(&MergeOptions {
        base: &twice.base,
        inc: None,
        out: &twice.base,
        removed: &[
            "Pkg.A".to_owned(),
            "Pkg.A".to_owned(),
            "Pkg.Ghost".to_owned(),
        ],
        modules: None,
        changed_out: None,
        timings: None,
    });
    let summary = result.expect("the merge runs");
    assert_eq!(summary.removed, 1, "one module left the index, not two");
    assert!(!twice.base.join("modules/Pkg.A.json").exists());
    covered.extend(fired);

    // An ablated base index: `ablations` is the one top-level key `merge` never
    // models, and it has to come out in the place the base index had it —
    // first, since `Json.mkObj` sorts and `a` precedes `d`.
    let ablated = FakeIr::new("merge-ablated");
    ablated.write_module(
        &ablated.base,
        "Pkg.A",
        &[decl("Pkg.a", &[("Dep.M", "Dep.x")])],
    );
    ablated.write_index(&ablated.base, &["Pkg.A"], true);
    ablated.write_module(
        &ablated.inc,
        "Pkg.A",
        &[decl("Pkg.a", &[("Dep.M", "Dep.x")])],
    );
    ablated.write_index(&ablated.inc, &["Pkg.A"], false);
    let (result, fired) = run_merge(&MergeOptions {
        base: &ablated.base,
        inc: Some(&ablated.inc),
        out: &ablated.base,
        removed: &[],
        modules: None,
        changed_out: None,
        timings: None,
    });
    result.expect("the merge runs");
    let written = fs::read_to_string(ablated.base.join("index.json")).expect("written");
    assert!(
        written.starts_with(r#"{"ablations":["members"],"declarationCount":"#),
        "the refusal marker moved or vanished: {}",
        &written[..written.len().min(80)]
    );
    covered.extend(fired);

    // An index whose entries are not objects: refused (exit 3) rather than
    // written out as a file called `undefined`.
    let broken = FakeIr::target_shaped("merge-broken-index");
    fs::write(
        broken.base.join("index.json"),
        r#"{"schemaVersion":4,"modules":["Pkg.A"],"dependencyMaps":[]}"#,
    )
    .expect("writable");
    let (result, fired) = run_merge(&MergeOptions {
        base: &broken.base,
        inc: Some(&broken.inc),
        out: &broken.base,
        removed: &[],
        modules: None,
        changed_out: None,
        timings: None,
    });
    let error = result.expect_err("an index of strings is not an index");
    assert_eq!(error.exit_code(), 3);
    assert!(error.to_string().contains("index entry"), "{error}");
    covered.extend(fired);

    covered
}

/// `--modules` (M3-d2b): the list orders the index, and a list that does not
/// describe the merged tree is refused.
///
/// A case of its own rather than more of [`curated_merge_branches`], because it
/// is the one flag here that the prototype offers and never reads — so what it
/// does is this port's own claim, and the claim is worth a name.
#[test]
fn the_module_list_orders_the_index_or_is_refused() {
    curated_module_list_branches();
}

fn curated_module_list_branches() -> BTreeSet<&'static str> {
    let mut covered = BTreeSet::new();

    // The merged index follows the package's list, and the list is deliberately
    // in **neither** sorted order nor the one the append rule produces — so
    // "it happened to be sorted" cannot pass this.
    let build = |what: &str| -> FakeIr {
        let repo = FakeIr::new(what);
        // Two modules referring to the same dependency **name** through two
        // different defining modules: the slice's last writer wins, so the
        // module order reaches bytes outside `index.json` too.
        repo.write_module(
            &repo.base,
            "Pkg.Zeta",
            &[decl("Pkg.z", &[("Dep.Second", "Dep.x")])],
        );
        repo.write_module(
            &repo.base,
            "Pkg.Alpha",
            &[decl("Pkg.a", &[("Dep.First", "Dep.x")])],
        );
        // The base index is in the order the extractor wrote it, which is not
        // the sorted one either.
        repo.write_index(&repo.base, &["Pkg.Zeta", "Pkg.Alpha"], false);
        repo.write_module(
            &repo.inc,
            "Pkg.Mid",
            &[decl("Pkg.m", &[("Dep.Third", "Dep.z")])],
        );
        repo.write_index(&repo.inc, &["Pkg.Mid"], false);
        repo
    };
    let listed = [
        "Pkg.Mid".to_owned(),
        "Pkg.Alpha".to_owned(),
        "Pkg.Zeta".to_owned(),
    ];
    let mut sorted = listed.to_vec();
    sorted.sort_by(|a, b| cmp_utf16(a, b));
    assert_ne!(
        sorted,
        listed.to_vec(),
        "the list is in sorted order, so following it would prove nothing"
    );
    let with = build("merge-modules-order");
    let (result, fired) = run_merge(&MergeOptions {
        base: &with.base,
        inc: Some(&with.inc),
        out: &with.base,
        removed: &[],
        modules: Some(&listed),
        changed_out: None,
        timings: None,
    });
    result.expect("the merge runs");
    assert_eq!(
        Tree::open(&with.base).expect("opens").module_names(),
        listed,
        "the merged index is not in the list's order",
    );
    covered.extend(fired);

    // The same trees, the same round, no list: the base index's order with the
    // new module appended — **and a different `deps/Dep.json`**, because the
    // slice is walked in that order and its last writer wins.
    let without = build("merge-modules-order-control");
    let (result, fired) = run_merge(&MergeOptions {
        base: &without.base,
        inc: Some(&without.inc),
        out: &without.base,
        removed: &[],
        modules: None,
        changed_out: None,
        timings: None,
    });
    result.expect("the merge runs");
    assert_eq!(
        Tree::open(&without.base).expect("opens").module_names(),
        ["Pkg.Zeta", "Pkg.Alpha", "Pkg.Mid"],
        "the append rule moved",
    );
    assert_eq!(
        fs::read_to_string(with.base.join("deps/Dep.json")).expect("written"),
        r#"{"declarations":{"Dep.x":"Dep.Second","Dep.z":"Dep.Third"},"package":"Dep","schemaVersion":4}"#,
    );
    assert_eq!(
        fs::read_to_string(without.base.join("deps/Dep.json")).expect("written"),
        r#"{"declarations":{"Dep.x":"Dep.First","Dep.z":"Dep.Third"},"package":"Dep","schemaVersion":4}"#,
        "the list reached only the index, so a merge that reordered nothing else would pass",
    );
    covered.extend(fired);

    // A list naming a module the merged tree has nothing behind: **exit 3, with
    // the tree untouched**. Following it would mean writing an index without
    // that module — a file on disk that every later stage reads as absent.
    let ghost = FakeIr::target_shaped("merge-modules-ghost");
    let before = tree_bytes(&ghost.base);
    let listed = [
        "Pkg.A".to_owned(),
        "Pkg.B".to_owned(),
        "Pkg.Ghost".to_owned(),
    ];
    let (result, fired) = run_merge(&MergeOptions {
        base: &ghost.base,
        inc: Some(&ghost.inc),
        out: &ghost.base,
        removed: &[],
        modules: Some(&listed),
        changed_out: None,
        timings: None,
    });
    let error = result.expect_err("a list naming a module nothing produced");
    assert_eq!(error.exit_code(), 3);
    let message = error.to_string();
    assert!(message.contains("Pkg.Ghost"), "{message}");
    assert!(
        message.contains("0 in the merged tree"),
        "the other direction is reported as empty rather than left out: {message}",
    );
    assert_eq!(
        before,
        tree_bytes(&ghost.base),
        "the refusal came after something was written",
    );
    covered.extend(fired);

    // The other direction: a module in the merged tree the list does not name.
    // Following it would append that one, which is exactly the divergence from a
    // from-scratch extraction M3-d2b removed.
    let stale = FakeIr::target_shaped("merge-modules-stale-list");
    let before = tree_bytes(&stale.base);
    let listed = ["Pkg.B".to_owned()];
    let (result, fired) = run_merge(&MergeOptions {
        base: &stale.base,
        inc: Some(&stale.inc),
        out: &stale.base,
        removed: &[],
        modules: Some(&listed),
        changed_out: None,
        timings: None,
    });
    let error = result.expect_err("a list that has fallen behind the tree");
    assert_eq!(error.exit_code(), 3);
    let message = error.to_string();
    assert!(message.contains("Pkg.A"), "{message}");
    assert!(message.contains("1 in the merged tree"), "{message}");
    assert_eq!(before, tree_bytes(&stale.base));
    covered.extend(fired);

    // A list that names one module twice is one list, not two modules: the
    // repeat is dropped and the first position stands, as an index's own
    // repeated entry does (`module_map`).
    let twice = FakeIr::target_shaped("merge-modules-twice");
    let listed = ["Pkg.B".to_owned(), "Pkg.A".to_owned(), "Pkg.B".to_owned()];
    let (result, fired) = run_merge(&MergeOptions {
        base: &twice.base,
        inc: Some(&twice.inc),
        out: &twice.base,
        removed: &[],
        modules: Some(&listed),
        changed_out: None,
        timings: None,
    });
    let summary = result.expect("the merge runs");
    assert_eq!(summary.modules, 2, "the repeat became a second entry");
    let merged = Tree::open(&twice.base).expect("opens");
    assert_eq!(merged.module_names(), ["Pkg.B", "Pkg.A"]);
    assert_eq!(merged.index["moduleCount"], json!(2));
    covered.extend(fired);

    covered
}

/// The two `verify` answers no scenario over the base IR produces.
fn curated_verify_branches() -> BTreeSet<&'static str> {
    let mut covered = BTreeSet::new();

    // B has a module A does not: the count differs in the other direction, and
    // nothing is "missing in B".
    let short = FakeIr::new("verify-short");
    short.write_module(&short.base, "Pkg.A", &[decl("Pkg.a", &[])]);
    short.write_index(&short.base, &["Pkg.A"], false);
    short.write_module(&short.inc, "Pkg.A", &[decl("Pkg.a", &[])]);
    short.write_module(&short.inc, "Pkg.B", &[decl("Pkg.b", &[])]);
    short.write_index(&short.inc, &["Pkg.A", "Pkg.B"], false);
    let (result, fired) = run_verify(&short.base, &short.inc);
    let report = result.expect("verify reads both trees");
    assert_eq!(report.lines[0], "FAIL module count 1 vs 2");
    assert_eq!(report.problems, 1);
    covered.extend(fired);

    // The same name in both trees, owned by different modules — and more than
    // ten of them, so the transcript stops naming and starts counting.
    let left = FakeIr::new("verify-deps-left");
    let right = FakeIr::new("verify-deps-right");
    for (repo, owner) in [(&left, "Dep.First"), (&right, "Dep.Second")] {
        let refs: Vec<(String, String)> = (0..12)
            .map(|i| (owner.to_owned(), format!("Dep.x{i:02}")))
            .collect();
        let borrowed: Vec<(&str, &str)> =
            refs.iter().map(|(m, n)| (m.as_str(), n.as_str())).collect();
        repo.write_module(&repo.base, "Pkg.A", &[decl("Pkg.a", &borrowed)]);
        repo.write_index(&repo.base, &["Pkg.A"], false);
        // The slice is what merge would have written.
        merge(&MergeOptions {
            base: &repo.base,
            inc: None,
            out: &repo.base,
            removed: &["Pkg.Ghost".to_owned()],
            modules: None,
            changed_out: None,
            timings: None,
        })
        .expect("the merge runs");
    }
    let (result, fired) = run_verify(&left.base, &right.base);
    let report = result.expect("verify reads both trees");
    assert_eq!(report.dependency_mismatches, 12);
    let named = report
        .lines
        .iter()
        .filter(|line| line.starts_with("FAIL dep "))
        .count();
    assert_eq!(named, 10, "the transcript names ten and counts the rest");
    assert!(
        report
            .lines
            .contains(&"dependency map entries: 12 vs 12, mismatches 12".to_owned())
    );
    covered.extend(fired);

    covered
}

// -------------------------------------------------------------- the fake IR

/// A synthetic IR tree pair: a `base` and an `inc`, both schema 4.
struct FakeIr {
    dir: TempDir,
    base: PathBuf,
    inc: PathBuf,
}

impl FakeIr {
    fn new(what: &str) -> Self {
        let dir = TempDir::new(what);
        let base = dir.path.join("base");
        let inc = dir.path.join("inc");
        for root in [&base, &inc] {
            fs::create_dir_all(root.join("modules")).expect("creatable");
            fs::create_dir_all(root.join("deps")).expect("creatable");
        }
        Self { dir, base, inc }
    }

    /// The shape one round over the target package has: two modules, one of them
    /// re-extracted with different bytes, one dependency package, and an index
    /// carrying a key this crate does not model.
    fn target_shaped(what: &str) -> Self {
        let repo = Self::new(what);
        repo.write_module(
            &repo.base,
            "Pkg.A",
            &[decl("Pkg.a", &[("Dep.M", "Dep.x"), ("Dep.N", "Dep.z")])],
        );
        repo.write_module(&repo.base, "Pkg.B", &[decl("Pkg.b", &[("Dep.M", "Dep.y")])]);
        repo.write_index(&repo.base, &["Pkg.A", "Pkg.B"], false);
        repo.write_module(&repo.inc, "Pkg.B", &[decl("Pkg.b", &[("Dep.M", "Dep.y")])]);
        repo.write_index(&repo.inc, &["Pkg.B"], false);
        // The incremental entry gets a different hash: a re-extraction whose IR
        // moved is what a round is for.
        let path = repo.inc.join("index.json");
        let mut index: Value =
            serde_json::from_str(&fs::read_to_string(&path).expect("reads")).expect("JSON");
        index["modules"][0]["contentHash"] = json!("0000000000000001");
        fs::write(&path, index.to_string()).expect("writable");
        repo
    }

    /// One round, observed: `ownership` then `merge`, in place, with every
    /// output file the pipeline asks for.
    fn one_round(&self) -> BTreeSet<&'static str> {
        let mut fired = BTreeSet::new();
        let stale = self.dir.path.join("stale.txt");
        let json = self.dir.path.join("ownership.json");
        let (result, own) = run_ownership(&OwnershipOptions {
            base: &self.base,
            inc: Some(&self.inc),
            removed: None,
            exclude: None,
            print_set: Some(&stale),
            json: Some(&json),
        });
        result.expect("the round runs");
        fired.extend(own);
        let changed = self.dir.path.join("changed.txt");
        let timings = self.dir.path.join("timings.json");
        let (result, merged) = run_merge(&MergeOptions {
            base: &self.base,
            inc: Some(&self.inc),
            out: &self.base,
            // The common round has no deletion at all: `incremental.sh` passes
            // `--remove` only in the first round, and only when something went.
            removed: &[],
            modules: None,
            changed_out: Some(&changed),
            timings: Some(&timings),
        });
        result.expect("the round merges");
        fired.extend(merged);
        fired
    }

    fn write_module(&self, root: &Path, module: &str, declarations: &[Value]) {
        let body = json!({
            "schemaVersion": 4,
            "module": module,
            "imports": [],
            "moduleDocs": [],
            "tactics": [],
            "declarations": declarations,
        });
        fs::write(
            root.join(format!("modules/{module}.json")),
            body.to_string(),
        )
        .expect("writable");
    }

    /// `ablated` writes the refusal marker `Extract.lean` puts in an IR that is
    /// deliberately incomplete. It is the one top-level key `merge` never models
    /// and always has to carry through, in its own alphabetical place.
    fn write_index(&self, root: &Path, modules: &[&str], ablated: bool) {
        let mut index = self.index_value(root, modules);
        if ablated {
            let object = index.as_object_mut().expect("an object");
            object.insert("ablations".to_owned(), json!(["members"]));
            object.sort_keys();
        }
        fs::write(root.join("index.json"), index.to_string()).expect("writable");
    }

    fn write_index_without_schema(&self, root: &Path, modules: &[&str]) {
        let mut index = self.index_value(root, modules);
        index
            .as_object_mut()
            .expect("an object")
            .shift_remove("schemaVersion");
        fs::write(root.join("index.json"), index.to_string()).expect("writable");
    }

    /// Keys in the alphabetical order `Json.mkObj` produces, so a merged index
    /// looks like one the extractor wrote.
    fn index_value(&self, root: &Path, modules: &[&str]) -> Value {
        let entries: Vec<Value> = modules
            .iter()
            .map(|module| {
                let file = format!("modules/{module}.json");
                let raw = fs::read(root.join(&file)).expect("the module file was written first");
                let body: Value = serde_json::from_slice(&raw).expect("JSON");
                json!({
                    "bytes": raw.len(),
                    "contentHash": fnv1a64(&raw),
                    "declarations": body["declarations"].as_array().expect("an array").len(),
                    "file": file,
                    "module": module,
                })
            })
            .collect();
        json!({
            "declarationCount": entries.len(),
            "dependencyMaps": [],
            "generator": "lean-doc/tests",
            "hashAlgorithm": "fnv1a64",
            "leanVersion": "4.31.0",
            "moduleCount": entries.len(),
            "modules": entries,
            "schemaVersion": 4,
        })
    }
}

/// One declaration, with the schema-4 keys the reader insists on.
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
        "endCol": 8,
        "index": 0,
        "members": [],
        "doc": null,
        "equations": [],
        "equationCode": [],
        "refs": refs.iter().map(|(m, n)| json!([m, n])).collect::<Vec<_>>(),
    })
}

/// Every file under `root`, keyed by its path relative to it.
///
/// What a refusal has to have left alone: `merge` decides whether the module
/// list describes the tree **before** it creates a directory, so a refused run
/// is one a caller can fix the list and repeat on the same tree.
fn tree_bytes(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut stack = vec![root.to_owned()];
    while let Some(dir) = stack.pop() {
        let Ok(listing) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in listing.flatten() {
            let path = entry.path();
            if entry.file_type().expect("a file type").is_dir() {
                stack.push(path);
            } else {
                let key = path
                    .strip_prefix(root)
                    .expect("under the root")
                    .to_string_lossy()
                    .into_owned();
                files.insert(key, fs::read(&path).expect("a readable file"));
            }
        }
    }
    files
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
            "lean-doc-merge-{}-{}-{slug}",
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
