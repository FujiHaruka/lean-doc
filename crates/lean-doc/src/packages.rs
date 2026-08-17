//! Which GitHub blob prefix each **dependency's** module root belongs to.
//!
//! Milestone **M7-b**, and the whole of it is offline. The plan's table
//! (`docs/implementation-plan.md` §M7) names three inputs and this file reads all
//! three without a socket, because "no network in the product" is the reason案
//! (c) was rejected and is not a thing to give back:
//!
//! | value | where it comes from |
//! |---|---|
//! | a dependency's `url` + 40-hex `rev` | the target's `lake-manifest.json` |
//! | which module roots that dependency provides | a scan of `<packagesDir>/<name>/` |
//! | **Lean core's revision** | **`lake env lean --githash`** — core is not in the manifest |
//!
//! The result is a [`lean_doc_render::ExternalLinks`], which is the value type
//! and says what a lookup does with it.
//!
//! # Four things the scan found that a reading of Lake would not have 【実測】
//!
//! 1. **A manifest name is not always the directory name.** doc-gen4 is
//!    `«doc-gen4»` in the manifest — Lake writes a name that needs quoting with
//!    its guillemets — and `doc-gen4` on disk. The quotes are stripped here.
//! 2. **A module root is not always a `Foo.lean` *and* a `Foo/`.** MD4Lean ships
//!    `MD4Lean.lean` with no `MD4Lean/` next to it, so a scan that wanted both
//!    would silently lose a real root. The file is the rule; the directory is not
//!    consulted. The other direction — `Foo/Bar.lean` with no `Foo.lean` — exists
//!    on disk only as test and script trees (`MathlibTest/`, `scripts/`,
//!    `AesopTest/`), none of which is imported, so a directory alone is not a
//!    root here either.
//! 3. **Core's four roots do not share one prefix.** `Init`, `Lean` and `Std` are
//!    under `src/`, but **`Lake` is under `src/lake/`** — the reference tree links
//!    `Lake.Build` at `…/blob/<hash>/src/lake/Lake/Build.lean`. One base per root,
//!    not one base for core.
//! 4. **Two packages can claim one root.** doc-gen4 and MD4Lean both ship a
//!    `Main.lean`. Nothing imports either, so this costs no link — but it is not
//!    nothing, and it is reported separately from a failure ([`Packages`]) rather
//!    than resolved in silence.
//!
//! # Everything degrades; nothing throws
//!
//! A missing manifest, a package with no directory, a `lake` that will not run —
//! each costs the roots it would have contributed and adds a line to
//! [`Packages::problems`]. The caller decides whether a partial map is worth
//! rendering with, because the answer differs: a site with no external links is
//! the site v0.1 already ships, while a site missing *mathlib's* links is a
//! regression worth stopping for. Nothing here can panic on a shape of the
//! world.
//!
//! # A dropped entry still contributes its roots — with **no** base 【実測 2026-08-17】
//!
//! Dropping an entry that cannot be version-pinned used to drop its module roots
//! with it, and that was a dead link rather than a missing one: a root the map
//! does not hold is read by the renderer as *the package being documented*, so
//! every link into that dependency became a relative link to a page this site
//! never writes. Measured on a fixture whose manifest declares
//! `{"type": "path", "name": "dep", "dir": "../micro-dep"}`: three dead internal
//! links, one per shape (the import list, a docstring's name reference, a
//! signature's constant).
//!
//! So the entry is still dropped as a *link target* — there is no `/blob/<rev>`
//! and never will be one from this input — and its directory is still scanned,
//! and its roots go into the map **with an empty base**, which is the value
//! `lean_doc_render::NameIndex::link_to` answers `None` to. The problem line is
//! unchanged: the
//! entry did not resolve, and saying otherwise would be the report drifting from
//! the map.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use lean_doc_render::ExternalLinks;

/// Lean core's repository. Core has no manifest entry — it is the toolchain —
/// so this is the one URL in the product that is written down rather than read.
pub const CORE_URL: &str = "https://github.com/leanprover/lean4";

/// Core's module roots and the directory each lives in inside that checkout.
///
/// **`Lake` is not under `src/` with the other three**, and the reference tree is
/// what says so 【実測】: `Lake.Build` links at `…/src/lake/Lake/Build.lean` while
/// `Std.Time` links at `…/src/Std/Time.lean`. Sharing one base would 404 every
/// Lake link.
pub const CORE_ROOTS: [(&str, &str); 4] = [
    ("Init", "src"),
    ("Lean", "src"),
    ("Std", "src"),
    ("Lake", "src/lake"),
];

/// Lake's default for `packagesDir`, used when the manifest does not say.
const DEFAULT_PACKAGES_DIR: &str = ".lake/packages";

/// What a resolution produced.
pub struct Packages {
    /// The map, core first (see [`ExternalLinks::new`]: first wins).
    pub links: ExternalLinks,
    /// Manifest entries that contributed at least one root **with a URL**.
    pub resolved: usize,
    /// Module roots carried with an empty base: known to belong to a dependency,
    /// with no version-pinned URL to link them at (see the heading).
    ///
    /// Counted rather than folded into [`Packages::resolved`] because it is the
    /// opposite answer — these are the roots whose links the pages *lose*, and a
    /// run that silently swapped one count for the other would report a map that
    /// links more than it does.
    pub unpinned_roots: usize,
    /// Manifest entries in total, whether or not they contributed.
    pub declared: usize,
    /// One line per thing that could not be resolved, in the order it was met.
    /// Empty means every package and core came out.
    pub problems: Vec<String>,
    /// One line per module root more than one package claims.
    ///
    /// Kept apart from [`Packages::problems`] because it is a different answer:
    /// nothing failed to resolve, and the map holds the root — it just holds one
    /// of the two candidates. On the measurement target there is exactly one, and
    /// it is `Main` (see the heading).
    pub collisions: Vec<String>,
}

/// The dependency link map for the package at `root`.
///
/// `lake` is the program `lean --githash` is run through — a name looked up on
/// `PATH` (`lake`) rather than a path, for the reason
/// [`crate::extract::extract`] gives: elan's shim is what picks the toolchain the
/// target pins.
#[must_use]
pub fn external_links(root: &Path, lake: &Path) -> Packages {
    let mut problems: Vec<String> = Vec::new();
    let mut collisions: Vec<String> = Vec::new();
    // Core first: its four roots are the toolchain's and are not a package's to
    // redefine, and `ExternalLinks::new` keeps the first of a repeated root.
    let mut entries: Vec<(String, String)> = match core_githash(root, lake) {
        Ok(hash) => CORE_ROOTS
            .iter()
            .map(|(name, dir)| ((*name).to_owned(), format!("{CORE_URL}/blob/{hash}/{dir}")))
            .collect(),
        Err(problem) => {
            problems.push(problem);
            Vec::new()
        }
    };

    let manifest_path = root.join("lake-manifest.json");
    let mut declared = 0;
    let mut resolved = 0;
    let mut unpinned_roots = 0;
    match read_manifest(&manifest_path) {
        Err(problem) => problems.push(problem),
        Ok(manifest) => {
            let packages_dir = root.join(&manifest.packages_dir);
            declared = manifest.listed;
            problems.extend(manifest.problems.iter().cloned());
            for package in &manifest.packages {
                // A `path` dependency is not under `packagesDir` at all — Lake
                // leaves it where the manifest's `dir` says, relative to the
                // package being documented — so the two are resolved apart.
                let dir = match &package.dir {
                    Some(relative) => root.join(relative),
                    None => packages_dir.join(&package.name),
                };
                // An unpinnable package is already reported, by `read_manifest`,
                // with the reason it cannot be linked. A second line saying its
                // directory could not be read as well would be the same failure
                // counted twice, and this scan is best-effort by construction:
                // what it buys is *not* linking, so failing it costs the roots
                // their `None` and nothing else.
                let unpinnable = package.blob_base.is_empty();
                let scanned = match module_roots(&dir) {
                    Ok(roots) if !roots.is_empty() => roots,
                    _ if unpinnable => continue,
                    Err(problem) => {
                        problems.push(problem);
                        continue;
                    }
                    Ok(_) => {
                        problems.push(format!(
                            "{}: no top-level .lean file, so no module root could be resolved for \
                             package `{}`",
                            dir.display(),
                            package.name,
                        ));
                        continue;
                    }
                };
                if !unpinnable {
                    resolved += 1;
                }
                for name in scanned {
                    // A root two packages both claim is reported rather than
                    // resolved silently: on the measurement target it is `Main`,
                    // which doc-gen4 and MD4Lean both ship and nothing imports,
                    // but a real collision would be links pointing into the
                    // wrong repository.
                    if let Some((_, base)) = entries.iter().find(|(seen, _)| *seen == name) {
                        collisions.push(format!(
                            "module root `{name}` is claimed by package `{}` and by {} — keeping \
                             the first",
                            package.name,
                            if base.is_empty() {
                                "a package with no version-pinned URL"
                            } else {
                                base
                            },
                        ));
                        continue;
                    }
                    if unpinnable {
                        unpinned_roots += 1;
                    }
                    entries.push((name, package.blob_base.clone()));
                }
            }
        }
    }

    Packages {
        links: ExternalLinks::new(entries),
        resolved,
        unpinned_roots,
        declared,
        problems,
        collisions,
    }
}

// --------------------------------------------------------------- the manifest

/// One `packages` entry, reduced to what a link needs.
#[derive(Debug)]
struct Package {
    /// Unquoted, so it is also the directory name under `packagesDir`.
    name: String,
    /// `<url>/blob/<rev>`, or **empty** when the entry carries no version-pinned
    /// URL — see the module heading. The empty string reaches
    /// [`ExternalLinks`] as-is and is what makes the root unlinkable rather
    /// than absent.
    blob_base: String,
    /// Where the package's sources are, **relative to the target root**, for the
    /// entries that say so themselves. `None` is the usual case:
    /// `<packagesDir>/<name>`, which is where Lake puts a fetched dependency.
    dir: Option<String>,
}

#[derive(Debug)]
struct Manifest {
    packages_dir: String,
    /// How many entries the `packages` array had, dropped ones included.
    listed: usize,
    /// Every entry that has a directory to scan, **in manifest order**, whether
    /// or not it has a URL. The ones without carry an empty
    /// [`Package::blob_base`].
    packages: Vec<Package>,
    /// One line per `packages` entry that was dropped **as a link target**. The
    /// manifest still resolved; that one entry has no `/blob/<rev>`.
    problems: Vec<String>,
}

/// `<root>/lake-manifest.json`, reduced to the git packages it declares.
///
/// **Parsed with `serde_json`, which this crate already depends on** — the
/// manifest is JSON and writing a second parser for it would be a new thing to
/// get wrong.
///
/// `Err` is only for the file: unreadable, not JSON, no `packages` array. **A
/// bad *entry* costs that entry and nothing else** — one dependency pinned to a
/// branch must not take the other fourteen's links with it. Dropped rather than
/// guessed at, though: `/blob/<a-branch-name>` is a URL that rots on the next
/// push to that branch, which is the whole failure M7 exists to fix, and
/// [`crate::build`]'s `--source-url` derivation refuses the same shape for the
/// same reason.
///
/// **"Dropped" means dropped as a *URL*, not as a package** (2026-08-17, see the
/// module heading). Such an entry is still returned, with an empty
/// [`Package::blob_base`], so that its module roots reach the map and the pages
/// stop linking into it. What that costs is knowing where the sources are, and
/// the one place the answer is not obvious is a `path` dependency, which lives
/// wherever its own `dir` says rather than under `packagesDir` — an entry with
/// neither is the only shape that is really dropped, because there is nothing
/// left to scan.
fn read_manifest(path: &Path) -> Result<Manifest, String> {
    let text = fs::read_to_string(path).map_err(|source| {
        format!(
            "{}: {source}. Without it no dependency's revision is known and the pages carry no \
             external links",
            path.display(),
        )
    })?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|source| format!("{}: {source}", path.display()))?;
    let packages_dir = value
        .get("packagesDir")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(DEFAULT_PACKAGES_DIR)
        .to_owned();
    let listed = value
        .get("packages")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("{}: no `packages` array", path.display()))?;

    let mut packages = Vec::with_capacity(listed.len());
    let mut problems = Vec::new();
    for (index, entry) in listed.iter().enumerate() {
        let field = |name: &str| entry.get(name).and_then(serde_json::Value::as_str);
        // The index, not just the name: an entry with no `name` has nothing else
        // to be called in a message.
        let Some(name) = field("name") else {
            problems.push(format!(
                "{}: packages[{index}] has no `name`",
                path.display()
            ));
            continue;
        };
        let name = unquote(name);
        // What every branch below hands back when it has no URL: the package,
        // with the directory it can still be scanned for roots.
        let unpinned = |problem: String, problems: &mut Vec<String>| {
            problems.push(problem);
            Package {
                name: name.clone(),
                blob_base: String::new(),
                dir: None,
            }
        };
        let kind = field("type").unwrap_or_default();
        if kind != "git" {
            let mut package = unpinned(
                format!(
                    "{}: package `{name}` is type `{kind}`, not `git` — only a git package has a \
                     /blob/<rev> to link into",
                    path.display(),
                ),
                &mut problems,
            );
            if kind == "path" {
                // Lake did not fetch this one, so `<packagesDir>/<name>` is not
                // where it is — `dir` is, relative to the package being
                // documented. Without it there is nothing to scan, and the entry
                // really is dropped.
                let Some(dir) = field("dir") else {
                    continue;
                };
                package.dir = Some(dir.to_owned());
            }
            packages.push(package);
            continue;
        }
        let (Some(url), Some(rev)) = (field("url"), field("rev")) else {
            packages.push(unpinned(
                format!(
                    "{}: package `{name}` has no `url` or no `rev`",
                    path.display(),
                ),
                &mut problems,
            ));
            continue;
        };
        if !is_revision(rev) {
            packages.push(unpinned(
                format!(
                    "{}: package `{name}` is pinned at `{rev}`, which is not 40 hex digits — a \
                     tag or a branch is not a version-pinned link",
                    path.display(),
                ),
                &mut problems,
            ));
            continue;
        }
        // `.git` and a trailing slash are spellings git accepts in a remote and
        // GitHub does not accept in a blob path. The target's manifest has
        // neither; another package's might.
        let url = url.trim_end_matches('/');
        let url = url.strip_suffix(".git").unwrap_or(url);
        packages.push(Package {
            name,
            blob_base: format!("{url}/blob/{rev}"),
            dir: None,
        });
    }
    Ok(Manifest {
        packages_dir,
        listed: listed.len(),
        packages,
        problems,
    })
}

/// A Lake name with its guillemets removed: `«doc-gen4»` is `doc-gen4` on disk.
fn unquote(name: &str) -> String {
    name.strip_prefix('«')
        .and_then(|rest| rest.strip_suffix('»'))
        .unwrap_or(name)
        .to_owned()
}

/// 40 lower-case hex digits, which is what plan 決定 1 requires of every
/// revision that reaches a URL.
fn is_revision(rev: &str) -> bool {
    rev.len() == 40 && rev.bytes().all(|b| b.is_ascii_hexdigit())
}

// ------------------------------------------------------------------ the scan

/// The module roots a package directory provides: the stem of every top-level
/// `*.lean` file, sorted.
///
/// Sorted rather than in `read_dir` order so that two machines resolve the same
/// map — the order is not a page's to see, but it is the map's, and the map is
/// logged.
///
/// `lakefile.lean` is dropped by name: it is Lake's configuration, never a
/// module anything imports, and six of the target's fifteen packages have one —
/// so keeping it would report five root collisions that mean nothing.
fn module_roots(dir: &Path) -> Result<Vec<String>, String> {
    let entries = fs::read_dir(dir).map_err(|source| {
        format!(
            "{}: {source}. The package is in the manifest but not on disk, so which module roots \
             it provides cannot be known",
            dir.display(),
        )
    })?;
    let mut roots = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "lean") {
            continue;
        }
        // `is_file` rather than `!is_dir`: a broken symlink is neither, and a
        // root that cannot be read is not a root.
        if !path.is_file() {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        if stem == "lakefile" {
            continue;
        }
        roots.push(stem.to_owned());
    }
    roots.sort();
    Ok(roots)
}

// ------------------------------------------------------------------ the core

/// The `lean` that answers for a given `lake`: its sibling.
///
/// `lake` reaches this project as a path (`--lake`, `$LAKE`) or as the bare word
/// `lake` to be found on `PATH`; [`Path::with_file_name`] turns both into the
/// matching spelling of `lean` (`/x/y/lake` -> `/x/y/lean`, `lake` -> `lean`).
///
/// **Sibling and not `PATH`**: with elan both are shims in one directory, but a
/// caller who names a toolchain-local `lake` means that toolchain's `lean`, and
/// the first `lean` on `PATH` could be another one entirely.
fn lean_beside(lake: &Path) -> PathBuf {
    lake.with_file_name("lean")
}

/// `lean --githash` inside the target.
///
/// The toolchain belongs to the package being documented, so the question "which
/// lean4 commit is this" can only be asked from inside it — the answer comes from
/// whatever `lean-toolchain` in `root` selects.
///
/// # Why not `lake env lean --githash`
///
/// That is what this was until 段 E, and it cost **0.763 s** of a 5.33 s
/// one-module incremental — the single largest item after the Lean environment
/// load【実測 2026-08-17 → `benchmarks/results/g3-attribution-2026-08-17.txt`】.
/// Nearly all of it is Lake's own start-up (`lake env` alone measured at 0.9618 s,
/// 段階 5), and `--githash` needs none of what Lake sets: not `LEAN_PATH`, not the
/// package's build tree, not its dependencies. **What it does need is the
/// toolchain, and that is elan's answer, not Lake's** — elan's `lean` shim
/// resolves the same `lean-toolchain` from the working directory that `lake env
/// lean` ultimately hands to the same shim. So the two spellings ask the same
/// question of the same program, and one of them starts Lake first.
///
/// **The two were measured to agree** before this was changed — on the
/// measurement target and on the synthetic second target, byte for byte, and the
/// sites built either way are byte-identical (同ログ). That equality is the whole
/// argument; if a setup ever breaks it, this returns a *different* revision
/// rather than an error, and every external link into Lean core points at the
/// wrong commit. The guard against that is [`is_revision`] plus the fact that the
/// value lands in `renderKey.externalLinks`, so a change re-renders every page
/// loudly rather than editing a few links quietly.
///
/// **Only stdout is read.** Warnings go to stderr — the target prints one about a
/// dependency with local changes 【実測】 — and folding those into the answer
/// would produce a revision that is not one.
fn core_githash(root: &Path, lake: &Path) -> Result<String, String> {
    let lean = lean_beside(lake);
    let output = Command::new(&lean)
        .current_dir(root)
        .arg("--githash")
        .output()
        .map_err(|source| {
            format!(
                "{} --githash in {}: {source}. Lean core's revision is unknown, so \
                 Init/Lean/Std/Lake carry no external links",
                lean.display(),
                root.display(),
            )
        })?;
    if !output.status.success() {
        return Err(format!(
            "{} --githash in {} failed: {}",
            lean.display(),
            root.display(),
            String::from_utf8_lossy(&output.stderr).trim(),
        ));
    }
    let hash = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if !is_revision(&hash) {
        return Err(format!(
            "{} --githash printed `{hash}`, which is not 40 hex digits — a toolchain \
             built without a git revision cannot be linked to",
            lean.display(),
        ));
    }
    Ok(hash)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashSet};
    use std::path::PathBuf;

    use super::*;

    /// The measurement target, as `lean-doc-incr`'s corpus tests spell it.
    const DEFAULT_TARGET: &str = "/Users/haruka/dev/lean-projects";

    /// Where `benchmarks/tools/check-lidx-urls.sh` — the driver that makes both
    /// — leaves them (its `WORK_DIR`). Tens of megabytes each, so they live
    /// outside the repository like every other corpus input.
    const DEFAULT_LINK_INDEX: &str = "/private/tmp/lean-doc-m7a/link-index.lidx";
    const DEFAULT_DECL_URLS: &str = "/private/tmp/lean-doc-m7a/decl-source-urls.tsv";

    // ------------------------------------------------------ without the target

    #[test]
    fn a_lake_name_loses_its_guillemets() {
        assert_eq!(unquote("«doc-gen4»"), "doc-gen4");
        assert_eq!(unquote("mathlib"), "mathlib");
        assert_eq!(unquote("«lean4-cli»"), "lean4-cli");
        // Only a matched pair, and only the outer one: Lake quotes the whole
        // name or none of it.
        assert_eq!(unquote("«half"), "«half");
        assert_eq!(unquote("half»"), "half»");
    }

    #[test]
    fn only_forty_hex_digits_are_a_revision() {
        assert!(is_revision("fabf563a7c95a166b8d7b6efca11c8b4dc9d911f"));
        assert!(!is_revision("v4.31.0"));
        assert!(!is_revision("main"));
        assert!(!is_revision("fabf563"));
    }

    #[test]
    fn a_git_package_becomes_a_blob_base() {
        let dir = TempDir::new("manifest");
        let path = dir.path.join("lake-manifest.json");
        fs::write(
            &path,
            r#"{"version": "1.2.0", "packagesDir": ".lake/packages", "packages":
               [{"url": "https://github.com/leanprover-community/mathlib4.git", "type": "git",
                 "rev": "fabf563a7c95a166b8d7b6efca11c8b4dc9d911f", "name": "mathlib"},
                {"url": "https://github.com/leanprover/doc-gen4/", "type": "git",
                 "rev": "0bc516c1b9db83658d6475c40d9b1ed71219b921", "name": "«doc-gen4»"}]}"#,
        )
        .expect("the temporary tree is writable");
        let manifest = read_manifest(&path).expect("it parses");
        assert_eq!(manifest.packages_dir, ".lake/packages");
        assert_eq!(
            manifest
                .packages
                .iter()
                .map(|p| (p.name.as_str(), p.blob_base.as_str()))
                .collect::<Vec<_>>(),
            [
                (
                    "mathlib",
                    "https://github.com/leanprover-community/mathlib4/blob/\
                     fabf563a7c95a166b8d7b6efca11c8b4dc9d911f"
                ),
                (
                    "doc-gen4",
                    "https://github.com/leanprover/doc-gen4/blob/\
                     0bc516c1b9db83658d6475c40d9b1ed71219b921"
                ),
            ],
            "the `.git` suffix and the trailing slash are not part of a blob path"
        );
    }

    /// A bad entry costs that entry **its URL**, and the fourteen beside it
    /// still resolve.
    ///
    /// The entry is still returned, with an empty `blob_base`, because its
    /// module roots are what stop the pages linking into it (2026-08-17, see the
    /// module heading). The one shape that is really dropped is the `path` entry
    /// with no `dir`: there is no directory to scan and therefore no root to
    /// learn.
    #[test]
    fn a_revision_that_is_not_forty_hex_costs_only_its_own_packages_url() {
        let dir = TempDir::new("tag");
        let path = dir.path.join("lake-manifest.json");
        fs::write(
            &path,
            r#"{"packages":
               [{"url": "https://github.com/o/tagged", "type": "git",
                 "rev": "v4.31.0", "name": "tagged"},
                {"url": "/somewhere/local", "type": "path", "name": "local"},
                {"type": "git", "rev": "0000000000000000000000000000000000000000"},
                {"url": "https://github.com/o/pinned", "type": "git",
                 "rev": "fabf563a7c95a166b8d7b6efca11c8b4dc9d911f", "name": "pinned"}]}"#,
        )
        .expect("the temporary tree is writable");
        let manifest = read_manifest(&path).expect("the file itself is fine");
        assert_eq!(manifest.listed, 4);
        assert_eq!(
            manifest
                .packages
                .iter()
                .map(|p| (p.name.as_str(), p.blob_base.is_empty(), p.dir.clone()))
                .collect::<Vec<_>>(),
            [("tagged", true, None), ("pinned", false, None),],
            "the tagged package keeps its directory and loses its URL; the `path` entry has \
             neither a URL nor a `dir`, so it is the only one dropped"
        );
        assert_eq!(manifest.problems.len(), 3);
        assert!(manifest.problems[0].contains("not 40 hex digits"));
        assert!(manifest.problems[1].contains("not `git`"));
        assert!(manifest.problems[2].contains("packages[2] has no `name`"));
    }

    #[test]
    fn a_manifest_that_is_not_a_manifest_stops_before_any_package() {
        let dir = TempDir::new("broken");
        let path = dir.path.join("lake-manifest.json");
        fs::write(&path, "{\"version\": \"1.2.0\"}").expect("the temporary tree is writable");
        let problem = read_manifest(&path).expect_err("no `packages` array");
        assert!(problem.contains("no `packages` array"), "{problem}");
    }

    /// The scan's rule, on the two shapes the target actually has: a root with a
    /// directory next to it and a root without one.
    #[test]
    fn a_root_is_a_top_level_lean_file_with_or_without_a_directory() {
        let dir = TempDir::new("scan");
        let pkg = dir.path.join("pkg");
        fs::create_dir_all(pkg.join("Mathlib/Order")).expect("writable");
        fs::create_dir_all(pkg.join("MathlibTest")).expect("writable");
        for name in [
            "Mathlib.lean",
            "Archive.lean",
            "MD4Lean.lean",
            "lakefile.lean",
        ] {
            fs::write(pkg.join(name), "").expect("writable");
        }
        fs::write(pkg.join("Mathlib/Order/Basic.lean"), "").expect("writable");
        fs::write(pkg.join("MathlibTest/Case.lean"), "").expect("writable");
        fs::create_dir_all(pkg.join("Archive")).expect("writable");
        assert_eq!(
            module_roots(&pkg).expect("the directory reads"),
            ["Archive", "MD4Lean", "Mathlib"],
            "the file is the rule: a directory with no `.lean` beside it (MathlibTest) is not a \
             root, and a file with no directory (MD4Lean) is"
        );
    }

    #[test]
    fn a_package_that_is_not_on_disk_is_a_problem_and_not_a_panic() {
        let dir = TempDir::new("absent");
        let problem = module_roots(&dir.path.join("nowhere")).expect_err("it is not there");
        assert!(problem.contains("not on disk"), "{problem}");
    }

    /// **The 2026-08-17 fix, end to end**: a dependency that cannot be
    /// version-pinned is in the map with an empty base, so links into it are
    /// `None` instead of relative links to pages this site never writes.
    ///
    /// Both shapes of unpinnable entry are here, because they are found in
    /// different places on disk: a `path` dependency by its own `dir`, relative
    /// to the package being documented, and a git dependency pinned at a tag
    /// under `packagesDir` like any other. The pinned package beside them is
    /// what says the map still links what it can.
    #[test]
    fn a_dependency_that_cannot_be_pinned_contributes_roots_with_no_base() {
        let tmp = TempDir::new("unpinned");
        let root = tmp.path.join("pkg");
        let dep = tmp.path.join("dep");
        for (dir, file) in [
            (dep.clone(), "DepAux.lean"),
            (root.join(".lake/packages/tagged"), "Tagged.lean"),
            (root.join(".lake/packages/pinned"), "Pinned.lean"),
        ] {
            fs::create_dir_all(&dir).expect("the temporary tree is writable");
            fs::write(dir.join(file), "").expect("the temporary tree is writable");
        }
        fs::write(
            root.join("lake-manifest.json"),
            r#"{"packagesDir": ".lake/packages", "packages":
               [{"type": "path", "name": "dep", "dir": "../dep"},
                {"url": "https://github.com/o/tagged", "type": "git",
                 "rev": "v4.31.0", "name": "tagged"},
                {"url": "https://github.com/o/pinned", "type": "git",
                 "rev": "fabf563a7c95a166b8d7b6efca11c8b4dc9d911f", "name": "pinned"}]}"#,
        )
        .expect("the temporary tree is writable");

        let resolved = external_links(&root, Path::new("lake-that-does-not-exist"));
        // Core did not resolve either, so the map is the three packages' roots
        // alone, in manifest order.
        assert_eq!(
            resolved.links.iter().collect::<Vec<_>>(),
            [
                ("DepAux", ""),
                ("Tagged", ""),
                (
                    "Pinned",
                    "https://github.com/o/pinned/blob/\
                            fabf563a7c95a166b8d7b6efca11c8b4dc9d911f"
                ),
            ],
        );
        assert_eq!(resolved.declared, 3);
        assert_eq!(resolved.resolved, 1, "only one of the three has a URL");
        assert_eq!(resolved.unpinned_roots, 2);
        // What a page can build out of that, which is the whole point: no URL
        // for the two, the blob URL for the third. The roots are *in* the map
        // with an empty base, which is what makes the renderer draw no link at
        // all rather than a relative one
        // (`lean_doc_render::NameIndex::link_to`, branch 2).
        assert_eq!(resolved.links.url_for("DepAux.Basic", None), None);
        assert_eq!(resolved.links.base_for("DepAux"), Some(""));
        assert_eq!(resolved.links.url_for("Tagged", None), None);
        assert_eq!(resolved.links.base_for("Tagged"), Some(""));
        assert_eq!(
            resolved.links.url_for("Pinned.M", None).as_deref(),
            Some(
                "https://github.com/o/pinned/blob/fabf563a7c95a166b8d7b6efca11c8b4dc9d911f/\
                 Pinned/M.lean"
            ),
        );
        // …and a module of the package being documented is still not in the map
        // at all, which is the state that leaves the page link on the table.
        assert_eq!(resolved.links.base_for("Pkg"), None);
        // One line for `lake`, one per unpinnable entry — the same lines as
        // before the fix, because the report is about what did not resolve.
        assert_eq!(resolved.problems.len(), 3, "{:?}", resolved.problems);
        let report = resolved.problems.join("\n");
        assert!(report.contains("is type `path`, not `git`"), "{report}");
        assert!(report.contains("not 40 hex digits"), "{report}");
        assert!(resolved.collisions.is_empty());
    }

    /// The scan is best-effort: an unpinnable entry whose directory is not there
    /// costs its roots and nothing else — no second problem line for the same
    /// entry, and no panic.
    #[test]
    fn an_unpinnable_entry_with_no_directory_to_scan_is_the_one_line_it_already_had() {
        let tmp = TempDir::new("unpinned-absent");
        let root = tmp.path.join("pkg");
        fs::create_dir_all(&root).expect("the temporary tree is writable");
        fs::write(
            root.join("lake-manifest.json"),
            r#"{"packages":
               [{"type": "path", "name": "nodir"},
                {"type": "path", "name": "gone", "dir": "../nowhere"},
                {"url": "https://github.com/o/tagged", "type": "git",
                 "rev": "v4.31.0", "name": "tagged"}]}"#,
        )
        .expect("the temporary tree is writable");

        let resolved = external_links(&root, Path::new("lake-that-does-not-exist"));
        assert!(resolved.links.is_empty());
        assert_eq!(resolved.unpinned_roots, 0);
        assert_eq!(resolved.resolved, 0);
        assert_eq!(resolved.declared, 3);
        assert_eq!(
            resolved.problems.len(),
            4,
            "one for `lake` and one per entry — not two for an entry that was \
             scanned and not found: {:?}",
            resolved.problems,
        );
    }

    /// The whole resolution against a package that has neither a manifest nor a
    /// `lake`: an empty map, and a line for each.
    #[test]
    fn a_root_with_nothing_in_it_degrades_to_an_empty_map() {
        let dir = TempDir::new("empty");
        let resolved = external_links(&dir.path, Path::new("lake-that-does-not-exist"));
        assert!(resolved.links.is_empty());
        assert_eq!(resolved.declared, 0);
        assert_eq!(resolved.resolved, 0);
        assert_eq!(resolved.problems.len(), 2, "{:?}", resolved.problems);
        assert!(resolved.collisions.is_empty());
        // One line per thing that did not resolve — the shape M7-c's caller
        // prints, one `external  note:` per line, so that a partial map says
        // which halves are missing rather than that something went wrong.
        let report = resolved.problems.join("\n");
        assert!(report.contains("lake-manifest.json"), "{report}");
        assert!(report.contains("--githash"), "{report}");
    }

    // --------------------------------------------------------- with the target

    /// doc-gen4's own output tree, which already holds the URLs this resolver is
    /// supposed to produce — or a panic naming what to set.
    ///
    /// Every caller is `#[ignore]`d, so reaching this function at all means the
    /// corpus gate asked for the test by name. Returning "not here, never mind"
    /// there would be a green result for a comparison that never ran.
    fn reference_tree() -> (PathBuf, PathBuf) {
        let target = PathBuf::from(
            std::env::var("LEAN_DOC_TARGET").unwrap_or_else(|_| DEFAULT_TARGET.to_owned()),
        );
        let tree = std::env::var("LEAN_DOC_DOCGEN4_TREE")
            .map_or_else(|_| target.join(".lake/build/doc"), PathBuf::from);
        assert!(
            target.is_dir(),
            "no target package at {}: set LEAN_DOC_TARGET, or run this test through \
             tools/corpus-gate.sh, which is the only thing that should be asking for it",
            target.display(),
        );
        assert!(
            tree.is_dir(),
            "no doc-gen4 reference tree at {}: set LEAN_DOC_DOCGEN4_TREE (or LEAN_DOC_TARGET), \
             or run this test through tools/corpus-gate.sh, which is the only thing that should \
             be asking for it",
            tree.display(),
        );
        (target, tree)
    }

    /// **The gate of M7-b**: every root this resolver knows produces the same URL
    /// doc-gen4 itself put on that root's pages.
    ///
    /// The oracle is exact and offline — the reference tree carries a
    /// version-pinned blob URL on every module page's `gh_nav_link`
    /// (`benchmarks/tools/extract-decl-source-urls.sh` mines the per-declaration
    /// ones the same way). A root the tree has no page for cannot be checked and
    /// is counted rather than passed over in silence: the target's site documents
    /// its own import closure, not every package in its manifest.
    #[test]
    #[ignore = "corpus: needs LEAN_DOC_TARGET + LEAN_DOC_DOCGEN4_TREE (tools/corpus-gate.sh)"]
    fn every_root_matches_doc_gen4s_own_blob_urls() {
        let (target, tree) = reference_tree();
        let resolved = external_links(&target, Path::new("lake"));
        assert_eq!(
            resolved.problems,
            Vec::<String>::new(),
            "the resolution reported problems"
        );
        assert_eq!(
            resolved.resolved, resolved.declared,
            "a manifest package contributed no module root"
        );
        assert_eq!(resolved.declared, 9, "the target's manifest declares 9");
        // 実測 2026-08-16: no root collision at all. At 15 declared packages two
        // of them claimed the root `Main`; at 9 only one claimant is left. A
        // collision reappearing is a fact about the dependency set worth failing
        // on, so this is asserted as an empty set rather than as a count.
        assert_eq!(
            resolved.collisions,
            Vec::<String>::new(),
            "{:?}",
            resolved.collisions,
        );

        let mut checked: BTreeMap<&str, String> = BTreeMap::new();
        let mut unpaged: Vec<&str> = Vec::new();
        let mut failures: Vec<String> = Vec::new();
        for (root, _) in resolved.links.iter() {
            let Some((module, want)) = sample_page(&tree, root) else {
                unpaged.push(root);
                continue;
            };
            let got = resolved
                .links
                .url_for(&module, None)
                .unwrap_or_else(|| panic!("{module} is under a root this map holds"));
            if got == want {
                checked.insert(root, module);
            } else {
                failures.push(format!("  {module}\n    want: {want}\n    got:  {got}"));
            }
        }
        assert!(
            failures.is_empty(),
            "{} of {} roots disagree with the reference tree:\n{}",
            failures.len(),
            checked.len() + failures.len(),
            failures.join("\n"),
        );
        eprintln!(
            "{} root(s) checked against {}: {:?}\n{} root(s) the tree has no page for: {unpaged:?}",
            checked.len(),
            tree.display(),
            checked,
            unpaged.len(),
        );
        assert!(
            checked.len() >= 12,
            "only {} roots were checked; the tree used to have pages for 12",
            checked.len(),
        );
        // The two the plan quotes, by name, so that a shrinking sample cannot
        // hide either of the two prefix shapes (a package, and core's `/src`).
        assert!(checked.contains_key("Mathlib") && checked.contains_key("Init"));
        // …and the third shape, which is core's and is *not* `/src` (see
        // [`CORE_ROOTS`]).
        assert!(checked.contains_key("Lake"));
    }

    /// The declaration-level anchor, against the one the plan quotes off the
    /// reference tree.
    #[test]
    #[ignore = "corpus: needs LEAN_DOC_TARGET + LEAN_DOC_DOCGEN4_TREE (tools/corpus-gate.sh)"]
    fn a_line_range_is_the_anchor_doc_gen4_writes() {
        let (target, tree) = reference_tree();
        let page = tree.join("Mathlib/Order/Basic.html");
        let html = fs::read_to_string(&page).unwrap_or_else(|source| {
            panic!(
                "{}: {source} — the reference tree named by LEAN_DOC_DOCGEN4_TREE has no page \
                 for Mathlib.Order.Basic, so it is not the tree this oracle reads; run this \
                 test through tools/corpus-gate.sh, which is the only thing that should be \
                 asking for it",
                page.display()
            )
        });
        let want = html
            .split_once("<div class=\"decl\" id=\"LE.ext\">")
            .and_then(|(_, rest)| first_blob_url(rest))
            .expect("LE.ext carries a source link");
        let resolved = external_links(&target, Path::new("lake"));
        let (from, to) = line_range(&want).expect("the plan quotes it with an anchor");
        assert_eq!(
            resolved
                .links
                .url_for("Mathlib.Order.Basic", Some((from, to)))
                .expect("Mathlib resolves"),
            want
        );
    }

    // ------------------------------------------------- the whole map, per name

    /// **The gate of M7-a**: for every declaration the `.lidx` carries, the URL
    /// built out of it — `ExternalLinks::url_for(module_of(name),
    /// range_of(name))` — is the one doc-gen4 itself wrote on that
    /// declaration's page.
    ///
    /// Both inputs live outside the repository and are ~10 MB and ~41 MB, so
    /// the test is `#[ignore]`d and reads them from
    /// [`DEFAULT_LINK_INDEX`]/[`DEFAULT_DECL_URLS`], or from:
    ///
    /// ```text
    /// LEAN_DOC_LINK_INDEX=<lean-doc build … writes <out>/link-index.lidx>
    /// LEAN_DOC_DECL_URLS=<benchmarks/tools/extract-decl-source-urls.sh out.tsv>
    /// ```
    ///
    /// `benchmarks/tools/check-lidx-urls.sh` is the driver that produces both
    /// and files the output under `benchmarks/results/`.
    ///
    /// **The defaults were added on 2026-08-16**, when this test ran for the
    /// first time: the gate had been cutting `lean_doc::packages::tests::NAME`
    /// down to `NAME`, which `--exact` matched nothing, so it never asked for
    /// these inputs and nobody noticed it was the only corpus test with no
    /// default path.
    ///
    /// **Only the mismatch bucket is a failure.** The two populations are not
    /// the same set and never were — the `.lidx` is the environment this
    /// extraction loaded, while the oracle is whatever pages that doc-gen4 build
    /// happened to write — so every bucket is printed with its 母数 and only a
    /// name the two *both* have and disagree about fails.
    #[test]
    #[ignore = "corpus: needs LEAN_DOC_LINK_INDEX + LEAN_DOC_DECL_URLS (tools/corpus-gate.sh)"]
    fn every_lidx_entry_matches_doc_gen4s_declaration_urls() {
        let lidx =
            std::env::var("LEAN_DOC_LINK_INDEX").unwrap_or_else(|_| DEFAULT_LINK_INDEX.to_owned());
        let oracle =
            std::env::var("LEAN_DOC_DECL_URLS").unwrap_or_else(|_| DEFAULT_DECL_URLS.to_owned());
        for (what, path, how) in [
            (
                "LEAN_DOC_LINK_INDEX",
                &lidx,
                "lean-doc build --root <target> --out <dir>  (writes <dir>/link-index.lidx)",
            ),
            (
                "LEAN_DOC_DECL_URLS",
                &oracle,
                "benchmarks/tools/extract-decl-source-urls.sh <out.tsv>",
            ),
        ] {
            assert!(
                Path::new(path).is_file(),
                "no input at {path}: set {what}, or make it with\n    {how}\n\
                 or run this test through tools/corpus-gate.sh, which is the only thing \
                 that should be asking for it",
            );
        }
        let target = PathBuf::from(
            std::env::var("LEAN_DOC_TARGET").unwrap_or_else(|_| DEFAULT_TARGET.to_owned()),
        );
        let index = lean_doc_render::LinkIndex::read(&lidx)
            .unwrap_or_else(|source| panic!("{lidx}: {source}"));
        let oracle_text =
            fs::read_to_string(&oracle).unwrap_or_else(|source| panic!("{oracle}: {source}"));
        let mut wanted: BTreeMap<String, &str> = BTreeMap::new();
        let mut oracle_lines = 0usize;
        let mut oracle_collisions = 0usize;
        for line in oracle_text.lines() {
            let Some((name, url)) = line.split_once('\t') else {
                continue;
            };
            oracle_lines += 1;
            // doc-gen4 writes the name into an HTML attribute, so `<` `>` `&`
            // arrive escaped; the `.lidx` writes `Name.toString`. Undoing the
            // *HTML* escape is the only difference between the two spellings —
            // the guillemets are in both.
            if wanted.insert(unescape_html(name), url).is_some() {
                oracle_collisions += 1;
            }
        }

        let resolved = external_links(&target, Path::new("lake"));
        assert_eq!(resolved.problems, Vec::<String>::new());

        let (mut matched, mut unlinkable) = (0usize, 0usize);
        let mut mismatched: Vec<String> = Vec::new();
        let mut lidx_only: Vec<&str> = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();
        for name in index.names() {
            let module = index.module_of(name).unwrap_or_default();
            let Some(got) = resolved.links.url_for(module, index.range_of(name)) else {
                // The package being documented: M7 changes dependency links
                // only, and a map that does not hold the root is how that is
                // said (`ExternalLinks`'s heading).
                unlinkable += 1;
                continue;
            };
            match wanted.get(name) {
                None => lidx_only.push(name),
                Some(want) => {
                    seen.insert(name);
                    if got == *want {
                        matched += 1;
                    } else if mismatched.len() < 20 {
                        mismatched.push(format!("  {name}\n    want: {want}\n    got:  {got}"));
                    } else {
                        mismatched.push(String::new());
                    }
                }
            }
        }
        let linkable = matched + mismatched.len() + lidx_only.len();
        let mut oracle_only: Vec<&str> = Vec::new();
        let mut oracle_unlinkable = 0usize;
        for name in wanted.keys() {
            if seen.contains(name.as_str()) {
                continue;
            }
            if index.module_of(name).is_some() {
                oracle_unlinkable += 1;
            } else {
                oracle_only.push(name);
            }
        }

        let sample = |names: &[&str]| {
            names
                .iter()
                .take(10)
                .copied()
                .collect::<Vec<_>>()
                .join(", ")
        };
        // Why each of the two one-sided buckets is one-sided, counted rather
        // than asserted: a `.lidx` entry whose *parent* is in the oracle is a
        // structure field or constructor, which doc-gen4 renders inside the
        // parent's `decl` div instead of giving it one of its own — so the
        // oracle has nothing to compare against. An oracle entry with no
        // `.lidx` name at all is a module the extraction never imported.
        let nested = lidx_only
            .iter()
            .filter(|name| {
                name.rsplit_once('.')
                    .is_some_and(|(parent, _)| wanted.contains_key(parent))
            })
            .count();
        let mut absent_roots: BTreeMap<&str, usize> = BTreeMap::new();
        for name in &oracle_only {
            *absent_roots
                .entry(name.split('.').next().unwrap_or(name))
                .or_default() += 1;
        }
        let mut absent_roots: Vec<_> = absent_roots.into_iter().collect();
        absent_roots.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
        absent_roots.truncate(5);
        eprintln!(
            "\n.lidx {lidx}\noracle {oracle}\n\
             .lidx entries                          : {}\n\
             \x20 under a root the map resolves        : {linkable}\n\
             \x20   matched                            : {matched}\n\
             \x20   mismatched                         : {}\n\
             \x20   not in the oracle                  : {}\n\
             \x20 under a root the map does not hold   : {unlinkable}\n\
             oracle entries                         : {} ({oracle_lines} lines, \
             {oracle_collisions} name collisions after unescaping)\n\
             \x20 not in the .lidx at all              : {}\n\
             \x20 in the .lidx, root not in the map    : {oracle_unlinkable}\n\
             .lidx entries with a line range        : {} of {}\n\
             of the {} not in the oracle, {nested} have a parent that is (a structure field or \
             constructor, which doc-gen4 renders inside its parent)\n\
             the {} the .lidx does not have, by first component: {:?}\n\
             .lidx-only sample : {}\n\
             oracle-only sample: {}",
            index.len(),
            mismatched.len(),
            lidx_only.len(),
            wanted.len(),
            oracle_only.len(),
            index.ranged_len(),
            index.len(),
            lidx_only.len(),
            oracle_only.len(),
            absent_roots,
            sample(&lidx_only),
            sample(&oracle_only),
        );
        assert!(
            mismatched.is_empty(),
            "{} of {linkable} resolvable .lidx entries disagree with doc-gen4:\n{}",
            mismatched.len(),
            mismatched.join("\n"),
        );
        assert!(matched > 0, "nothing was compared");
    }

    /// The five entities doc-gen4's HTML escape produces, undone. `&amp;` last,
    /// so that a name that really contains `&lt;` is not turned into `<`.
    fn unescape_html(name: &str) -> String {
        let mut out = name.to_owned();
        for (entity, ch) in [
            ("&lt;", "<"),
            ("&gt;", ">"),
            ("&quot;", "\""),
            ("&#39;", "'"),
        ] {
            if out.contains(entity) {
                out = out.replace(entity, ch);
            }
        }
        if out.contains("&amp;") {
            out = out.replace("&amp;", "&");
        }
        out
    }

    /// One module under `root` that the reference tree has a page and a source
    /// link for, as `(module name, the URL doc-gen4 wrote)`.
    fn sample_page(tree: &Path, root: &str) -> Option<(String, String)> {
        let mut pages = Vec::new();
        collect_pages(&tree.join(root), root, &mut pages);
        pages.sort();
        pages.into_iter().find_map(|module| {
            let path = tree.join(format!("{}.html", module.replace('.', "/")));
            let html = fs::read_to_string(path).ok()?;
            // The first one on the page is the nav's `gh_nav_link`, which is the
            // *module's* source file and therefore has no line anchor — exactly
            // what `url_for(module, None)` builds.
            Some((module, first_blob_url(&html)?))
        })
    }

    fn collect_pages(at: &Path, prefix: &str, out: &mut Vec<String>) {
        let Ok(entries) = fs::read_dir(at) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            if path.is_dir() {
                collect_pages(&path, &format!("{prefix}.{name}"), out);
            } else if let Some(stem) = name.strip_suffix(".html") {
                out.push(format!("{prefix}.{stem}"));
            }
        }
    }

    /// The first `https://github.com/…/blob/…` href in `html`, with any line
    /// anchor kept.
    fn first_blob_url(html: &str) -> Option<String> {
        let at = html.find("https://github.com/")?;
        let rest = &html[at..];
        let end = rest.find('"')?;
        let url = &rest[..end];
        url.contains("/blob/").then(|| url.to_owned())
    }

    fn line_range(url: &str) -> Option<(u32, u32)> {
        let (_, anchor) = url.rsplit_once("#L")?;
        let (from, to) = anchor.split_once("-L")?;
        Some((from.parse().ok()?, to.parse().ok()?))
    }

    /// A directory that removes itself. Hand-rolled for the same reason
    /// `lean-doc-render/tests/pages.rs` hand-rolls one: ten lines against a
    /// dependency the workspace has no other use for.
    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(what: &str) -> Self {
            use std::sync::atomic::{AtomicU32, Ordering};
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let path = std::env::temp_dir().join(format!(
                "lean-doc-packages-{}-{}-{what}",
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
