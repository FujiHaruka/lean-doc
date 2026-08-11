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
use crate::model::{DepMap, DepMapEntry, Index, IndexEntry, ModuleFile};

/// The schema this reader understands. Schema 3 has no attributes, no instance
/// index and no member binders / docstrings / origin, so a schema-3 IR cannot
/// produce a byte-identical page.
pub const MIN_SCHEMA_VERSION: u32 = 4;

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
        let index: Index = read_json(&root.join("index.json"))?;
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
        let module: ModuleFile = read_json(&path)?;
        if module.schema_version < MIN_SCHEMA_VERSION {
            return Err(Error::Schema {
                what: entry.file.clone(),
                found: module.schema_version,
                required: MIN_SCHEMA_VERSION,
            });
        }
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
        read_json(&self.path(&entry.file))
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

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
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
