//! Linking a dependency's declarations at **its own documentation site**,
//! for the names that site was verified to document.
//!
//! Feature **A-1** (`docs/plans/feature-sweep.md` §4). Everything that touches
//! the network is here, and nothing downstream of this file knows there is a
//! network: [`litedoc4_render::DepDocs`] is a value — a base URL and two
//! name -> page maps — and [`litedoc4_render::ExternalLinks`] is where the
//! renderer reads it.
//!
//! # The rule is one rule
//!
//! A dependency's documentation is built from *a* revision (mathlib4_docs from
//! `master`; no versioned copy exists 【実測 2026-08-19,
//! `benchmarks/results/deps-link-rot-2026-08-19.txt` §9】) and the manifest pins
//! *another*, so the two can disagree about whether a name exists. The answer is
//! not to link optimistically and hope:
//!
//! * the site's declaration table holds the name ⇒ **the docs page**;
//! * it does not ⇒ **the version-pinned source**, which is what every
//!   dependency link was before this feature;
//! * the table could not be read at all ⇒ **the version-pinned source for
//!   everything of that root**, and the run says so on its own line.
//!
//! There is no "try the docs site and fall back on a 404": a build cannot see a
//! 404, and avoiding one is the entire reason the source link is pinned.
//! 【実測】 at a two month pin the fallback fires 0 times in 396 names; 【外挿】
//! at twelve months, 10.3 times.
//!
//! # Why `curl` and not an HTTP client
//!
//! This workspace depends on `serde_json`, `sha2` and its own crates. An HTTP
//! stack is a dependency tree with a TLS implementation under it, for one GET
//! per build of an optional feature — so the fetch shells out, exactly as
//! [`crate::build`] shells out to `git` and [`crate::extract`] to `lake`. A
//! missing `curl` is refused **by name**; a `curl` that ran and failed is the
//! "table could not be read" state above, because that is what it is.
//!
//! # What is kept in memory, and what is not
//!
//! The real table is **66,715,005 B of JSON with 420,714 declarations and
//! 11,351 modules** 【実測 2026-08-19】, and a build of the measurement target
//! already peaks near 4 GB. So it is streamed, and the two halves are bounded
//! differently because their smallest available bound differs:
//!
//! * **declarations are kept only if this run can refer to them** — the names
//!   in the IR's own `deps/*.json` slices, 527 of them on the target. The
//!   request set exists, so it is used.
//! * **modules are kept in full for the roots being linked** (11,351 entries at
//!   most, about a megabyte). There is no request set for them: the import list
//!   is not in `index.json`, so building one would mean reading every module
//!   file — a whole extra pass over the IR, which is a number this project
//!   reports and gates on (`work.irReads`).
//!
//! A name resolved only through the `.lidx` — a docstring that mentions a
//! dependency's declaration nothing in this package refers to — is **not** in
//! the request set and keeps its source link. The run's line names its own
//! denominator so that this is readable off the log rather than inferred.
//!
//! # When it is resolved
//!
//! **Once per run, from the IR tree whose render key the ledger is about to
//! record.** On a full generation that is the tree the extraction just wrote; on
//! an incremental run it is the tree the round starts from, because the render
//! key has to be known before `detect` compares it with the ledger's. Resolving
//! it twice in one run is how the pages and the ledger would come to disagree,
//! and a disagreement there re-renders every page on every run for ever.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use litedoc4_render::{DepDocs, ExternalLinks};
use serde::Deserialize;
use serde::de::{DeserializeSeed, Deserializer, Error as _, IgnoredAny, MapAccess, Visitor};

use crate::{Failure, usage};

/// Where doc-gen4 writes the declaration table under a site's root, and
/// therefore the default for `--deps-docs-index`.
///
/// The extension says `bmp` and the bytes are JSON; that is the other side's
/// choice and copying it is how the default finds the file.
const DEFAULT_INDEX: &str = "declarations/declaration-data.bmp";

/// How long `curl` may spend getting a connection.
///
/// Not a read timeout: 5.7 MB gzipped 【実測】 over a slow link is a long
/// download and a correct one. What this catches is a host that never answers,
/// which would otherwise hang a build with no output at all.
const CONNECT_TIMEOUT: &str = "10";

/// `--max-time`, the whole transfer.
///
/// `--connect-timeout` bounds the handshake and nothing after it, so a stalled
/// transfer hangs the build with no output — which is the shape of doc-gen4
/// #404 ("lake build … hangs indefinitely"), one of the reports this feature
/// exists because of. A ceiling is not a tuning knob: the measured fetch of
/// mathlib4_docs' 5.7 MB gzipped table is 0.63 s 【実測 2026-08-19,
/// benchmarks/results/deps-link-rot-2026-08-19.txt】, so anything near this is
/// a link that is never going to finish, and the run says so and links to
/// source instead.
const MAX_TIME: &str = "120";

// --------------------------------------------------------------- the request

/// One `--deps-docs-url <Root>=<url>`, with the index that goes with it.
///
/// `Debug` so that a `Result<_, Failure>` can be `expect_err`ed in a test, as
/// [`Failure`] itself is.
#[derive(Debug)]
pub(crate) struct Site {
    pub root: String,
    pub base: String,
    pub index: Source,
}

/// Where a declaration table is read from.
///
/// **A local path is not a testing convenience.** It is what makes the feature
/// usable from a machine with no outbound network and what lets this file be
/// tested at all — a test that needs mathlib4_docs to be up is not a test.
#[derive(Debug)]
pub(crate) enum Source {
    Url(String),
    File(PathBuf),
}

impl Source {
    /// `http://` and `https://` are URLs; everything else is a path.
    ///
    /// A scheme this cannot fetch (`ftp://`, `file://`) would be handed to
    /// `curl` as a path and fail to open with its own name in the message,
    /// which is the right amount of guessing to do.
    fn parse(value: &str) -> Self {
        if value.starts_with("https://") || value.starts_with("http://") {
            Self::Url(value.to_owned())
        } else {
            Self::File(PathBuf::from(value))
        }
    }

    fn describe(&self) -> String {
        match self {
            Self::Url(url) => url.clone(),
            Self::File(path) => path.display().to_string(),
        }
    }
}

/// `--deps-docs-url` and `--deps-docs-index`, checked against each other.
///
/// Repeats and an index for a root with no site are refused rather than
/// resolved: both are a caller saying two things about one root, and picking one
/// of them is how a build ends up linking somewhere nobody asked for.
pub(crate) fn parse(urls: &[String], indexes: &[String]) -> Result<Vec<Site>, Failure> {
    let mut sites: Vec<Site> = Vec::new();
    for raw in urls {
        let (root, base) = split(raw, "--deps-docs-url")?;
        if base.is_empty() {
            return usage(format!(
                "--deps-docs-url {root}= has no URL: the site's own root is what a docLink is \
                 joined onto, and an empty one would produce an absolute path on whatever host \
                 serves this site"
            ));
        }
        if sites.iter().any(|site| site.root == root) {
            return usage(format!(
                "--deps-docs-url names `{root}` twice: a root has one documentation site, and \
                 taking either of two would be this command choosing which links the site gets"
            ));
        }
        sites.push(Site {
            index: Source::parse(&format!("{}/{DEFAULT_INDEX}", base.trim_end_matches('/'))),
            root,
            base,
        });
    }
    for raw in indexes {
        let (root, value) = split(raw, "--deps-docs-index")?;
        let Some(site) = sites.iter_mut().find(|site| site.root == root) else {
            return usage(format!(
                "--deps-docs-index {root}=… without --deps-docs-url {root}=…: this flag says where \
                 a site's declaration table is, and there is no site for `{root}` to have one"
            ));
        };
        site.index = Source::parse(&value);
    }
    Ok(sites)
}

fn split(raw: &str, flag: &str) -> Result<(String, String), Failure> {
    match raw.split_once('=') {
        Some((root, value)) if !root.is_empty() => Ok((root.to_owned(), value.to_owned())),
        _ => usage(format!(
            "{flag} wants <Root>=<value>, not `{raw}`: the root is the module name's first \
             component, as `litedoc4 links` prints it"
        )),
    }
}

/// Refuses a root that is not a dependency of the package being documented.
///
/// **Before anything is written**, because the answer is a typo or a package
/// that is not there, and both are things to say now rather than after an
/// extraction. It cannot be a warning: a run that quietly linked nowhere would
/// be reported by the one line this feature prints as a run that linked
/// nothing, which reads like a table that came out empty.
pub(crate) fn check_roots(sites: &[Site], links: &ExternalLinks) -> Result<(), Failure> {
    for site in sites {
        if links.base_for(&site.root).is_some() {
            continue;
        }
        let known: Vec<&str> = links.iter().map(|(root, _)| root).collect();
        return Err(Failure::Refused {
            code: 3,
            message: format!(
                "--deps-docs-url {}=…: `{}` is not a module root of any dependency this package \
                 resolves. The roots it does resolve are: {}. (`litedoc4 links --root <repo>` \
                 prints them with their sources.)",
                site.root,
                site.root,
                if known.is_empty() {
                    "none — no dependency could be resolved at all, so there is nothing to link"
                        .to_owned()
                } else {
                    known.join(", ")
                },
            ),
        });
    }
    Ok(())
}

// -------------------------------------------------------------- the resolved

/// One root's answer: the site, what was asked of it, and what it holds.
#[derive(Debug)]
pub(crate) struct Resolved {
    pub root: String,
    /// How many names of this root the IR refers to — the denominator of the
    /// line below, and the reason it is stored in the artifact rather than
    /// recomputed: `litedoc4 render` has no IR request set of its own and has to
    /// report the same fact `build` did.
    pub requested_names: usize,
    pub docs: DepDocs,
}

impl Resolved {
    /// The one line a run prints per documentation site.
    ///
    /// Both halves of the rule are in it, with their denominators: what went to
    /// the site and what did not. A line that reported only the first would make
    /// a table that answers nothing look like a table that answers everything.
    fn line(&self) -> String {
        format!(
            "deps    {}: {}/{} name(s) and {} module(s) -> {}, {} name(s) not in the table -> \
             version-pinned source",
            self.root,
            self.docs.declaration_count(),
            self.requested_names,
            self.docs.module_count(),
            self.docs.base(),
            self.requested_names
                .saturating_sub(self.docs.declaration_count()),
        )
    }
}

/// Attaches what was resolved to the source map, printing one line each.
///
/// The printing is here rather than at the two producers so that a run driven by
/// `--deps-docs-url` and a run driven by `--deps-docs-map` report the same fact
/// in the same words: the second is supposed to reproduce the first, and two
/// spellings of the report would hide it when they stop doing so.
pub(crate) fn attach(links: &ExternalLinks, resolved: Vec<Resolved>) -> ExternalLinks {
    let mut entries: Vec<(String, DepDocs)> = Vec::with_capacity(resolved.len());
    for site in resolved {
        println!("{}", site.line());
        entries.push((site.root, site.docs));
    }
    links.clone().with_docs(entries)
}

/// Reads every configured site's table and keeps what this IR can ask about.
///
/// A site whose table will not read costs that site and nothing else: its line
/// says the whole root went to the source, and the map comes back without it —
/// which is a *different digest* from a run that read it, so the ledger cannot
/// record a half-applied feature as up to date.
pub(crate) fn resolve(
    sites: &[Site],
    ir: &Path,
    map_out: Option<&Path>,
) -> Result<Vec<Resolved>, Failure> {
    if sites.is_empty() {
        return Ok(Vec::new());
    }
    let roots: BTreeSet<String> = sites.iter().map(|site| site.root.clone()).collect();
    let want = Want::of(ir, &roots)?;

    // One pass per *table*, not per root: mathlib4_docs documents `Init` and
    // `Lean` as well as `Mathlib`, so two roots pointed at one site read one
    // file. Grouped by the index's spelling, which is what a caller controls.
    let mut groups: BTreeMap<String, Vec<&Site>> = BTreeMap::new();
    for site in sites {
        groups.entry(site.index.describe()).or_default().push(site);
    }

    let mut resolved: Vec<Resolved> = Vec::new();
    for (index, group) in groups {
        let asked: BTreeSet<String> = group.iter().map(|site| site.root.clone()).collect();
        let found = match read(&group[0].index, &want, &asked) {
            Ok(found) => found,
            Err(Problem::Missing(message)) => {
                return Err(Failure::Refused {
                    code: 3,
                    message,
                });
            }
            Err(Problem::Unreadable(why)) => {
                for site in &group {
                    println!(
                        "deps    {}: the declaration table could not be read, so every link -> \
                         version-pinned source ({index}: {why})",
                        site.root,
                    );
                }
                continue;
            }
        };
        for site in group {
            let declarations = found.declarations.get(&site.root).cloned().unwrap_or_default();
            let modules = found.modules.get(&site.root).cloned().unwrap_or_default();
            resolved.push(Resolved {
                requested_names: want.count(&site.root),
                docs: DepDocs::new(site.base.clone(), declarations, modules),
                root: site.root.clone(),
            });
        }
    }
    if let Some(path) = map_out {
        write_map(path, &resolved)?;
    }
    Ok(resolved)
}

// ---------------------------------------------------------- what is asked for

/// The names this run can refer to, by the root that documents them.
struct Want {
    /// Full name -> the root whose site would answer for it.
    names: BTreeMap<String, String>,
}

impl Want {
    /// Every name in the IR's dependency slices whose **defining module** is
    /// under one of `roots`.
    ///
    /// Bucketed by the defining module rather than by which `deps/<Package>.json`
    /// the name was in, because that is the question the renderer asks:
    /// [`litedoc4_render::ExternalLinks::docs_url_for`] finds the root from the
    /// module the IR says a name lives in, so a name filed under one package and
    /// defined in another root's module has to be filed here the same way or it
    /// would be looked for in a site that was never asked about it.
    fn of(ir: &Path, roots: &BTreeSet<String>) -> Result<Self, Failure> {
        let tree = litedoc4_ir::IrTree::open(ir)
            .map_err(|source| Failure::Failed(format!("{}: {source}", ir.display())))?;
        let mut names = BTreeMap::new();
        for entry in &tree.index().dependency_maps {
            let map = tree
                .dep_map(entry)
                .map_err(|source| Failure::Failed(format!("{}: {source}", entry.file)))?;
            for (name, module) in &map.declarations {
                let Some(root) = litedoc4_ir::module_components(module).first().copied() else {
                    continue;
                };
                if let Some(root) = roots.get(root) {
                    names.insert(name.clone(), root.clone());
                }
            }
        }
        Ok(Self { names })
    }

    /// Which root would answer for a name, if any.
    fn owner(&self, name: &str) -> Option<&str> {
        self.names.get(name).map(String::as_str)
    }

    fn count(&self, root: &str) -> usize {
        self.names.values().filter(|owner| *owner == root).count()
    }
}

// ------------------------------------------------------------- reading a table

/// Why a table did not produce a map.
enum Problem {
    /// The machine cannot fetch at all. Refused by name, as a missing `git` is.
    Missing(String),
    /// The table itself: not there, not served, not the shape it has to be.
    Unreadable(String),
}

/// What one pass over one table kept.
#[derive(Debug, Default)]
struct Found {
    /// root -> name -> `docLink`.
    declarations: BTreeMap<String, BTreeMap<String, String>>,
    /// root -> module -> `url`.
    modules: BTreeMap<String, BTreeMap<String, String>>,
}

fn read(source: &Source, want: &Want, roots: &BTreeSet<String>) -> Result<Found, Problem> {
    match source {
        Source::File(path) => {
            // A path that is not there is `Unreadable`, not `Missing`: the
            // machine is fine and the file the caller named is not, which is the
            // same state a 404 is and takes the same answer.
            let file = File::open(path)
                .map_err(|source| Problem::Unreadable(source.to_string()))?;
            parse_table(BufReader::new(file), want, roots).map_err(Problem::Unreadable)
        }
        Source::Url(url) => fetch(url, want, roots),
    }
}

/// `curl` writing to a pipe this parses as it arrives.
///
/// **The exit code that matters is `curl`'s, not the parser's.** A failed
/// request produces an empty body, so the parser fails first with "expected a
/// JSON object" — which is true and useless. So the child is waited for before
/// any parse error is believed, and its own message wins. This is the trap
/// CLAUDE.md records for `| tail` seen from the other side: the last stage of a
/// pipeline is not the one that failed.
fn fetch(url: &str, want: &Want, roots: &BTreeSet<String>) -> Result<Found, Problem> {
    let mut child = Command::new("curl")
        // `--fail` so that an HTML error page is an exit code rather than a
        // parse failure; `--compressed` because the table is 8.5% of its size
        // gzipped 【実測】; `--location` because a docs site is usually a
        // redirect away.
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--compressed",
            "--connect-timeout",
            CONNECT_TIMEOUT,
            "--max-time",
            MAX_TIME,
            url,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                Problem::Missing(format!(
                    "curl is not on PATH, and --deps-docs-url needs it to read {url}: the \
                     declaration table is fetched by running `curl`, the way this command runs \
                     `git` for --source-url and `lake` for the extraction. Install curl, or pass \
                     --deps-docs-index <Root>=<path> to read a table from disk"
                ))
            } else {
                Problem::Unreadable(format!("curl: {source}"))
            }
        })?;
    let stdout = child.stdout.take().expect("curl's stdout was piped");
    let parsed = parse_table(BufReader::new(stdout), want, roots);
    let finished = child
        .wait_with_output()
        .map_err(|source| Problem::Unreadable(format!("curl: {source}")))?;
    if !finished.status.success() {
        let said = String::from_utf8_lossy(&finished.stderr);
        let said = said.trim();
        return Err(Problem::Unreadable(if said.is_empty() {
            format!("curl exited {}", finished.status)
        } else {
            said.to_owned()
        }));
    }
    parsed.map_err(Problem::Unreadable)
}

/// The streaming parse: the whole table in, the requested entries out.
///
/// Every value this run has no use for goes through [`IgnoredAny`], which walks
/// the JSON without building it — that is what keeps a 66 MB table from
/// becoming a 66 MB `Value` and then a map fifty times larger than the answer.
fn parse_table<R: Read>(
    reader: R,
    want: &Want,
    roots: &BTreeSet<String>,
) -> Result<Found, String> {
    let mut found = Found::default();
    let mut de = serde_json::Deserializer::from_reader(reader);
    Table {
        want,
        roots,
        found: &mut found,
    }
    .deserialize(&mut de)
    .map_err(|source| source.to_string())?;
    // A second JSON value after the table is a table this does not understand,
    // and reading the first one anyway would be guessing which half is the
    // answer.
    de.end()
        .map_err(|source| format!("trailing content after the table: {source}"))?;
    Ok(found)
}

/// `{"declarations": {...}, "modules": {...}, …}` — the two sections that are
/// read, and every other key skipped.
///
/// **Both sections are required.** A table with no `declarations` is not a table
/// with nothing in it: it is a file of some other shape, and treating the two
/// alike is how a run reports "0 names found" for a URL that answered with
/// somebody's index page.
struct Table<'a> {
    want: &'a Want,
    roots: &'a BTreeSet<String>,
    found: &'a mut Found,
}

impl<'de> DeserializeSeed<'de> for Table<'_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, de: D) -> Result<(), D::Error> {
        de.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for Table<'_> {
    type Value = ();

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a declaration table object with `declarations` and `modules`")
    }

    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<(), M::Error> {
        let (mut declarations, mut modules) = (false, false);
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "declarations" => {
                    declarations = true;
                    map.next_value_seed(Section {
                        kind: Kind::Declaration,
                        want: self.want,
                        roots: self.roots,
                        found: self.found,
                    })?;
                }
                "modules" => {
                    modules = true;
                    map.next_value_seed(Section {
                        kind: Kind::Module,
                        want: self.want,
                        roots: self.roots,
                        found: self.found,
                    })?;
                }
                _ => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        for (present, name) in [(declarations, "declarations"), (modules, "modules")] {
            if !present {
                return Err(M::Error::custom(format!(
                    "no `{name}` object: this is not a doc-gen4 declaration table, and reading one \
                     that is missing a half would report names as absent when they were never \
                     looked for"
                )));
            }
        }
        Ok(())
    }
}

/// Which of the two sections is being walked. They differ in what a value looks
/// like and in what decides whether an entry is kept.
#[derive(Clone, Copy)]
enum Kind {
    Declaration,
    Module,
}

struct Section<'a> {
    kind: Kind,
    want: &'a Want,
    roots: &'a BTreeSet<String>,
    found: &'a mut Found,
}

impl<'de> DeserializeSeed<'de> for Section<'_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, de: D) -> Result<(), D::Error> {
        de.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for Section<'_> {
    type Value = ();

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a name -> entry object")
    }

    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<(), M::Error> {
        while let Some(key) = map.next_key::<String>()? {
            // Which root, if any, would answer for this entry. A declaration is
            // answered by the root that defines it *according to this run's IR*;
            // a module by its own first component, since there is nothing else
            // it could belong to.
            let root = match self.kind {
                Kind::Declaration => self.want.owner(&key).map(str::to_owned),
                Kind::Module => litedoc4_ir::module_components(&key)
                    .first()
                    .and_then(|root| self.roots.get(*root))
                    .cloned(),
            };
            let Some(root) = root else {
                map.next_value::<IgnoredAny>()?;
                continue;
            };
            let (link, into) = match self.kind {
                Kind::Declaration => (
                    map.next_value::<DeclarationEntry>()?.doc_link,
                    &mut self.found.declarations,
                ),
                Kind::Module => (
                    map.next_value::<ModuleEntry>()?.url,
                    &mut self.found.modules,
                ),
            };
            into.entry(root).or_default().insert(key, link);
        }
        Ok(())
    }
}

/// `{"kind": "def", "docLink": "./Mathlib/Order/Basic.html#Foo.bar"}`.
///
/// `kind` is deliberately not read: it is the other side's classification and
/// this side has its own out of the IR. Unknown keys are skipped rather than
/// refused, because the table gains them — `sourceLink` and `line` are in the
/// per-module files already — and a reader that broke on a new one would break
/// on the day mathlib4_docs adds a field.
#[derive(Deserialize)]
struct DeclarationEntry {
    #[serde(rename = "docLink")]
    doc_link: String,
}

/// `{"importedBy": [...], "url": "./Mathlib/Order/Basic.html"}`.
///
/// **The link is `url` here and `docLink` there**, which is doc-gen4's
/// `JsonIndexedModule` against its `JsonIndexedDeclarationInfo`
/// (`DocGen4/Output/ToJson.lean`). `importedBy` is a list per module and is the
/// bulk of the file; it is skipped without being built.
#[derive(Deserialize)]
struct ModuleEntry {
    url: String,
}

// -------------------------------------------------------------- the artifact

/// The format version of the file [`write_map`] writes.
const MAP_VERSION: u64 = 1;

/// What `litedoc4 build` writes and `litedoc4 site` / `litedoc4 render` read.
///
/// # Why an artifact and not the same flags on three commands
///
/// `crates/litedoc4-render/src/frame.rs:66-70` records the rule this follows:
/// three commands render, and a flag that has to be repeated on all three is one
/// that will be forgotten on one of them — after which two of the three disagree
/// about what the site says, silently. So the resolution happens **once**, in
/// the command that has a package and a network, and the other two read its
/// answer. They cannot re-derive it and therefore cannot differ from it.
fn write_map(path: &Path, resolved: &[Resolved]) -> Result<(), Failure> {
    let roots: Vec<serde_json::Value> = resolved
        .iter()
        .map(|site| {
            serde_json::json!({
                "root": site.root,
                "base": site.docs.base(),
                "requestedNames": site.requested_names,
                "declarations": site
                    .docs
                    .declarations()
                    .map(|(name, link)| (name.to_owned(), serde_json::Value::from(link)))
                    .collect::<serde_json::Map<String, serde_json::Value>>(),
                "modules": site
                    .docs
                    .modules()
                    .map(|(name, link)| (name.to_owned(), serde_json::Value::from(link)))
                    .collect::<serde_json::Map<String, serde_json::Value>>(),
            })
        })
        .collect();
    let record = serde_json::json!({
        "tool": "litedoc4 deps-docs",
        "version": MAP_VERSION,
        "roots": roots,
    });
    let body = serde_json::to_string(&record).expect("names and links serialise") + "\n";
    if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        crate::pipeline::create_dir(dir)?;
    }
    std::fs::write(path, body)
        .map_err(|source| Failure::Failed(format!("{}: {source}", path.display())))
}

/// The artifact, read back.
///
/// Every shape that is not this one is **refused by name** rather than read as
/// far as it goes: this file decides where a third of a page's links point, and
/// a partial read of it would move some of them and not others with nothing in
/// the output to say which.
pub(crate) fn read_map(path: &Path) -> Result<Vec<Resolved>, Failure> {
    let refuse = |why: String| Failure::Refused {
        code: 3,
        message: format!("{}: {why}", path.display()),
    };
    let text = std::fs::read_to_string(path)
        .map_err(|source| refuse(format!("{source}. `litedoc4 build --deps-docs-url …` writes it")))?;
    let record: serde_json::Value =
        serde_json::from_str(&text).map_err(|source| refuse(source.to_string()))?;
    let version = record.get("version").and_then(serde_json::Value::as_u64);
    if version != Some(MAP_VERSION) {
        return Err(refuse(format!(
            "resolved documentation map version {}, and this build reads version {MAP_VERSION}. \
             Rebuild it with `litedoc4 build --deps-docs-url …`",
            version.map_or_else(|| "absent".to_owned(), |found| found.to_string()),
        )));
    }
    let Some(roots) = record.get("roots").and_then(serde_json::Value::as_array) else {
        return Err(refuse("no `roots` array".to_owned()));
    };
    let mut resolved = Vec::with_capacity(roots.len());
    for site in roots {
        let field = |name: &str| -> Result<&str, Failure> {
            site.get(name)
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| refuse(format!("a root with no `{name}` string")))
        };
        let entries = |name: &str| -> Result<Vec<(String, String)>, Failure> {
            let Some(map) = site.get(name).and_then(serde_json::Value::as_object) else {
                return Err(refuse(format!("a root with no `{name}` object")));
            };
            map.iter()
                .map(|(key, value)| {
                    value.as_str().map(|link| (key.clone(), link.to_owned())).ok_or_else(|| {
                        refuse(format!("`{name}.{key}` is not a string"))
                    })
                })
                .collect()
        };
        let root = field("root")?.to_owned();
        let base = field("base")?.to_owned();
        let requested_names = site
            .get("requestedNames")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| refuse("a root with no `requestedNames` number".to_owned()))?;
        resolved.push(Resolved {
            root,
            requested_names: usize::try_from(requested_names).unwrap_or(usize::MAX),
            docs: DepDocs::new(base, entries("declarations")?, entries("modules")?),
        });
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fixture table.
    ///
    /// **Written here, not copied from mathlib4_docs.** The real file is 66 MB
    /// and its contents are somebody else's build; what a test needs is the
    /// *shape*, which is doc-gen4's `JsonIndex`
    /// (`DocGen4/Output/ToJson.lean`: `declarations`, `instances`, `modules`,
    /// `instancesFor`, with a module's link under `url` and a declaration's
    /// under `docLink`). Seven entries, chosen so that every branch of the rule
    /// has a witness: a name asked for and present, one present and not asked
    /// for, one asked for and absent, and a root that was never asked about.
    const TABLE: &str = include_str!("../tests/data/declaration-data.json");

    fn want(pairs: &[(&str, &str)]) -> Want {
        Want {
            names: pairs
                .iter()
                .map(|(name, root)| ((*name).to_owned(), (*root).to_owned()))
                .collect(),
        }
    }

    fn roots(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    fn read_fixture(text: &str, want: &Want, roots: &BTreeSet<String>) -> Result<Found, String> {
        parse_table(text.as_bytes(), want, roots)
    }

    /// **Both directions of the rule, off one table.** `Dep.wanted` is asked for
    /// and is there; `Dep.gone` is asked for and is not; `Dep.unwanted` is there
    /// and is not asked for, so it is not carried.
    #[test]
    fn a_requested_name_the_table_holds_is_kept_and_one_it_does_not_is_not() {
        let want = want(&[("Dep.wanted", "Dep"), ("Dep.gone", "Dep")]);
        let found = read_fixture(TABLE, &want, &roots(&["Dep"])).expect("the fixture parses");
        let declarations = &found.declarations["Dep"];
        assert_eq!(
            declarations.get("Dep.wanted").map(String::as_str),
            Some("./Dep/Home.html#Dep.wanted"),
        );
        assert_eq!(declarations.get("Dep.gone"), None);
        assert_eq!(declarations.get("Dep.unwanted"), None);
        assert_eq!(declarations.len(), 1);
    }

    /// Modules are kept in full for the roots being linked, and the link comes
    /// out of `url` — the key doc-gen4 uses for a module and not for a
    /// declaration.
    #[test]
    fn every_module_of_a_linked_root_is_kept_and_others_are_not() {
        let found = read_fixture(TABLE, &want(&[]), &roots(&["Dep"])).expect("the fixture parses");
        let modules = &found.modules["Dep"];
        assert_eq!(
            modules.get("Dep.Home").map(String::as_str),
            Some("./Dep/Home.html"),
        );
        assert_eq!(modules.get("Dep").map(String::as_str), Some("./Dep.html"));
        // `Other.Elsewhere` is in the table and its root was not asked about.
        assert_eq!(modules.len(), 2);
        assert!(!found.modules.contains_key("Other"));
    }

    /// A root nothing was asked about produces nothing, rather than everything.
    #[test]
    fn a_table_read_for_no_root_keeps_nothing() {
        let found = read_fixture(TABLE, &want(&[]), &roots(&[])).expect("the fixture parses");
        assert!(found.declarations.is_empty());
        assert!(found.modules.is_empty());
    }

    /// **Refused by name, not guessed at.** Each of these is a file somebody
    /// could plausibly point this at, and each has to say what is wrong with it
    /// rather than come back with an empty map.
    #[test]
    fn a_table_of_another_shape_is_refused_by_name() {
        let cases: [(&str, &str); 6] = [
            ("[]", "invalid type"),
            ("{\"modules\":{}}", "`declarations`"),
            ("{\"declarations\":{}}", "`modules`"),
            (
                "{\"declarations\":{\"Dep.wanted\":{\"kind\":\"def\"}},\"modules\":{}}",
                "docLink",
            ),
            (
                "{\"declarations\":{\"Dep.wanted\":\"./x.html\"},\"modules\":{}}",
                "invalid type",
            ),
            ("{\"declarations\":{},\"modules\":{}} {}", "trailing content"),
        ];
        let want = want(&[("Dep.wanted", "Dep")]);
        for (text, expected) in cases {
            let error = read_fixture(text, &want, &roots(&["Dep"]))
                .expect_err("this is not a declaration table");
            assert!(error.contains(expected), "{text}: {error}");
        }
    }

    /// A module entry with no `url` is the same failure one section over.
    #[test]
    fn a_module_entry_with_no_url_is_refused() {
        let text = "{\"declarations\":{},\"modules\":{\"Dep.Home\":{\"importedBy\":[]}}}";
        let error =
            read_fixture(text, &want(&[]), &roots(&["Dep"])).expect_err("no `url` to link at");
        assert!(error.contains("url"), "{error}");
    }

    /// The keys the reader does not use are walked past rather than built:
    /// `instances` and `instancesFor` are half the file and `importedBy` is most
    /// of the rest.
    #[test]
    fn the_sections_this_does_not_read_are_skipped() {
        let found = read_fixture(TABLE, &want(&[("Dep.wanted", "Dep")]), &roots(&["Dep"]))
            .expect("the fixture parses");
        assert_eq!(found.declarations["Dep"].len(), 1);
        assert_eq!(found.modules["Dep"].len(), 2);
    }

    // ------------------------------------------------------------ the flags

    #[test]
    fn the_index_defaults_to_the_sites_own_declaration_table() {
        let sites = parse(
            &["Dep=https://host.invalid/docs/".to_owned()],
            &[],
        )
        .expect("one site");
        assert_eq!(sites[0].root, "Dep");
        assert_eq!(sites[0].base, "https://host.invalid/docs/");
        assert_eq!(
            sites[0].index.describe(),
            format!("https://host.invalid/docs/{DEFAULT_INDEX}"),
        );
    }

    #[test]
    fn an_index_override_may_be_a_local_file() {
        let sites = parse(
            &["Dep=https://host.invalid/docs".to_owned()],
            &["Dep=/tmp/table.json".to_owned()],
        )
        .expect("one site");
        assert!(matches!(sites[0].index, Source::File(_)));
        assert_eq!(sites[0].index.describe(), "/tmp/table.json");
    }

    #[test]
    fn a_flag_that_says_two_things_about_one_root_is_refused() {
        for (urls, indexes, expected) in [
            (vec!["Dep"], vec![], "<Root>=<value>"),
            (vec!["=https://host.invalid"], vec![], "<Root>=<value>"),
            (vec!["Dep="], vec![], "has no URL"),
            (
                vec!["Dep=https://a.invalid", "Dep=https://b.invalid"],
                vec![],
                "twice",
            ),
            (
                vec!["Dep=https://a.invalid"],
                vec!["Other=/tmp/x.json"],
                "without --deps-docs-url",
            ),
        ] {
            let urls: Vec<String> = urls.iter().map(|raw| (*raw).to_owned()).collect();
            let indexes: Vec<String> = indexes.iter().map(|raw| (*raw).to_owned()).collect();
            let error = parse(&urls, &indexes).expect_err("a refusal");
            let Failure::Usage(message) = error else {
                panic!("a bad command line is exit 2, not {error:?}");
            };
            assert!(message.contains(expected), "{message}");
        }
    }

    /// A root that is not a dependency is a refusal that names the roots that
    /// are — the answer to a typo is the list it was nearly in.
    #[test]
    fn a_root_that_is_not_a_dependency_is_refused_with_the_ones_that_are() {
        let links = ExternalLinks::new([("Mathlib", "https://host/m/blob/abc"), ("Init", "")]);
        let sites = parse(&["Mathib=https://host.invalid/docs".to_owned()], &[]).expect("parsed");
        let error = check_roots(&sites, &links).expect_err("a refusal");
        let Failure::Refused { code, message } = error else {
            panic!("a world that disagrees with the flags is exit 3, not {error:?}");
        };
        assert_eq!(code, 3);
        assert!(message.contains("Mathib"), "{message}");
        assert!(message.contains("Mathlib, Init"), "{message}");

        // …and a root that *is* one passes, including the unpinnable third
        // state: a `path` dependency can still publish documentation.
        let ok = parse(&["Init=https://host.invalid/docs".to_owned()], &[]).expect("parsed");
        check_roots(&ok, &links).expect("Init is a root of this map");
    }

    // -------------------------------------------------------- the artifact

    /// The artifact round-trips: what `build` writes is what `render` reads, to
    /// the same links and the same reported numbers.
    #[test]
    fn the_resolved_map_round_trips() {
        let dir = std::env::temp_dir().join(format!("litedoc4-deps-docs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a writable temporary directory");
        let path = dir.join("deps-docs-map.json");
        let written = vec![Resolved {
            root: "Dep".to_owned(),
            requested_names: 3,
            docs: DepDocs::new(
                "https://host.invalid/docs",
                [("Dep.wanted", "./Dep/Home.html#Dep.wanted")],
                [("Dep.Home", "./Dep/Home.html"), ("Dep", "./Dep.html")],
            ),
        }];
        write_map(&path, &written).expect("the artifact is written");

        let read = read_map(&path).expect("the artifact is read back");
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].root, "Dep");
        assert_eq!(read[0].requested_names, 3);
        assert_eq!(read[0].docs, written[0].docs);
        assert_eq!(read[0].line(), written[0].line());
        // The line is the fact the run reports, so it is asserted rather than
        // only compared: a round trip between two wrong values is also a round
        // trip.
        assert_eq!(
            read[0].line(),
            "deps    Dep: 1/3 name(s) and 2 module(s) -> https://host.invalid/docs, 2 name(s) not \
             in the table -> version-pinned source",
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An artifact of another shape is refused by name rather than half-read.
    #[test]
    fn a_resolved_map_of_another_shape_is_refused() {
        let dir = std::env::temp_dir().join(format!(
            "litedoc4-deps-docs-bad-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("a writable temporary directory");
        for (body, expected) in [
            ("not json", "expected"),
            ("{\"version\":2,\"roots\":[]}", "version 2"),
            ("{\"version\":1}", "`roots`"),
            ("{\"version\":1,\"roots\":[{}]}", "`root`"),
            (
                "{\"version\":1,\"roots\":[{\"root\":\"Dep\",\"base\":\"https://x\"}]}",
                "`requestedNames`",
            ),
            (
                "{\"version\":1,\"roots\":[{\"root\":\"Dep\",\"base\":\"https://x\",\
                  \"requestedNames\":0}]}",
                "`declarations`",
            ),
        ] {
            let path = dir.join("map.json");
            std::fs::write(&path, body).expect("writable");
            let error = read_map(&path).expect_err("a refusal");
            let Failure::Refused { code, message } = error else {
                panic!("a map that will not read is exit 3, not {error:?}");
            };
            assert_eq!(code, 3);
            assert!(message.contains(expected), "{body}: {message}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
