//! `litedoc4 build` — the one command (milestone M4-d).
//!
//! Four things are checked here, and they are different in kind.
//!
//! **That the two paths agree.** A first run extracts everything and renders
//! everything; a second run over an unchanged package runs the incremental
//! pipeline. The site after the second run has to be the site after the first,
//! byte for byte, and the first one has to be the site `litedoc4 site` writes
//! from the same IR. That is the M4-d gate, in miniature and on a machine that
//! has never seen the measurement target.
//!
//! **That the ledger is written at the right moment.** The whole of the
//! write-back's design is *when*: a ledger written before the pages licenses a
//! site nobody rendered, and once written it is never questioned again — the
//! next run reports 0 changed and stops. So the failing-extractor case is here,
//! and it asserts the ledger did **not** move.
//!
//! **That `--lib` has an origin.** The lakefile recogniser reads exactly one
//! shape and refuses everything else by name; the refusals are the tests,
//! because the failure they prevent is silent under-reading — a library that is
//! skipped produces a shorter module list, which looks exactly like a package
//! whose modules were deleted.
//!
//! **That the command line cannot be got wrong quietly.** Every refusal is a
//! flag that names a decision this command has taken over, or a directory it
//! would otherwise delete somebody's files in.
//!
//! The extractor is a `/bin/sh` script, as in `tests/incremental.rs`: this file
//! is about the sequencing, and needing a built Lean toolchain to run it would
//! mean it is not run.

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use litedoc4_render::ASSETS;
use serde_json::{Value, json};

const BIN: &str = env!("CARGO_BIN_EXE_litedoc4");

/// Plan 決定 1: 40 hex digits, or the acceptance oracle's revless normalisation
/// misses and the score drops 3.1103 points 【実測】.
const URL: &str =
    "https://example.invalid/owner/repo/blob/0123456789abcdef0123456789abcdef01234567";

/// The package the fixtures build. Three modules, one of which nothing imports.
const MODULES: [&str; 3] = ["Pkg", "Pkg.B", "Pkg.C"];

// ------------------------------------------------------------------ the world

/// A module as the fixture knows it: what its olean hashes to, and what its IR
/// says.
///
/// The two move **independently**, which is the whole reason the ledger exists:
/// a re-extraction whose IR comes out identical rewrites no page, and an olean
/// that did not move is not re-extracted at all.
#[derive(Clone)]
struct ModuleSpec {
    name: &'static str,
    olean: String,
    imports: Vec<&'static str>,
    doc: Option<String>,
}

fn base_world() -> Vec<ModuleSpec> {
    vec![
        ModuleSpec {
            name: "Pkg",
            olean: "olean-Pkg-1".to_owned(),
            imports: vec![],
            doc: Some("The root. See `Pkg.B.b`.".to_owned()),
        },
        ModuleSpec {
            name: "Pkg.B",
            olean: "olean-Pkg.B-1".to_owned(),
            imports: vec!["Pkg"],
            doc: Some("Mentions `Pkg.a`.".to_owned()),
        },
        ModuleSpec {
            name: "Pkg.C",
            olean: "olean-Pkg.C-1".to_owned(),
            imports: vec!["Pkg", "Pkg.B"],
            doc: None,
        },
    ]
}

/// One declaration, with every schema-4 key `litedoc4_ir` requires.
fn decl(name: &str, doc: Option<&str>) -> Value {
    json!({
        "name": name, "kind": "def", "modifiers": [], "binders": [], "implicits": [],
        "binderCode": [], "type": "Prop", "typeCode": [], "line": 1, "col": 0,
        "endLine": 1, "endCol": 1, "index": 0, "members": [], "doc": doc,
        "equations": [], "equationCode": [], "refs": [],
    })
}

/// The declaration name a module owns: `Pkg` owns `Pkg.a`, `Pkg.B` owns
/// `Pkg.B.b`.
fn decl_name(module: &str) -> String {
    let leaf = module.rsplit('.').next().expect("a leaf");
    format!("{module}.{}", leaf.to_lowercase())
}

/// The baked IR of the whole world, and the `index.json` entry of each module,
/// as the fake extractor copies them.
fn write_world(root: &Path, world: &[ModuleSpec]) {
    let _ = fs::remove_dir_all(root);
    for module in world {
        let names = [decl_name(module.name)];
        let decls: Vec<Value> = names
            .iter()
            .map(|name| decl(name, module.doc.as_deref()))
            .collect();
        let body = serde_json::to_string(&json!({
            "schemaVersion": 4,
            "module": module.name,
            "imports": module.imports,
            "moduleDocs": [],
            "tactics": [],
            "declarations": decls,
        }))
        .expect("serialises");
        write(
            &root.join(format!("ir/modules/{}.json", module.name)),
            body.as_bytes(),
        );
        // The `contentHash` is the fixture's own: the extractor computes it with
        // Lean's `String.hash` and nothing here re-implements that. What matters
        // is that it moves when the IR moves and not otherwise.
        let entry = json!({
            "bytes": body.len(),
            "contentHash": format!("{:016x}", fnv(&body)),
            "declarations": decls.len(),
            "file": format!("modules/{}.json", module.name),
            "module": module.name,
        });
        write(
            &root.join(format!("entries/{}.json", module.name)),
            serde_json::to_string(&entry)
                .expect("serialises")
                .as_bytes(),
        );
    }
}

/// A 64-bit hash, spelled in hex, standing in for Lean's `String.hash`.
fn fnv(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// The repository: the lakefile, the sources the glob finds and the oleans the
/// ledger hashes.
fn write_repo(repo: &Path, world: &[ModuleSpec]) {
    write(
        &repo.join("lakefile.toml"),
        b"name = \"pkg\"\nversion = \"0.1.0\"\ndefaultTargets = [\"Pkg\"]\n\n\
          [[lean_lib]]\nname = \"Pkg\"\n",
    );
    write(&repo.join("lean-toolchain"), b"leanprover/lean4:v4.31.0\n");
    write(
        &repo.join("lake-manifest.json"),
        br#"{"version":"1.1.0","packages":[]}"#,
    );
    for module in world {
        let path = module.name.replace('.', "/");
        write(&repo.join(format!("{path}.lean")), b"-- a source file\n");
        write(
            &repo.join(format!(".lake/build/lib/lean/{path}.olean")),
            module.olean.as_bytes(),
        );
    }
}

/// A dependency closure holding a name no module defines.
fn write_lidx(path: &Path) {
    write(
        path,
        b"#lidx1\n@Dep.Home\nDep.Home\n\tDep.elsewhere\n\tDep.Home.other\n",
    );
}

/// The fake extractor: a module list in, a partial IR tree out, copied byte for
/// byte from the baked world so that an incrementally merged tree and a
/// from-scratch one are comparable.
///
/// It appends its whole command line to `<work>/extractor-calls.txt`, which is
/// how the tests below count extractions without owning the code that asks for
/// them.
fn write_fake_extractor(path: &Path) {
    write(
        path,
        br#"#!/bin/sh
# The fake extractor of crates/litedoc4/tests/build.rs.
set -eu
WORLD=""; MODULES=""; IRDIR=""; TIMINGS=""; FAIL=0; CORRUPT=""
while [ $# -gt 0 ]; do
  case "$1" in
    --world) WORLD="$2"; shift 2 ;;
    --modules) MODULES="$2"; shift 2 ;;
    --ir-dir) IRDIR="$2"; shift 2 ;;
    --timings) TIMINGS="$2"; shift 2 ;;
    --fail) FAIL=1; shift ;;
    --corrupt) CORRUPT="$2"; shift 2 ;;
    *) echo "fake extractor: unknown option: $1" >&2; exit 2 ;;
  esac
done
[ -n "$MODULES" ] && [ -n "$IRDIR" ] && [ -n "$TIMINGS" ] || {
  echo "fake extractor: --modules, --ir-dir and --timings are all required" >&2; exit 2; }
WORK=$(dirname "$TIMINGS")
echo "--modules $MODULES --ir-dir $IRDIR --timings $TIMINGS" >> "$WORK/extractor-calls.txt"
tr -d '\n' < "$MODULES" | tr ' ' '\n' > /dev/null
[ "$FAIL" = 0 ] || { echo "fake extractor: asked to fail" >&2; exit 3; }
mkdir -p "$IRDIR/modules"
# The dependency slices, when the world has any. Its `deps-index.json` is the
# `dependencyMaps` array verbatim, because a shell that computed the byte counts
# itself would be a second writer of a format the extractor owns.
DEPS=""
if [ -f "$WORLD/ir/deps-index.json" ]; then
  DEPS=$(cat "$WORLD/ir/deps-index.json")
  mkdir -p "$IRDIR/deps"
  cp "$WORLD"/ir/deps/*.json "$IRDIR/deps/"
fi
ENTRIES="$WORK/.entries"
: > "$ENTRIES"
n=0
while IFS= read -r m; do
  [ -n "$m" ] || continue
  cp "$WORLD/ir/modules/$m.json" "$IRDIR/modules/$m.json"
  [ "$n" -eq 0 ] || printf ',' >> "$ENTRIES"
  cat "$WORLD/entries/$m.json" >> "$ENTRIES"
  n=$((n + 1))
done < "$MODULES"
{
  printf '{"declarationCount":0,"dependencyMaps":[%s],' "$DEPS"
  printf '"generator":"fake-extractor","hashAlgorithm":"lean-string-hash-64/hex16",'
  printf '"leanVersion":"4.31.0","moduleCount":%s,"modules":[' "$n"
  cat "$ENTRIES"
  printf '],"schemaVersion":4}'
} > "$IRDIR/index.json"
rm -f "$ENTRIES"
# `--corrupt <Module>`: an IR file the renderer cannot read. The extraction
# still succeeds, so the run fails in the renderer, which is the half of the
# ledger's ordering no failing extractor can reach.
[ -z "$CORRUPT" ] || printf 'not json' > "$IRDIR/modules/$CORRUPT.json"
printf '{"targetModules":%s,"extractor":"fake"}\n' "$n" > "$TIMINGS"
"#,
    );
    let mut perms = fs::metadata(path).expect("the script exists").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).expect("the script is chmod-able");
}

// ----------------------------------------------------------------- the harness

/// One package and one `--out` directory, run over and over.
struct Live {
    trees: TempDir,
    repo: PathBuf,
    out: PathBuf,
    world: PathBuf,
    lidx: PathBuf,
    script: PathBuf,
}

impl Live {
    fn new(what: &str) -> Self {
        let trees = TempDir::new(what);
        let live = Self {
            repo: trees.path.join("repo"),
            out: trees.path.join("out"),
            world: trees.path.join("world"),
            lidx: trees.path.join("link-index.lidx"),
            script: trees.path.join("extract.sh"),
            trees,
        };
        let world = base_world();
        write_repo(&live.repo, &world);
        write_world(&live.world, &world);
        write_lidx(&live.lidx);
        write_fake_extractor(&live.script);
        live
    }

    /// The world both the oleans and the baked IR come from, replaced.
    fn set_world(&self, world: &[ModuleSpec]) {
        write_repo(&self.repo, world);
        write_world(&self.world, world);
    }

    /// `litedoc4 build`, with the fixture's extractor and the flags every run
    /// needs.
    fn build(&self, extra: &[&str]) -> Output {
        let mut args: Vec<String> = vec![
            "build".to_owned(),
            "--root".to_owned(),
            self.repo.display().to_string(),
            "--out".to_owned(),
            self.out.display().to_string(),
            "--link-index".to_owned(),
            self.lidx.display().to_string(),
            "--source-url".to_owned(),
            URL.to_owned(),
            "--extractor".to_owned(),
            "/bin/sh".to_owned(),
            "--extractor-arg".to_owned(),
            self.script.display().to_string(),
            "--extractor-arg".to_owned(),
            "--world".to_owned(),
            "--extractor-arg".to_owned(),
            self.world.display().to_string(),
        ];
        args.extend(extra.iter().map(|arg| (*arg).to_owned()));
        let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
        litedoc4(&borrowed)
    }

    fn site(&self) -> PathBuf {
        self.out.join("site")
    }

    /// How many times the extractor has been called, and with how many modules.
    fn extractions(&self) -> Vec<usize> {
        let calls = self.out.join("work/extractor-calls.txt");
        let Ok(text) = fs::read_to_string(&calls) else {
            return Vec::new();
        };
        text.lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                let path = line
                    .split_whitespace()
                    .nth(1)
                    .expect("--modules has a value");
                fs::read_to_string(path)
                    .expect("the module list the extractor was handed")
                    .lines()
                    .filter(|name| !name.trim().is_empty())
                    .count()
            })
            .collect()
    }

    fn ledger(&self) -> Value {
        let text =
            fs::read_to_string(self.out.join("ledger.json")).expect("the ledger was written");
        serde_json::from_str(&text).expect("the ledger is JSON")
    }
}

// ------------------------------------------------------------------ the paths

/// **The gate, in miniature.** One command over a package that has never been
/// built produces a site; a second command over an unchanged package produces
/// the same bytes and starts no extraction at all; and the site is the one
/// `litedoc4 site` writes from the same IR.
#[test]
fn the_first_run_builds_and_the_second_one_does_nothing() {
    let live = Live::new("build-twice");

    let first = live.build(&[]);
    assert_eq!(code(&first), 0, "{}", stderr(&first));
    let log = stdout(&first);
    assert!(log.contains("plan    full generation"), "{log}");
    assert!(log.contains("lib     Pkg (from"), "{log}");
    let after_first = tree(&live.site());
    // 3 module pages + the 8 whole-package artifacts + the 3 static assets:
    // the target's 440 + 3 (M8-d made it 439 + 3 by dropping the five
    // doc-gen4-only artifacts; `docs/plans/search-v2.md` P0 adds
    // `instances.json`) with its 432 pages replaced by 3.
    assert_eq!(after_first.len(), 14, "{:?}", after_first.keys());
    assert_eq!(live.extractions(), vec![3], "the first run extracts all");

    let second = live.build(&[]);
    assert_eq!(code(&second), 0, "{}", stderr(&second));
    let log = stdout(&second);
    assert!(log.contains("plan    incremental"), "{log}");
    assert!(log.contains("0 to re-extract"), "{log}");
    assert!(log.contains("render  nothing to render"), "{log}");
    assert_eq!(
        live.extractions(),
        vec![3],
        "the second run started the extractor",
    );
    assert_eq!(
        tree(&live.site()),
        after_first,
        "the incremental run moved a byte of the site",
    );

    // …and the site is `litedoc4 site`'s **plus the static assets**, from the IR
    // the run left behind.
    //
    // The difference is M8-a and it is deliberate (`docs/plans/ui-redesign.md`
    // 決定 6): `litedoc4 site` is the composition of `render` and `global` — the
    // claim `tests/site.rs` makes and checks file by file — while `build` is the
    // command that produces something publishable, and a tree whose `<head>`
    // names a `style.css` nobody wrote is not that. So the gate is stated with
    // the three named rather than dropped.
    let reference = live.trees.path.join("reference-site");
    let ok = litedoc4(&[
        "site",
        "--ir",
        &live.out.join("ir").display().to_string(),
        "--out",
        &reference.display().to_string(),
        "--source-url",
        URL,
        "--link-index",
        &live.lidx.display().to_string(),
    ]);
    assert_eq!(code(&ok), 0, "{}", stderr(&ok));
    let mut pages_and_artifacts = after_first;
    for (name, body) in ASSETS {
        let taken = pages_and_artifacts.remove(Path::new(name));
        assert_eq!(
            taken.as_deref(),
            Some(body.as_bytes()),
            "{name} is not the asset the binary carries",
        );
    }
    assert_eq!(
        tree(&reference),
        pages_and_artifacts,
        "`build` is not `site` plus the three static assets",
    );
}

/// A module whose olean and IR both moved is re-extracted, its page is
/// rewritten, and **the run after it is quiet** — which is the write-back doing
/// its job. Without it the same module would be re-extracted for ever.
#[test]
fn a_changed_module_is_re_extracted_once() {
    let live = Live::new("build-change");
    assert_eq!(code(&live.build(&[])), 0);
    let before = tree(&live.site());

    let mut world = base_world();
    world[1].olean = "olean-Pkg.B-2".to_owned();
    world[1].doc = Some("Mentions `Pkg.a` twice: `Pkg.a`.".to_owned());
    live.set_world(&world);

    let changed = live.build(&[]);
    assert_eq!(code(&changed), 0, "{}", stderr(&changed));
    let log = stdout(&changed);
    assert!(log.contains("1 to re-extract"), "{log}");
    assert_eq!(live.extractions(), vec![3, 1], "one module, one round");
    let after = tree(&live.site());
    assert_ne!(
        after[Path::new("Pkg/B.html")],
        before[Path::new("Pkg/B.html")],
        "the changed module's page was not rewritten",
    );

    // The point of the write-back: the same edit is not re-extracted twice.
    let quiet = live.build(&[]);
    assert_eq!(code(&quiet), 0, "{}", stderr(&quiet));
    assert!(
        stdout(&quiet).contains("0 to re-extract"),
        "{}",
        stdout(&quiet)
    );
    assert_eq!(
        live.extractions(),
        vec![3, 1],
        "the ledger was not written back: the change was re-extracted",
    );
    assert_eq!(tree(&live.site()), after, "the quiet run moved a byte");
}

/// **The ordering that has a silent failure.** A run whose extractor fails
/// leaves the ledger where it was, so the next run re-extracts the same module.
/// Writing the ledger any earlier would license a site nobody rendered, and
/// nothing downstream would ever ask again.
#[test]
fn a_failed_run_does_not_move_the_ledger() {
    let live = Live::new("build-fail");
    assert_eq!(code(&live.build(&[])), 0);
    let ledger_before = live.ledger();
    let site_before = tree(&live.site());

    let mut world = base_world();
    world[2].olean = "olean-Pkg.C-2".to_owned();
    live.set_world(&world);

    let failed = live.build(&["--extractor-arg", "--fail"]);
    assert_eq!(code(&failed), 4, "{}", stderr(&failed));
    assert_eq!(
        live.ledger(),
        ledger_before,
        "the ledger moved on a run that never rendered",
    );
    assert_eq!(tree(&live.site()), site_before, "the site moved");

    // The marker says the run did not finish, so the repair is a full one.
    let repair = live.build(&[]);
    assert_eq!(code(&repair), 0, "{}", stderr(&repair));
    assert!(
        stdout(&repair).contains("full generation (the previous run did not finish)"),
        "{}",
        stdout(&repair),
    );
    // 3 (the first run), 1 (the one module whose olean moved, which failed),
    // 3 (the repair, which is a full generation).
    assert_eq!(live.extractions(), vec![3, 1, 3]);
    let after = tree(&live.site());
    assert_eq!(after.len(), 14);
    assert_ne!(
        live.ledger(),
        ledger_before,
        "the repair left a stale ledger"
    );

    // And the repaired tree is still the incremental fixed point.
    assert_eq!(code(&live.build(&[])), 0);
    assert_eq!(tree(&live.site()), after);
}

/// The same ordering on the **first** run, where there is no previous ledger to
/// leave alone: a full generation whose extractor fails must leave no ledger at
/// all.
///
/// This is the half the type does not cover. On the incremental path the module
/// hashes only exist inside the value `run_incremental` returns on success, so
/// "write the ledger before the pages" is not a line to move; on the full path
/// they are in hand before the extractor is called, and writing them there is
/// one line — which would license a site nobody rendered on the very first run.
#[test]
fn a_first_run_that_fails_leaves_no_ledger() {
    let live = Live::new("build-first-fails");
    let failed = live.build(&["--extractor-arg", "--fail"]);
    assert_eq!(code(&failed), 4, "{}", stderr(&failed));
    assert!(
        !live.out.join("ledger.json").exists(),
        "a run that never rendered left a ledger saying every module is up to date",
    );
    assert!(
        !live.site().exists(),
        "a run that never rendered left a site"
    );

    // …and the next run does the whole thing, rather than believing a ledger
    // the previous run had no right to write.
    let repair = live.build(&[]);
    assert_eq!(code(&repair), 0, "{}", stderr(&repair));
    assert_eq!(live.extractions(), vec![3, 3]);
    assert_eq!(tree(&live.site()).len(), 14);
}

/// The other half of the ordering: a run whose **renderer** fails leaves no
/// ledger either.
///
/// A failing extractor cannot reach this — it stops the run before the IR
/// exists — so without this case "write the ledger once the extraction is done"
/// would pass every other test in this file while licensing a site that was
/// never written.
#[test]
fn a_run_that_fails_in_the_renderer_leaves_no_ledger() {
    let live = Live::new("build-render-fails");
    let failed = live.build(&["--extractor-arg", "--corrupt", "--extractor-arg", "Pkg.C"]);
    assert_ne!(code(&failed), 0, "the corrupt IR was rendered anyway");
    assert!(
        !live.out.join("ledger.json").exists(),
        "the ledger was written for a site the renderer never finished",
    );

    // The repair is a full one, and it succeeds once the IR is readable again.
    let repair = live.build(&[]);
    assert_eq!(code(&repair), 0, "{}", stderr(&repair));
    assert_eq!(tree(&live.site()).len(), 14);
    assert!(live.out.join("ledger.json").is_file());
}

/// The ledger the run writes is the one a `ledger build` over the same tree
/// would write — the module hashes and the two keys, with `irGenerator` taken
/// from **the IR that now exists**.
///
/// The last one is the trap: writing back `detect`'s copy of the key would name
/// whatever wrote the *previous* tree, and if the two differ every later run
/// re-extracts every module for ever.
#[test]
fn the_ledger_names_the_tree_that_now_exists() {
    let live = Live::new("build-ledger");
    assert_eq!(code(&live.build(&[])), 0);

    let ledger = live.ledger();
    assert_eq!(ledger["ledgerSchema"], json!(2));
    // The canonical path: `--root` is resolved before anything is compared
    // against it, and the ledger records the target it hashed.
    assert_eq!(
        ledger["target"],
        json!(
            fs::canonicalize(&live.repo)
                .expect("the repository exists")
                .display()
                .to_string()
        ),
    );
    assert_eq!(
        ledger["modules"]
            .as_array()
            .expect("an array of entries")
            .len(),
        MODULES.len(),
    );
    assert_eq!(ledger["extractKey"]["irGenerator"], json!("fake-extractor"));
    assert_eq!(ledger["extractKey"]["irSchemaVersion"], json!("4"));
    assert_eq!(ledger["renderKey"]["sourceUrl"], json!(URL));

    // `ledger check` over the same tree is the independent statement of the
    // same thing: nothing changed, nothing added, nothing removed.
    let check = litedoc4(&[
        "ledger",
        "check",
        "--ledger",
        &live.out.join("ledger.json").display().to_string(),
        "--ir",
        &live.out.join("ir").display().to_string(),
        "--source-url",
        URL,
    ]);
    assert_eq!(code(&check), 0, "{}", stderr(&check));
    assert!(
        stdout(&check).contains("0 changed, 0 added, 0 removed"),
        "{}",
        stdout(&check),
    );
}

/// A module that vanished from the sources loses its page, and the ledger stops
/// naming it. The full-generation path answers the same question by removing the
/// site first — the renderer only ever writes.
#[test]
fn a_deleted_module_leaves_the_site_and_the_ledger() {
    let live = Live::new("build-delete");
    assert_eq!(code(&live.build(&[])), 0);
    assert!(live.site().join("Pkg/C.html").is_file());

    let world: Vec<ModuleSpec> = base_world().into_iter().take(2).collect();
    live.set_world(&world);
    fs::remove_file(live.repo.join("Pkg/C.lean")).expect("the source goes");
    // Lake does not remove the orphaned olean and neither does this: the module
    // list is a glob over the *sources* (plan §5, M3-d).
    let deleted = live.build(&[]);
    assert_eq!(code(&deleted), 0, "{}", stderr(&deleted));
    assert!(
        !live.site().join("Pkg/C.html").exists(),
        "the deleted module kept its page",
    );
    let ledger = live.ledger();
    let named: Vec<&str> = ledger["modules"]
        .as_array()
        .expect("entries")
        .iter()
        .map(|entry| entry["module"].as_str().expect("a name"))
        .collect();
    assert_eq!(named, ["Pkg", "Pkg.B"]);
    // 2 pages + 8 artifacts + 3 static assets.
    assert_eq!(tree(&live.site()).len(), 13);
}

/// **The gate of M8-a** (`docs/plans/ui-redesign.md` §5): a build into an empty
/// directory leaves the assets the pages reference, and no later run loses them
/// — not the quiet one that renders nothing, and not the one whose incremental
/// pipeline runs `prune` over the tree.
///
/// The clobbered file in the middle is the point of writing them unconditionally
/// rather than keying on them. They are **not** in `renderKey` (a page's bytes
/// do not depend on their content), so a run that skipped them because "the file
/// is already there" would ship whatever is on disk for ever.
#[test]
fn every_run_writes_the_static_assets() {
    let live = Live::new("build-assets");
    assert_eq!(code(&live.build(&[])), 0);
    assert_shipped(&live.site());
    // The `<head>` names all three since M8-b, and before M8-a nothing in the
    // tree answered it.
    let page = fs::read_to_string(live.site().join("Pkg.html")).expect("the page is there");
    for (name, _) in ASSETS {
        assert!(page.contains(name), "the page stopped naming {name}");
    }

    // A run that re-extracts and re-renders nothing still puts them back.
    fs::write(live.site().join("style.css"), "/* clobbered */").expect("the asset is writable");
    let quiet = live.build(&[]);
    assert_eq!(code(&quiet), 0, "{}", stderr(&quiet));
    let log = stdout(&quiet);
    assert!(log.contains("0 to re-extract"), "{log}");
    assert!(log.contains("render  nothing to render"), "{log}");
    assert_shipped(&live.site());

    // …and so does the run whose pipeline prunes: a module vanishes, its page
    // goes, the assets stay.
    let world: Vec<ModuleSpec> = base_world().into_iter().take(2).collect();
    live.set_world(&world);
    fs::remove_file(live.repo.join("Pkg/C.lean")).expect("the source goes");
    let pruned = live.build(&[]);
    assert_eq!(code(&pruned), 0, "{}", stderr(&pruned));
    assert!(
        !live.site().join("Pkg/C.html").exists(),
        "the deleted module kept its page",
    );
    assert_shipped(&live.site());
}

/// Every asset is on disk with the bytes the binary carries.
fn assert_shipped(site: &Path) {
    for (name, body) in ASSETS {
        let path = site.join(name);
        let on_disk = fs::read_to_string(&path)
            .unwrap_or_else(|source| panic!("{}: {source}", path.display()));
        assert_eq!(on_disk, body, "{name} is not what the binary carries");
    }
}

/// `--full` regenerates, and the tree it leaves is the tree the incremental path
/// was maintaining. It is the escape hatch for the inputs no ledger key covers —
/// the dependency map is one (150 of 432 pages 【実測, plan 決定 4】).
#[test]
fn full_regenerates_the_same_tree() {
    let live = Live::new("build-full");
    assert_eq!(code(&live.build(&[])), 0);
    let incremental = tree(&live.site());

    let forced = live.build(&["--full"]);
    assert_eq!(code(&forced), 0, "{}", stderr(&forced));
    assert!(
        stdout(&forced).contains("full generation (--full)"),
        "{}",
        stdout(&forced),
    );
    assert_eq!(live.extractions(), vec![3, 3]);
    assert_eq!(tree(&live.site()), incremental);
}

// ------------------------------------------------- the dependency's own docs

/// A dependency with a version-pinned URL, a module root and a declaration this
/// package refers to — everything A-1 needs to have something to link.
///
/// Returns the declaration table on disk. **A file rather than a URL**: a test
/// that needs mathlib4_docs to be up is not a test, and the local-path spelling
/// of `--deps-docs-index` is a shipped feature rather than a hook for this.
fn write_dependency(live: &Live) -> PathBuf {
    write(
        &live.repo.join("lake-manifest.json"),
        br#"{"version":"1.1.0","packagesDir":".lake/packages","packages":[
             {"url":"https://github.com/example/dep.git","type":"git",
              "rev":"0123456789abcdef0123456789abcdef01234567","name":"dep"}]}"#,
    );
    write(
        &live.repo.join(".lake/packages/dep/Dep.lean"),
        b"-- the dependency's module root\n",
    );

    // The IR the fake extractor copies: `Dep.elsewhere` is a name this package
    // refers to, so it is what the table is asked about.
    let slice =
        br#"{"schemaVersion":4,"package":"Dep","declarations":{"Dep.elsewhere":"Dep.Home"}}"#;
    write(&live.world.join("ir/deps/Dep.json"), slice);
    write(
        &live.world.join("ir/deps-index.json"),
        format!(
            r#"{{"package":"Dep","file":"deps/Dep.json","entries":1,"bytes":{}}}"#,
            slice.len(),
        )
        .as_bytes(),
    );

    let table = live.trees.path.join("declaration-data.json");
    write(
        &table,
        include_bytes!("data/declaration-data.json").as_slice(),
    );
    table
}

/// The package as A-1 needs it: one docstring naming a dependency declaration
/// the table documents **and** one it does not.
fn docs_world() -> Vec<ModuleSpec> {
    let mut world = base_world();
    world[0].doc = Some("See `Dep.elsewhere` and `Dep.Home.other`.".to_owned());
    world
}

/// **The rule, on a real page.** `Dep.elsewhere` is in the dependency's
/// declaration table, so its link is the dependency's own documentation;
/// `Dep.Home.other` is not, so its link stays the version-pinned source. One
/// docstring, one page, both answers.
#[test]
fn a_verified_name_links_at_the_dependencys_documentation_and_the_rest_at_its_source() {
    let live = Live::new("build-deps-docs");
    live.set_world(&docs_world());
    let table = write_dependency(&live);

    let ok = live.build(&[
        "--deps-docs-url",
        "Dep=https://docs.invalid/dep",
        "--deps-docs-index",
        &format!("Dep={}", table.display()),
    ]);
    assert_eq!(code(&ok), 0, "{}", stderr(&ok));

    // **The line, with both halves of the rule and their denominators.** One
    // name was asked for and found; the table's two `Dep.*` modules came with
    // it; nothing fell through.
    let log = stdout(&ok);
    assert!(
        log.contains(
            "deps    Dep: 1/1 name(s) and 2 module(s) -> https://docs.invalid/dep, 0 name(s) not \
             in the table -> version-pinned source"
        ),
        "{log}",
    );

    let page = fs::read_to_string(live.site().join("Pkg.html")).expect("the root page");
    assert!(
        page.contains("https://docs.invalid/dep/Dep/Home.html#Dep.elsewhere"),
        "the table documents Dep.elsewhere and the page did not link there: {page}",
    );
    assert!(
        page.contains(
            "https://github.com/example/dep/blob/0123456789abcdef0123456789abcdef01234567/\
             Dep/Home.lean"
        ),
        "the table does not document Dep.Home.other and the page did not fall back to the \
         version-pinned source: {page}",
    );
}

/// **The artifact round-trips**: `build` resolves once and writes the map, and
/// `render` reading that map produces the same page bytes.
///
/// This is what the artifact is *for* — three commands render, and a flag
/// repeated on all three is one that gets forgotten on one of them
/// (`crates/litedoc4-render/src/frame.rs:66-70`). `render` here is given no
/// `--deps-docs-url` at all and has no way to fetch anything.
#[test]
fn the_resolved_map_is_written_and_render_reproduces_the_same_page() {
    let live = Live::new("build-deps-docs-map");
    live.set_world(&docs_world());
    let table = write_dependency(&live);
    let ok = live.build(&[
        "--deps-docs-url",
        "Dep=https://docs.invalid/dep",
        "--deps-docs-index",
        &format!("Dep={}", table.display()),
    ]);
    assert_eq!(code(&ok), 0, "{}", stderr(&ok));

    let map = live.out.join("work/deps-docs-map.json");
    assert!(map.is_file(), "the resolved map was not written");

    let pages = live.trees.path.join("re-rendered");
    let rendered = litedoc4(&[
        "render",
        "--ir",
        &live.out.join("ir").display().to_string(),
        "--pages",
        &pages.display().to_string(),
        "--source-url",
        URL,
        "--link-index",
        &live.lidx.display().to_string(),
        "--root",
        &live.repo.display().to_string(),
        "--deps-docs-map",
        &map.display().to_string(),
    ]);
    assert_eq!(code(&rendered), 0, "{}", stderr(&rendered));
    // The same line, out of the artifact rather than out of a table: the second
    // command has to report the fact the first one established, in the same
    // words, or a drift between them is invisible.
    assert!(
        stdout(&rendered).contains("deps    Dep: 1/1 name(s) and 2 module(s)"),
        "{}",
        stdout(&rendered),
    );

    for module in MODULES {
        let path = format!("{}.html", module.replace('.', "/"));
        assert_eq!(
            fs::read(live.site().join(&path)).expect("built"),
            fs::read(pages.join(&path)).expect("re-rendered"),
            "{path} came out differently from the resolved map",
        );
    }
}

/// **A table that will not read sends the whole root to the source**, says so,
/// and does not stop the build.
///
/// The plan's 撤退ライン, as behaviour: guessing at a table this could not read
/// would put a link on every page of a dependency nobody verified. The run still
/// produces a site — the links are the ones v0.1 shipped — and the line is the
/// only way anyone finds out, so it is what is asserted.
#[test]
fn a_table_that_will_not_read_costs_the_root_its_documentation_links() {
    let live = Live::new("build-deps-docs-unreadable");
    live.set_world(&docs_world());
    write_dependency(&live);
    let missing = live.trees.path.join("no-such-table.json");

    let ok = live.build(&[
        "--deps-docs-url",
        "Dep=https://docs.invalid/dep",
        "--deps-docs-index",
        &format!("Dep={}", missing.display()),
    ]);
    assert_eq!(code(&ok), 0, "{}", stderr(&ok));
    let log = stdout(&ok);
    assert!(
        log.contains(
            "deps    Dep: the declaration table could not be read, so every link -> \
             version-pinned source"
        ),
        "{log}",
    );
    assert!(log.contains("no-such-table.json"), "{log}");

    // Both names take the source, and no resolved map is left behind claiming
    // otherwise.
    let page = fs::read_to_string(live.site().join("Pkg.html")).expect("the root page");
    assert!(!page.contains("docs.invalid"), "{page}");
    assert!(
        page.contains(
            "https://github.com/example/dep/blob/0123456789abcdef0123456789abcdef01234567/\
             Dep/Home.lean"
        ),
        "{page}",
    );
    assert!(
        !live.out.join("work/deps-docs-map.json").exists()
            || fs::read_to_string(live.out.join("work/deps-docs-map.json"))
                .expect("readable")
                .contains("\"roots\":[]"),
        "a map that resolved nothing must not look like one that resolved something",
    );
}

/// **The verification is in the render key.** Turning the feature off moves
/// `renderKey.externalLinks`, so the next run re-renders instead of reporting a
/// site whose links point somewhere it no longer says they do.
#[test]
fn the_documentation_map_reaches_the_render_key() {
    let live = Live::new("build-deps-docs-key");
    live.set_world(&docs_world());
    let table = write_dependency(&live);

    let with = live.build(&[
        "--deps-docs-url",
        "Dep=https://docs.invalid/dep",
        "--deps-docs-index",
        &format!("Dep={}", table.display()),
    ]);
    assert_eq!(code(&with), 0, "{}", stderr(&with));
    let documented = live.ledger()["renderKey"]["externalLinks"].clone();

    let without = live.build(&["--full"]);
    assert_eq!(code(&without), 0, "{}", stderr(&without));
    let plain = live.ledger()["renderKey"]["externalLinks"].clone();

    assert!(
        documented.is_string() && plain.is_string(),
        "{documented} / {plain}"
    );
    assert_ne!(
        documented, plain,
        "the ledger records the same key with and without the documentation map, so a run that \
         gained or lost it would report success without re-rendering",
    );
    // …and the map is not in the tree any more, so the run cannot be read as
    // still using it.
    assert!(
        !stdout(&without).contains("deps    Dep:"),
        "{}",
        stdout(&without)
    );
}

/// **`litedoc4 ledger` computes the key `build` recorded — with the map, and
/// only with it.**
///
/// `ledger build` and `ledger check` render nothing, so no page of theirs can
/// show which links a site carries; what they produce is the key that decides
/// whether those pages are re-rendered at all. A `ledger` run that cannot see
/// the resolved documentation map therefore hashes a *different*
/// `externalLinks` from the run that wrote the pages, and then answers about a
/// difference that is not there — the silent divergence this flag closes.
///
/// Both directions are asserted, because only the pair distinguishes "the flag
/// is read" from "the key does not depend on it": with the map the whole
/// `renderKey` is `build`'s object for object, and without it `check` says the
/// render key moved and every page has to be rendered again.
#[test]
fn the_ledger_command_reproduces_the_builds_render_key_only_with_the_map() {
    let live = Live::new("ledger-deps-docs");
    live.set_world(&docs_world());
    let table = write_dependency(&live);
    let built = live.build(&[
        "--deps-docs-url",
        "Dep=https://docs.invalid/dep",
        "--deps-docs-index",
        &format!("Dep={}", table.display()),
    ]);
    assert_eq!(code(&built), 0, "{}", stderr(&built));
    let recorded = live.ledger()["renderKey"].clone();
    assert!(
        recorded["externalLinks"].is_string(),
        "the build recorded no externalLinks to reproduce: {recorded}",
    );

    let map = live.out.join("work/deps-docs-map.json");
    assert!(map.is_file(), "the resolved map was not written");
    let map = map.display().to_string();
    // Exactly what `build` handed its own detect stage: same IR, same
    // --source-url, same dependency closure, same package.
    let modules = live.out.join("work/modules.txt").display().to_string();
    let ir = live.out.join("ir").display().to_string();
    let repo = live.repo.display().to_string();
    let lidx = live.lidx.display().to_string();
    let common = |extra: &[&str]| -> Vec<String> {
        let mut args: Vec<String> = [
            "--ir",
            &ir,
            "--source-url",
            URL,
            "--link-index",
            &lidx,
            "--root",
            &repo,
        ]
        .iter()
        .map(|arg| (*arg).to_owned())
        .collect();
        args.extend(extra.iter().map(|arg| (*arg).to_owned()));
        args
    };

    // `ledger build`, with the map and without it.
    let rebuilt = |name: &str, extra: &[&str]| -> Value {
        let out = live.trees.path.join(name);
        let mut args: Vec<String> = [
            "ledger",
            "build",
            "--modules",
            &modules,
            "--target",
            &repo,
            "--out",
            &out.display().to_string(),
        ]
        .iter()
        .map(|arg| (*arg).to_owned())
        .collect();
        args.extend(common(extra));
        let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
        let done = litedoc4(&borrowed);
        assert_eq!(code(&done), 0, "{}", stderr(&done));
        serde_json::from_str(&fs::read_to_string(&out).expect("the ledger was written"))
            .expect("the ledger is JSON")
    };
    assert_eq!(
        rebuilt("ledger-with.json", &["--deps-docs-map", &map])["renderKey"],
        recorded,
        "`ledger build --deps-docs-map` did not reproduce the key `build` recorded, so the two \
         disagree about which links the pages carry",
    );
    assert_ne!(
        rebuilt("ledger-without.json", &[])["renderKey"],
        recorded,
        "`ledger build` without the map recorded the same key as a build that had one, so the \
         flag reaches nothing and the map's names are not in the key",
    );

    // `ledger check` against the ledger `build` wrote: the same divergence, seen
    // from the side that decides whether to re-render.
    let checked = |extra: &[&str]| -> String {
        let mut args: Vec<String> = [
            "ledger",
            "check",
            "--ledger",
            &live.out.join("ledger.json").display().to_string(),
            "--modules",
            &modules,
        ]
        .iter()
        .map(|arg| (*arg).to_owned())
        .collect();
        args.extend(common(extra));
        let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
        let done = litedoc4(&borrowed);
        assert_eq!(code(&done), 0, "{}", stderr(&done));
        stdout(&done)
    };
    let agreed = checked(&["--deps-docs-map", &map]);
    assert!(
        !agreed.contains("render key changed"),
        "nothing moved and `check` with the map still wants every page re-rendered: {agreed}",
    );
    let blind = checked(&[]);
    assert!(
        blind.contains("render key changed (externalLinks)"),
        "`check` without the map reported the key `build` recorded, which would license a site \
         whose links it never saw: {blind}",
    );
}

// ---------------------------------------------------------------- the lakefile

/// The one shape that is read, and every refusal, each naming `--lib`.
#[test]
fn the_lakefile_is_read_or_refused_by_name() {
    let trees = TempDir::new("build-lakefile");
    let world = base_world();

    // What the measurement target's lakefile.toml looks like, plus the shapes a
    // real one has around it.
    let read: [(&str, &str, &[&str]); 3] = [
        ("plain", "[[lean_lib]]\nname = \"Pkg\"\n", &["Pkg"]),
        (
            "with-options",
            "name = \"pkg\"\ndefaultTargets = [\"Pkg\"]\n\n[[lean_lib]]\nname = \"Pkg\" # a comment\n\
             leanOptions = { weak.linter.all = true }\n",
            &["Pkg"],
        ),
        (
            "two-libraries",
            "[[lean_lib]]\nname=\"Pkg\"\n\n[[lean_exe]]\nname = \"tool\"\n\n[[lean_lib]]\nname = \"Other\"\n",
            &["Pkg", "Other"],
        ),
    ];
    for (what, body, expected) in read {
        let repo = trees.path.join(format!("read-{what}"));
        write_repo(&repo, &world);
        write(&repo.join("lakefile.toml"), body.as_bytes());
        // The second library needs a root of its own, or the glob refuses it —
        // which is a different refusal than the one under test here.
        write(&repo.join("Other.lean"), b"-- another library\n");
        let ok = litedoc4(&["modules", "--root", &repo.display().to_string()]);
        assert_eq!(code(&ok), 0, "{what}: {}", stderr(&ok));
        // The diagnostic is on stderr: stdout is the module list, and a caller
        // redirecting it into a file must not get a library name as its first
        // module.
        let log = stderr(&ok);
        assert!(
            log.contains(&format!("lib     {} (from", expected.join(", "))),
            "{what}: {log}",
        );
        let listed = stdout(&ok);
        let listed: Vec<&str> = listed.lines().collect();
        assert_eq!(
            listed.len(),
            MODULES.len() + usize::from(expected.len() > 1)
        );
        assert!(listed.contains(&"Pkg"), "{what}: {listed:?}");
    }

    // Everything else stops, with the same last sentence.
    let refused: [(&str, Option<&str>, &str); 6] = [
        ("lakefile-lean", None, "is Lean code, not data"),
        (
            "no-lean-lib",
            Some("name = \"pkg\"\n"),
            "no [[lean_lib]] block",
        ),
        (
            "no-name",
            Some("[[lean_lib]]\nleanOptions = {}\n"),
            "has no `name` key",
        ),
        (
            "odd-header",
            Some("[[lean_lib.extra]]\nname = \"Pkg\"\n"),
            "in a spelling this does not read",
        ),
        (
            "computed-name",
            Some("[[lean_lib]]\nname = { from = \"pkg\" }\n"),
            "is a `name` this does not read",
        ),
        (
            "multiline",
            Some("description = \"\"\"\n[[lean_lib]]\n\"\"\"\n[[lean_lib]]\nname = \"Pkg\"\n"),
            "multi-line strings are not read",
        ),
    ];
    for (what, body, expected) in refused {
        let repo = trees.path.join(format!("refuse-{what}"));
        write_repo(&repo, &world);
        fs::remove_file(repo.join("lakefile.toml")).expect("the toml goes");
        match body {
            Some(text) => write(&repo.join("lakefile.toml"), text.as_bytes()),
            None => write(&repo.join("lakefile.lean"), b"import Lake\nlean_lib Pkg\n"),
        }
        let output = litedoc4(&["modules", "--root", &repo.display().to_string()]);
        assert_eq!(code(&output), 3, "{what}: {}", stdout(&output));
        let message = stderr(&output);
        assert!(message.contains(expected), "{what}: {message}");
        assert!(message.contains("--lib"), "{what}: {message}");
    }

    // A package with no lakefile at all says so, and still names `--lib`.
    let bare = trees.path.join("bare");
    write_repo(&bare, &world);
    fs::remove_file(bare.join("lakefile.toml")).expect("the toml goes");
    let output = litedoc4(&["modules", "--root", &bare.display().to_string()]);
    assert_eq!(code(&output), 3, "{}", stdout(&output));
    assert!(
        stderr(&output).contains("no lakefile.toml and no lakefile.lean"),
        "{}",
        stderr(&output),
    );

    // …and naming it by hand still works, which is what every refusal offers.
    let ok = litedoc4(&[
        "modules",
        "--root",
        &bare.display().to_string(),
        "--lib",
        "Pkg",
    ]);
    assert_eq!(code(&ok), 0, "{}", stderr(&ok));
    for module in MODULES {
        assert!(stdout(&ok).contains(module), "{}", stdout(&ok));
    }
}

// --------------------------------------------------------------- the source URL

/// `--source-url` from the checkout: `git rev-parse HEAD` and the origin remote,
/// which is what `incremental.sh:106` hard-codes.
///
/// Only github.com is derived — the `/blob/<rev>/` shape is GitHub's and the
/// acceptance oracle normalises exactly it — so the second half of this test is
/// a remote that is refused rather than guessed at.
#[test]
fn the_source_url_comes_from_git() {
    let live = Live::new("build-git");
    git_init(&live.repo, "https://github.com/owner/repo.git");
    let rev = git_head(&live.repo);

    let ok = litedoc4(&[
        "build",
        "--root",
        &live.repo.display().to_string(),
        "--out",
        &live.out.display().to_string(),
        "--link-index",
        &live.lidx.display().to_string(),
        "--extractor",
        "/bin/sh",
        "--extractor-arg",
        &live.script.display().to_string(),
        "--extractor-arg",
        "--world",
        "--extractor-arg",
        &live.world.display().to_string(),
    ]);
    assert_eq!(code(&ok), 0, "{}", stderr(&ok));
    let expected = format!("https://github.com/owner/repo/blob/{rev}");
    assert!(stdout(&ok).contains(&expected), "{}", stdout(&ok));
    assert_eq!(rev.len(), 40, "the fixture's revision is not a full one");
    assert_eq!(
        live.ledger()["renderKey"]["sourceUrl"],
        json!(expected),
        "the derived URL did not reach the render key",
    );
    // A page links to it, which is the only statement that matters.
    let page = fs::read_to_string(live.site().join("Pkg/B.html")).expect("a page");
    assert!(
        page.contains(&format!("{expected}/Pkg/B.lean")),
        "{page:.400}"
    );

    // A remote whose /blob/ shape is not knowable stops, naming --source-url.
    let other = TempDir::new("build-git-other");
    let repo = other.path.join("repo");
    write_repo(&repo, &base_world());
    git_init(&repo, "https://gitlab.com/owner/repo.git");
    let refused = litedoc4(&[
        "build",
        "--root",
        &repo.display().to_string(),
        "--out",
        &other.path.join("out").display().to_string(),
        "--link-index",
        &live.lidx.display().to_string(),
        "--extractor",
        "/bin/sh",
    ]);
    assert_eq!(code(&refused), 3, "{}", stdout(&refused));
    let message = stderr(&refused);
    assert!(message.contains("only github.com remotes"), "{message}");
    assert!(message.contains("--source-url"), "{message}");
}

// ---------------------------------------------------------------- the refusals

/// `--out` is the directory this command owns, and it will not take over one it
/// cannot see it wrote — because a full generation removes the site tree.
#[test]
fn a_directory_this_command_did_not_write_is_refused() {
    let live = Live::new("build-not-ours");
    write(&live.out.join("important.txt"), b"somebody's work\n");

    let output = live.build(&[]);
    assert_eq!(code(&output), 3, "{}", stdout(&output));
    let message = stderr(&output);
    assert!(message.contains("litedoc4-build.json"), "{message}");
    assert!(
        fs::read(live.out.join("important.txt")).is_ok(),
        "the refusal deleted the file it refused to overwrite",
    );

    // **`--full` does not get past it either**, and that is the ordering that
    // matters: a full generation is the path that *deletes* <out>/site and
    // <out>/ir, so a `--full` answered before the marker was read would be the
    // one way to remove a directory this command never checked it owns.
    write(&live.out.join("site/index.html"), b"somebody's site\n");
    let forced = live.build(&["--full"]);
    assert_eq!(code(&forced), 3, "{}", stdout(&forced));
    assert!(
        stderr(&forced).contains("litedoc4-build.json"),
        "{}",
        stderr(&forced)
    );
    assert!(
        live.out.join("site/index.html").is_file(),
        "--full deleted a site directory whose marker was never read",
    );

    // A marker naming another package is refused too: the ledger under it stores
    // the target whose oleans it hashed.
    let live = Live::new("build-other-root");
    assert_eq!(code(&live.build(&[])), 0);
    let elsewhere = live.trees.path.join("elsewhere");
    write_repo(&elsewhere, &base_world());
    let output = litedoc4(&[
        "build",
        "--root",
        &elsewhere.display().to_string(),
        "--out",
        &live.out.display().to_string(),
        "--link-index",
        &live.lidx.display().to_string(),
        "--source-url",
        URL,
        "--extractor",
        "/bin/sh",
    ]);
    assert_eq!(code(&output), 3, "{}", stdout(&output));
    assert!(
        stderr(&output).contains("was built from"),
        "{}",
        stderr(&output)
    );
}

/// The package being documented is opened read-only, and the guard is stated
/// before anything is written rather than one stage later in `extract`.
#[test]
fn an_out_inside_the_root_is_refused() {
    let live = Live::new("build-out-inside");
    let inside = live.repo.join(".lake/build/doc");
    let output = litedoc4(&[
        "build",
        "--root",
        &live.repo.display().to_string(),
        "--out",
        &inside.display().to_string(),
        "--link-index",
        &live.lidx.display().to_string(),
        "--source-url",
        URL,
        "--extractor",
        "/bin/sh",
    ]);
    assert_eq!(code(&output), 3, "{}", stdout(&output));
    let message = stderr(&output);
    assert!(message.contains("is inside --root"), "{message}");
    assert!(!inside.exists(), "the refused directory was created anyway");
}

/// Every flag that names a decision `build` has taken over is refused **by
/// name**, with the decision as the reason. "unknown argument" would send the
/// caller looking for a typo.
#[test]
fn the_command_line_is_checked() {
    let live = Live::new("build-cli");
    let repo = live.repo.display().to_string();
    let out = live.out.display().to_string();
    let lidx = live.lidx.display().to_string();

    let cases: [(&[&str], i32, &str); 12] = [
        (&["build"], 2, "--root <repo> is required"),
        (&["build", "--root", &repo], 2, "--out <dir> is required"),
        // M5-b: `--link-index` is optional now — left out, the map is
        // <out>/link-index.lidx and the resident extractor writes it. The one
        // shape that cannot work is a `--extractor <program>`, whose interface
        // has no room to ask for one.
        (
            &[
                "build",
                "--root",
                &repo,
                "--out",
                &out,
                "--extractor",
                "/bin/sh",
            ],
            2,
            "--extractor <program> needs --link-index <file>",
        ),
        (
            &[
                "build",
                "--root",
                &repo,
                "--out",
                &out,
                "--link-index",
                &lidx,
                "--ir",
                "x",
            ],
            2,
            "owns the layout under --out",
        ),
        (
            &[
                "build",
                "--root",
                &repo,
                "--out",
                &out,
                "--link-index",
                &lidx,
                "--modules",
                "x",
            ],
            2,
            "the same list has to reach detect",
        ),
        (
            &[
                "build",
                "--root",
                &repo,
                "--out",
                &out,
                "--link-index",
                &lidx,
                "--target",
                "x",
            ],
            2,
            "the package being documented is --root",
        ),
        (
            &["build", "--root", &repo, "--out", &out, "--no-link-index"],
            2,
            // The refusal's own words, not the usage text's: this assertion used
            // to be satisfied by a line of `USAGE` that happened to carry the
            // same phrase, so editing the help text broke a test about a
            // refusal (M5-b).
            "150 of the target package's 432 pages",
        ),
        (
            &[
                "build",
                "--root",
                &repo,
                "--out",
                &out,
                "--link-index",
                &lidx,
                "--serve",
            ],
            2,
            "this command *is* the resident path",
        ),
        (
            &[
                "build",
                "--root",
                &repo,
                "--out",
                &out,
                "--link-index",
                &lidx,
                "--mode",
                "nonsens",
                "--extractor",
                "/bin/sh",
            ],
            2,
            "--mode takes self|referrers|importers|all",
        ),
        // A-1. The resolved map is this command's *output*, so naming one as an
        // input would render against somebody else's answer while recording the
        // digest of this run's.
        (
            &[
                "build",
                "--root",
                &repo,
                "--out",
                &out,
                "--link-index",
                &lidx,
                "--deps-docs-map",
                "x",
            ],
            2,
            "resolves the documentation map itself",
        ),
        (
            &[
                "build",
                "--root",
                &repo,
                "--out",
                &out,
                "--link-index",
                &lidx,
                "--deps-docs-url",
                "https://docs.invalid",
            ],
            2,
            "--deps-docs-url wants <Root>=<value>",
        ),
        // The root has to be a dependency this package resolves — exit 3, not
        // 2: the command line is well formed and the world disagrees with it.
        (
            &[
                "build",
                "--root",
                &repo,
                "--out",
                &out,
                "--link-index",
                &lidx,
                "--deps-docs-url",
                "Nope=https://docs.invalid",
            ],
            3,
            "is not a module root of any dependency",
        ),
    ];
    for (args, expected, message) in cases {
        let output = litedoc4(args);
        assert_eq!(code(&output), expected, "{args:?}: {}", stdout(&output));
        assert!(
            stderr(&output).contains(message),
            "{args:?}: {}",
            stderr(&output),
        );
    }
    assert!(
        !live.out.exists(),
        "a refused command line created the output directory",
    );
}

/// `--timings` is one JSON line, and it says which of the two paths ran.
#[test]
fn the_timings_record_names_the_path() {
    let live = Live::new("build-timings");
    let timings = live.trees.path.join("nested/build-timings.json");
    let path = timings.display().to_string();

    assert_eq!(code(&live.build(&["--timings", &path])), 0);
    let record: Value =
        serde_json::from_str(&fs::read_to_string(&timings).expect("the timings file was written"))
            .expect("one JSON object");
    assert_eq!(record["command"], json!("build"));
    assert_eq!(record["path"], json!("full"));
    assert_eq!(record["modules"], json!(3));
    assert_eq!(record["extracted"], json!(3));
    // 3 pages + 8 artifacts + 3 static assets: counted **after** the assets are
    // written, so the number is the tree that shipped and not a stage of it.
    assert_eq!(record["pagesInSite"], json!(14));
    assert_eq!(record["pagesRendered"], json!(3));

    assert_eq!(code(&live.build(&["--timings", &path])), 0);
    let record: Value =
        serde_json::from_str(&fs::read_to_string(&timings).expect("rewritten")).expect("JSON");
    assert_eq!(record["path"], json!("incremental"));
    assert_eq!(record["extracted"], json!(0));
    assert_eq!(record["pagesRendered"], json!(0));
    assert_eq!(record["pagesInSite"], json!(14));
    for phase in [
        "extractSeconds",
        "renderSeconds",
        "globalSeconds",
        "totalSeconds",
    ] {
        assert!(record[phase].is_number(), "{phase}: {record}");
    }
}

/// The marker's `work` record — **the performance gate this project could not
/// otherwise have.**
///
/// Nothing here can be judged by a clock: the oleans are `mmap`ed, so the same
/// unchanged run's environment load moves by 5x with the page cache 【実測】. So
/// the gate is over deterministic integers instead, and this test pins the two
/// shapes that matter — a first run does all the work, a second run over a world
/// that did not move does **none** of it. `tools/e2e-micro.sh`'s GATE 5 asserts
/// the same thing through a real Lean extractor; this asserts it in `cargo test`,
/// where it actually runs on every change.
///
/// `extractorRequests` is cross-checked against the fixture's own tally of how
/// often it was called, which is the point of the number: it is the one counter
/// whose zero says Lean was never started.
#[test]
fn the_marker_records_the_work() {
    let live = Live::new("build-work");

    assert_eq!(code(&live.build(&[])), 0);
    let full = work(&live);
    assert_eq!(full["modulesExtracted"], json!(3));
    assert_eq!(full["pagesRendered"], json!(3));
    assert_eq!(full["extractorRequests"], json!(1));
    assert_eq!(full["extractorRequests"], json!(live.extractions().len()));
    // Nothing was cached before the first run, so every module is a miss.
    assert_eq!(full["globalCacheHits"], json!(0));
    assert_eq!(full["globalCacheMisses"], json!(3));
    // Two whole passes over the module files: the renderer's and the
    // whole-package derivation's (approach.md §5.6's unit). Pinned rather than
    // bounded — a change to it is a change to what the pipeline does, and this
    // is where that has to be noticed.
    assert_eq!(full["irReads"]["module"], json!(2 * 3));

    let before = live.extractions().len();
    assert_eq!(code(&live.build(&[])), 0);
    let incremental = work(&live);
    assert_eq!(incremental["modulesExtracted"], json!(0));
    assert_eq!(incremental["pagesRendered"], json!(0));
    assert_eq!(incremental["extractorRequests"], json!(0));
    assert_eq!(
        live.extractions().len(),
        before,
        "the second run started the extractor",
    );
    assert_eq!(incremental["globalCacheHits"], json!(3));
    assert_eq!(incremental["globalCacheMisses"], json!(0));
    // One pass, and it is `impact`'s: the round loop never runs (nothing to
    // re-extract, nothing removed) and the renderer is skipped on an empty set.
    assert_eq!(incremental["irReads"]["module"], json!(3));
    assert_eq!(
        incremental["irReads"]["total"],
        json!(
            incremental["irReads"]["index"].as_u64().expect("a number")
                + incremental["irReads"]["module"].as_u64().expect("a number")
                + incremental["irReads"]["depMap"].as_u64().expect("a number")
        ),
    );
}

/// A run that dies leaves **`work: null`**, not a record of zeros.
///
/// Zeros are the exact shape a *successful* incremental run has, so a gate
/// reading a crashed run's marker would see "re-extracted nothing, rendered
/// nothing" and pass. `null` makes that read fail instead.
#[test]
fn an_unfinished_run_records_no_work() {
    let live = Live::new("build-work-unfinished");
    assert_eq!(code(&live.build(&["--extractor-arg", "--fail"])), 4);
    let marker = marker(&live);
    assert_eq!(marker["complete"], json!(false));
    assert_eq!(marker["work"], Value::Null);
}

// ------------------------------------------------------------------- plumbing

fn marker(live: &Live) -> Value {
    let path = live.out.join("litedoc4-build.json");
    serde_json::from_str(&fs::read_to_string(&path).expect("the marker was written"))
        .expect("one JSON object")
}

fn work(live: &Live) -> Value {
    let marker = marker(live);
    assert_eq!(marker["complete"], json!(true), "{marker}");
    marker["work"].clone()
}

fn litedoc4(args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .output()
        .expect("the binary under test runs")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn code(output: &Output) -> i32 {
    output
        .status
        .code()
        .unwrap_or_else(|| panic!("the process was killed by a signal: {}", stderr(output)))
}

/// A checkout with one commit and one remote, for the `--source-url` derivation.
fn git_init(repo: &Path, remote: &str) {
    let run = |args: &[&str]| {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .expect("git runs");
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr),
        );
    };
    run(&["init", "-q"]);
    run(&["config", "user.name", "litedoc4 tests"]);
    run(&["config", "user.email", "tests@example.invalid"]);
    run(&["remote", "add", "origin", remote]);
    run(&["add", "-A"]);
    run(&["commit", "-q", "-m", "the fixture"]);
}

/// **M5-b: the dependency map is in `renderKey`, and a map that moved
/// re-renders every page.**
///
/// M4-d left this as a named hole (plan §7): the key was `renderer` +
/// `sourceUrl`, so a run whose IR was unchanged and whose map was not went
/// undetected — and the map reaches 150 of the measurement target's 432 pages'
/// bytes 【実測, plan 決定 4】. `--full` was the escape hatch. Since M5-a the
/// product derives the map, so it has an identity worth recording, and the
/// identity is the file's SHA-256.
#[test]
fn a_moved_dependency_map_re_renders_every_page() {
    let live = Live::new("build-link-index-key");

    let first = live.build(&[]);
    assert_eq!(code(&first), 0, "{}", stderr(&first));
    let digest = sha256_of(&live.lidx);
    assert_eq!(
        live.ledger()["renderKey"]["linkIndex"],
        json!(digest),
        "the ledger records the map the pages were rendered against",
    );
    let before = tree(&live.site());

    // The same package, the same IR, a different map. Nothing else moves.
    write(
        &live.lidx,
        b"#lidx1\n@Dep.Home\nDep.Home\n\tDep.elsewhere\n\tDep.Home.other\n\tDep.Home.third\n",
    );
    let second = live.build(&[]);
    assert_eq!(code(&second), 0, "{}", stderr(&second));
    let log = stdout(&second);
    assert!(log.contains("0 to re-extract"), "{log}");
    assert!(
        log.contains("render key moved (linkIndex)"),
        "detect has to name the key that moved: {log}",
    );
    assert!(
        log.contains("impact  mode all"),
        "a moved render key overrides --mode (plan §6, constraint 4): {log}",
    );
    assert_eq!(
        live.extractions(),
        vec![3],
        "a moved map re-extracts nothing: it cannot change the IR",
    );
    assert_eq!(
        live.ledger()["renderKey"]["linkIndex"],
        json!(sha256_of(&live.lidx)),
        "and the new map is what the next run compares against",
    );

    // Three module pages were rewritten; this fixture's map reaches none of
    // their bytes, so the tree is the same tree. **The gate is the decision,
    // not the diff** — what M4-d could not do was notice.
    assert_eq!(
        before.keys().collect::<Vec<_>>(),
        tree(&live.site()).keys().collect::<Vec<_>>()
    );

    // A third run with the map put back where it was is a change again, in the
    // other direction: `KeySet::diff` is a union, so there is no "restored"
    // state that compares equal to the wrong thing.
    write_lidx(&live.lidx);
    let third = live.build(&[]);
    assert_eq!(code(&third), 0, "{}", stderr(&third));
    assert!(
        stdout(&third).contains("render key moved (linkIndex)"),
        "{}",
        stdout(&third),
    );
    assert_eq!(live.ledger()["renderKey"]["linkIndex"], json!(digest));
}

/// A map that is **gone** is answered with a full generation, not with a
/// refusal and not with a subset render (M5-b).
///
/// An incremental run renders a subset, so a round that could not read the map
/// would leave pages whose links are missing mixed into a tree of pages that
/// still have theirs — a site that is wrong in a way no count reports. A full
/// generation writes every page, so it is the answer that cannot be half-right.
#[test]
fn a_missing_dependency_map_forces_a_full_generation() {
    let live = Live::new("build-link-index-gone");
    assert_eq!(code(&live.build(&[])), 0);
    assert_eq!(live.extractions(), vec![3]);

    fs::remove_file(&live.lidx).expect("the map was there");
    let again = live.build(&[]);
    // The renderer still needs the file, so this run does not succeed — but it
    // fails having chosen to regenerate everything, which is the decision under
    // test. A run that had chosen `incremental` would have rendered a subset
    // and reported success.
    let log = stdout(&again);
    assert!(
        log.contains("plan    full generation (the previous run's files are not all there)"),
        "{log}",
    );

    // Put it back and the next run continues incrementally again.
    write_lidx(&live.lidx);
    let restored = live.build(&[]);
    assert_eq!(code(&restored), 0, "{}", stderr(&restored));
    assert!(
        stdout(&restored).contains("plan    full generation"),
        "{}",
        stdout(&restored)
    );
}

/// SHA-256 of a file, lower-case hex — the same value `renderKey.linkIndex`
/// carries.
fn sha256_of(path: &Path) -> String {
    litedoc4_incr::sha256_hex(&fs::read(path).expect("the file is readable"))
}

fn git_head(repo: &Path) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("git runs");
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// Every file under `root`, keyed by its path relative to it.
fn tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut stack = vec![root.to_owned()];
    while let Some(dir) = stack.pop() {
        let Ok(listing) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in listing {
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

fn write(path: &Path, body: &[u8]) {
    fs::create_dir_all(path.parent().expect("a parent")).expect("writable");
    fs::write(path, body).expect("writable");
}

/// A directory that removes itself.
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
            "litedoc4-build-{}-{}-{slug}",
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
