//! The run: an IR tree in, six whole-package artifacts out.
//!
//! Ported from `experiments/stage7h/global.ts:238-361` (frozen), minus
//! everything M2-b owns: the `--state` cache, the map delta and its
//! `--print-set` / `--delta-json` outputs, and the timings record. What is left
//! is the from-scratch path the prototype's own oracle compares against — the
//! prototype calls it "not a fallback, it is the from-scratch build".
//!
//! # Where the cache goes
//!
//! [`facts_for`] is the only place a module's IR becomes [`ModuleFacts`], and it
//! reads `contentHash` off the index entry it did not need. M2-b replaces its
//! body with "hash unchanged -> reuse the stored facts" and nothing else in this
//! crate moves. That is plan §3's structural constraint, kept rather than
//! implemented.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use lean_doc_ir::IrTree;

use crate::artifacts::Artifacts;
use crate::facts::ModuleFacts;

/// What a run needs to know that the IR does not carry.
#[derive(Clone, Copy, Debug)]
pub struct GlobalOptions<'a> {
    /// The IR tree: `index.json`, `modules/`, `deps/`.
    pub ir: &'a Path,
    /// The site root. `declarations/` is created under it; the other four
    /// artifacts sit directly in it.
    pub out: &'a Path,
}

/// What a run did, in the units its inputs are counted in.
///
/// The prototype prints the same numbers, so the two runs can be put side by
/// side without re-deriving anything.
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
}

/// Reads every module of the IR and derives its facts, in index order.
///
/// **The one funnel** — see the module heading. Index order is not incidental:
/// [`Artifacts::derive`] resolves a duplicated declaration name in favour of the
/// later module.
pub fn facts_for(tree: &IrTree) -> Result<Vec<ModuleFacts>, lean_doc_ir::Error> {
    tree.index()
        .modules
        .iter()
        .map(|entry| {
            let module = tree.module(entry)?;
            Ok(ModuleFacts::of(&module, &entry.content_hash))
        })
        .collect()
}

/// Reads the IR, derives the six artifacts and writes them.
pub fn build_global(options: &GlobalOptions<'_>) -> Result<GlobalSummary, Error> {
    let tree = IrTree::open(options.ir)?;
    let facts = facts_for(&tree)?;
    let dep_maps = tree.load_dep_maps()?;
    let artifacts = Artifacts::derive(&facts, &dep_maps);

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

    // Read back off the artifact rather than recounted from the facts: a
    // summary that counts the inputs again can disagree with the file it is
    // reporting on, and then the number quoted in a document is nobody's.
    let sections = sections(&artifacts.declaration_data_bmp);

    Ok(GlobalSummary {
        modules: facts.len(),
        declarations: sections("declarations"),
        dependency_names: sections("dependencies"),
        instance_classes: sections("instances"),
        tactic_docs: facts.iter().map(|facts| facts.tactics).sum(),
        bmp_bytes: artifacts.declaration_data_bmp.len(),
        name_map_bytes: artifacts.name_map_json.len(),
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

/// Why a run stopped.
#[derive(Debug)]
pub enum Error {
    Ir(lean_doc_ir::Error),
    Io { path: PathBuf, source: io::Error },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ir(source) => write!(f, "{source}"),
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Ir(source) => Some(source),
            Self::Io { source, .. } => Some(source),
        }
    }
}

impl From<lean_doc_ir::Error> for Error {
    fn from(source: lean_doc_ir::Error) -> Self {
        Self::Ir(source)
    }
}
