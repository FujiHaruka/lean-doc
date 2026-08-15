//! Milestone **M4-b**: `lean-doc extract`.
//!
//! The real oracle for this subcommand is a comparison with the frozen
//! prototype: `stage7g/extract-once.sh` and `lean-doc extract` over the same 432
//! modules write **436/436 identical IR files** and two timings records with the
//! same 91 keys and the same 59 non-duration values 【実測 2026-08-15】. That
//! comparison needs a Lean toolchain, a built target package and 20 seconds, so
//! it lives in the gate and not here.
//!
//! What is here is the part of this subcommand that is **not** the extraction:
//! the command line it builds, the directory it refuses, and the fold from the
//! events JSONL into one JSON object. All of it is exercised against a fake
//! `lake` and a fake extractor — two shell scripts — for the same reason
//! `tests/incremental.rs` fakes the extractor: without the seam none of these
//! assertions would exist at all.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lean-doc");

/// The events three phases of a real run emit, shortened. Two properties are
/// load-bearing and both come from a real `*-events.jsonl`: the `stage4b.`
/// prefix is on every phase, and the extra keys are **typed** — numbers,
/// booleans and strings in the same file.
const EVENTS: &str = r#"{"phase":"stage4b.initSearchPath","pid":7,"us":34}
{"phase":"stage4b.importModules","pid":7,"us":2498376,"directImports":432,"resident":0}

{"phase":"stage4b.writeIR","pid":7,"us":123456,"taggedCode":"true","moduleFiles":2,"ok":true}
"#;

#[test]
fn the_events_become_one_timings_object() {
    let world = World::new("fold");
    let output = world.run(&["--jobs", "3"]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));

    let record = world.timings();
    // A phase's own `us`, in seconds, under the name with `stage4b.` gone.
    assert_eq!(record["initSearchPath"], serde_json::json!(0.000_034));
    assert_eq!(record["importModules"], serde_json::json!(2.498_376));
    assert_eq!(record["writeIR"], serde_json::json!(0.123_456));
    // Every other key of the event, `pid` excepted, with its JSON type intact.
    assert_eq!(
        record["importModules:directImports"],
        serde_json::json!(432)
    );
    assert_eq!(record["importModules:resident"], serde_json::json!(0));
    assert_eq!(record["writeIR:taggedCode"], serde_json::json!("true"));
    assert_eq!(record["writeIR:moduleFiles"], serde_json::json!(2));
    assert_eq!(record["writeIR:ok"], serde_json::json!(true));
    assert!(
        record.get("initSearchPath:pid").is_none(),
        "pid identifies the process, not the measurement: {record}"
    );
    // The two the wrapper adds. `targetModules` counts the list the way the
    // prototype counts it: blank lines and `#` comments are not modules.
    assert_eq!(record["targetModules"], serde_json::json!(2));
    assert_eq!(record["jobsRequested"], serde_json::json!(3));
    assert_eq!(
        record.as_object().expect("an object").len(),
        10,
        "nothing else is invented: {record}"
    );
}

#[test]
fn the_extractor_is_run_inside_the_target_with_the_schema_4_flags() {
    let world = World::new("command-line");
    let output = world.run(&["--jobs", "4"]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));

    let argv = world.argv();
    assert_eq!(
        argv,
        vec![
            world.modules.display().to_string(),
            world.events().display().to_string(),
            "--equations".to_owned(),
            "--refs".to_owned(),
            "--write-ir".to_owned(),
            "--tagged-code".to_owned(),
            "--jobs".to_owned(),
            "4".to_owned(),
            "--ir-dir".to_owned(),
            world.ir_dir.display().to_string(),
        ],
        "the order is `extract-once.sh:63-64`'s",
    );
    assert_eq!(
        fs::canonicalize(
            fs::read_to_string(world.root.join("cwd"))
                .expect("recorded")
                .trim()
        )
        .expect("a real directory"),
        fs::canonicalize(&world.target).expect("a real directory"),
        "`lake env` has to run inside the package being documented",
    );
    assert!(
        world.ir_dir.is_dir(),
        "the IR directory is created before the extractor is started",
    );
}

#[test]
fn the_events_file_defaults_beside_the_timings_and_is_never_appended_to() {
    let world = World::new("stale-events");
    // What an earlier round left behind. `incremental` passes only `--timings`,
    // so this is the file the default names — and a fold that appended would
    // report a phase this run never ran.
    fs::write(
        world.events(),
        "{\"phase\":\"stage4b.ghost\",\"pid\":1,\"us\":9}\n",
    )
    .expect("writable");
    let output = world.run(&[]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    let record = world.timings();
    assert!(
        record.get("ghost").is_none(),
        "a stale events file is removed, not folded in: {record}"
    );
    assert_eq!(record["jobsRequested"], serde_json::json!(1), "the default");
}

#[test]
fn an_ir_dir_inside_the_target_is_refused() {
    let world = World::new("inside-target");
    let inside = world.target.join("build").join("ir");
    let output = world.run(&["--ir-dir", &inside.display().to_string()]);
    assert_eq!(code(&output), 3, "{}", stderr(&output));
    assert!(
        stderr(&output).contains("read-only"),
        "the message says why: {}",
        stderr(&output)
    );
    assert!(
        !inside.exists(),
        "the refusal comes before the directory is created — creating it is \
         already a write into the target",
    );
    assert!(
        !world.root.join("argv").exists(),
        "and before the extractor is started",
    );
}

/// A relative path on the child's command line resolves against the **target**,
/// because that is the child's working directory 【実測 2026-08-15, M4-c】.
///
/// The failure this prevents is the worst one this project has: `--ir-dir out`
/// passes the guard above — it resolves against *this* process's directory,
/// which is not under the target — and then the extractor writes several MB into
/// `<target>/out`, which is a write into the measurement target arriving through
/// the one command whose heading says it never happens. The mirror-image case is
/// loud rather than silent (`--modules list.txt` makes the extractor exit 1 with
/// "no such file or directory"), and that is how this was found.
#[test]
fn every_path_reaches_the_child_absolute_because_its_directory_is_the_target() {
    let world = World::new("relative-paths");
    // Run from inside the fixture with every path spelled relative to it — a
    // caller in their own project directory, in other words.
    let output = Command::new(BIN)
        .current_dir(&world.root)
        .args([
            "extract",
            "--modules",
            "modules.txt",
            "--ir-dir",
            "ir",
            "--timings",
            "timings.json",
            "--extractor-bin",
            "extract",
            "--target",
            "target-repo",
            // Absolute, and the only one: `--lake` is a name looked up on PATH,
            // so it is the one path this rule does not apply to.
            "--lake",
            &world.lake.display().to_string(),
        ])
        .output()
        .expect("the binary under test runs");
    assert_eq!(code(&output), 0, "{}", stderr(&output));

    let argv = world.argv();
    for (what, path) in [
        ("--modules", PathBuf::from(&argv[0])),
        ("--events", PathBuf::from(&argv[1])),
        ("--ir-dir", PathBuf::from(argv.last().expect("--ir-dir"))),
    ] {
        assert!(path.is_absolute(), "{what} reached the child as {path:?}");
        assert!(
            !path.starts_with(fs::canonicalize(&world.target).expect("a real directory")),
            "{what} would have been written inside the target: {path:?}",
        );
    }
    assert!(
        !world.target.join("ir").exists(),
        "and nothing was written into the target",
    );
    assert!(
        world.root.join("ir").is_dir(),
        "the tree is where it was asked for"
    );
}

#[test]
fn a_failing_extractor_is_exit_4_and_says_the_tree_is_incomplete() {
    let world = World::new("extractor-fails");
    fs::write(
        &world.extractor,
        "#!/bin/sh\necho 'unknown constant' >&2\nexit 1\n",
    )
    .expect("writable");
    make_executable(&world.extractor);
    let output = world.run(&[]);
    assert_eq!(code(&output), 4, "{}", stderr(&output));
    assert!(
        stderr(&output).contains("incomplete"),
        "a partial IR tree is the thing a caller must not merge: {}",
        stderr(&output)
    );
    assert!(
        !world.timings_path.exists(),
        "and no timings record is written for a run that did not happen",
    );
}

/// Every flag refused by name, and the word each refusal has to carry.
///
/// Refusing by name rather than as "unknown argument" is this CLI's rule
/// (`pipeline.rs`, `main.rs`): each of these *is* a real flag of the program
/// behind this one, so the answer a caller needs is why it is not offered.
#[test]
fn the_flags_that_are_not_offered_are_refused_by_name() {
    for (flag, word) in [
        ("--serve", "incremental --serve"),
        ("--serve-dir", "incremental --serve"),
        ("--serve-from", "incremental --serve"),
        ("--write-ir", "always on"),
        ("--tagged-code", "always on"),
        ("--equations", "always on"),
        ("--refs", "always on"),
        ("--no-attrs", "ablation"),
        ("--no-inst-index", "ablation"),
        ("--no-member-extra", "ablation"),
        ("--pp-breakdown", "measurement"),
        ("--decl-profile", "measurement"),
        ("--only", "measurement"),
    ] {
        let world = World::new(&format!("refuse{flag}"));
        let output = world.run(&[flag, "x"]);
        assert_eq!(code(&output), 2, "{flag}: {}", stderr(&output));
        let said = message(&output);
        assert!(said.contains(flag), "{flag} is named: {said}");
        assert!(said.contains(word), "{flag} says `{word}`: {said}");
        assert!(
            !said.contains("unknown argument"),
            "{flag} is not a typo: {said}"
        );
    }
}

#[test]
fn the_paths_with_no_default_are_required() {
    let world = World::new("required");
    for (missing, word) in [
        ("--modules", "one name per line"),
        ("--ir-dir", "no default"),
        ("--timings", "phase timers"),
        ("--extractor-bin", "no default"),
        ("--target", "lake env"),
    ] {
        let output = world.run_without(missing);
        assert_eq!(code(&output), 2, "{missing}: {}", stderr(&output));
        // The *message*, not the usage text printed under it: every one of
        // these words also appears in `USAGE`, so a test that read the whole of
        // stderr would pass whatever the refusal said.
        let said = message(&output);
        assert!(said.contains(missing), "{missing} is named: {said}");
        assert!(said.contains(word), "{missing} says `{word}`: {said}");
    }
}

// ------------------------------------------------------------------- plumbing

/// A fake target package, a fake `lake` and a fake extractor.
struct World {
    root: PathBuf,
    target: PathBuf,
    modules: PathBuf,
    ir_dir: PathBuf,
    timings_path: PathBuf,
    extractor: PathBuf,
    lake: PathBuf,
    _guard: TempDir,
}

impl World {
    fn new(what: &str) -> Self {
        let guard = TempDir::new(what);
        let root = guard.path.clone();
        let target = root.join("target-repo");
        fs::create_dir_all(&target).expect("writable");

        let modules = root.join("modules.txt");
        // A blank line and a comment, because `targetModules` counts neither.
        fs::write(&modules, "A.One\n\n# a comment\nA.Two\n").expect("writable");

        // `lake env <program> <args…>` — the one thing about `lake` this
        // subcommand relies on.
        let lake = root.join("lake");
        fs::write(
            &lake,
            "#!/bin/sh\n[ \"$1\" = env ] || { echo \"expected env, got $1\" >&2; exit 9; }\n\
             shift\nexec \"$@\"\n",
        )
        .expect("writable");
        make_executable(&lake);

        // The extractor: records its argv and its working directory, writes the
        // events its caller is about to fold, and prints to stdout (which is
        // sent to /dev/null, as the prototype sends it).
        let extractor = root.join("extract");
        fs::write(
            &extractor,
            format!(
                "#!/bin/sh\n\
                 : > {argv}\n\
                 for a in \"$@\"; do printf '%s\\n' \"$a\" >> {argv}; done\n\
                 pwd -P > {cwd}\n\
                 cat > \"$2\" <<'JSONL'\n{EVENTS}JSONL\n\
                 echo 'phase report nobody reads'\n",
                argv = shell_quote(&root.join("argv")),
                cwd = shell_quote(&root.join("cwd")),
            ),
        )
        .expect("writable");
        make_executable(&extractor);

        Self {
            target,
            modules,
            ir_dir: root.join("ir"),
            timings_path: root.join("timings.json"),
            extractor,
            lake,
            root,
            _guard: guard,
        }
    }

    /// Where `--events` lands when nobody passes it (`extract-once.sh:52`).
    fn events(&self) -> PathBuf {
        self.root.join("timings-events.jsonl")
    }

    fn run(&self, extra: &[&str]) -> Output {
        let mut args = self.base_args();
        args.extend(extra.iter().map(|arg| (*arg).to_owned()));
        lean_doc(&args)
    }

    /// The same command line with one flag and its value dropped.
    fn run_without(&self, flag: &str) -> Output {
        let base = self.base_args();
        let mut args: Vec<String> = Vec::new();
        let mut skip = false;
        for arg in base {
            if skip {
                skip = false;
                continue;
            }
            if arg == flag {
                skip = true;
                continue;
            }
            args.push(arg);
        }
        lean_doc(&args)
    }

    fn base_args(&self) -> Vec<String> {
        [
            "extract",
            "--modules",
            &self.modules.display().to_string(),
            "--ir-dir",
            &self.ir_dir.display().to_string(),
            "--timings",
            &self.timings_path.display().to_string(),
            "--extractor-bin",
            &self.extractor.display().to_string(),
            "--target",
            &self.target.display().to_string(),
            "--lake",
            &self.lake.display().to_string(),
        ]
        .iter()
        .map(|arg| (*arg).to_owned())
        .collect()
    }

    fn timings(&self) -> serde_json::Value {
        let text = fs::read_to_string(&self.timings_path).expect("a timings record");
        assert!(
            !text.ends_with('\n'),
            "no trailing newline, as `json.dump` writes none",
        );
        serde_json::from_str(&text).expect("valid JSON")
    }

    fn argv(&self) -> Vec<String> {
        fs::read_to_string(self.root.join("argv"))
            .expect("the extractor recorded its arguments")
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

fn lean_doc(args: &[String]) -> Output {
    Command::new(BIN)
        .args(args)
        .output()
        .expect("the binary under test runs")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// The refusal itself. `main` prints `lean-doc: <message>\n\n<USAGE>`, and the
/// usage text names every flag — so an assertion over the whole of stderr is an
/// assertion about `USAGE` wearing a refusal's name.
fn message(output: &Output) -> String {
    let said = stderr(output);
    said.split("\n\n").next().unwrap_or_default().to_owned()
}

fn code(output: &Output) -> i32 {
    output
        .status
        .code()
        .unwrap_or_else(|| panic!("the process was killed by a signal: {}", stderr(output)))
}

fn make_executable(path: &Path) {
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("chmod");
}

/// The temporary paths hold a process id and a counter, never a quote — but a
/// path that reaches a shell script is quoted anyway, so the fixture cannot be
/// the thing that breaks.
fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "'\\''"))
}

/// A directory that removes itself, as in `tests/incremental.rs`.
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
            "lean-doc-extract-{}-{}-{slug}",
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
