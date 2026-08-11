//! The run: an IR tree in, six whole-package artifacts out — plus, since M2-b,
//! the `contentHash` cache, the map delta and the timings record.
//!
//! Ported from `experiments/stage7h/global.ts:238-467` (frozen). The phases and
//! their boundaries are the prototype's, in its order:
//!
//! ```text
//! open index -> State::load -> facts_for -> derive+write six -> delta -> State::save -> timings
//!               t_state        t_read       t_write            t_delta   t_save
//! ```
//!
//! # The cache attaches at exactly one place
//!
//! [`facts_for`] is the only place a module's IR becomes [`ModuleFacts`], which
//! is why plan §3 asked for it before there was a cache to put in it. Its body
//! grew a hash test and its signature grew the cache and the hit/miss counts;
//! nothing downstream of [`ModuleFacts`] moved, and the six artifacts are byte
//! for byte what M2-a produced.
//!
//! # Without `--state` this is still the from-scratch build
//!
//! Not a fallback — the program the prototype's own oracle compares the cached
//! run against, and it has to be the same program. `tests/state_and_delta.rs`
//! makes that comparison over seven IR states.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Instant;

use lean_doc_ir::IrTree;
use serde::Serialize;

use crate::artifacts::Artifacts;
use crate::delta::{Delta, DeltaTimings};
use crate::facts::ModuleFacts;
use crate::state::State;

/// What a run needs to know that the IR does not carry.
#[derive(Clone, Copy, Debug)]
pub struct GlobalOptions<'a> {
    /// The IR tree: `index.json`, `modules/`, `deps/`.
    pub ir: &'a Path,
    /// The site root. `declarations/` is created under it; the other four
    /// artifacts sit directly in it.
    pub out: &'a Path,
    /// `--state <dir>`: where the `contentHash` cache lives. `None` reads every
    /// module, which is the from-scratch build.
    pub state: Option<&'a Path>,
    /// `--before <map.json>`: a previous `name-map.json`. `Some` turns the delta
    /// on; nothing else does.
    pub before: Option<&'a Path>,
    /// `--print-set <p>`: the affected modules, one per line, for the
    /// incremental pipeline. Ignored without `before`.
    pub print_set: Option<&'a Path>,
    /// `--delta-json <p>`: the delta's diagnostic summary. Ignored without
    /// `before`.
    pub delta_json: Option<&'a Path>,
    /// `--timings <p>`: one JSON line of counts and durations.
    pub timings: Option<&'a Path>,
}

impl<'a> GlobalOptions<'a> {
    /// The from-scratch build: no cache, no delta, no records.
    #[must_use]
    pub fn new(ir: &'a Path, out: &'a Path) -> Self {
        Self {
            ir,
            out,
            state: None,
            before: None,
            print_set: None,
            delta_json: None,
            timings: None,
        }
    }
}

/// What a run did, in the units its inputs are counted in.
///
/// The prototype prints the same numbers, so the two runs can be put side by
/// side without re-deriving anything. No durations: they are in the timings
/// file, they are not comparable between runs, and keeping them out is what
/// lets this type be compared with `==`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GlobalSummary {
    pub modules: usize,
    /// Distinct declaration names, i.e. the size of `declarations`.
    pub declarations: usize,
    pub dependency_names: usize,
    pub instance_classes: usize,
    pub tactic_docs: usize,
    pub bmp_bytes: usize,
    pub name_map_bytes: usize,
    /// Modules whose facts came from the state file.
    pub cache_hits: usize,
    /// Modules whose IR was read. Everything, when there is no state.
    pub cache_misses: usize,
    /// Size of the state file written, or 0 when there is no state directory.
    pub state_bytes: usize,
    /// `Some` iff `--before` was given.
    pub delta: Option<Delta>,
}

/// One [`ModuleFacts`] per index entry, in index order, and where each came
/// from.
#[derive(Clone, Debug, Default)]
pub struct FactsRun {
    pub facts: Vec<ModuleFacts>,
    pub cache_hits: usize,
    pub cache_misses: usize,
}

/// Derives the facts of every module of the IR, in index order, reading only
/// the modules the cache cannot answer for.
///
/// **The one funnel** — see the module heading. Index order is not incidental:
/// [`Artifacts::derive`] resolves a duplicated declaration name in favour of the
/// later module.
///
/// The hit test is `cached.content_hash == entry.content_hash` and nothing else.
/// The hash is the extractor's `String.hash` of the module JSON, the same value
/// `merge-ir.ts` uses to decide which pages are stale: equal hash, equal bytes,
/// equal facts. Pass [`State::empty`] for the from-scratch build.
pub fn facts_for(tree: &IrTree, cached: &State) -> Result<FactsRun, lean_doc_ir::Error> {
    let mut run = FactsRun {
        facts: Vec::with_capacity(tree.index().modules.len()),
        ..FactsRun::default()
    };
    for entry in &tree.index().modules {
        match cached.get(&entry.module) {
            Some(facts) if facts.content_hash == entry.content_hash => {
                run.facts.push(facts.clone());
                run.cache_hits += 1;
            }
            _ => {
                let module = tree.module(entry)?;
                run.facts
                    .push(ModuleFacts::of(&module, &entry.content_hash));
                run.cache_misses += 1;
            }
        }
    }
    Ok(run)
}

/// Reads the IR, derives the six artifacts, writes them, and — when asked —
/// updates the cache and reports the map delta.
pub fn build_global(options: &GlobalOptions<'_>) -> Result<GlobalSummary, Error> {
    let started = Instant::now();
    let tree = IrTree::open(options.ir)?;
    let cached = State::load(options.state, tree.index());
    let state_loaded = started.elapsed();

    let run = facts_for(&tree, &cached)?;
    let read = started.elapsed();

    let dep_maps = tree.load_dep_maps()?;
    let artifacts = Artifacts::derive(&run.facts, &dep_maps);

    for (relative, body) in artifacts.files() {
        let path = options.out.join(relative);
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|source| Error::Io {
                path: dir.to_owned(),
                source,
            })?;
        }
        fs::write(&path, body).map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
    }
    let written = started.elapsed();

    // The prototype's `diffSeconds` covers the read of `before` and the key
    // comparison, `scanSeconds` the token scan. Diagnostics either way — see
    // `Delta::to_json`.
    let mut split = DeltaTimings::default();
    let delta = match options.before {
        None => None,
        Some(path) => {
            let before = read_name_map(path)?;
            let changed = Delta::changed(&before, &artifacts.name_map);
            let diffed = started.elapsed();
            let delta = Delta::scan(before.len(), artifacts.name_map.len(), changed, &run.facts);
            let scanned = started.elapsed();
            split = DeltaTimings {
                diff_seconds: (diffed - written).as_secs_f64(),
                scan_seconds: (scanned - diffed).as_secs_f64(),
                total_seconds: (scanned - written).as_secs_f64(),
            };
            if let Some(path) = options.print_set {
                write(path, &delta.print_set())?;
            }
            if let Some(path) = options.delta_json {
                write(path, &delta.to_json(split))?;
            }
            Some(delta)
        }
    };
    let delta_done = started.elapsed();

    let state_bytes = State::save(options.state, tree.index(), &run.facts)?;
    let total = started.elapsed();

    // Read back off the artifact rather than recounted from the facts: a
    // summary that counts the inputs again can disagree with the file it is
    // reporting on, and then the number quoted in a document is nobody's.
    let sections = sections(&artifacts.declaration_data_bmp);

    let summary = GlobalSummary {
        modules: run.facts.len(),
        declarations: sections("declarations"),
        dependency_names: sections("dependencies"),
        instance_classes: sections("instances"),
        tactic_docs: run.facts.iter().map(|facts| facts.tactics).sum(),
        bmp_bytes: artifacts.declaration_data_bmp.len(),
        name_map_bytes: artifacts.name_map_json.len(),
        cache_hits: run.cache_hits,
        cache_misses: run.cache_misses,
        state_bytes,
        delta,
    };

    if let Some(path) = options.timings {
        // Same phase boundaries as the prototype's, so the two records
        // subtract. `readSeconds` covers the state load *and* the modules that
        // missed: together they are what a from-scratch run spends reading all
        // of them.
        let record = TimingsRecord {
            command: "build",
            state: if options.state.is_some() { "on" } else { "off" },
            cache_hits: summary.cache_hits,
            cache_misses: summary.cache_misses,
            state_bytes: summary.state_bytes,
            modules: summary.modules,
            declarations: summary.declarations,
            dependency_names: summary.dependency_names,
            instance_classes: summary.instance_classes,
            tactic_docs: summary.tactic_docs,
            bmp_bytes: summary.bmp_bytes,
            name_map_bytes: summary.name_map_bytes,
            state_load_seconds: state_loaded.as_secs_f64(),
            read_seconds: read.as_secs_f64(),
            write_seconds: (written - read).as_secs_f64(),
            delta_seconds: (delta_done - written).as_secs_f64(),
            state_save_seconds: (total - delta_done).as_secs_f64(),
            total_seconds: total.as_secs_f64(),
            delta: summary.delta.as_ref().map(|delta| TimingsDelta {
                changed_names: delta.changed.len(),
                affected: delta.affected.len(),
                diff_seconds: split.diff_seconds,
                scan_seconds: split.scan_seconds,
            }),
        };
        write(
            path,
            &(serde_json::to_string(&record).expect("counts and durations serialise") + "\n"),
        )?;
    }

    Ok(summary)
}

/// A `name -> module` map as `name-map.json` holds it.
///
/// Loudly, unlike [`State::load`]: `--before` names a file the caller believes
/// in, and a delta computed against an unreadable map would report every name
/// in the package as changed.
fn read_name_map(path: &Path) -> Result<BTreeMap<String, String>, Error> {
    let text = fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.to_owned(),
        source,
    })?;
    serde_json::from_str(&text).map_err(|source| Error::Json {
        path: path.to_owned(),
        source,
    })
}

fn write(path: &Path, body: &str) -> Result<(), Error> {
    if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        fs::create_dir_all(dir).map_err(|source| Error::Io {
            path: dir.to_owned(),
            source,
        })?;
    }
    fs::write(path, body).map_err(|source| Error::Io {
        path: path.to_owned(),
        source,
    })
}

/// How many keys each of `declaration-data.bmp`'s four sections has, from one
/// parse of it.
fn sections(bmp: &str) -> impl Fn(&str) -> usize {
    let parsed: Option<serde_json::Value> = serde_json::from_str(bmp).ok();
    move |section| {
        parsed
            .as_ref()
            .and_then(|value| value.get(section)?.as_object().map(serde_json::Map::len))
            .unwrap_or(0)
    }
}

/// The `--timings` record. Key order is the prototype's object literal, and the
/// durations in it are **diagnostics**: they are wall clock, they differ between
/// runs, and no test may assert on them. What the oracle reads out of this file
/// is `cacheHits` and `cacheMisses`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TimingsRecord<'a> {
    command: &'a str,
    /// `"on"` or `"off"` — whether a state directory was given, not whether the
    /// cache hit anything.
    state: &'a str,
    cache_hits: usize,
    cache_misses: usize,
    state_bytes: usize,
    modules: usize,
    declarations: usize,
    dependency_names: usize,
    instance_classes: usize,
    tactic_docs: usize,
    bmp_bytes: usize,
    name_map_bytes: usize,
    state_load_seconds: f64,
    read_seconds: f64,
    write_seconds: f64,
    delta_seconds: f64,
    state_save_seconds: f64,
    total_seconds: f64,
    delta: Option<TimingsDelta>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TimingsDelta {
    changed_names: usize,
    affected: usize,
    diff_seconds: f64,
    scan_seconds: f64,
}

/// Why a run stopped.
#[derive(Debug)]
pub enum Error {
    Ir(lean_doc_ir::Error),
    Io {
        path: PathBuf,
        source: io::Error,
    },
    /// A file this crate reads rather than writes — today only `--before`.
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ir(source) => write!(f, "{source}"),
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Json { path, source } => write!(f, "{}: {source}", path.display()),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Ir(source) => Some(source),
            Self::Io { source, .. } => Some(source),
            Self::Json { source, .. } => Some(source),
        }
    }
}

impl From<lean_doc_ir::Error> for Error {
    fn from(source: lean_doc_ir::Error) -> Self {
        Self::Ir(source)
    }
}
