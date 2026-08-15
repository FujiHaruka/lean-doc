//! Milestone **M4-c**: the command-line surface of `lean-doc incremental
//! --serve`.
//!
//! Three layers judge the resident extractor and this is the outermost of them:
//!
//! - **The gate** — the resident path and the one-shot path write the same 436 IR
//!   files over the same module list, and the M3-d3 harness gets the same 3,213
//!   answers through either. That needs a Lean toolchain, a built package and a
//!   3 GB process, so it is a gate and not a test.
//! - **`src/resident.rs`'s own tests** — the protocol, the olean-generation
//!   guard and the teardown, against a fake server. That is where "a failure does
//!   not leak a 3 GB process" is asserted rather than observed once by hand.
//! - **Here** — which flag means what, and which combinations are refused before
//!   anything is started. Every case below ends before a process exists, which is
//!   the property being tested: **a command line that is wrong costs nothing**.
//!
//! The one rule the whole surface follows: `--serve` and `--extractor` are two
//! answers to the same question, so every flag belongs to exactly one of them and
//! saying so is the refusal's job.

#![cfg(unix)]

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lean-doc");

/// 40 lower-case hex digits after `/blob/`, or `incremental` refuses the URL
/// before it ever looks at these flags (plan 決定 1).
const URL: &str = "https://example.invalid/o/r/blob/0123456789abcdef0123456789abcdef01234567";

#[test]
fn serve_and_extractor_are_two_answers_to_the_same_question() {
    let world = World::new("exclusive");
    let refusal = world.run(&["--extractor", "/bin/true", "--serve"]);
    assert_eq!(code(&refusal), 2, "{}", stderr(&refusal));
    assert!(
        message(&refusal).contains("exclusive"),
        "{}",
        message(&refusal)
    );
    // And with neither, the refusal has to name both — otherwise it reads as
    // "--extractor is required", which after M4-c is only half true.
    let refusal = world.run(&[]);
    assert_eq!(code(&refusal), 2, "{}", stderr(&refusal));
    let said = message(&refusal);
    assert!(said.contains("--extractor"), "{said}");
    assert!(said.contains("--serve"), "{said}");
}

#[test]
fn the_servers_own_flags_are_refused_without_it() {
    let world = World::new("serve-only-flags");
    // Each is a real flag of `lean-doc extract`, so a caller who tries it here
    // has to hear which command owns it rather than "unknown argument".
    for (flag, value) in [
        ("--extractor-bin", "/tmp/extract"),
        ("--target", "/tmp/repo"),
        ("--lake", "/tmp/lake"),
    ] {
        let refusal = world.run(&["--extractor", "/bin/true", flag, value]);
        assert_eq!(code(&refusal), 2, "{flag}: {}", stderr(&refusal));
        let said = message(&refusal);
        assert!(said.contains("a flag of --serve"), "{flag}: {said}");
    }
    // `--jobs` is the same rule with the reason plan §6 constraint 6 gives: the
    // resident server's job count is its start-up `cfg`, so it is this command's
    // to choose exactly when this command starts the server.
    let refusal = world.run(&["--extractor", "/bin/true", "--jobs", "4"]);
    assert_eq!(code(&refusal), 2, "{}", stderr(&refusal));
    let said = message(&refusal);
    assert!(said.contains("--extractor-arg --jobs"), "{said}");
    assert!(said.contains("constraint 6"), "{said}");
}

#[test]
fn the_two_paths_with_no_default_are_required_by_serve() {
    let world = World::new("serve-required");
    // No `--extractor-bin`, and `EXTRACT_BIN` unset.
    let refusal = world.run(&["--serve", "--target", "/tmp/repo"]);
    assert_eq!(code(&refusal), 2, "{}", stderr(&refusal));
    let said = message(&refusal);
    assert!(said.contains("EXTRACT_BIN"), "{said}");
    assert!(
        said.contains("171 MB"),
        "the reason there is no default: {said}"
    );

    // No `--target`, and `TARGET_REPO` unset.
    let refusal = world.run(&["--serve", "--extractor-bin", "/tmp/extract"]);
    assert_eq!(code(&refusal), 2, "{}", stderr(&refusal));
    let said = message(&refusal);
    assert!(said.contains("TARGET_REPO"), "{said}");
    assert!(
        said.contains("generation"),
        "why the target matters: {said}"
    );
}

#[test]
fn the_two_paths_may_come_from_the_environment_instead() {
    let world = World::new("serve-env");
    // The prototype's variable names (`serve-ctl.sh:48-50`), so a shell that
    // already exports them keeps working. Reaching the *next* refusal — the
    // target does not exist — is the proof both were picked up.
    let refusal = world.run_with_env(
        &["--serve"],
        &[
            ("EXTRACT_BIN", "/tmp/extract"),
            ("TARGET_REPO", "/tmp/no-such-repository"),
        ],
    );
    assert_eq!(code(&refusal), 3, "{}", stderr(&refusal));
    assert!(
        message(&refusal).contains("--target /tmp/no-such-repository"),
        "{}",
        message(&refusal),
    );
}

#[test]
fn an_empty_environment_variable_is_not_a_value() {
    let world = World::new("serve-empty-env");
    // `TARGET_REPO=` in a wrapper script is how a shell spells "I did not set
    // this"; taking it literally would make the target the filesystem root.
    let refusal = world.run_with_env(
        &["--serve", "--extractor-bin", "/tmp/extract"],
        &[("TARGET_REPO", "")],
    );
    assert_eq!(code(&refusal), 2, "{}", stderr(&refusal));
    assert!(
        message(&refusal).contains("TARGET_REPO"),
        "{}",
        message(&refusal)
    );
}

#[test]
fn a_job_count_of_zero_is_refused() {
    let world = World::new("serve-zero-jobs");
    let refusal = world.run(&[
        "--serve",
        "--extractor-bin",
        "/tmp/extract",
        "--target",
        "/tmp/repo",
        "--jobs",
        "0",
    ]);
    assert_eq!(code(&refusal), 2, "{}", stderr(&refusal));
    assert!(
        message(&refusal).contains("at least 1"),
        "{}",
        message(&refusal)
    );
    let refusal = world.run(&["--serve", "--jobs", "many"]);
    assert_eq!(code(&refusal), 2, "{}", stderr(&refusal));
    assert!(
        message(&refusal).contains("not many"),
        "{}",
        message(&refusal)
    );
}

#[test]
fn the_two_flags_that_stayed_retired_say_what_replaced_them() {
    let world = World::new("retired");
    // `--serve-dir` and `--serve-from` are the prototype's two spellings for a
    // server the caller owns. Both are refused, and the reason is the same
    // measurement: correctness comes from the server's olean generation and never
    // from the round number 【実測, stage 6a】.
    let refusal = world.run(&["--extractor", "/bin/true", "--serve-dir", "/tmp/server"]);
    assert_eq!(code(&refusal), 2, "{}", stderr(&refusal));
    let said = message(&refusal);
    assert!(said.contains("stage 6a"), "{said}");
    assert!(said.contains("generation"), "{said}");

    let refusal = world.run(&["--extractor", "/bin/true", "--serve-from", "2"]);
    assert_eq!(code(&refusal), 2, "{}", stderr(&refusal));
    assert!(
        message(&refusal).contains("round number"),
        "{}",
        message(&refusal)
    );
}

#[test]
fn extract_still_refuses_every_serve_spelling_and_now_says_why() {
    // M4-b refused these three with "M4-c will bring them". M4-c brought them
    // somewhere else, so the refusal has to say where and why rather than name a
    // milestone that has passed.
    for flag in ["--serve", "--serve-dir", "--serve-from"] {
        let refusal = lean_doc(&["extract".to_owned(), flag.to_owned()], &[]);
        assert_eq!(code(&refusal), 2, "{flag}: {}", stderr(&refusal));
        let said = message(&refusal);
        assert!(said.contains("incremental --serve"), "{flag}: {said}");
        assert!(
            said.contains("one-shot") || said.contains("one request"),
            "{flag}: {said}",
        );
    }
}

// ------------------------------------------------------------------- plumbing

/// A module list and nothing else: every case here is decided before the
/// pipeline opens an IR tree, which is the point of them.
struct World {
    root: PathBuf,
    modules: PathBuf,
}

impl World {
    fn new(what: &str) -> Self {
        use std::sync::atomic::{AtomicU32, Ordering};
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let root = std::env::temp_dir().join(format!(
            "lean-doc-resident-cli-{}-{}-{what}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("the temporary directory is creatable");
        let modules = root.join("modules.txt");
        fs::write(&modules, "Pkg.A\nPkg.B\n").expect("writable");
        Self { root, modules }
    }

    /// The eight flags every run needs, plus whatever the case adds.
    fn base(&self) -> Vec<String> {
        [
            "incremental",
            "--ir",
            "ir",
            "--pages",
            "pages",
            "--ledger",
            "ledger.json",
            "--work",
            "work",
            "--modules",
            &self.modules.display().to_string(),
            "--source-url",
            URL,
            "--link-index",
            "link-index.lidx",
            "--state",
            "state",
        ]
        .iter()
        .map(|arg| (*arg).to_owned())
        .collect()
    }

    /// Every run clears `EXTRACT_BIN`, `TARGET_REPO` and `LAKE` (see
    /// [`lean_doc`]), so a developer who exports one cannot make a "this is
    /// required" case pass for the wrong reason.
    fn run(&self, extra: &[&str]) -> Output {
        self.run_with_env(extra, &[])
    }

    fn run_with_env(&self, extra: &[&str], env: &[(&str, &str)]) -> Output {
        let mut args = self.base();
        args.extend(extra.iter().map(|arg| (*arg).to_owned()));
        lean_doc(&args, env)
    }
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Runs the binary with `EXTRACT_BIN`, `TARGET_REPO` and `LAKE` removed unless
/// the case sets them.
fn lean_doc(args: &[String], env: &[(&str, &str)]) -> Output {
    let mut command = Command::new(BIN);
    command.args(args);
    for name in ["EXTRACT_BIN", "TARGET_REPO", "LAKE"] {
        command.env_remove(name);
    }
    for (name, value) in env {
        command.env(name, value);
    }
    command.output().expect("the binary under test runs")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// The refusal itself. `main` prints `lean-doc: <message>\n\n<USAGE>` for a usage
/// error, and the usage text names every flag — so an assertion over the whole of
/// stderr would be an assertion about `USAGE` wearing a refusal's name.
fn message(output: &Output) -> String {
    stderr(output)
        .split("\n\n")
        .next()
        .unwrap_or_default()
        .to_owned()
}

fn code(output: &Output) -> i32 {
    output
        .status
        .code()
        .unwrap_or_else(|| panic!("the process was killed by a signal: {}", stderr(output)))
}
