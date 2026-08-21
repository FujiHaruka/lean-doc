//! `litedoc4 site` — full generation in one command (milestone M3-d1).
//!
//! Two things are checked here, and they are different in kind.
//!
//! **That the composition adds and drops nothing.** The prototype's full
//! generation is three lines of shell (`stage7h/run.sh:78-80`) that call the
//! renderer and then `global.ts build` over the same IR and the same output
//! directory. `site` is those two calls inside one process, so the site it
//! writes must be byte-identical to the site `render` and `global` write
//! separately — over a synthetic package, so this runs on a machine that has
//! never seen the target one. The corpus-scale statement of the same thing is
//! `tools/site-compare.sh` against `m2/gate/ref-site` (438 files at M6; M8-d
//! moved the denominator to 439 — see `litedoc4_global::artifacts`).
//!
//! **That the command line cannot be got wrong quietly.** Every refusal below
//! is a flag combination that would otherwise produce a site that looks
//! finished and is not: a missing dependency map costs 150 of 432 pages their
//! bytes 【実測, plan 決定 4】, and a subset flag would make "full generation"
//! mean whatever subset was passed. They exit 2, which is what the prototype
//! exits on a bad command line, and they say why.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};

const BIN: &str = env!("CARGO_BIN_EXE_litedoc4");

/// Plan 決定 1: 40 hex digits, or the acceptance oracle's revless normalisation
/// misses and the score drops by 3.1103 points 【実測】. Nothing here reads the
/// host, but a fixture that models the URL wrongly teaches the wrong shape.
const URL: &str =
    "https://example.invalid/owner/repo/blob/0123456789abcdef0123456789abcdef01234567";

fn litedoc4(args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .output()
        .expect("the binary under test runs")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// The exit code, with the process's own diagnostics attached: a test that only
/// says "expected 2, got 1" makes the reader run the command by hand.
fn code(output: &Output) -> i32 {
    output
        .status
        .code()
        .unwrap_or_else(|| panic!("the process was killed by a signal: {}", stderr(output)))
}

// --------------------------------------------------------------- the refusals

/// **The refusal M3-d1 exists for.** `run.sh`'s `render()` passes no dependency
/// map and this project has ported that omission twice; the third guard is that
/// the choice cannot be made by saying nothing.
#[test]
fn neither_link_index_flag_is_refused() {
    let trees = TempDir::new("no-link-index-flag");
    let ir = trees.path.join("ir");
    synthetic_ir(&ir);
    let out = trees.path.join("site");
    let output = litedoc4(&[
        "site",
        "--ir",
        &ir.display().to_string(),
        "--out",
        &out.display().to_string(),
        "--source-url",
        URL,
    ]);

    assert_eq!(code(&output), 2, "{}", stderr(&output));
    let message = stderr(&output);
    // The count *and* its denominator: "some pages change" is a sentence
    // nobody acts on, and the reason this is not a default is the size of it.
    assert!(message.contains("150"), "{message}");
    assert!(message.contains("432"), "{message}");
    assert!(message.contains("--no-link-index"), "{message}");
    assert!(
        !out.exists(),
        "a refused command line wrote a tree: {}",
        out.display(),
    );
}

/// Both spellings is not "the last one wins": one of them is a statement about
/// the run that the other contradicts.
#[test]
fn both_link_index_flags_are_refused() {
    let trees = TempDir::new("both-link-index-flags");
    let ir = trees.path.join("ir");
    synthetic_ir(&ir);
    let lidx = trees.path.join("link-index.lidx");
    write_lidx(&lidx);
    let out = trees.path.join("site");
    let output = litedoc4(&[
        "site",
        "--ir",
        &ir.display().to_string(),
        "--out",
        &out.display().to_string(),
        "--source-url",
        URL,
        "--link-index",
        &lidx.display().to_string(),
        "--no-link-index",
    ]);

    assert_eq!(code(&output), 2, "{}", stderr(&output));
    assert!(stderr(&output).contains("150"), "{}", stderr(&output));
    assert!(!out.exists(), "a refused command line wrote a tree");
}

/// A subset flag is refused **by name**, because it is a real flag of the
/// subcommand `site` calls: the answer the caller needs is why it is not here.
#[test]
fn a_subset_flag_is_refused() {
    for (flag, value) in [
        ("--only", Some("Pkg.B")),
        ("--only-from", Some("/dev/null")),
    ] {
        let trees = TempDir::new("subset-flag");
        let ir = trees.path.join("ir");
        synthetic_ir(&ir);
        let out = trees.path.join("site");
        let mut args = vec![
            "site".to_owned(),
            "--ir".to_owned(),
            ir.display().to_string(),
            "--out".to_owned(),
            out.display().to_string(),
            "--source-url".to_owned(),
            URL.to_owned(),
            "--no-link-index".to_owned(),
            flag.to_owned(),
        ];
        args.extend(value.map(str::to_owned));
        let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
        let output = litedoc4(&borrowed);

        assert_eq!(code(&output), 2, "{flag}: {}", stderr(&output));
        let message = stderr(&output);
        assert!(message.contains(flag), "{flag}: {message}");
        assert!(message.contains("every module"), "{flag}: {message}");
        assert!(!out.exists(), "{flag} wrote a tree");
    }
}

/// The delta flags belong to the incremental round (M3-d2). A full run
/// re-renders every page, so a delta computed here would describe a decision
/// nobody took.
#[test]
fn a_delta_flag_is_refused() {
    for flag in ["--before", "--print-set", "--delta-json"] {
        let trees = TempDir::new("delta-flag");
        let ir = trees.path.join("ir");
        synthetic_ir(&ir);
        let out = trees.path.join("site");
        let output = litedoc4(&[
            "site",
            "--ir",
            &ir.display().to_string(),
            "--out",
            &out.display().to_string(),
            "--source-url",
            URL,
            "--no-link-index",
            flag,
            "/dev/null",
        ]);

        assert_eq!(code(&output), 2, "{flag}: {}", stderr(&output));
        let message = stderr(&output);
        assert!(message.contains(flag), "{flag}: {message}");
        assert!(message.contains("incremental"), "{flag}: {message}");
        assert!(!out.exists(), "{flag} wrote a tree");
    }
}

/// `render` spells the output tree `--pages` and `site` spells it `--out`,
/// because for `site` it is not only the pages. Saying so beats "unknown
/// argument".
#[test]
fn the_renderers_spelling_of_the_output_tree_is_refused() {
    let trees = TempDir::new("pages-flag");
    let ir = trees.path.join("ir");
    synthetic_ir(&ir);
    let out = trees.path.join("site");
    let output = litedoc4(&[
        "site",
        "--ir",
        &ir.display().to_string(),
        "--pages",
        &out.display().to_string(),
        "--source-url",
        URL,
        "--no-link-index",
    ]);

    assert_eq!(code(&output), 2, "{}", stderr(&output));
    assert!(stderr(&output).contains("--out"), "{}", stderr(&output));
}

/// The three required flags, a misspelling, and a value-taking flag with no
/// value left on the line. All exit 2 and all name what is wrong.
#[test]
fn the_rest_of_the_command_line_is_checked() {
    let trees = TempDir::new("required-flags");
    let ir = trees.path.join("ir");
    synthetic_ir(&ir);
    let ir = ir.display().to_string();
    let out = trees.path.join("site").display().to_string();

    let cases: [(Vec<&str>, &str); 5] = [
        (
            vec![
                "site",
                "--out",
                &out,
                "--source-url",
                URL,
                "--no-link-index",
            ],
            "--ir",
        ),
        (
            vec!["site", "--ir", &ir, "--source-url", URL, "--no-link-index"],
            "--out",
        ),
        (
            vec!["site", "--ir", &ir, "--out", &out, "--no-link-index"],
            "--source-url",
        ),
        (
            vec!["site", "--ir", &ir, "--out", &out, "--jobs", "4"],
            "--jobs",
        ),
        (
            vec!["site", "--ir", &ir, "--out", &out, "--source-url"],
            "--source-url",
        ),
    ];
    for (args, expected) in cases {
        let output = litedoc4(&args);
        assert_eq!(code(&output), 2, "{expected}: {}", stderr(&output));
        assert!(
            stderr(&output).contains(expected),
            "{expected}: {}",
            stderr(&output),
        );
    }
}

/// `--help` is not a refusal, and the usage it prints has to mention `site` —
/// a subcommand that only exists in the dispatch is one nobody finds.
#[test]
fn help_is_answered_and_the_usage_names_the_subcommand() {
    for args in [vec!["site", "--help"], vec!["--help"]] {
        let output = litedoc4(&args);
        assert_eq!(code(&output), 0, "{args:?}: {}", stderr(&output));
        let text = String::from_utf8_lossy(&output.stdout).into_owned();
        assert!(text.contains("litedoc4 site"), "{args:?}: {text}");
        assert!(text.contains("--no-link-index"), "{args:?}: {text}");
    }
}

// ------------------------------------------------------------ the composition

/// **The composition is exactly the two subcommands.** Same IR, same URL, same
/// map: one tree from `site`, one from `render` followed by `global`, compared
/// file by file.
///
/// The file *set* is asserted too. Byte equality between two empty trees is
/// also byte equality, and the failure this test is really watching for — one
/// stage overwriting the other's file — would show up as a tree that is
/// internally consistent and short.
#[test]
fn the_site_is_render_then_global_over_the_same_tree() {
    let trees = TempDir::new("site-vs-parts");
    let ir = trees.path.join("ir");
    synthetic_ir(&ir);
    let lidx = trees.path.join("link-index.lidx");
    write_lidx(&lidx);

    let one = trees.path.join("site");
    let ok = litedoc4(&[
        "site",
        "--ir",
        &ir.display().to_string(),
        "--out",
        &one.display().to_string(),
        "--source-url",
        URL,
        "--link-index",
        &lidx.display().to_string(),
    ]);
    assert_eq!(code(&ok), 0, "{}", stderr(&ok));

    let two = trees.path.join("parts");
    let rendered = litedoc4(&[
        "render",
        "--ir",
        &ir.display().to_string(),
        "--pages",
        &two.display().to_string(),
        "--source-url",
        URL,
        "--link-index",
        &lidx.display().to_string(),
    ]);
    assert_eq!(code(&rendered), 0, "{}", stderr(&rendered));
    let derived = litedoc4(&[
        "global",
        "--ir",
        &ir.display().to_string(),
        "--out",
        &two.display().to_string(),
    ]);
    assert_eq!(code(&derived), 0, "{}", stderr(&derived));

    let from_site = tree(&one);
    let from_parts = tree(&two);
    assert_eq!(
        from_site.keys().collect::<Vec<_>>(),
        from_parts.keys().collect::<Vec<_>>(),
        "the two trees hold different files",
    );
    let differing: Vec<&PathBuf> = from_site
        .keys()
        .filter(|path| from_site[*path] != from_parts[*path])
        .collect();
    assert!(differing.is_empty(), "differing: {differing:?}");

    // Five module pages, seven whole-package artifacts (M8-d). Written out
    // rather than computed from the same walk that produced the trees.
    //
    // No static assets: `write_assets` is `build`'s step 5, and `site` is the
    // composition of `render` and `global` and nothing else — which is what
    // makes the comparison above a comparison of those two stages.
    let mut expected: Vec<PathBuf> = [
        "Pkg.html",
        "Pkg/B.html",
        "Pkg/C.html",
        "Pkg/Leaf1.html",
        "Pkg/Leaf2.html",
        "declarations/name-map.json",
        "index.html",
        "404.html",
        "search.html",
        "foundational_types.html",
        "modules.json",
        "search-index.bin",
        "instances.json",
    ]
    .iter()
    .map(PathBuf::from)
    .collect();
    expected.sort();
    assert_eq!(from_site.keys().cloned().collect::<Vec<_>>(), expected);

    // `site` reports both stages, and neither report is the other's.
    let log = String::from_utf8_lossy(&ok.stdout).into_owned();
    assert!(log.contains("render  modules 5/5"), "{log}");
    assert!(log.contains("global  modules 5"), "{log}");
}

/// The dependency map is not decoration: with it and without it the same IR
/// produces different pages. 決定 4's 150-of-432 at synthetic scale — if this
/// ever stops failing, the flag stopped doing anything and the refusal above is
/// guarding nothing.
#[test]
fn the_dependency_map_changes_the_bytes() {
    let trees = TempDir::new("with-and-without-map");
    let ir = trees.path.join("ir");
    synthetic_ir(&ir);
    let lidx = trees.path.join("link-index.lidx");
    write_lidx(&lidx);

    let with = trees.path.join("with");
    let ok = litedoc4(&[
        "site",
        "--ir",
        &ir.display().to_string(),
        "--out",
        &with.display().to_string(),
        "--source-url",
        URL,
        "--link-index",
        &lidx.display().to_string(),
    ]);
    assert_eq!(code(&ok), 0, "{}", stderr(&ok));

    let without = trees.path.join("without");
    let ok = litedoc4(&[
        "site",
        "--ir",
        &ir.display().to_string(),
        "--out",
        &without.display().to_string(),
        "--source-url",
        URL,
        "--no-link-index",
    ]);
    assert_eq!(code(&ok), 0, "{}", stderr(&ok));

    let with = tree(&with);
    let without = tree(&without);
    assert_eq!(
        with.keys().collect::<Vec<_>>(),
        without.keys().collect::<Vec<_>>()
    );
    let differing: Vec<&PathBuf> = with
        .keys()
        .filter(|path| with[*path] != without[*path])
        .collect();
    assert_eq!(
        differing,
        [&PathBuf::from("Pkg.html")],
        "the map reached a different set of pages than the docstring says it should",
    );
}

/// `--timings` is one JSON line, and its two phase names are
/// `incremental.sh:416-419`'s, so a full run and an incremental one subtract.
///
/// The durations are wall clock and nothing asserts on them; the counts are
/// what a report quotes.
#[test]
fn the_timings_record_names_both_stages() {
    let trees = TempDir::new("site-timings");
    let ir = trees.path.join("ir");
    synthetic_ir(&ir);
    let out = trees.path.join("site");
    let timings = trees.path.join("nested/site-timings.json");
    let ok = litedoc4(&[
        "site",
        "--ir",
        &ir.display().to_string(),
        "--out",
        &out.display().to_string(),
        "--source-url",
        URL,
        "--no-link-index",
        "--timings",
        &timings.display().to_string(),
    ]);
    assert_eq!(code(&ok), 0, "{}", stderr(&ok));

    let text = fs::read_to_string(&timings).expect("the timings file was written");
    assert!(text.ends_with('\n'), "one line, terminated: {text:?}");
    let record: Value = serde_json::from_str(&text).expect("one JSON object");
    assert_eq!(record["command"], json!("site"));
    assert_eq!(record["pagesWritten"], json!(5));
    assert_eq!(record["modulesInIr"], json!(5));
    // No `--state`: the from-scratch build reads every module.
    assert_eq!(record["cacheHits"], json!(0));
    assert_eq!(record["cacheMisses"], json!(5));
    for phase in ["renderSeconds", "globalSeconds", "totalSeconds"] {
        assert!(record[phase].is_number(), "{phase}: {text}");
    }
}

/// `--state` reaches `global` and nothing else: a second run over an unchanged
/// IR hits the cache for every module and writes the same site anyway.
#[test]
fn the_cache_directory_reaches_the_derivation() {
    let trees = TempDir::new("site-state");
    let ir = trees.path.join("ir");
    synthetic_ir(&ir);
    let state = trees.path.join("state");

    let mut sites = Vec::new();
    for (run, expected) in [
        (0usize, "cache 0 hit / 5 miss"),
        (1, "cache 5 hit / 0 miss"),
    ] {
        let out = trees.path.join(format!("site{run}"));
        let ok = litedoc4(&[
            "site",
            "--ir",
            &ir.display().to_string(),
            "--out",
            &out.display().to_string(),
            "--source-url",
            URL,
            "--no-link-index",
            "--state",
            &state.display().to_string(),
        ]);
        assert_eq!(code(&ok), 0, "{}", stderr(&ok));
        let log = String::from_utf8_lossy(&ok.stdout).into_owned();
        assert!(log.contains(expected), "run {run}: {log}");
        sites.push(tree(&out));
    }
    assert_eq!(sites[0], sites[1], "the cached run wrote a different site");
}

// ------------------------------------------------------------------- fixtures

/// Every file under `root`, keyed by its path relative to it.
fn tree(root: &Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
    let mut files = std::collections::BTreeMap::new();
    let mut stack = vec![root.to_owned()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("a readable directory") {
            let entry = entry.expect("a readable entry");
            let path = entry.path();
            if entry.file_type().expect("a file type").is_dir() {
                stack.push(path);
            } else {
                let key = path.strip_prefix(root).expect("under the root").to_owned();
                files.insert(key, fs::read(&path).expect("a readable file"));
            }
        }
    }
    files
}

/// A declaration with every schema-5 key `litedoc4_ir` requires — the same
/// shape `litedoc4-global/tests/state_and_delta.rs` writes.
fn decl(name: &str, kind: &str, doc: Option<&str>) -> Value {
    json!({
        "name": name, "kind": kind, "modifiers": [], "binders": [], "implicits": [],
        "binderCode": [], "type": "Prop", "typeCode": [], "line": 1, "col": 0,
        "endLine": 1, "endCol": 1, "index": 0, "members": [], "doc": doc,
        "equations": [], "equationCode": [], "refs": [],
    })
}

/// A five-module package with one docstring reference (`Dep.Home.other`) that
/// is resolvable only through the `.lidx`.
fn synthetic_ir(root: &Path) {
    let modules = [
        (
            "Pkg",
            "1111111111111111",
            vec![],
            vec![decl(
                "Pkg.a",
                "def",
                // `Dep.elsewhere` is in the IR's own dependency slice, so it
                // links either way; `Dep.Home.other` and `Pkg.B.only_in_lidx`
                // exist only in the `.lidx`, which is the difference the flag
                // makes. The second of the two is the one that reaches the
                // bytes: since 2026-08-17 a name whose module has no page is
                // not linked at all, and `Dep.Home` is not a page this site
                // writes — `Pkg.B` is.
                Some("See `Pkg.B.b`, `Dep.elsewhere`, `Dep.Home.other` and `Pkg.B.only_in_lidx`."),
            )],
        ),
        (
            "Pkg.B",
            "2222222222222222",
            vec!["Pkg"],
            vec![
                decl("Pkg.B.b", "theorem", Some("An entry mentioning `Pkg.a`.")),
                decl("Pkg.B.c", "theorem", None),
            ],
        ),
        (
            "Pkg.C",
            "3333333333333333",
            vec!["Pkg", "Pkg.B"],
            vec![decl("Pkg.C.d", "def", Some("`Pkg.B.b` and [x](Pkg.a)."))],
        ),
        (
            "Pkg.Leaf1",
            "4444444444444444",
            vec!["Pkg"],
            vec![decl("Pkg.Leaf1.e", "def", None)],
        ),
        (
            "Pkg.Leaf2",
            "5555555555555555",
            vec!["Pkg.B"],
            vec![decl("Pkg.Leaf2.f", "def", Some("`Pkg.a`"))],
        ),
    ];

    let mut entries = Vec::new();
    let mut declarations = 0usize;
    for (module, hash, imports, decls) in &modules {
        let file = format!("modules/{module}.json");
        let body = serde_json::to_string(&json!({
            "schemaVersion": 5,
            "module": module,
            "imports": imports,
            "moduleDocs": [],
            "tactics": [],
            "declarations": decls,
        }))
        .expect("serialises");
        write(&root.join(&file), body.as_bytes());
        declarations += decls.len();
        entries.push(json!({
            "bytes": body.len(),
            "contentHash": hash,
            "declarations": decls.len(),
            "file": file,
            "module": module,
        }));
    }

    let map = json!({ "Dep.elsewhere": "Dep.Home" });
    let body = serde_json::to_string(&json!({
        "schemaVersion": 5, "package": "Dep", "declarations": map,
    }))
    .expect("serialises");
    write(&root.join("deps/Dep.json"), body.as_bytes());

    let index = json!({
        "declarationCount": declarations,
        "dependencyMaps": [{
            "bytes": body.len(),
            "entries": map.as_object().expect("a map").len(),
            "file": "deps/Dep.json",
            "package": "Dep",
        }],
        "generator": "litedoc4/crates/litedoc4/tests/site.rs",
        "hashAlgorithm": "lean-string-hash-64/hex16",
        "leanVersion": "4.31.0",
        "moduleCount": modules.len(),
        "modules": entries,
        "schemaVersion": 5,
    });
    write(
        &root.join("index.json"),
        serde_json::to_string(&index)
            .expect("serialises")
            .as_bytes(),
    );
}

/// A dependency closure holding the names `Pkg.a`'s docstring mentions and the
/// IR does not define.
///
/// It is the **environment**, not only the dependencies: `Pkg.B` is a module of
/// the package being documented and the map names it too, which is what a real
/// `.lidx` does. That group is the one whose absence moves a byte — a name in
/// `Dep.Home` gets no link either way, because this site writes no page for it.
fn write_lidx(path: &Path) {
    write(
        path,
        b"#lidx1\n@Dep.Home\n@Pkg.B\nDep.Home\n\tDep.elsewhere\n\tDep.Home.other\n\
          Pkg.B\n\tPkg.B.only_in_lidx\n",
    );
}

fn write(path: &Path, body: &[u8]) {
    fs::create_dir_all(path.parent().expect("a parent")).expect("writable");
    fs::write(path, body).expect("writable");
}

/// A directory that removes itself, as in `litedoc4-global/tests/global.rs`.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(what: &str) -> Self {
        use std::sync::atomic::{AtomicU32, Ordering};
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let slug: String = what
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .take(40)
            .collect();
        let path = std::env::temp_dir().join(format!(
            "litedoc4-site-{}-{}-{slug}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
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
