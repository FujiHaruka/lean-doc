//! The `contentHash` cache: `--state <dir>` keeps `<dir>/global-state.json`.
//!
//! Ported from `experiments/stage7h/global.ts:127-229` (frozen). Milestone
//! **M2-b** — see `docs/implementation-plan.md` §3, which asks for "the cache
//! version key plus the test that breaks it" rather than for a cache.
//!
//! ```text
//! index.json ──┬─> State::load ──> facts_for ──> [ModuleFacts] ──> State::save
//!              └─ contentHash: the only thing a hit is decided on
//! ```
//!
//! # The key is the IR's own hash, not the caller's idea of what changed
//!
//! A driver that passes a wrong changed-set cannot corrupt this cache: an entry
//! is used iff its `contentHash` equals the one `index.json` carries now. A
//! stale entry needs the extractor to emit the same hash for different bytes,
//! which is the assumption the whole incremental pipeline already makes.
//!
//! # Everything that can go wrong loads as "empty"
//!
//! [`State::load`] returns an empty cache when the file is missing, when it does
//! not parse, and when any of the four version keys disagrees with the index. It
//! says nothing on the way out. That is the prototype's behaviour and this is a
//! port: a cold cache is the normal first run, and a `--state` directory that
//! has been left behind by another tool is not an error the caller can act on —
//! the only correct response is to rebuild, which is what happens. The cost of
//! being wrong here is time; the cost of trusting a foreign entry is a wrong
//! artifact that nobody reports.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use lean_doc_ir::Index;
use serde::{Deserialize, Serialize, Serializer};

use crate::facts::ModuleFacts;
use crate::site::Error;

/// The one file a state directory holds.
pub const STATE_FILE: &str = "global-state.json";

/// Bumped when the *file format* changes. Kept separate from
/// [`STATE_DERIVATION`], which is bumped when the *facts* change, because the
/// two rot for different reasons.
pub const STATE_VERSION: u64 = 1;

/// Which rule built the facts in the file.
///
/// **Deliberately not the prototype's `"stage7h/global.ts facts v1"`.** Plan §6
/// states the discipline for `extractKey.extractor` and `renderKey.renderer`:
/// the identity string names an implementation, so a different implementation
/// with the same string is a silent wrong-cache hit. This crate derives
/// [`ModuleFacts`] with its own code — same intent, different tokeniser in one
/// documented place (see [`crate::facts::autolink_tokens`]) — so a state written
/// by the TypeScript prototype must miss on every module here, and one written
/// here must miss over there. Changing this string to match would make the two
/// caches interchangeable, which is exactly the claim nobody has checked.
///
/// Bump this whenever a field of [`ModuleFacts`] or the way one is derived
/// changes. Bumping makes every entry a miss, which is correct and slow;
/// keeping entries built by another rule is fast and wrong.
pub const STATE_DERIVATION: &str = "lean-doc-global facts v1";

/// The facts a previous run left behind, already checked against this run's
/// index.
///
/// Empty is a complete and valid value: it means every module will be read.
#[derive(Clone, Debug, Default)]
pub struct State {
    modules: HashMap<String, ModuleFacts>,
}

impl State {
    /// A cache that hits nothing — what `--state` being absent gives, and what
    /// every failure in [`State::load`] gives.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Reads `<dir>/global-state.json`, or gives an empty cache.
    ///
    /// See the module heading for why no failure is reported. The four version
    /// keys are checked against `index` here rather than at the hit test, so
    /// that a foreign state costs one parse and not 432 comparisons.
    #[must_use]
    pub fn load(dir: Option<&Path>, index: &Index) -> Self {
        let Some(dir) = dir else {
            return Self::empty();
        };
        let Ok(raw) = fs::read_to_string(dir.join(STATE_FILE)) else {
            return Self::empty();
        };
        // A missing key, a key of the wrong type and a key with the wrong value
        // all end here. The prototype reaches the same place by a different
        // road — `undefined !== STATE_VERSION` — because JSON.parse accepts
        // anything and the comparison rejects it.
        let Ok(state) = serde_json::from_str::<StateFile>(&raw) else {
            return Self::empty();
        };
        // A state written by a different rule, a different IR schema or a
        // different extractor is not a cache, it is a guess. Drop it whole.
        if state.state_version != STATE_VERSION
            || state.derivation != STATE_DERIVATION
            || state.schema_version != index.schema_version
            || state.generator != index.generator
        {
            return Self::empty();
        }
        Self {
            modules: state.modules,
        }
    }

    /// The stored facts for a module, whatever hash they were built for.
    ///
    /// The hash test is the caller's ([`crate::facts_for`]) so that this type
    /// has no opinion about what a hit is: `cached.content_hash ==
    /// entry.content_hash`, and nothing else.
    #[must_use]
    pub fn get(&self, module: &str) -> Option<&ModuleFacts> {
        self.modules.get(module)
    }

    /// How many modules the file offered — not how many will hit.
    #[must_use]
    pub fn len(&self) -> usize {
        self.modules.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }

    /// Writes `<dir>/global-state.json` and returns its size in bytes, or 0
    /// when there is no state directory.
    ///
    /// **Only modules the index still lists are written, in index order.** An
    /// entry for a module that has left the package has to disappear with it:
    /// keeping it would leave a name in `name-map.json` and a module in
    /// `importedBy` that no IR file backs, and a cache that only ever grows
    /// passes every other test (`oracle.sh` state 4 exists for this).
    /// Index order rather than hash order so that two runs over the same module
    /// set write the same bytes; a file that churns for no reason is a file
    /// nobody can diff.
    pub fn save(dir: Option<&Path>, index: &Index, facts: &[ModuleFacts]) -> Result<usize, Error> {
        let Some(dir) = dir else {
            return Ok(0);
        };
        fs::create_dir_all(dir).map_err(|source| Error::Io {
            path: dir.to_owned(),
            source,
        })?;
        // Keyed by the facts' own module name, which is the index entry's:
        // `IrTree::module` refuses a file that disagrees with the index about
        // which module it holds, so this is the prototype's `facts.get(e.module)`.
        let by_module: HashMap<&str, &ModuleFacts> = facts
            .iter()
            .map(|facts| (facts.module.as_str(), facts))
            .collect();
        let kept: Vec<&ModuleFacts> = index
            .modules
            .iter()
            .filter_map(|entry| by_module.get(entry.module.as_str()).copied())
            .collect();

        let body = serde_json::to_string(&StateOut {
            state_version: STATE_VERSION,
            derivation: STATE_DERIVATION,
            schema_version: index.schema_version,
            generator: &index.generator,
            modules: Modules(&kept),
        })
        .expect("module facts are strings, numbers and arrays of them");

        let path = dir.join(STATE_FILE);
        fs::write(&path, &body).map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        Ok(body.len())
    }
}

/// The file as read. Field order is irrelevant here and load-bearing in
/// [`StateOut`].
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StateFile {
    state_version: u64,
    derivation: String,
    schema_version: u32,
    generator: String,
    /// `st.modules ?? {}`: a file with no module map is a valid empty cache
    /// rather than a parse failure.
    #[serde(default)]
    modules: HashMap<String, ModuleFacts>,
}

/// The file as written.
///
/// **The key order is the bytes.** `JSON.stringify` of the prototype's object
/// literal emits `stateVersion` / `derivation` / `schemaVersion` / `generator` /
/// `modules`, and serde emits a struct's fields in declaration order, so this
/// list is a transcription. [`ModuleFacts`]'s own field order is the same kind
/// of transcription — see `tests/state_and_delta.rs`, which compares this file
/// with one the prototype wrote, byte for byte.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StateOut<'a> {
    state_version: u64,
    derivation: &'a str,
    schema_version: u32,
    generator: &'a str,
    modules: Modules<'a>,
}

/// The module map, serialised in the order given rather than through a
/// `serde_json::Map`.
///
/// A map's *serialisation* order is the iterator's, so this needs no
/// `preserve_order` — unlike the artifacts, which build `Value` trees.
struct Modules<'a>(&'a [&'a ModuleFacts]);

impl Serialize for Modules<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_map(self.0.iter().map(|facts| (facts.module.as_str(), *facts)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lean_doc_ir::IndexEntry;

    fn facts(module: &str) -> ModuleFacts {
        ModuleFacts {
            module: module.to_owned(),
            content_hash: "0".repeat(16),
            imports: Vec::new(),
            tactics: 0,
            decls: Vec::new(),
            instances: Vec::new(),
            tokens: Vec::new(),
        }
    }

    fn index(modules: &[&str]) -> Index {
        Index {
            schema_version: 4,
            generator: "test".to_owned(),
            lean_version: "4.31.0".to_owned(),
            hash_algorithm: "lean-string-hash-64/hex16".to_owned(),
            module_count: modules.len() as u32,
            declaration_count: 0,
            ablations: Vec::new(),
            modules: modules
                .iter()
                .map(|module| IndexEntry {
                    module: (*module).to_owned(),
                    file: format!("modules/{module}.json"),
                    bytes: 0,
                    declarations: 0,
                    content_hash: "0".repeat(16),
                })
                .collect(),
            dependency_maps: Vec::new(),
        }
    }

    /// The contract of [`State::save`], which the whole-package tests cannot
    /// make non-vacuous: they only ever hand it facts that came from the index
    /// it is passed, so the filter below is unreachable from there and a port
    /// that dropped it would pass every one of them.
    ///
    /// What is at stake is `oracle.sh` state 4: a module that has left the
    /// package must leave the cache with it, and the entry that names it must
    /// not survive into the file.
    #[test]
    fn only_the_index_survives_into_the_file() {
        let dir = std::env::temp_dir().join(format!("lean-doc-state-save-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let index = index(&["Pkg.A", "Pkg.B"]);
        // Out of index order, and one module the index does not list.
        let facts = [facts("Pkg.B"), facts("Pkg.Gone"), facts("Pkg.A")];
        let bytes = State::save(Some(&dir), &index, &facts).expect("the state is writable");

        let body = fs::read_to_string(dir.join(STATE_FILE)).expect("the state was written");
        assert_eq!(body.len(), bytes);
        let written: serde_json::Value = serde_json::from_str(&body).expect("the state is JSON");
        let modules: Vec<&String> = written["modules"]
            .as_object()
            .expect("a module map")
            .keys()
            .collect();
        assert_eq!(modules, ["Pkg.A", "Pkg.B"], "in index order, index only");
        let _ = fs::remove_dir_all(&dir);
    }

    /// No state directory, no file, no bytes — and no error either.
    #[test]
    fn without_a_directory_nothing_is_written() {
        assert_eq!(
            State::save(None, &index(&["Pkg.A"]), &[facts("Pkg.A")]).expect("no io happens"),
            0
        );
        assert!(State::load(None, &index(&["Pkg.A"])).is_empty());
    }
}
