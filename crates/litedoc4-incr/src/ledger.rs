//! The ledger file: what a module's inputs hash to, and the two global keys.
//!
//! Ported from `experiments/stage5/ledger.ts` (frozen). Milestone **M3-a** —
//! see `docs/implementation-plan.md` §6, whose data-format table pins the shape:
//! one `ledger.json`, `ledgerSchema: 2`, `extractKey` / `renderKey` inside it,
//! values in the clear.
//!
//! ```text
//! module name ──> <libDir>/<Module/Path>{.olean,.olean.server,.olean.private}
//!                   │
//!                   ├─ --algorithm sha256: read the bytes, hash them here
//!                   └─ --algorithm lake:   read <file>.hash, already on disk
//!                   │
//!                   └──> ModuleEntry.hash = sha256("<path> <hash>\n…")
//! ```
//!
//! # What is hashed, and why it is the olean
//!
//! The extractor's only view of a module is its `.olean`: `importModules` reads
//! nothing else. The `.lean` source is one step too early — it carries changes
//! the olean does not (whitespace inside a proof) and misses changes the olean
//! has (anything a rebuild of a dependency puts there) — and the IR's own
//! `contentHash` is one step too late, because computing it means running the
//! extraction this stage exists to skip. That hash has the other job: which
//! *pages* to rewrite.
//!
//! Modules built with Lean's module system have up to three olean files and
//! **all present ones are hashed**: declaration ranges and docstrings live in
//! the server data and private declarations in the private one. On the
//! measurement target the package's own modules have only `.olean`; Mathlib's
//! have all three.
//!
//! # The per-module hash is over the per-file hashes, not over the bytes
//!
//! [`ModuleEntry::hash`] is a SHA-256 of `"<relative path> <file hash>"` lines,
//! so a module that gains or loses one of its three files changes even when
//! every file it kept is identical.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use litedoc4_ir::cmp_utf16;
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};

use crate::detect::Error;

/// The ledger file format. `1` had a single `envKey`; `2` splits it into
/// `extractKey` and `renderKey`, which do not invalidate the same thing.
pub const LEDGER_SCHEMA: u64 = 2;

/// `extractKey.extractor`: which implementation will run when the key says
/// "re-extract".
///
/// **Deliberately not the prototype's `"lean-doc/experiments/stage4b"`.** Plan
/// §6 states the discipline and the failure it prevents: the string names an
/// implementation, so leaving the prototype's there would let a ledger written
/// by one pipeline be trusted by the other — "a different implementation with
/// the same key", and the cache hits when it must not. This is the same rule
/// [`litedoc4_global::STATE_DERIVATION`] follows.
///
/// Note what does **not** change with it: `extractKey.irGenerator` still
/// carries whatever wrote the IR (today `lean-doc/experiments/stage4b`), because
/// that is a fact about the tree on disk rather than about this crate.
///
/// Bump the version when a re-extraction can produce different IR bytes.
///
/// **v2** is B-2 (`docs/plans/feature-sweep.md`): `attrs` elements became
/// `[name, value]` arrays where they had been one concatenated string. That is
/// the case this constant exists for and `irSchemaVersion` cannot cover —
/// the schema was already 5 when the elements were still strings, so the only
/// thing that separates the two IRs is which extractor wrote them. Without the
/// bump an IR extracted before the change is reused, renders the same bytes
/// today, and silently has nothing for bundle C to link `@[deprecated Foo]` to.
///
/// **v3** is B-3, and it is the same case a second time: `selectionRange` and
/// `generated` are new keys under a schema number that was already 5. Neither
/// changes a rendered byte today, so the failure this bump prevents is again
/// silent — a reused IR would carry no origin for bundle C to fold `@[ext]`'s
/// theorems by, and would say so by omitting exactly the key whose omission
/// means "not realized by `@[ext]`".
///
/// [`litedoc4_global::STATE_DERIVATION`]: ../litedoc4_global/constant.STATE_DERIVATION.html
pub const EXTRACTOR_ID: &str = "litedoc4 extractor v3";

/// `renderKey.renderer`: which implementation will run when the key says
/// "re-render everything".
///
/// Deliberately not the prototype's `"lean-doc/experiments/stage4c"`, for the
/// reason above and for one that is already measured: the Rust renderer and
/// `render.ts` do **not** agree on every byte (plan §5 registers the CommonMark
/// divergence, 1 page of 432). A ledger written by one and believed by the
/// other would report that page as up to date while its bytes differ.
///
/// Bump the version when the renderer's output bytes can change with the IR
/// held fixed.
///
/// **v1 -> v2 (M7-b)**: the render key gained `externalLinks`, so a v1 ledger and
/// a v2 one are not comparable — a ledger written before the key existed has no
/// value for it, and "the key is absent" would otherwise be read as "the map did
/// not move" for every page rendered without one.
///
/// **v2 -> v3 (feature-sweep bundle C)**: three changes to what a page holds,
/// none of which any other key can see. `$…$` became MathML (C-1), every
/// declaration gained a `Used by` block (C-2), and the site's title can now come
/// from `litedoc4.toml` (C-3). The IR did not move for any of them, so without
/// this bump an incremental build over a site rendered by a v2 binary would find
/// every page up to date and **leave the old bytes in place** — a site half of
/// whose pages have mathematics and half of which do not, reported as "0 pages
/// rendered" and therefore as success.
pub const RENDERER_ID: &str = "litedoc4 renderer v3";

/// The olean files a module can have, in the order they are hashed.
pub const OLEAN_SUFFIXES: [&str; 3] = [".olean", ".olean.server", ".olean.private"];

/// Where a file's hash comes from.
///
/// A string rather than an enum because the ledger stores it verbatim and
/// **anything that is not `lake` reads and hashes the bytes** — that is the
/// prototype's `algorithm === "lake" ? … : …` transcribed, and it means an
/// unknown algorithm degrades to the reference one rather than to nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Algorithm(String);

impl Algorithm {
    /// Read `<file>.hash`, the 64-bit content hash Lake already wrote next to
    /// every olean (`computeBinFileHash`, `Lake/Build/Common.lean`). A string
    /// read, no hashing: 6.9 KB instead of 227 MB on the target.
    pub const LAKE: &'static str = "lake";
    /// SHA-256 over the olean bytes. The reference: cryptographic, and not an
    /// undocumented implementation detail of the build tool.
    pub const SHA256: &'static str = "sha256";

    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    #[must_use]
    pub fn sha256() -> Self {
        Self::new(Self::SHA256)
    }

    #[must_use]
    pub fn lake() -> Self {
        Self::new(Self::LAKE)
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.0
    }

    /// Whether the olean bytes are read and hashed here, rather than Lake's
    /// hash being read off the disk.
    #[must_use]
    pub fn hashes_bytes(&self) -> bool {
        self.0 != Self::LAKE
    }
}

/// One olean file of one module.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    /// Relative to the ledger's `target`.
    pub path: String,
    /// **`-1` under `--algorithm lake`**: nothing was read but the hash file, so
    /// there is no byte count to report and a zero would read as an empty
    /// olean. Every sum over this field clamps at zero.
    pub bytes: i64,
    pub hash: String,
}

/// One module's entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleEntry {
    pub module: String,
    pub files: Vec<FileEntry>,
    /// Over the per-file hashes, so a missing or extra file shows up.
    pub hash: String,
}

/// An ordered `name -> value` map, as `extractKey` and `renderKey` are stored.
///
/// Ordered because the ledger's bytes are `JSON.stringify` of an object built
/// key by key, and a `BTreeMap` would re-sort it. Values are **in the clear,
/// not hashed**, so that a mismatch names itself in a log (plan §6).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeySet(Vec<(String, String)>);

impl KeySet {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a key, or replaces the value of one already present **in place** —
    /// which is what assigning to a JavaScript object property does, and what a
    /// duplicated key in a hand-edited file has to do to round-trip.
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let key = key.into();
        let value = value.into();
        match self.0.iter_mut().find(|(name, _)| *name == key) {
            Some(slot) => slot.1 = value,
            None => self.0.push((key, value)),
        }
    }

    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The names, in UTF-16 order, of the keys **present in either set** whose
    /// values differ.
    ///
    /// The union is the rule plan §5 states for M3, and it is what makes a
    /// missing key loud: a key that vanished — an `--ir` that was not passed
    /// this time, a forgotten `--source-url` — compares `None != Some(_)` and
    /// counts as a change. Over-extracting and over-rendering are the safe
    /// directions; the failure this prevents is silently rendering too little,
    /// which nobody reports because the site still looks built.
    ///
    /// Sorted with [`cmp_utf16`]: `ledger.ts:213` is a bare
    /// `Array.prototype.sort()` (plan §7, U1). The order reaches a file — the
    /// reason lines of `--render-all-out`.
    #[must_use]
    pub fn diff(&self, now: &Self) -> Vec<String> {
        let mut seen: HashSet<&str> = HashSet::new();
        let mut names: Vec<&str> = Vec::new();
        for (name, _) in self.iter().chain(now.iter()) {
            if seen.insert(name) {
                names.push(name);
            }
        }
        let mut changed: Vec<String> = names
            .into_iter()
            .filter(|name| self.get(name) != now.get(name))
            .map(str::to_owned)
            .collect();
        changed.sort_by(|a, b| cmp_utf16(a, b));
        changed
    }
}

impl Serialize for KeySet {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_map(self.iter())
    }
}

impl<'de> Deserialize<'de> for KeySet {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct KeySetVisitor;
        impl<'de> Visitor<'de> for KeySetVisitor {
            type Value = KeySet;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a map of strings to strings")
            }

            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<KeySet, M::Error> {
                let mut keys = KeySet::new();
                while let Some((key, value)) = map.next_entry::<String, String>()? {
                    keys.insert(key, value);
                }
                Ok(keys)
            }
        }
        deserializer.deserialize_map(KeySetVisitor)
    }
}

/// The ledger file.
///
/// Field order **is** the file's key order: `serde` emits a struct's fields as
/// declared, and this list transcribes the prototype's object literal
/// (`ledger.ts:244-252`). `tests/ledger.rs` compares the result with a ledger
/// the prototype wrote, byte for byte.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ledger {
    /// `ledger.ledgerSchema ?? 1`: a file without the field is a schema-1 file,
    /// not a parse failure, so that `check` can name what is wrong with it.
    #[serde(default = "schema_before_the_split")]
    pub ledger_schema: u64,
    pub algorithm: Algorithm,
    pub target: String,
    pub lib_dir: String,
    pub extract_key: KeySet,
    /// `ledger.renderKey ?? {}` at the comparison, but **`None` stays absent on
    /// the way out**: `touch` rewrites the file it read, and a key this crate
    /// invented would be a byte nobody asked for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub render_key: Option<KeySet>,
    pub modules: Vec<ModuleEntry>,
}

fn schema_before_the_split() -> u64 {
    1
}

impl Ledger {
    /// The file's bytes: `JSON.stringify(ledger) + "\n"`.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("a ledger is strings, numbers and arrays of them") + "\n"
    }

    /// The entry for a module, or `None` when the ledger has never seen it.
    #[must_use]
    pub fn entry(&self, module: &str) -> Option<&ModuleEntry> {
        self.modules.iter().find(|entry| entry.module == module)
    }

    /// The render key as the comparison sees it: an absent one is empty, which
    /// makes every current render key a change rather than no change at all.
    #[must_use]
    pub fn render_key_or_empty(&self) -> KeySet {
        self.render_key.clone().unwrap_or_default()
    }
}

/// The `extractKey`: everything that can change the IR bytes.
///
/// Changed ⇒ re-extract everything. Which pages are then stale still follows
/// from the IR diff, as usual — a re-extraction that lands byte-identical
/// rewrites no page.
///
/// The split from [`render_key`] is not cosmetic (`ledger.ts:159-185`).
/// `--source-url` carries a 40-hex git revision, so it changes on *every
/// commit*, which is exactly when an incremental build runs; under one key every
/// real incremental build would pay a full re-extraction — Lean started, 27 s —
/// for an input Lean cannot see. The test for which side an input belongs on is
/// not "does it change the output" (both do) but "can it change the IR".
pub fn extract_key(target: &str, ir: Option<&Path>) -> Result<KeySet, Error> {
    let mut key = KeySet::new();
    let root = Path::new(target);
    key.insert(
        "leanToolchain",
        read_to_string(&root.join("lean-toolchain"))?.trim(),
    );
    key.insert(
        "manifestSha256",
        // The manifest is hashed rather than stored: it is 20 KB, and unlike the
        // other values there is nothing in it a human reads out of a log.
        sha256_text(&read_to_string(&root.join("lake-manifest.json"))?),
    );
    key.insert("extractor", EXTRACTOR_ID);
    if let Some(ir) = ir {
        let path = ir.join("index.json");
        // Two fields of the index, read as plain JSON — but a read of an IR file
        // all the same, so it is counted like every other (`litedoc4_ir::metrics`).
        // `detect` and `build`'s final ledger write both come through here, which
        // is why an unchanged run still shows `index` reads with `module` at zero.
        litedoc4_ir::metrics::record(litedoc4_ir::IrFile::Index);
        let index: serde_json::Value = serde_json::from_str(&read_to_string(&path)?)
            .map_err(|source| Error::Json { path, source })?;
        key.insert("irSchemaVersion", js_string(index.get("schemaVersion")));
        key.insert("irGenerator", js_string(index.get("generator")));
    }
    Ok(key)
}

/// The `renderKey`: what changes the page bytes with the IR held fixed.
///
/// Changed ⇒ re-extract **nothing**, re-render **everything**. The generator id
/// stands in for the renderer's configuration that has no flag of its own;
/// everything that does have a flag and reaches the output bytes belongs here
/// beside it.
///
/// # `linkIndex` — the hole M4-d left, closed in M5-b 【実測】
///
/// The third input is the dependency map. Until M5 it was a file somebody
/// handed the renderer, derived outside the product from a doc-gen4 site, and
/// the plan recorded the consequence rather than fixing it: with the same IR and
/// the same `--source-url`, **having the map or not moves 150 of the target's
/// 432 pages' bytes** (plan 決定 4), and nothing in the ledger named it — so a
/// run whose only changed input was the map re-rendered nothing and reported
/// success. `--full` was the escape hatch.
///
/// M5-a made the map something the product derives from the environment the
/// extractor has already imported, which is what gives it an identity worth
/// recording. The identity is the **SHA-256 of the file's bytes**, not its path
/// and not its size: the bytes are what the renderer reads, so two maps that
/// hash the same produce the same pages whoever wrote them and wherever they
/// came from. `None` — no map at all — leaves the key absent, and
/// [`KeySet::diff`] counts an appearing or vanishing key as a change, which is
/// the loud direction.
///
/// The cost is one hash of 8.5 MB per `detect`, ~11 ms with the `asm` feature
/// this crate already needs for the olean hashes.
///
/// # `externalLinks` — the same hole one level out (M7-b)
///
/// The fourth input is where a **dependency's** source lives: `Mathlib` ->
/// `…/mathlib4/blob/<rev>`, and eighteen more
/// (`litedoc4_render::ExternalLinks`). It is an input to every page for the same
/// reason the dependency map is — a bumped dependency moves a `rev`, which moves
/// the href of every link into it — and it changes on exactly the occasion an
/// incremental build runs, so a run whose only changed input was that map has to
/// re-render rather than report success.
///
/// The identity is [`litedoc4_render::ExternalLinks::digest`], which is a
/// function of what the map *resolves* rather than of how it was built: a
/// resolver that reorders its scan must not re-render 432 pages for nothing.
/// `None` leaves the key absent, and an appearing key is a change — the loud
/// direction, as above.
///
/// [`litedoc4_render::ExternalLinks`]: ../litedoc4_render/struct.ExternalLinks.html
/// [`litedoc4_render::ExternalLinks::digest`]: ../litedoc4_render/struct.ExternalLinks.html#method.digest
#[must_use]
pub fn render_key(
    source_url: &str,
    link_index: Option<&str>,
    external_links: Option<&str>,
) -> KeySet {
    let mut key = KeySet::new();
    key.insert("renderer", RENDERER_ID);
    if !source_url.is_empty() {
        key.insert("sourceUrl", source_url.trim_end_matches('/'));
    }
    if let Some(digest) = link_index {
        key.insert("linkIndex", digest);
    }
    if let Some(digest) = external_links {
        key.insert("externalLinks", digest);
    }
    key
}

/// The identity [`render_key`] records for a dependency map: SHA-256 of its
/// bytes.
///
/// `Ok(None)` when the file is not there, which is a real state and not an
/// error — a first `build` computes the ledger's hashes *before* the extraction
/// that writes the map, and `litedoc4 render --no-link-index` never has one. Any
/// other I/O failure is an error: a map that exists and cannot be read is not a
/// map that is absent, and treating the two alike is how a key goes missing
/// without a word.
pub fn link_index_digest(path: Option<&Path>) -> Result<Option<String>, Error> {
    let Some(path) = path else {
        return Ok(None);
    };
    match fs::read(path) {
        Ok(bytes) => Ok(Some(sha256_hex(&bytes))),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(Error::Io {
            path: path.to_owned(),
            source,
        }),
    }
}

/// `String(value)` for the two values read out of `index.json`.
///
/// A missing key is the string `"undefined"`, which is what the prototype writes
/// into the ledger and what makes an IR without a `schemaVersion` compare equal
/// to another IR without one. Numbers and booleans print as JSON prints them,
/// which for the integers involved is what JavaScript prints.
fn js_string(value: Option<&serde_json::Value>) -> String {
    match value {
        None => "undefined".to_owned(),
        Some(serde_json::Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
    }
}

/// The olean files a module actually has, in [`OLEAN_SUFFIXES`] order.
///
/// A suffix that is absent is not an error: this build simply does not use
/// Lean's module system for that module.
#[must_use]
pub fn module_paths(lib_dir: &str, module: &str) -> Vec<String> {
    // The olean is at the source's path, so the components are unescaped
    // (M5-b): `Alpha.«Odd-Name»` is built from `Alpha/Odd-Name.lean` into
    // `Alpha/Odd-Name.olean`.
    let base = format!("{lib_dir}/{}", litedoc4_ir::module_path(module));
    OLEAN_SUFFIXES
        .iter()
        .map(|suffix| format!("{base}{suffix}"))
        .filter(|path| fs::metadata(path).is_ok_and(|meta| meta.is_file()))
        .collect()
}

/// One module's entry, or `None` when the module has no olean at all.
///
/// **`None` is a real answer, not an error**: a module can be deleted between
/// `build` and `check`, and the deletion is exactly what the caller needs to
/// hear about. Throwing here is what made the prototype's `check` die with an
/// exception instead of reporting a removed module (stage 5b, S4).
///
/// A module that has an olean but whose `<file>.hash` is missing under
/// `--algorithm lake` is a different case and *is* an error: the file the
/// algorithm names is not there, and reporting the module as removed would
/// delete its pages.
// `bytes.len() as i64`: the field is signed because `-1` is how the `lake`
// algorithm says "this file was not read", so a size and that sentinel share one
// number. An olean larger than 8 EiB is not the case to degrade gracefully on.
#[expect(
    clippy::cast_possible_wrap,
    reason = "the field is i64 to carry the -1 sentinel; no olean approaches i64::MAX"
)]
pub fn hash_module(
    target: &str,
    lib_dir: &str,
    module: &str,
    algorithm: &Algorithm,
) -> Result<Option<ModuleEntry>, Error> {
    let mut files: Vec<FileEntry> = Vec::new();
    for path in module_paths(lib_dir, module) {
        if algorithm.hashes_bytes() {
            let bytes = fs::read(&path).map_err(|source| Error::Io {
                path: path.clone().into(),
                source,
            })?;
            files.push(FileEntry {
                path: relative_path(target, &path),
                bytes: bytes.len() as i64,
                hash: sha256_hex(&bytes),
            });
        } else {
            // Lake's own content hash of exactly this file, already on disk.
            let hash_path = format!("{path}.hash");
            files.push(FileEntry {
                path: relative_path(target, &path),
                bytes: -1,
                hash: read_to_string(Path::new(&hash_path))?.trim().to_owned(),
            });
        }
    }
    if files.is_empty() {
        return Ok(None);
    }
    let combined: Vec<String> = files
        .iter()
        .map(|file| format!("{} {}", file.path, file.hash))
        .collect();
    Ok(Some(ModuleEntry {
        module: module.to_owned(),
        files,
        hash: sha256_text(&combined.join("\n")),
    }))
}

/// The path as the ledger records it: relative to the target repository.
///
/// The prototype slices by length (`p.slice(target.length + 1)`). Every path
/// this is called with begins with the target and a separator, so stripping the
/// prefix is the same string; when it does not — a `libDir` hand-edited to point
/// outside the target — the prototype cuts the path at an arbitrary offset and
/// this keeps it whole. Nothing `build` writes can reach that.
fn relative_path(target: &str, path: &str) -> String {
    path.strip_prefix(target)
        .and_then(|rest| rest.strip_prefix('/'))
        .unwrap_or(path)
        .to_owned()
}

/// Bounded-concurrency map, so the read path can be measured at 1 and at N.
///
/// The result is in input order whatever the scheduling, which is what lets the
/// ledger's bytes be independent of `--concurrency` — `tests/ledger.rs` asserts
/// that rather than assuming it.
pub(crate) fn map_pool<T: Send>(
    items: &[String],
    concurrency: usize,
    f: impl Fn(&str) -> T + Sync,
) -> Vec<T> {
    if concurrency <= 1 {
        return items.iter().map(|item| f(item)).collect();
    }
    let next = AtomicUsize::new(0);
    let workers = concurrency.min(items.len());
    let chunks: Vec<Vec<(usize, T)>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                let next = &next;
                let f = &f;
                scope.spawn(move || {
                    let mut out = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(item) = items.get(i) else { return out };
                        out.push((i, f(item)));
                    }
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("a hashing worker panicked"))
            .collect()
    });
    let mut out: Vec<Option<T>> = (0..items.len()).map(|_| None).collect();
    for chunk in chunks {
        for (i, value) in chunk {
            out[i] = Some(value);
        }
    }
    out.into_iter()
        .map(|value| value.expect("every index was assigned exactly once"))
        .collect()
}

/// `Deno.readTextFile`, with the path in the error.
pub(crate) fn read_to_string(path: &Path) -> Result<String, Error> {
    fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.to_owned(),
        source,
    })
}

/// SHA-256 of some bytes, lower-case hex — `crypto.subtle.digest("SHA-256")`.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        // `write!` rather than `push_str(&format!(..))`: the corpus test hashes
        // about 4 GB, and the temporary `String` per byte is 32 allocations per
        // digest that nothing reads.
        write!(hex, "{byte:02x}").expect("writing to a String cannot fail");
    }
    hex
}

/// SHA-256 of a string's UTF-8 bytes — `TextEncoder` then the digest.
#[must_use]
pub fn sha256_text(text: &str) -> String {
    sha256_hex(text.as_bytes())
}
