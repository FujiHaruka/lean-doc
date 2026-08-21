//! Opening an IR tree and reading files out of it.
//!
//! [`IrTree::module`] is the single funnel every module read goes through —
//! including the ones [`IrTree::modules`] and [`IrTree::load_modules`] make.
//! That is the structural constraint plan §3 asks for: the incremental pipeline
//! still reads the whole IR five times, and the `contentHash` cache that would
//! remove four of them has exactly one place to go. The cache itself is not
//! here yet — it is performance, not correctness, and gate A does not include
//! it — but nothing else has to move when it arrives.

use std::fs;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;

use crate::error::{Error, Result};
use crate::metrics::{self, IrFile};
use crate::model::{DepMap, DepMapEntry, Index, IndexEntry, ModuleFile};

/// The schema this reader understands. Schema 3 has no attributes, no instance
/// index and no member binders / docstrings / origin, so a schema-3 IR cannot
/// produce a byte-identical page.
///
/// **It did not move to 5 with the writer**, and the reason is worth stating
/// because the reverse looks obvious. Schema 5 adds `sorry`, whose *absent* key
/// means "no `sorry`" — a meaning a schema-4 file cannot carry, since there the
/// key could not exist. Raising the floor is one way to keep the two apart;
/// [`crate::ModuleFile::sorry_of`] is the other, and it is the one taken here,
/// because raising the floor would mean hand-editing the curated schema-4 IR
/// frozen inside `litedoc4-global/tests/data/global-expected.json` — the
/// fixtures' re-freeze is a step of its own with a procedure of its own
/// (`docs/plans/feature-sweep.md` §3 決定 1 / C-4), not something to do in
/// passing. Nothing reads [`crate::Decl::sorry`] without going through
/// `sorry_of`, so no consumer can conflate the two states in the meantime.
pub const MIN_SCHEMA_VERSION: u32 = 4;

/// The first schema whose module files carry [`crate::Decl::sorry`].
///
/// Below it the key's absence says nothing; at or above it, it says "no
/// `sorry`". [`crate::ModuleFile::sorry_of`] is the only place this is applied.
pub const SORRY_SCHEMA_VERSION: u32 = 5;

/// The first schema whose module files carry [`crate::Decl::selection_range`]
/// and [`crate::Decl::generated`] (`docs/plans/feature-sweep.md` B-3).
///
/// The same number as [`SORRY_SCHEMA_VERSION`] and a constant of its own on
/// purpose: the two keys arrived under one version bump, but what each absence
/// means is a fact about that key, and a reader that shared one constant would
/// have to be edited in two places the day they stop being the same schema.
/// [`crate::ModuleFile::naming_of`] and [`crate::ModuleFile::generated_by`] are
/// the only places this is applied.
pub const SELECTION_RANGE_SCHEMA_VERSION: u32 = 5;

/// An IR tree on disk: `index.json`, `modules/`, `deps/`.
#[derive(Debug)]
pub struct IrTree {
    root: PathBuf,
    index: Index,
}

impl IrTree {
    /// Reads `index.json` and refuses an IR that must not be rendered — too old
    /// a schema, or written with an ablation.
    ///
    /// Only the index is read; module files are read on demand.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let tree = Self::open_unvalidated(root)?;
        tree.index.require_renderable()?;
        Ok(tree)
    }

    /// As [`IrTree::open`] without the refusals, for tools that want to look at
    /// an ablated or older tree rather than render it.
    pub fn open_unvalidated(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        let index: Index = read_json(&root.join("index.json"), IrFile::Index)?;
        Ok(Self { root, index })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn index(&self) -> &Index {
        &self.index
    }

    /// Resolves a path recorded in the index (`modules/Foo.json`,
    /// `deps/Mathlib.json`) against the tree root.
    pub fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    /// Reads one module file.
    ///
    /// Checks that the file agrees with the index about which module it is, and
    /// that its own `schemaVersion` is new enough: an incremental tree is a
    /// merge of files from several extractor runs, so the index's version does
    /// not vouch for the modules'.
    pub fn module(&self, entry: &IndexEntry) -> Result<ModuleFile> {
        let path = self.path(&entry.file);
        let module = read_module_file(&path)?;
        if module.module != entry.module {
            return Err(Error::ModuleMismatch {
                path,
                expected: entry.module.clone(),
                found: module.module,
            });
        }
        Ok(module)
    }

    /// Every module, in index order, read lazily.
    ///
    /// Lazily so that a caller which only needs a few modules — or which wants
    /// to stream rather than hold 16 MB of IR — does not pay for the rest.
    pub fn modules(&self) -> impl Iterator<Item = Result<ModuleFile>> + '_ {
        self.index.modules.iter().map(|entry| self.module(entry))
    }

    /// Every module, in index order, all at once. Stops at the first failure.
    pub fn load_modules(&self) -> Result<Vec<ModuleFile>> {
        self.modules().collect()
    }

    /// Reads one dependency slice.
    pub fn dep_map(&self, entry: &DepMapEntry) -> Result<DepMap> {
        read_json(&self.path(&entry.file), IrFile::DepMap)
    }

    /// Every dependency slice, in index order.
    pub fn load_dep_maps(&self) -> Result<Vec<DepMap>> {
        self.index
            .dependency_maps
            .iter()
            .map(|entry| self.dep_map(entry))
            .collect()
    }
}

/// Reads one module file by path, checking only its own `schemaVersion`.
///
/// The funnel [`IrTree::module`] goes through, and the way in for the one caller
/// that has no index to check the file against: the **merger** reads the tree it
/// is in the middle of writing, whose `index.json` is written last. Keeping it
/// here rather than letting that caller reach for `serde_json` is the plan §3
/// constraint — the `contentHash` cache has one place to go.
///
/// **That constraint holds for module files and not for `index.json`**【実測
/// 2026-08-16, the work counters】. Three callers read the index outside this
/// crate: `merge` (which round-trips index keys this crate does not model),
/// `ledger::extract_key` and `prune::read_index_modules`. The V2 cache belongs
/// on the module files, so the design stands — but a counter placed only inside
/// this crate under-reports every incremental run by three or four index reads,
/// and the claim as originally written ("every read of the IR is in this crate")
/// was false.
pub fn read_module_file(path: &Path) -> Result<ModuleFile> {
    let module: ModuleFile = read_json(path, IrFile::Module)?;
    if module.schema_version < MIN_SCHEMA_VERSION {
        return Err(Error::Schema {
            what: path.display().to_string(),
            found: module.schema_version,
            required: MIN_SCHEMA_VERSION,
        });
    }
    Ok(module)
}

/// The one place this crate touches the disk — and therefore the one place the
/// work counter can be kept honest.
///
/// `kind` is a parameter rather than something derived from `path` 【判断】: a
/// caller that adds a read has to say what it is reading, and a guess from the
/// file name would be a fourth spelling of the tree's layout that nothing checks.
fn read_json<T: DeserializeOwned>(path: &Path, kind: IrFile) -> Result<T> {
    metrics::record(kind);
    // Read the whole file, then parse, rather than `from_reader`: the largest
    // file in the tree is a few MB, and `serde_json` is documented as being
    // faster over a string than over a reader.
    let text = fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.to_owned(),
        source,
    })?;
    serde_json::from_str(&text).map_err(|source| Error::Json {
        path: path.to_owned(),
        source,
    })
}
