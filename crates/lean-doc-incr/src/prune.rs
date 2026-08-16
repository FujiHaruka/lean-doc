//! The `prune` stage: the deletion path's page third.
//!
//! Ported from `experiments/stage5/prune-pages.ts` (frozen). Milestone **M3-c**.
//!
//! A module that no longer exists leaves three things behind, and the renderer
//! cleans up none of them because **it only ever writes**: `render --only` writes
//! the pages it was asked for and never looks at what else is in the tree. So a
//! deleted module's page survives every later incremental run and looks exactly
//! like a live one. The failure is silent, which is why stage 5b's S4 found the
//! pipeline exiting rather than pretending to have succeeded.
//!
//! The other two thirds: the IR is [`crate::merge`]'s `--remove`, and the ledger
//! is rebuilt outright (`build` costs the same ~0.05 s as `check`, so there is no
//! reason for an incremental ledger-update path).
//!
//! ```text
//! --remove <file> ──> the pages of modules that are gone
//! --ir <dir>      ──> every .html under the root that is not a live module page
//!                     (orphans)
//! either          ──> directories the deletions left empty
//! ```
//!
//! # This is the one stage that deletes, so the guards are structural
//!
//! 1. **Every path is built by the renderer's own rule and checked against the
//!    root.** [`page_of`] turns dots into separators, so a name cannot carry a
//!    `..` past it — but that is an argument, and [`PageRoot`] is a check:
//!    lexically before the path is used, physically (`canonicalize`) before
//!    anything is unlinked. A path that resolves outside the page root is a
//!    refusal, not a deletion.
//! 2. **Paths are concatenated, never `Path::join`ed.** The prototype builds
//!    `` `${PAGES}/${page}` ``, and `Path::join` with an absolute right-hand side
//!    *discards the left* — so a `--remove` line of `/etc/passwd` would name
//!    `<pages>//etc/passwd.html` in JavaScript and `/etc/passwd.html` here. The
//!    difference is a deletion outside the tree.
//! 3. **The walk never descends a symlink.** `file_type()` does not follow, so a
//!    symlinked subdirectory is neither a directory to recurse into nor a file to
//!    keep — exactly as `Deno.readDir`'s `isDirectory` behaves.
//! 4. **`--dry-run` computes everything and writes nothing**, so the harness can
//!    compare *what would be deleted* without deleting it.
//!
//! # The orphan rule is about `.html`, and the site holds more than pages
//!
//! `--ir` deletes every `.html` under the root whose relative path is not
//! [`page_of`] of a module in the IR. On the target's **site** that is not only
//! the dead pages: four of the seven whole-package artifacts are `.html` files
//! no module owns, so the rule calls them orphans【実測 2026-08-12 —
//! `tools/impact-reference.sh` の `orphans-site` シナリオ】. **M8-d changed
//! which four, and raised the stakes**: it used to be `navbar.html`,
//! `references.html` and `tactics.html`, none of which anything read; it is now
//! `index.html`, `404.html`, `search.html` and `foundational_types.html` — the
//! site's front door, its not-found page, and the target of a link on all 432
//! module pages. The other three artifacts are not `.html`
//! (`declarations/name-map.json`, `modules.json`, `search-index.json`) and are
//! invisible to it, as are the static assets (`style.css`, `app.js`,
//! `favicon.svg` since M8-a), which are **not in the byte-reproduction
//! denominator at all** (432 pages + 7 artifacts = 439 since M8-d; 438 at M6).
//!
//! **The pipeline never passes `--ir`** — `incremental.sh:304-305` passes only
//! `--remove` — so nothing has been deleting them. Reproduced as it is, and
//! written down here because "prune the orphans" is a plausible thing for M3-d to
//! start doing and it would take three artifacts with it.
//!
//! # The static assets survive both halves, and that is now asserted (M8-a)
//!
//! From M8-a every build writes `style.css`, `app.js` and `favicon.svg` into the
//! site root (`docs/plans/ui-redesign.md` 決定 6). They are **not** module
//! pages, no ledger names them, and nothing regenerates them mid-pipeline — so
//! an incremental run that deleted them would leave a site that renders unstyled
//! until the next full generation, which is a silent failure of exactly the kind
//! this stage's heading is otherwise about. Nothing here deletes them today: the
//! orphan rule only looks at names ending in `.html`, and the empty-directory
//! pass never removes the root. The two tests at the foot of this file hold it
//! to that, so a later change to either rule fails here rather than in a
//! browser.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::Serialize;

use crate::detect::{Error, read_module_list, write};

/// What `prune` needs to know.
#[derive(Clone, Copy, Debug)]
pub struct PruneOptions<'a> {
    /// The page tree. Nothing outside it is ever touched.
    pub pages: &'a Path,
    /// Modules whose pages are to be deleted, one name per line.
    pub remove: Option<&'a Path>,
    /// An IR tree. With it, every `.html` with no module in that IR is deleted
    /// too — see the module heading before turning this on over a whole site.
    pub ir: Option<&'a Path>,
    /// Report and delete nothing. The empty-directory pass does not run either,
    /// as in the prototype: there is nothing for it to have emptied.
    pub dry_run: bool,
    pub json: Option<&'a Path>,
}

/// What `prune` did.
///
/// **No `PartialEq`**: `total_seconds` is wall clock, and a summary that
/// compared equal to another one would be asserting on it.
#[derive(Clone, Debug)]
pub struct PruneSummary {
    /// `--pages` as it was given — the prototype reports the string.
    pub pages: String,
    pub dry_run: bool,
    /// Lines in `--remove`.
    pub requested: usize,
    /// Modules whose page was there. Under `--dry-run` these are the ones that
    /// *would* be deleted.
    pub deleted: Vec<String>,
    /// Asked for, no page there — already gone. Not an error: a module can be
    /// deleted before it was ever rendered.
    pub already_absent: Vec<String>,
    /// Pages with no module in the IR, relative to the page root, in walk order.
    pub orphans: Vec<String>,
    /// Directories the deletions left empty, relative to the page root, deepest
    /// first. Empty under `--dry-run`.
    pub emptied: Vec<String>,
    pub total_seconds: f64,
}

/// How many orphan paths the `--json` summary keeps.
pub const ORPHANS_IN_SUMMARY: usize = 20;
/// How many the log line prints.
pub const ORPHANS_IN_LOG: usize = 10;

/// The renderer's path rule: dots become directory separators.
///
/// `"A.B".split(".").join("/") + ".html"`. A name cannot smuggle a `..` through
/// it — the dots that would spell one are exactly the characters this replaces —
/// but [`PageRoot`] checks rather than trusts that.
#[must_use]
pub fn page_of(module: &str) -> String {
    // `lean_doc_ir::module_path`, not `replace('.', "/")` (M5-b): a module whose
    // name needs `«…»` has its page under the unescaped path, and pruning the
    // wrong path leaves a dead page behind while reporting a deletion.
    let mut path = lean_doc_ir::module_path(module);
    path.push_str(".html");
    path
}

/// The tree `prune` is allowed to delete inside.
///
/// Holds the string the caller gave — which is what the summary reports, what
/// relative paths hang off, and what the prototype concatenates. The canonical
/// form is resolved **at the moment of a deletion** rather than up front, for a
/// reason that is behaviour and not taste: the prototype does not look at
/// `--pages` at all until it needs it, so `--dry-run --remove` over a page tree
/// that is not there reports every module as already absent and exits 0.
/// Canonicalising in a constructor would turn that into a failure.
#[derive(Clone, Debug)]
pub struct PageRoot {
    given: String,
}

/// The three guards are **public**, and that is the point: they are the
/// contract, not an implementation detail. No module name can reach either
/// refusal — [`page_of`] destroys the dots a `..` is made of — so the only way to
/// hold them to it is to call them, which `tests/impact.rs` does.
impl PageRoot {
    #[must_use]
    pub fn new(pages: &Path) -> Self {
        Self {
            // `${PAGES}/${page}` — trailing slashes and all, as given.
            given: pages.display().to_string(),
        }
    }

    /// `` `${root}/${relative}` ``, refused if `relative` could leave the tree.
    ///
    /// Lexical, and it runs before the path is used for anything — a missing
    /// file must still be a *refusal* when the name was suspect, not an
    /// "already absent". **Concatenation, not [`Path::join`]**: see guard 2 in
    /// the module heading.
    pub fn resolve(&self, relative: &str) -> Result<PathBuf, Error> {
        let escapes = relative
            .split('/')
            .any(|component| component == ".." || component.contains('\0'));
        if escapes {
            return Err(Error::OutsidePageRoot {
                root: PathBuf::from(&self.given),
                path: PathBuf::from(relative),
            });
        }
        Ok(PathBuf::from(format!("{}/{relative}", self.given)))
    }

    /// The last check before an unlink: the file's directory really is inside
    /// the root once every symlink on the way has been followed.
    pub fn allow_delete(&self, path: &Path) -> Result<(), Error> {
        let parent = path.parent().unwrap_or(Path::new("."));
        self.contains(path, parent, false)
    }

    /// The same check for a directory this stage is about to remove, plus the
    /// root itself: the caller stops one level above it (the prototype's
    /// `dir !== PAGES`), and this says so a second time.
    pub fn allow_remove_dir(&self, path: &Path) -> Result<(), Error> {
        self.contains(path, path, true)
    }

    fn contains(&self, path: &Path, resolve: &Path, strictly: bool) -> Result<(), Error> {
        let root = fs::canonicalize(&self.given).map_err(|source| Error::Io {
            path: PathBuf::from(&self.given),
            source,
        })?;
        let resolved = fs::canonicalize(resolve).map_err(|source| Error::Io {
            path: resolve.to_owned(),
            source,
        })?;
        if !resolved.starts_with(&root) || (strictly && resolved == root) {
            return Err(Error::OutsidePageRoot {
                root,
                path: path.to_owned(),
            });
        }
        Ok(())
    }
}

/// Deletes the pages of removed modules, the orphans and the emptied
/// directories.
pub fn prune(options: &PruneOptions<'_>) -> Result<PruneSummary, Error> {
    let started = Instant::now();
    let root = PageRoot::new(options.pages);
    let requested: Vec<String> = match options.remove {
        Some(path) => read_module_list(path)?,
        None => Vec::new(),
    };

    let mut deleted: Vec<String> = Vec::new();
    let mut already_absent: Vec<String> = Vec::new();
    for module in &requested {
        let path = root.resolve(&page_of(module))?;
        // `Deno.statSync` follows symlinks, so a dangling link is "not there"
        // and the link itself survives — the prototype's behaviour, kept.
        if fs::metadata(&path).is_err() {
            already_absent.push(module.clone());
            continue;
        }
        if !options.dry_run {
            root.allow_delete(&path)?;
            fs::remove_file(&path).map_err(|source| Error::Io {
                path: path.clone(),
                source,
            })?;
        }
        deleted.push(module.clone());
    }

    let mut orphans: Vec<String> = Vec::new();
    if let Some(ir) = options.ir {
        // Read as plain JSON rather than through `IrTree`: the prototype needs
        // one column of `index.json` and this stage runs on a tree that the
        // merge may have just rewritten.
        let index = read_index_modules(ir)?;
        let live: std::collections::HashSet<String> =
            index.iter().map(|module| page_of(module)).collect();
        walk_orphans(&root, options.dry_run, &live, "", &mut orphans)?;
    }

    // Directories the deletions left empty. Harmless if left, but then the page
    // tree is not equal to a from-scratch one — and byte equality with a
    // from-scratch build is the only oracle this project trusts.
    let mut emptied: Vec<String> = Vec::new();
    if !options.dry_run {
        prune_empty(&root, "", &mut emptied)?;
    }

    let summary = PruneSummary {
        pages: root.given.clone(),
        dry_run: options.dry_run,
        requested: requested.len(),
        deleted,
        already_absent,
        orphans,
        emptied,
        total_seconds: started.elapsed().as_secs_f64(),
    };
    if let Some(path) = options.json {
        let record = PruneJson {
            pages: &summary.pages,
            dry_run: summary.dry_run,
            requested: summary.requested,
            deleted: summary.deleted.len(),
            deleted_modules: &summary.deleted,
            already_absent: summary.already_absent.len(),
            orphans: summary.orphans.len(),
            orphan_pages: &summary.orphans[..summary.orphans.len().min(ORPHANS_IN_SUMMARY)],
            emptied_directories: summary.emptied.len(),
            total_seconds: summary.total_seconds,
        };
        let body = serde_json::to_string_pretty(&record)
            .expect("counts, strings and one duration serialise")
            + "\n";
        write(path, &body)?;
    }
    Ok(summary)
}

/// `index.modules[].module`, read as plain JSON.
fn read_index_modules(ir: &Path) -> Result<Vec<String>, Error> {
    let path = ir.join("index.json");
    // Counted like every other IR read (`lean_doc_ir::metrics`). Never fires on
    // the pipeline's path — `prune_removed` passes no `--ir`, deliberately (see
    // `lean-doc/src/pipeline.rs`) — so a run whose counter moves here is a run
    // that turned the orphan rule on.
    lean_doc_ir::metrics::record(lean_doc_ir::IrFile::Index);
    let text = fs::read_to_string(&path).map_err(|source| Error::Io {
        path: path.clone(),
        source,
    })?;
    let index: serde_json::Value = serde_json::from_str(&text).map_err(|source| Error::Json {
        path: path.clone(),
        source,
    })?;
    let Some(modules) = index.get("modules").and_then(serde_json::Value::as_array) else {
        return Err(Error::IndexShape {
            path,
            message: "modules is not an array".to_owned(),
        });
    };
    // An entry with no `module` string would make the prototype call
    // `undefined.split(".")`; refused rather than skipped, as `merge` refuses
    // the same shape (plan §7's IndexShape).
    modules
        .iter()
        .map(
            |entry| match entry.get("module").and_then(serde_json::Value::as_str) {
                Some(module) => Ok(module.to_owned()),
                None => Err(Error::IndexShape {
                    path: path.clone(),
                    message: "an index entry has no string `module`".to_owned(),
                }),
            },
        )
        .collect()
}

/// Depth first in directory order, as `Deno.readDir` gives it.
///
/// `relative` is the path of `dir` under the root, `""` at the top. The
/// prototype cuts the same string off the front of the absolute path
/// (`prune-pages.ts:87`); building it up instead means a `--pages` with a
/// trailing slash does not shift the cut by one.
fn walk_orphans(
    root: &PageRoot,
    dry_run: bool,
    live: &std::collections::HashSet<String>,
    relative: &str,
    orphans: &mut Vec<String>,
) -> Result<(), Error> {
    for entry in read_dir(root, relative)? {
        let (name, kind) = entry;
        let child = join_relative(relative, &name);
        if kind.is_dir() {
            walk_orphans(root, dry_run, live, &child, orphans)?;
        } else if name.ends_with(".html") && !live.contains(&child) {
            // Note the prototype does not ask whether the entry is a *file*:
            // anything not a directory whose name ends in `.html` is a
            // candidate, symlinks included. Unlinking one removes the link.
            orphans.push(child.clone());
            if !dry_run {
                let path = root.resolve(&child)?;
                root.allow_delete(&path)?;
                fs::remove_file(&path).map_err(|source| Error::Io {
                    path: path.clone(),
                    source,
                })?;
            }
        }
    }
    Ok(())
}

/// Removes directories with nothing left in them, deepest first.
///
/// Returns whether `relative` was itself removed, which is how the caller knows
/// whether it still counts as content. The root is never removed.
fn prune_empty(root: &PageRoot, relative: &str, emptied: &mut Vec<String>) -> Result<bool, Error> {
    let mut any = false;
    for (name, kind) in read_dir(root, relative)? {
        if kind.is_dir() {
            if !prune_empty(root, &join_relative(relative, &name), emptied)? {
                any = true;
            }
        } else {
            any = true;
        }
    }
    if !any && !relative.is_empty() {
        let path = root.resolve(relative)?;
        root.allow_remove_dir(&path)?;
        fs::remove_dir(&path).map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        emptied.push(relative.to_owned());
        return Ok(true);
    }
    Ok(false)
}

/// One directory's entries, in the order the filesystem lists them.
///
/// **Not sorted.** `Deno.readDir` does not sort either, and the order reaches
/// the `orphanPages` field of the summary; sorting here would be a change to the
/// bytes under comparison rather than to the answer. Both runtimes call
/// `readdir(3)` on the same directory, which is why the two implementations
/// agree — `tools/impact-compare.sh` checks that rather than assuming it.
fn read_dir(root: &PageRoot, relative: &str) -> Result<Vec<(String, fs::FileType)>, Error> {
    let dir = if relative.is_empty() {
        PathBuf::from(&root.given)
    } else {
        root.resolve(relative)?
    };
    let listing = fs::read_dir(&dir).map_err(|source| Error::Io {
        path: dir.clone(),
        source,
    })?;
    let mut out: Vec<(String, fs::FileType)> = Vec::new();
    for found in listing {
        let found = found.map_err(|source| Error::Io {
            path: dir.clone(),
            source,
        })?;
        let kind = found.file_type().map_err(|source| Error::Io {
            path: found.path(),
            source,
        })?;
        out.push((found.file_name().to_string_lossy().into_owned(), kind));
    }
    Ok(out)
}

fn join_relative(relative: &str, name: &str) -> String {
    if relative.is_empty() {
        name.to_owned()
    } else {
        format!("{relative}/{name}")
    }
}

/// The `--json` summary. Key order is the prototype's object literal
/// (`prune-pages.ts:120-131`); `totalSeconds` is a **diagnostic** — wall clock,
/// different every run, and no test may assert on it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PruneJson<'a> {
    pages: &'a str,
    dry_run: bool,
    requested: usize,
    deleted: usize,
    deleted_modules: &'a [String],
    already_absent: usize,
    orphans: usize,
    orphan_pages: &'a [String],
    emptied_directories: usize,
    total_seconds: f64,
}

#[cfg(test)]
mod tests {
    //! **M8-a's gate on this stage**: the static assets are not pages, and
    //! neither half of `prune` may take them.
    //!
    //! The corpus tests (`tests/impact.rs`) compare this stage against the
    //! frozen prototype over a site that had **no** assets in it — the prototype
    //! never wrote any — so they cannot answer this question at all. These two
    //! are curated for the same reason four of the sixteen branches over there
    //! are: the shape does not exist in the recorded corpus.

    use super::*;

    /// The three files `lean-doc build` writes into the site root (M8-a).
    ///
    /// Spelled out rather than imported: this crate does not depend on the
    /// renderer, and the property under test is not about *these* names — the
    /// rule is "a file that is not a module page stays", and any non-page the
    /// site grows later has to survive the same way.
    const ASSETS: [&str; 3] = ["style.css", "app.js", "favicon.svg"];

    /// A site with two module pages, one of which no IR knows about, plus the
    /// assets. Returns the page tree and the IR tree.
    fn site_with_assets(dir: &Path) -> (PathBuf, PathBuf) {
        let pages = dir.join("site");
        fs::create_dir_all(pages.join("Pkg")).expect("the page tree is creatable");
        write_file(&pages.join("Pkg.html"), "<html>Pkg</html>");
        write_file(&pages.join("Pkg/B.html"), "<html>Pkg.B</html>");
        for name in ASSETS {
            write_file(&pages.join(name), "/* the shipped bytes */");
        }
        // A whole-package artifact too: four of the seven are `.html` and the
        // orphan rule really does take them (see the module heading). Keeping
        // one here is what stops this test from passing because the rule
        // stopped deleting anything at all.
        write_file(&pages.join("index.html"), "<html>the front page</html>");

        let ir = dir.join("ir");
        fs::create_dir_all(&ir).expect("the IR tree is creatable");
        write_file(
            &ir.join("index.json"),
            r#"{"modules":[{"module":"Pkg"},{"module":"Pkg.B"}]}"#,
        );
        (pages, ir)
    }

    fn write_file(path: &Path, body: &str) {
        fs::write(path, body).unwrap_or_else(|source| panic!("{}: {source}", path.display()));
    }

    fn assets_are_intact(pages: &Path) {
        for name in ASSETS {
            let path = pages.join(name);
            assert_eq!(
                fs::read_to_string(&path).ok().as_deref(),
                Some("/* the shipped bytes */"),
                "{} was deleted or rewritten by prune",
                path.display(),
            );
        }
    }

    /// The pipeline's own call — a deletion list and no `--ir`. The empty
    /// directory pass runs on this path, which is the one that could plausibly
    /// take the site root's contents with it.
    #[test]
    fn the_deletion_path_leaves_the_assets() {
        let dir = TempDir::new("prune-assets-remove");
        let (pages, _) = site_with_assets(&dir.path);
        let remove = dir.path.join("removed.txt");
        write_file(&remove, "Pkg.B\n");

        let summary = prune(&PruneOptions {
            pages: &pages,
            remove: Some(&remove),
            ir: None,
            dry_run: false,
            json: None,
        })
        .expect("prune runs");

        assert_eq!(summary.deleted, ["Pkg.B"]);
        assert!(!pages.join("Pkg/B.html").exists(), "the page survived");
        assert_eq!(summary.emptied, ["Pkg"], "the emptied directory went");
        assets_are_intact(&pages);
    }

    /// The orphan rule, which is the half that walks the whole tree. It deletes
    /// `.html` files no module owns — `index.html` here, as the heading says —
    /// and must not widen to the assets, which no module owns either.
    #[test]
    fn the_static_assets_are_not_orphans() {
        let dir = TempDir::new("prune-assets-orphans");
        let (pages, ir) = site_with_assets(&dir.path);

        let summary = prune(&PruneOptions {
            pages: &pages,
            remove: None,
            ir: Some(&ir),
            dry_run: false,
            json: None,
        })
        .expect("prune runs");

        assert_eq!(
            summary.orphans,
            ["index.html"],
            "the orphan rule changed which files it takes",
        );
        assets_are_intact(&pages);
        assert!(pages.join("Pkg.html").is_file(), "a live page went");
        assert!(pages.join("Pkg/B.html").is_file(), "a live page went");
    }

    /// A unique directory under the system temporary one, removed with its
    /// contents when the test ends.
    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(what: &str) -> Self {
            use std::sync::atomic::{AtomicU32, Ordering};
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let path = std::env::temp_dir().join(format!(
                "lean-doc-prune-{}-{}-{what}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed),
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
}
