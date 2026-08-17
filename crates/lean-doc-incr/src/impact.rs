//! The `impact` stage (layer L3-2): a changed module set in, the set of pages
//! that have to be rewritten out.
//!
//! Ported from `experiments/stage5/impact.ts` (frozen). Milestone **M3-c**.
//!
//! ```text
//! IR ──> imports  ──reverse, transitive──> IMPORTERS(M)
//!    └─> refs     ──reverse, direct─────> REFERRERS(M)
//!                                            │
//!                 --changed / --changed-file ┴──> --print-set
//! ```
//!
//! # Two closures, because they answer different questions
//!
//! **IMPORTERS(M)** is the reverse *transitive* import closure of `M`, cut down
//! to the package's own modules. It is the **sound** bound: a module that does
//! not transitively import `M` cannot observe anything `M` declares — not a
//! constant, not notation, not an instance. Nothing in Lean reaches further.
//!
//! **REFERRERS(M)** is the modules with at least one `refs` entry whose defining
//! module is `M`. It is a *subset* of IMPORTERS(M) — to name `M`'s constant you
//! must import `M` — and it is the set whose **printed text** mentions something
//! of `M`'s.
//!
//! Neither is "the answer" on its own, which is why `--mode` exists and why
//! nothing here picks a default for the pipeline.
//!
//! # What this stage is *not* given, and why that is not an oversight
//!
//! The whole-package name map's delta (plan §6, constraint 2: `global` runs
//! **before** `impact`) is the other half of L3-2 — the pages whose docstring
//! links went stale because a name moved somewhere else entirely. It arrives as
//! [`crate::detect`]'s sibling file, `global-set.txt`, and **this stage never
//! reads it**: `incremental.sh:354-360` unions the two sets *after* both have
//! been computed, which is the point of deriving them separately. So the
//! constraint is the pipeline's to keep (M3-d), and the union is the pipeline's
//! to take.
//!
//! Two consequences worth writing down before M3-d inherits them:
//!
//! - **A `global-set.txt` with no changes is 0 bytes, not a blank line**
//!   (plan §7, M2 の結果). An empty *file* is a real answer here — "no page went
//!   stale through the map" — and must not be read as "nobody said".
//! - **When the changed set is empty and `--mode` is not `all`, this stage
//!   writes no `--print-set` at all** (`impact.ts:179` guards the whole block).
//!   The prototype's pipeline then feeds `sort -u` a file that does not exist.
//!   Reproduced exactly ([`ImpactRun::summary`] is `None`), because changing it
//!   would move bytes that the oracle compares — but the caller has to treat a
//!   missing file as the empty set, and M3-d is where that belongs.
//!
//! # M3-b's key-order change does not reach here【実測 2026-08-12】
//!
//! The merger now writes `deps/*.json` and `index.json`'s `dependencyMaps`
//! entries in **Lean's** order rather than the prototype's (plan §7, M3-b の
//! 決着). This stage reads neither: it needs `index.modules[]`'s `module`,
//! `file` and `bytes`, and each module file's `imports` / `declarations`.
//! Measured rather than reasoned — two trees merged from the same inputs by the
//! two implementations differ in exactly those four files (same byte counts,
//! different bytes) and produce **byte-identical** census, `--print-set` and
//! summary, and a byte-identical `prune --ir` report.
//!
//! # What builds the module list is not this stage either
//!
//! Same division as `detect`'s (plan §5, M3-d): the package's module list comes
//! from a **glob over the sources**. A walk of `.lake/build` picks up 659 orphan
//! oleans【実測】. Here the list comes from `index.json`, so the question does
//! not arise — but `--changed` names are checked against it, and a name that is
//! not one of the package's modules is a **refusal** (exit 3) rather than a
//! silently empty answer.
//!
//! # Two places this port is not the prototype, both refusals
//!
//! - **A module file that disagrees with the index about which module it is.**
//!   The prototype keys every map by the *file's* own `module` field, so an
//!   index entry pointing at the wrong file silently produces a graph with a
//!   node nobody asked for; the reader here checks the two agree and stops
//!   (exit 1). Same choice `ownership` makes, and for the same reason: a graph
//!   with the wrong nodes under-renders, and under-rendering has to be loud.
//! - **A summary is printed after the files are written, not before.** The
//!   prototype prints it first (`impact.ts:228-230`), so a failing write leaves
//!   the answer on stdout and a non-zero exit. Nothing downstream reads stdout,
//!   and the two orders differ only on a failing write.
//!
//! Reads the IR only. Lean is never started and the measurement target is never
//! touched.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;

use lean_doc_ir::{IrTree, sort_utf16};
use serde::Serialize;

use crate::detect::{Error, write};

/// Which set of modules `--mode` asks for.
///
/// [`Mode::Unrecognised`] is a state the prototype really has: `--mode` is a
/// string until the `switch` runs, and the `switch` only runs when there is
/// something to select (`impact.ts:179`). So `--mode nonsense` with an empty
/// changed set **exits 0 having done nothing**, and a type that refused the
/// string at parse time would not reproduce that.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// The changed modules themselves — the rule the olean-hash ledger already
    /// implements on its own.
    SelfOnly,
    /// self + REFERRERS, **direct only**. The transitive count is reported
    /// beside it but is not what this mode selects.
    Referrers,
    /// self + IMPORTERS, transitively: the sound bound.
    ///
    /// The default, because `opt("--mode", "importers")` is.
    #[default]
    Importers,
    /// Every module of the package, whatever changed — **and valid with an empty
    /// changed set**. Not a wider closure over the same graph: it is the answer
    /// when the *renderer's* input moved rather than any module's, so no module
    /// IR is stale and every page is (`detect`'s `--render-all-out`).
    All,
    /// A `--mode` nobody recognises, carried rather than rejected. See the type
    /// heading.
    Unrecognised(String),
}

impl Mode {
    /// `opt("--mode", "…")` — the string as the caller wrote it.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        match text {
            "self" => Self::SelfOnly,
            "referrers" => Self::Referrers,
            "importers" => Self::Importers,
            "all" => Self::All,
            other => Self::Unrecognised(other.to_owned()),
        }
    }

    /// The string the summary reports, which is the one the caller passed.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::SelfOnly => "self",
            Self::Referrers => "referrers",
            Self::Importers => "importers",
            Self::All => "all",
            Self::Unrecognised(text) => text,
        }
    }
}

/// What `impact` needs to know.
#[derive(Clone, Copy, Debug)]
pub struct ImpactOptions<'a> {
    /// The IR tree. Its `index.json` defines the package's modules.
    pub ir: &'a Path,
    /// The changed modules, **in the order they were given** — the `--changed`
    /// flags first, then the lines of `--changed-file`. The order reaches the
    /// summary's `changed` array, and repeats are kept.
    pub changed: &'a [String],
    pub mode: &'a Mode,
    /// A per-module census (TSV). Written whatever the changed set is, and
    /// **before** the selection, as in the prototype.
    pub census: Option<&'a Path>,
    /// The selected modules, one name per line — the render set's first half.
    pub print_set: Option<&'a Path>,
    /// The summary, as JSON.
    pub json: Option<&'a Path>,
}

/// What one `impact` run did.
///
/// Both halves are optional because both are conditional in the prototype: the
/// census on `--census`, the selection on "there is something to select".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImpactRun {
    /// Modules in the census, when one was written. `index.modules.length`.
    pub census_modules: Option<usize>,
    /// `None` when the changed set was empty and the mode was not
    /// [`Mode::All`]: the prototype skips the whole block, writes no
    /// `--print-set` and prints nothing.
    pub summary: Option<ImpactSummary>,
}

/// What `impact` selected.
///
/// **`PartialEq`**, unlike the other stages' summaries: nothing in it is a
/// clock. Every number here is a denominator something else gets quoted
/// against, so tests are meant to assert on all of them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImpactSummary {
    /// The `--ir` argument as it was given — the prototype reports the string,
    /// not a resolved path.
    pub ir: String,
    pub changed: Vec<String>,
    pub mode: String,
    pub own_modules: usize,
    /// Distinct changed modules.
    pub self_modules: usize,
    pub referrers_direct: usize,
    pub referrers_transitive: usize,
    pub importers_transitive: usize,
    /// The selection, in **UTF-16 order** (plan §7, U1): this list is what
    /// `--print-set` writes and what the renderer is then asked for.
    pub selected: Vec<String>,
    /// Summed from the **module files**, not from `index.json`'s `declarations`
    /// column — that is where the prototype reads it (`impact.ts:113`).
    pub selected_declarations: usize,
    /// Summed from `index.json`'s `bytes` column, over the index entries whose
    /// module was selected. A repeated index entry counts twice, as it does
    /// there (`impact.ts:223`).
    pub selected_ir_bytes: u64,
}

/// Computes the two closures and selects a module set.
pub fn impact(options: &ImpactOptions<'_>) -> Result<ImpactRun, Error> {
    let tree = IrTree::open_unvalidated(options.ir).map_err(Error::Ir)?;
    let index = tree.index();
    let own: HashSet<&str> = index
        .modules
        .iter()
        .map(|entry| entry.module.as_str())
        .collect();

    // One pass over the IR: the import edges, the reference edges and the
    // declaration counts all come out of the same read.
    let mut direct_imports: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut ref_modules: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut decl_count: HashMap<&str, usize> = HashMap::new();
    let mut loaded: Vec<lean_doc_ir::ModuleFile> = Vec::with_capacity(index.modules.len());
    for entry in &index.modules {
        loaded.push(tree.module(entry).map_err(Error::Ir)?);
    }
    for module in &loaded {
        let name = module.module.as_str();
        direct_imports.insert(
            name,
            module
                .imports
                .iter()
                .map(String::as_str)
                .filter(|import| own.contains(import))
                .collect(),
        );
        decl_count.insert(name, module.declarations.len());
        // A `Set`: each named module once, whatever how many declarations name
        // it. Insertion order is not observable — every use is a count or a
        // membership test.
        let mut named: Vec<&str> = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();
        for decl in &module.declarations {
            for reference in &decl.refs {
                let owner = reference.module.as_str();
                if own.contains(owner) && owner != name && seen.insert(owner) {
                    named.push(owner);
                }
            }
        }
        ref_modules.insert(name, named);
    }

    // Reverse both graphs. Every own-package module gets a list, so a module
    // nobody imports is an empty list rather than an absent key.
    let mut imported_by: HashMap<&str, Vec<&str>> = own.iter().map(|m| (*m, Vec::new())).collect();
    for (module, imports) in &direct_imports {
        for import in imports {
            imported_by
                .get_mut(import)
                .expect("the imports were filtered to own-package modules")
                .push(module);
        }
    }
    let mut referred_by: HashMap<&str, Vec<&str>> = own.iter().map(|m| (*m, Vec::new())).collect();
    for (module, named) in &ref_modules {
        for owner in named {
            referred_by
                .get_mut(owner)
                .expect("the references were filtered to own-package modules")
                .push(module);
        }
    }

    let census_modules = match options.census {
        Some(path) => {
            let mut rows = String::from(CENSUS_HEADER);
            for entry in &index.modules {
                let module = entry.module.as_str();
                rows.push('\n');
                write!(
                    rows,
                    "{module}\t{}\t{}\t{}\t{}\t{}",
                    decl_count.get(module).copied().unwrap_or(0),
                    direct_imports.get(module).map_or(0, Vec::len),
                    imported_by.get(module).map_or(0, Vec::len),
                    closure(&[module], &imported_by).len(),
                    referred_by.get(module).map_or(0, Vec::len),
                )
                .expect("writing to a String cannot fail");
            }
            rows.push('\n');
            write(path, &rows)?;
            Some(index.modules.len())
        }
        None => None,
    };

    // The whole selection is conditional: with nothing changed and a mode that
    // is not `all` there is no question. See the module heading.
    if options.changed.is_empty() && *options.mode != Mode::All {
        return Ok(ImpactRun {
            census_modules,
            summary: None,
        });
    }

    for module in options.changed {
        if !own.contains(module.as_str()) {
            return Err(Error::NotAModule {
                module: module.clone(),
            });
        }
    }
    let seeds: Vec<&str> = options.changed.iter().map(String::as_str).collect();
    let self_set: HashSet<&str> = seeds.iter().copied().collect();
    let importers = closure(&seeds, &imported_by);
    // Transitive over the reference edges: reported, never selected. `--mode
    // referrers` takes the direct set below, and the gap between the two counts
    // is the whole reason both are in the summary.
    let referrers_transitive = closure(&seeds, &referred_by);
    let mut referrers_direct: HashSet<&str> = HashSet::new();
    for module in &seeds {
        for referrer in referred_by.get(module).into_iter().flatten() {
            referrers_direct.insert(referrer);
        }
    }

    let selected: HashSet<&str> = match options.mode {
        Mode::SelfOnly => self_set.clone(),
        Mode::Referrers => self_set.union(&referrers_direct).copied().collect(),
        Mode::Importers => self_set.union(&importers).copied().collect(),
        Mode::All => own.clone(),
        Mode::Unrecognised(text) => {
            return Err(Error::UnknownMode { mode: text.clone() });
        }
    };

    // Plan §7, U1: `.sort()` is UTF-16 code unit order, and this list decides
    // the order of `--print-set`.
    let mut list: Vec<String> = selected.iter().map(|m| (*m).to_owned()).collect();
    sort_utf16(&mut list);

    let summary = ImpactSummary {
        ir: options.ir.display().to_string(),
        changed: options.changed.to_vec(),
        mode: options.mode.name().to_owned(),
        own_modules: own.len(),
        self_modules: self_set.len(),
        referrers_direct: referrers_direct.len(),
        referrers_transitive: referrers_transitive.len(),
        importers_transitive: importers.len(),
        selected_declarations: list
            .iter()
            .map(|module| decl_count.get(module.as_str()).copied().unwrap_or(0))
            .sum(),
        selected_ir_bytes: index
            .modules
            .iter()
            .filter(|entry| selected.contains(entry.module.as_str()))
            .map(|entry| entry.bytes)
            .sum(),
        selected: list,
    };

    let body = summary.to_json();
    if let Some(path) = options.json {
        write(path, &(body + "\n"))?;
    }
    if let Some(path) = options.print_set {
        // **Not** [`crate::detect::write_text`]: the prototype writes
        // `list.join("\n") + "\n"`, so an empty selection would be one blank
        // line rather than an empty file. That case needs an IR with no modules
        // at all (every mode selects a superset of a non-empty changed set, and
        // `all` selects every module), and a blank line and an empty file are
        // the same set to `--only-from`, which drops blank lines. Kept as the
        // prototype has it so the bytes compare.
        write(path, &(summary.selected.join("\n") + "\n"))?;
    }
    Ok(ImpactRun {
        census_modules,
        summary: Some(summary),
    })
}

impl ImpactSummary {
    /// `JSON.stringify(summary, null, 2)` — what the prototype prints and what
    /// `--json` holds (with a trailing newline added there, not here).
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(&ImpactJson {
            ir: &self.ir,
            changed: &self.changed,
            mode: &self.mode,
            own_modules: self.own_modules,
            self_modules: self.self_modules,
            referrers_direct: self.referrers_direct,
            referrers_transitive: self.referrers_transitive,
            importers_transitive: self.importers_transitive,
            selected: self.selected.len(),
            selected_declarations: self.selected_declarations,
            selected_ir_bytes: self.selected_ir_bytes,
        })
        .expect("counts and strings serialise")
    }
}

/// The census's header row, tab separated.
const CENSUS_HEADER: &str =
    "module\tdeclarations\tdirectImports\timportedByDirect\timportersTransitive\treferrersDirect";

/// The summary as it is written. Key order is the prototype's object literal
/// (`impact.ts:212-227`), and `self` is a Rust keyword so the field is renamed.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ImpactJson<'a> {
    ir: &'a str,
    changed: &'a [String],
    mode: &'a str,
    own_modules: usize,
    #[serde(rename = "self")]
    self_modules: usize,
    referrers_direct: usize,
    referrers_transitive: usize,
    importers_transitive: usize,
    selected: usize,
    selected_declarations: usize,
    selected_ir_bytes: u64,
}

/// Everything reachable from `seeds` along `edges`.
///
/// **The seeds are not in the result unless something leads back to them.** The
/// prototype seeds the stack and not the visited set (`impact.ts:138-151`), and
/// every caller unions the seeds back in itself — which is what makes
/// `importersTransitive` a count of *other* modules. On an acyclic import graph
/// the two spellings differ by exactly the seeds; on a cyclic reference graph
/// they do not, and that is measurable rather than hypothetical.
fn closure<'a>(seeds: &[&'a str], edges: &HashMap<&'a str, Vec<&'a str>>) -> HashSet<&'a str> {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut stack: Vec<&str> = seeds.to_vec();
    while let Some(current) = stack.pop() {
        for next in edges.get(current).into_iter().flatten() {
            if seen.insert(next) {
                stack.push(next);
            }
        }
    }
    seen
}
