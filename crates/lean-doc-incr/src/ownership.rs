//! The `ownership` stage (layer L3-1): which modules point at a name that has
//! moved.
//!
//! Ported from `experiments/stage5/ownership.ts` (frozen). Milestone **M3-b**.
//!
//! ```text
//! base IR + inc IR ──> lost(M)   names M no longer defines
//!                      gained(M) names M now defines
//!                        │
//! every other base module's refs ──> stale modules ──> --print-set
//! ```
//!
//! # The hole this closes, and why nothing else closes it
//!
//! The IR stores every reference as a `(defining module, name)` pair, because
//! the printed token and the constant it links to often have no textual relation
//! (`ℕ` -> `Nat`). That pair is a fact about *where a name lives*, and it goes
//! stale when the name moves — even though nothing about the referring module
//! changed.
//!
//! Stage 5c measured that this is invisible to every other layer: moving a
//! declaration from A to X leaves the referring module C's `.olean` **byte
//! identical** (and Lake's hash unmoved), so [`crate::detect`] cannot see it, and
//! no widening of the *render* set can fix it either — the stale bytes are in
//! C's IR, so C has to be re-**extracted**. Renaming, by contrast, does change
//! C's olean, because the new name is embedded in C's terms.
//!
//! # Why it runs before the merge
//!
//! It needs the base IR's idea of who owns each name, and the merge is about to
//! overwrite exactly that (plan §6, constraint 1). The two stages read the same
//! tree in the same round and only one order gives an answer.
//!
//! # Deletion and relocation are one mechanism
//!
//! `--removed` names modules that no longer exist. Every name they defined is
//! lost, which is the same computation as a move with an empty "gained" side —
//! so there is one rule here, not two.
//!
//! Reads the IR only. Lean is never started and the measurement target is never
//! touched.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::time::Instant;

use lean_doc_ir::{IrTree, sort_utf16};
use serde::Serialize;

use crate::detect::{Error, read_module_list, write, write_text};

/// What `ownership` needs to know.
#[derive(Clone, Copy, Debug)]
pub struct OwnershipOptions<'a> {
    /// The IR as it was before this round.
    pub base: &'a Path,
    /// The partial extraction's tree. `None` is a real case, not a misuse: a
    /// pure deletion re-extracts nothing.
    pub inc: Option<&'a Path>,
    /// Modules that no longer exist, one per line.
    pub removed: Option<&'a Path>,
    /// Modules already scheduled for re-extraction, one per line. They are fresh
    /// by definition and are never reported. Normally the changed-set file.
    pub exclude: Option<&'a Path>,
    /// The stale modules, one per line — the next round's input.
    pub print_set: Option<&'a Path>,
    /// The diagnostic summary.
    pub json: Option<&'a Path>,
}

/// One reason one module was called stale, for the log and the summary.
///
/// Field order is the file's: `ownership.ts` pushes an object literal in this
/// order and `JSON.stringify` keeps it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Witness {
    pub module: String,
    /// `lostOwner` or `movedElsewhere`.
    pub rule: &'static str,
    /// The reference that gave the module away: `[defining module, name]`.
    #[serde(rename = "ref")]
    pub reference: [String; 2],
}

/// The two rules, kept apart so the summary can report which one fired.
///
/// (b) is not implied by (a): (a) needs the losing module itself to be in the
/// re-extracted set, which holds for a move (removing a declaration changes the
/// source module's olean) but is worth checking separately rather than assuming.
pub const RULE_LOST_OWNER: &str = "lostOwner";
pub const RULE_MOVED_ELSEWHERE: &str = "movedElsewhere";

/// What `ownership` found.
///
/// **No `PartialEq`**: the three `*Seconds` are wall clock, and a summary that
/// compares equal to another one would be asserting on them.
#[derive(Clone, Debug)]
pub struct OwnershipSummary {
    pub inc_modules: usize,
    pub removed_modules: usize,
    /// `baseEntries.length - exclude.size`, or 0 when nothing was watched.
    ///
    /// **Signed**, because the prototype subtracts two counts that are not
    /// nested: the exclude file may name modules the base IR never had, and then
    /// the prototype reports a negative number rather than none at all.
    pub scanned_base_modules: i64,
    /// Occurrences, not distinct names: one per (name, module) pair.
    pub lost_names: usize,
    pub gained_names: usize,
    pub lost_names_distinct: usize,
    pub gained_names_distinct: usize,
    pub stale_by_lost_owner: usize,
    pub stale_by_moved_elsewhere: usize,
    /// The union of the two rules, in **UTF-16 order** (plan §7, U1): the order
    /// reaches `--print-set`, which the next round re-extracts.
    pub stale_modules: Vec<String>,
    /// Every witness, in scan order — at most one per (module, rule). The
    /// summary file keeps the first [`WITNESSES_IN_SUMMARY`], the log the first
    /// [`WITNESSES_IN_LOG`].
    pub witnesses: Vec<Witness>,
    pub diff_seconds: f64,
    pub scan_seconds: f64,
    pub total_seconds: f64,
}

/// How many witnesses the `--json` summary keeps.
pub const WITNESSES_IN_SUMMARY: usize = 20;
/// How many witnesses the log line prints.
pub const WITNESSES_IN_LOG: usize = 10;

/// Diffs the re-extracted modules against the base IR and scans every other
/// module's references for names that moved.
pub fn ownership(options: &OwnershipOptions<'_>) -> Result<OwnershipSummary, Error> {
    let started = Instant::now();
    let base = IrTree::open_unvalidated(options.base).map_err(Error::Ir)?;
    let base_file_of: HashMap<&str, &lean_doc_ir::IndexEntry> = base
        .index()
        .modules
        .iter()
        .map(|entry| (entry.module.as_str(), entry))
        .collect();

    let inc = match options.inc {
        Some(dir) => Some(IrTree::open_unvalidated(dir).map_err(Error::Ir)?),
        None => None,
    };
    let inc_entries: &[lean_doc_ir::IndexEntry] =
        inc.as_ref().map_or(&[], |tree| &tree.index().modules);
    let fresh: HashSet<&str> = inc_entries
        .iter()
        .map(|entry| entry.module.as_str())
        .collect();

    // A module that is not in the base IR cannot have lost anything, so a name
    // in `--removed` that the base never had is dropped rather than being an
    // error.
    let removed_modules: Vec<String> = match options.removed {
        Some(path) => read_module_list(path)?
            .into_iter()
            .filter(|module| base_file_of.contains_key(module.as_str()))
            .collect(),
        None => Vec::new(),
    };

    // ------------------------------------------------------------- 1. the diff

    /// name -> the modules that lost it / the modules that gained it.
    type Owners = HashMap<String, HashSet<String>>;
    let mut lost_owners: Owners = HashMap::new();
    let mut gained_owners: Owners = HashMap::new();
    let mut lost_count = 0usize;
    let mut gained_count = 0usize;

    for entry in inc_entries {
        let inc = inc
            .as_ref()
            .expect("there are entries only when there is a tree");
        let now: HashSet<String> = inc
            .module(entry)
            .map_err(Error::Ir)?
            .declarations
            .into_iter()
            .map(|decl| decl.name)
            .collect();
        // A module absent from the base IR is new: it has no ownership history,
        // so nothing can be pointing at it wrongly yet.
        let was: HashSet<String> = match base_file_of.get(entry.module.as_str()) {
            Some(base_entry) => base
                .module(base_entry)
                .map_err(Error::Ir)?
                .declarations
                .into_iter()
                .map(|decl| decl.name)
                .collect(),
            None => HashSet::new(),
        };
        for name in &was {
            if !now.contains(name) {
                lost_owners
                    .entry(name.clone())
                    .or_default()
                    .insert(entry.module.clone());
                lost_count += 1;
            }
        }
        for name in &now {
            if !was.contains(name) {
                gained_owners
                    .entry(name.clone())
                    .or_default()
                    .insert(entry.module.clone());
                gained_count += 1;
            }
        }
    }

    // A deleted module is a module whose whole name set was lost. Same
    // computation, empty "gained" side.
    //
    // The declarations are walked as the **array** the file holds, not as a set:
    // that is what the prototype does, so a module that declared one name twice
    // counts twice in `lostNames`. The distinct count beside it is the set.
    for module in &removed_modules {
        let entry = base_file_of[module.as_str()];
        for decl in base.module(entry).map_err(Error::Ir)?.declarations {
            lost_owners
                .entry(decl.name)
                .or_default()
                .insert(module.clone());
            lost_count += 1;
        }
    }
    let diff_done = started.elapsed();

    // ------------------------------------------------------------- 2. the scan

    let mut exclude: HashSet<String> = match options.exclude {
        Some(path) => read_module_list(path)?.into_iter().collect(),
        None => HashSet::new(),
    };
    exclude.extend(fresh.iter().map(|module| (*module).to_owned()));
    // A removed module must not be reported as needing re-extraction: it is gone.
    exclude.extend(removed_modules.iter().cloned());

    let mut stale_lost_owner: BTreeSet<String> = BTreeSet::new();
    let mut stale_moved_elsewhere: BTreeSet<String> = BTreeSet::new();
    let mut witnesses: Vec<Witness> = Vec::new();

    // Nothing moved and nothing was deleted: no module can be pointing anywhere
    // wrong, and the whole base IR does not have to be read.
    let watching = !lost_owners.is_empty() || !gained_owners.is_empty();
    if watching {
        for entry in &base.index().modules {
            if exclude.contains(&entry.module) {
                continue;
            }
            let module = base.module(entry).map_err(Error::Ir)?;
            for decl in &module.declarations {
                for reference in &decl.refs {
                    let (owner, name) = (&reference.module, &reference.name);
                    if lost_owners
                        .get(name)
                        .is_some_and(|owners| owners.contains(owner))
                    {
                        if !stale_lost_owner.contains(&entry.module) {
                            witnesses.push(Witness {
                                module: entry.module.clone(),
                                rule: RULE_LOST_OWNER,
                                reference: [owner.clone(), name.clone()],
                            });
                        }
                        stale_lost_owner.insert(entry.module.clone());
                    } else if gained_owners
                        .get(name)
                        .is_some_and(|owners| !owners.contains(owner))
                    {
                        if !stale_moved_elsewhere.contains(&entry.module) {
                            witnesses.push(Witness {
                                module: entry.module.clone(),
                                rule: RULE_MOVED_ELSEWHERE,
                                reference: [owner.clone(), name.clone()],
                            });
                        }
                        stale_moved_elsewhere.insert(entry.module.clone());
                    }
                }
            }
        }
    }
    let scan_done = started.elapsed();

    let mut stale: Vec<String> = stale_lost_owner
        .union(&stale_moved_elsewhere)
        .cloned()
        .collect();
    sort_utf16(&mut stale);

    let summary = OwnershipSummary {
        inc_modules: inc_entries.len(),
        removed_modules: removed_modules.len(),
        scanned_base_modules: if watching {
            i64::try_from(base.index().modules.len()).unwrap_or(i64::MAX)
                - i64::try_from(exclude.len()).unwrap_or(i64::MAX)
        } else {
            0
        },
        lost_names: lost_count,
        gained_names: gained_count,
        lost_names_distinct: lost_owners.len(),
        gained_names_distinct: gained_owners.len(),
        stale_by_lost_owner: stale_lost_owner.len(),
        stale_by_moved_elsewhere: stale_moved_elsewhere.len(),
        stale_modules: stale,
        witnesses,
        diff_seconds: diff_done.as_secs_f64(),
        scan_seconds: scan_done.saturating_sub(diff_done).as_secs_f64(),
        total_seconds: scan_done.as_secs_f64(),
    };

    if let Some(path) = options.json {
        let record = OwnershipJson {
            base: &options.base.display().to_string(),
            // `opt("--inc")` defaults to the empty string, and the empty string
            // is what lands in the file when no tree was given.
            inc: &options
                .inc
                .map(|dir| dir.display().to_string())
                .unwrap_or_default(),
            inc_modules: summary.inc_modules,
            removed_modules: summary.removed_modules,
            scanned_base_modules: summary.scanned_base_modules,
            lost_names: summary.lost_names,
            gained_names: summary.gained_names,
            lost_names_distinct: summary.lost_names_distinct,
            gained_names_distinct: summary.gained_names_distinct,
            stale_by_lost_owner: summary.stale_by_lost_owner,
            stale_by_moved_elsewhere: summary.stale_by_moved_elsewhere,
            stale: summary.stale_modules.len(),
            stale_modules: &summary.stale_modules,
            witnesses: &summary.witnesses[..summary.witnesses.len().min(WITNESSES_IN_SUMMARY)],
            diff_seconds: summary.diff_seconds,
            scan_seconds: summary.scan_seconds,
            total_seconds: summary.total_seconds,
        };
        // `JSON.stringify(summary, null, 2) + "\n"`.
        let body = serde_json::to_string_pretty(&record)
            .expect("counts, strings and durations serialise")
            + "\n";
        write(path, &body)?;
    }
    if let Some(path) = options.print_set {
        write_text(path, &summary.stale_modules)?;
    }
    Ok(summary)
}

/// The `--json` summary. Key order is the prototype's object literal
/// (`ownership.ts:188-206`); the three `*Seconds` in it are **diagnostics** —
/// wall clock, different every run, and no test may assert on them.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OwnershipJson<'a> {
    base: &'a str,
    inc: &'a str,
    inc_modules: usize,
    removed_modules: usize,
    scanned_base_modules: i64,
    lost_names: usize,
    gained_names: usize,
    lost_names_distinct: usize,
    gained_names_distinct: usize,
    stale_by_lost_owner: usize,
    stale_by_moved_elsewhere: usize,
    stale: usize,
    stale_modules: &'a [String],
    witnesses: &'a [Witness],
    diff_seconds: f64,
    scan_seconds: f64,
    total_seconds: f64,
}
