//! The `detect` stage: `build`, `check` and `touch`.
//!
//! Ported from `experiments/stage5/ledger.ts:216-422` (frozen). The three
//! commands are the whole of the input side of incrementality — which modules
//! must be re-extracted, answered **without starting Lean**.
//!
//! ```text
//! build  modules + target ──> ledger.json
//! check  ledger.json + the oleans as they are now
//!          ├─ extractKey diff ──> every module re-extracted
//!          ├─ per-module hash  ──> --changed-out   (changed ∪ added)
//!          ├─ module gone      ──> --removed-out
//!          └─ renderKey diff   ──> --render-all-out (one reason per line)
//! touch  ledger.json ──> the same ledger with one module's hash invalidated
//! ```
//!
//! # Why `touch` is in the product and not in a test
//!
//! It is the honest fake the whole stage-5 experiment rests on. The measurement
//! target must not be modified — its `.lake/build` is the baseline for every
//! number in this repository — so the *fact* "module M changed" is injected by
//! invalidating M's ledger entry. Everything downstream is real: the
//! re-extraction, the merge, the page set, the clocks. Without it the pipeline
//! cannot be exercised end to end on any repository somebody cares about.
//!
//! # The two sets are reported separately on purpose
//!
//! A changed `extractKey` invalidates every module's IR; a changed `renderKey`
//! invalidates **no** IR at all and is deliberately not folded into the
//! re-extract set. Computing the key diff and then not acting on it was the
//! whole failure of the prototype's S1.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Instant;

use lean_doc_ir::cmp_utf16;
use serde::Serialize;

use crate::ledger::{
    Algorithm, LEDGER_SCHEMA, Ledger, ModuleEntry, extract_key, hash_module, map_pool,
    read_to_string, render_key,
};

/// What `build` needs to know.
#[derive(Clone, Copy, Debug)]
pub struct BuildOptions<'a> {
    /// The modules to hash, in the order they will appear in the ledger.
    /// **From a glob over the sources, never from a walk of `.lake/build`** —
    /// the build tree carries 659 orphan oleans on the target (plan §5).
    pub modules: &'a [String],
    /// The target repository. A trailing slash is stripped, as the prototype
    /// strips it, because the string is a prefix of every recorded path.
    pub target: &'a str,
    /// Where the ledger goes.
    pub out: &'a Path,
    /// An IR tree. Its `schemaVersion` and `generator` join the extract key;
    /// without it those two keys are absent, and an absent key is a change
    /// (see [`KeySet::diff`]).
    pub ir: Option<&'a Path>,
    /// `--source-url`. Empty means the key is absent, which is loud rather than
    /// silent: the next `check` that passes one will re-render everything.
    pub source_url: &'a str,
    pub algorithm: &'a Algorithm,
    pub concurrency: usize,
    pub timings: Option<&'a Path>,
}

/// What `build` did.
#[derive(Clone, Debug)]
pub struct BuildSummary {
    pub ledger: Ledger,
    pub modules: usize,
    pub files: usize,
    /// Olean bytes read and hashed. Zero under `--algorithm lake`, which reads
    /// no olean at all.
    pub hashed_bytes: u64,
    pub ledger_bytes: usize,
    /// Seconds spent hashing, for the log line the prototype prints.
    pub hash_seconds: f64,
}

/// Hashes every module and writes the ledger.
///
/// A module in the list with no olean at all stops the run
/// ([`Error::NoOlean`], exit 3): at `build` time the list and the build tree are
/// supposed to agree, and a ledger that silently omits modules would report them
/// as *added* on the next `check` — the one direction that looks like progress.
pub fn build_ledger(options: &BuildOptions<'_>) -> Result<BuildSummary, Error> {
    let started = Instant::now();
    let target = options.target.trim_end_matches('/');
    let lib_dir = format!("{target}/.lake/build/lib/lean");

    let extract = extract_key(target, options.ir)?;
    let render = render_key(options.source_url);
    let key_done = started.elapsed();

    let hashed = map_pool(options.modules, options.concurrency, |module| {
        hash_module(target, &lib_dir, module, options.algorithm)
    });
    let hash_done = started.elapsed();

    // First by index rather than first to fail, so that two runs of a failing
    // build at different concurrencies report the same module.
    let mut entries: Vec<ModuleEntry> = Vec::with_capacity(hashed.len());
    let mut missing: Vec<String> = Vec::new();
    for (module, entry) in options.modules.iter().zip(hashed) {
        match entry? {
            Some(entry) => entries.push(entry),
            None => missing.push(module.clone()),
        }
    }
    if !missing.is_empty() {
        return Err(Error::NoOlean {
            lib_dir,
            modules: missing,
        });
    }

    let files = entries.iter().map(|entry| entry.files.len()).sum();
    let hashed_bytes = hashed_bytes(&entries);
    let ledger = Ledger {
        ledger_schema: LEDGER_SCHEMA,
        algorithm: options.algorithm.clone(),
        target: target.to_owned(),
        lib_dir,
        extract_key: extract,
        render_key: Some(render),
        modules: entries,
    };
    let body = ledger.to_json();
    write(options.out, &body)?;
    let total = started.elapsed();

    let summary = BuildSummary {
        modules: ledger.modules.len(),
        files,
        hashed_bytes,
        ledger_bytes: body.len(),
        hash_seconds: (hash_done - key_done).as_secs_f64(),
        ledger,
    };
    if let Some(path) = options.timings {
        let record = BuildTimings {
            command: "build",
            algorithm: options.algorithm.name(),
            concurrency: options.concurrency,
            modules: summary.modules,
            files: summary.files,
            hashed_bytes: summary.hashed_bytes,
            key_seconds: key_done.as_secs_f64(),
            hash_seconds: summary.hash_seconds,
            write_seconds: (total - hash_done).as_secs_f64(),
            total_seconds: total.as_secs_f64(),
        };
        write_json_line(path, &record)?;
    }
    Ok(summary)
}

/// What `check` needs to know.
#[derive(Clone, Copy, Debug)]
pub struct CheckOptions<'a> {
    pub ledger: &'a Path,
    /// `None` keeps the ledger's own algorithm, which is the usual case: the
    /// two hashes are not comparable, so changing it makes every module differ.
    pub algorithm: Option<&'a Algorithm>,
    /// **`None` and `Some(&[])` are different questions.** `None` re-reads the
    /// ledger's module list, which cannot see a module that appeared or
    /// vanished since `build`; `Some` is the current list, and an empty one
    /// says every module in the ledger is gone. Freezing the list at build time
    /// is what made a vanished module throw instead of being reported (stage 5b,
    /// S4).
    pub modules: Option<&'a [String]>,
    pub ir: Option<&'a Path>,
    pub source_url: &'a str,
    pub concurrency: usize,
    /// The re-extract set, one module per line. Empty file, not a blank line.
    pub changed_out: Option<&'a Path>,
    pub removed_out: Option<&'a Path>,
    /// The reasons the render key changed, one `renderKey:<name>` per line. An
    /// empty file means "the render set follows from the IR diff as usual",
    /// which is what the caller tests for.
    pub render_all_out: Option<&'a Path>,
    pub timings: Option<&'a Path>,
}

/// What `check` found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckSummary {
    /// The algorithm actually used — the ledger's, unless overridden.
    pub algorithm: Algorithm,
    /// Modules that still have an olean.
    pub modules: usize,
    pub files: usize,
    pub hashed_bytes: u64,
    /// Names of the extract keys that differ, in UTF-16 order.
    pub extract_key_changed: Vec<String>,
    pub render_key_changed: Vec<String>,
    /// In the ledger, hash differs.
    pub changed: Vec<String>,
    /// In the list, not in the ledger.
    pub added: Vec<String>,
    /// In the list with no olean, or in the ledger and no longer in the list.
    pub removed: Vec<String>,
    /// What must be re-extracted: every present module when the extract key
    /// changed, otherwise `changed ∪ added`.
    pub re_extract: Vec<String>,
    /// **The ledger this check would leave behind if the run it licenses
    /// succeeds** — the hashes as they were *here*, before anything was
    /// extracted from them.
    ///
    /// Not written by this function 【判断】: `check` answers a question and a
    /// caller that stops on the answer must not have moved the ledger. Who
    /// writes it is `lean-doc build` (M4-d), and the reason the value is taken
    /// here rather than re-hashed afterwards is a race with one silent
    /// direction: an olean that moves *while* the run is in flight, re-hashed at
    /// the end, would be recorded as the one its IR came from, and that module
    /// is then never re-extracted again. Recording the hash the extraction was
    /// licensed by can only make a later run re-extract something it need not —
    /// wasteful, and loud in the next run's `changed` count.
    ///
    /// The module entries are the ones **that still have an olean**, in the
    /// order `--modules` named them: a removed module has to leave the ledger,
    /// or the next `check` reports it removed for ever. The two keys are this
    /// check's own; a caller whose run replaces the IR tree recomputes
    /// [`crate::extract_key`] against the tree that now exists (see
    /// `crates/lean-doc/src/build.rs`).
    pub fresh: Ledger,
}

impl CheckSummary {
    /// Whether the extract key changed, i.e. whether every module's IR is
    /// invalid regardless of its own hash.
    #[must_use]
    pub fn extract_invalidated(&self) -> bool {
        !self.extract_key_changed.is_empty()
    }

    /// Whether every page has to be re-rendered from IR that is still valid.
    #[must_use]
    pub fn render_all(&self) -> bool {
        !self.render_key_changed.is_empty()
    }
}

/// Compares the ledger with the oleans as they are now.
pub fn check_ledger(options: &CheckOptions<'_>) -> Result<CheckSummary, Error> {
    let started = Instant::now();
    let text = read_to_string(options.ledger)?;
    let ledger: Ledger = serde_json::from_str(&text).map_err(|source| Error::Json {
        path: options.ledger.to_owned(),
        source,
    })?;
    let read_done = started.elapsed();
    if ledger.ledger_schema < LEDGER_SCHEMA {
        return Err(Error::LedgerSchema {
            path: options.ledger.to_owned(),
            found: ledger.ledger_schema,
        });
    }
    let algorithm = options.algorithm.unwrap_or(&ledger.algorithm).clone();

    let extract = extract_key(&ledger.target, options.ir)?;
    let render = render_key(options.source_url);
    let key_done = started.elapsed();

    // L1 in two halves (see [`extract_key`]): one invalidates the IR, the other
    // only the pages rendered from it.
    let extract_key_changed = ledger.extract_key.diff(&extract);
    let render_key_changed = ledger.render_key_or_empty().diff(&render);

    // A JavaScript `Map` built from the entries: a repeated module keeps its
    // first position and takes its last value.
    let mut previous: HashMap<&str, &ModuleEntry> = HashMap::new();
    let mut previous_order: Vec<&str> = Vec::new();
    for entry in &ledger.modules {
        if previous.insert(entry.module.as_str(), entry).is_none() {
            previous_order.push(entry.module.as_str());
        }
    }
    let from_list = options.modules.is_some();
    let owned_from_ledger: Vec<String>;
    let current: &[String] = match options.modules {
        Some(modules) => modules,
        None => {
            owned_from_ledger = previous_order.iter().map(|m| (*m).to_owned()).collect();
            &owned_from_ledger
        }
    };
    let now = map_pool(current, options.concurrency, |module| {
        hash_module(&ledger.target, &ledger.lib_dir, module, &algorithm)
    });
    let hash_done = started.elapsed();

    let mut present: Vec<ModuleEntry> = Vec::new();
    let mut changed: Vec<String> = Vec::new();
    let mut added: Vec<String> = Vec::new();
    let mut removed: Vec<String> = Vec::new();
    for (module, entry) in current.iter().zip(now) {
        let Some(entry) = entry? else {
            // In the list, no olean on disk.
            removed.push(module.clone());
            continue;
        };
        match previous.get(module.as_str()) {
            None => added.push(module.clone()),
            Some(was) if was.hash != entry.hash => changed.push(module.clone()),
            Some(_) => {}
        }
        present.push(entry);
    }
    let in_current: HashSet<&str> = current.iter().map(String::as_str).collect();
    let in_removed: HashSet<String> = removed.iter().cloned().collect();
    for module in &previous_order {
        if !in_current.contains(module) && !in_removed.contains(*module) {
            removed.push((*module).to_owned());
        }
    }

    // A changed extract key invalidates every module's IR (L1). The set is the
    // present modules in list order, not the sorted union below: the prototype
    // reports what it just hashed.
    let re_extract = if extract_key_changed.is_empty() {
        let mut union: Vec<String> = changed.iter().chain(&added).cloned().collect();
        union.sort_by(|a, b| cmp_utf16(a, b));
        union
    } else {
        present.iter().map(|entry| entry.module.clone()).collect()
    };
    let compare_done = started.elapsed();

    if let Some(path) = options.changed_out {
        write(path, &lines_file(&re_extract))?;
    }
    if let Some(path) = options.removed_out {
        write(path, &lines_file(&removed))?;
    }
    if let Some(path) = options.render_all_out {
        let reasons: Vec<String> = render_key_changed
            .iter()
            .map(|name| format!("renderKey:{name}"))
            .collect();
        write(path, &lines_file(&reasons))?;
    }

    let summary = CheckSummary {
        modules: present.len(),
        files: present.iter().map(|entry| entry.files.len()).sum(),
        hashed_bytes: hashed_bytes(&present),
        extract_key_changed,
        render_key_changed,
        changed,
        added,
        removed,
        re_extract,
        // See [`CheckSummary::fresh`]: built here, written by nobody. The
        // algorithm is the one that produced these hashes — the ledger's own
        // unless `--algorithm` overrode it — because a ledger whose entries and
        // whose declared algorithm disagree compares every module as changed
        // for ever after.
        fresh: Ledger {
            ledger_schema: LEDGER_SCHEMA,
            algorithm: algorithm.clone(),
            target: ledger.target,
            lib_dir: ledger.lib_dir,
            extract_key: extract,
            render_key: Some(render),
            modules: present,
        },
        algorithm,
    };
    if let Some(path) = options.timings {
        let record = CheckTimings {
            command: "check",
            algorithm: summary.algorithm.name(),
            concurrency: options.concurrency,
            modules: summary.modules,
            module_list_source: if from_list { "list" } else { "ledger" },
            files: summary.files,
            hashed_bytes: summary.hashed_bytes,
            extract_key_changed: &summary.extract_key_changed,
            extract_invalidated: summary.extract_invalidated(),
            render_key_changed: &summary.render_key_changed,
            render_all: summary.render_all(),
            changed: summary.changed.len(),
            changed_modules: &summary.changed,
            added: summary.added.len(),
            added_modules: &summary.added,
            removed: summary.removed.len(),
            removed_modules: &summary.removed,
            re_extract: summary.re_extract.len(),
            read_ledger_seconds: read_done.as_secs_f64(),
            key_seconds: (key_done - read_done).as_secs_f64(),
            hash_seconds: (hash_done - key_done).as_secs_f64(),
            compare_seconds: (compare_done - hash_done).as_secs_f64(),
            total_seconds: started.elapsed().as_secs_f64(),
        };
        write_json_line(path, &record)?;
    }
    Ok(summary)
}

/// What `touch` needs to know.
#[derive(Clone, Copy, Debug)]
pub struct TouchOptions<'a> {
    pub ledger: &'a Path,
    pub module: &'a str,
    /// Defaults to the ledger itself in the prototype; here the caller says so.
    pub out: &'a Path,
}

/// Invalidates one module's entry, so that the next `check` reports it as
/// **changed**.
///
/// Invalidating rather than deleting is the point: a deleted entry would be
/// reported as *added*, and an added module is not what an edit produces. The
/// olean is not touched — the injected fact is a fact about the ledger.
pub fn touch_ledger(options: &TouchOptions<'_>) -> Result<usize, Error> {
    let text = read_to_string(options.ledger)?;
    let mut ledger: Ledger = serde_json::from_str(&text).map_err(|source| Error::Json {
        path: options.ledger.to_owned(),
        source,
    })?;
    let Some(entry) = ledger
        .modules
        .iter_mut()
        .find(|entry| entry.module == options.module)
    else {
        return Err(Error::NoSuchModule {
            path: options.ledger.to_owned(),
            module: options.module.to_owned(),
        });
    };
    entry.hash = format!("injected-change:{}", entry.hash);
    let body = ledger.to_json();
    write(options.out, &body)?;
    Ok(body.len())
}

/// A module list file: one name per line, `#` comments and blank lines dropped.
pub fn read_module_list(path: &Path) -> Result<Vec<String>, Error> {
    Ok(read_to_string(path)?
        .split('\n')
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect())
}

/// One name per line, and **no line at all** when there are no names.
///
/// The empty file is load-bearing: it is what the pipeline hands to
/// `--only-from`, where an empty set has to mean "render nothing" rather than
/// "render everything" (plan §5).
fn lines_file(items: &[String]) -> String {
    if items.is_empty() {
        String::new()
    } else {
        items.join("\n") + "\n"
    }
}

/// Writes a set of names as [`lines_file`] spells it.
///
/// Shared by every stage that hands a module set to the next one:
/// `--changed-out`, `--removed-out`, `--render-all-out`, `--print-set`. One
/// spelling, so the empty file cannot be empty in one stage and a blank line in
/// another.
pub(crate) fn write_text(path: &Path, items: &[String]) -> Result<(), Error> {
    write(path, &lines_file(items))
}

/// Olean bytes actually read. `-1` (the `lake` path) clamps to zero.
fn hashed_bytes(entries: &[ModuleEntry]) -> u64 {
    entries
        .iter()
        .flat_map(|entry| &entry.files)
        .map(|file| u64::try_from(file.bytes).unwrap_or(0))
        .sum()
}

pub(crate) fn write(path: &Path, body: &str) -> Result<(), Error> {
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

pub(crate) fn write_json_line(path: &Path, record: &impl Serialize) -> Result<(), Error> {
    let body = serde_json::to_string(record).expect("counts and durations serialise") + "\n";
    write(path, &body)
}

/// The `--timings` record of a `build`. Key order is the prototype's object
/// literal; the durations in it are **diagnostics** — wall clock, different
/// every run, and no test may assert on them.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BuildTimings<'a> {
    command: &'a str,
    algorithm: &'a str,
    concurrency: usize,
    modules: usize,
    files: usize,
    hashed_bytes: u64,
    key_seconds: f64,
    hash_seconds: f64,
    write_seconds: f64,
    total_seconds: f64,
}

/// The `--timings` record of a `check`.
///
/// The four durations partition the run the way the prototype's do, so the two
/// records subtract: read the ledger, build the keys, hash the modules, compare
/// the sets. The output files are written after the last of them, as there.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CheckTimings<'a> {
    command: &'a str,
    algorithm: &'a str,
    concurrency: usize,
    modules: usize,
    module_list_source: &'a str,
    files: usize,
    hashed_bytes: u64,
    extract_key_changed: &'a [String],
    extract_invalidated: bool,
    render_key_changed: &'a [String],
    render_all: bool,
    changed: usize,
    changed_modules: &'a [String],
    added: usize,
    added_modules: &'a [String],
    removed: usize,
    removed_modules: &'a [String],
    re_extract: usize,
    read_ledger_seconds: f64,
    key_seconds: f64,
    hash_seconds: f64,
    compare_seconds: f64,
    total_seconds: f64,
}

/// Why a run stopped.
///
/// The three refusals below are the prototype's **exit 3**: a run that stopped
/// because the ledger and the world disagree, as against one that could not read
/// a file. The caller maps them back onto the same exit codes.
#[derive(Debug)]
pub enum Error {
    Io {
        path: PathBuf,
        source: io::Error,
    },
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    /// `build`: modules in the list have no olean under `lib_dir`.
    NoOlean {
        lib_dir: String,
        modules: Vec<String>,
    },
    /// `check`: the ledger predates the `extractKey` / `renderKey` split.
    LedgerSchema {
        path: PathBuf,
        found: u64,
    },
    /// `touch`: no such module in the ledger.
    NoSuchModule {
        path: PathBuf,
        module: String,
    },
    /// `ownership` / `merge`: the IR would not read. Carried rather than
    /// flattened so the reader's own message — which names the module file and
    /// the schema it wanted — survives.
    Ir(lean_doc_ir::Error),
    /// `merge` / `prune`: an `index.json` that parses as JSON but is not an
    /// index.
    ///
    /// The prototype reads `e.file` off whatever the JSON held and would write a
    /// file called `undefined`; there is no shape of index this can reach from a
    /// real extraction, so it is a refusal (exit 3) rather than a guess.
    IndexShape {
        path: PathBuf,
        message: String,
    },
    /// `merge --modules`: the package's module list and the tree the merge is
    /// about to write name different modules.
    ///
    /// **Exit 3, and refused before the first byte is written** 【判断】. The
    /// list is what `index.json`'s `modules` array is ordered by, so a mismatch
    /// leaves the order of the odd ones out to a guess — and the only guesses
    /// available are the two this project refuses to make silently: append the
    /// unlisted ones (which is the divergence from a from-scratch extraction
    /// that M3-d2b removed) or drop the unbacked ones (a module with a file and
    /// no index entry, invisible to every later stage). Neither shows up in a
    /// page byte, so neither would ever be noticed.
    ModuleListMismatch {
        /// In the list, with nothing in the merged tree behind it.
        missing: Vec<String>,
        /// In the merged tree, and not in the list.
        extra: Vec<String>,
    },
    /// `impact`: a `--changed` module the IR's index does not have. The
    /// prototype's **exit 3** — under-rendering has to be loud (plan §5, M3).
    NotAModule {
        module: String,
    },
    /// `impact`: a `--mode` nobody recognises. The prototype's **exit 2**, and
    /// it is only reached when there was something to select — see
    /// [`crate::impact::Mode::Unrecognised`].
    UnknownMode {
        mode: String,
    },
    /// `prune`: a path that would be deleted outside the page root.
    ///
    /// Not reachable from a module name — [`crate::prune::page_of`] turns every
    /// dot into a separator, so the `..` cannot survive — which is exactly why
    /// it is a check and not a comment. Exit 3: the world and the files
    /// disagree, and retrying will not help.
    OutsidePageRoot {
        root: PathBuf,
        path: PathBuf,
    },
}

impl Error {
    /// The process exit code, as the prototype's.
    #[must_use]
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Io { .. } | Self::Json { .. } | Self::Ir(_) => 1,
            Self::UnknownMode { .. } => 2,
            Self::NoOlean { .. }
            | Self::LedgerSchema { .. }
            | Self::NoSuchModule { .. }
            | Self::IndexShape { .. }
            | Self::ModuleListMismatch { .. }
            | Self::NotAModule { .. }
            | Self::OutsidePageRoot { .. } => 3,
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Json { path, source } => write!(f, "{}: {source}", path.display()),
            Self::NoOlean { lib_dir, modules } => {
                write!(f, "no olean under {lib_dir} for: {}", modules.join(", "))
            }
            Self::LedgerSchema { path, found } => write!(
                f,
                "{} is ledgerSchema {found}; this build needs {LEDGER_SCHEMA} (the single envKey \
                 was split into extractKey/renderKey). Rebuild the ledger.",
                path.display()
            ),
            Self::NoSuchModule { path, module } => write!(
                f,
                "no such module in the ledger {}: {module}",
                path.display()
            ),
            Self::Ir(source) => write!(f, "{source}"),
            Self::IndexShape { path, message } => write!(f, "{}: {message}", path.display()),
            Self::ModuleListMismatch { missing, extra } => write!(
                f,
                "--modules and the merged IR name different modules: {} in the list with nothing \
                 behind them ({}), {} in the merged tree the list does not name ({}). index.json's \
                 module order is this list's, so the odd ones out would have to be guessed at — \
                 and a wrong guess moves index.json alone, where no page byte follows it",
                missing.len(),
                some_of(missing),
                extra.len(),
                some_of(extra),
            ),
            // The prototype's exact wording: `impact.ts:182` and `:207`.
            Self::NotAModule { module } => write!(f, "not a module of this package: {module}"),
            Self::UnknownMode { mode } => write!(f, "unknown --mode {mode}"),
            Self::OutsidePageRoot { root, path } => write!(
                f,
                "refusing to delete {} — it is not under the page root {}",
                path.display(),
                root.display()
            ),
        }
    }
}

/// How many names a refusal spells out before it only counts them.
///
/// The same shape as `merge --verify`'s `VERIFY_DEP_FAILURES`: a caller needs a
/// name to start from, not a list that scrolls a screenful of terminal away.
const NAMES_IN_REFUSAL: usize = 10;

fn some_of(names: &[String]) -> String {
    if names.is_empty() {
        return "none".to_owned();
    }
    let shown = names
        .iter()
        .take(NAMES_IN_REFUSAL)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    if names.len() > NAMES_IN_REFUSAL {
        format!("{shown}, … and {} more", names.len() - NAMES_IN_REFUSAL)
    } else {
        shown
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Json { source, .. } => Some(source),
            Self::Ir(source) => Some(source),
            _ => None,
        }
    }
}
