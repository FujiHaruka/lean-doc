//! The resident extractor: **one Lean environment, many extractions**.
//!
//! Milestone **M4-c**. Ported from `experiments/stage7g/serve-ctl.sh` (185
//! lines, frozen) and the `--serve auto` half of `stage7h/incremental.sh`
//! (`incremental.sh:53-80, 175-195`).
//!
//! The environment load is the extraction's fixed cost and it is paid **per
//! process** (`Extract.lean:2700-2705`). L3-1 turns re-extraction into rounds, so
//! anything that runs the extractor once per round pays it once per round. The
//! server is the answer: import once, then answer requests over a pipe.
//!
//! ```text
//! lake env <extract> <modules.txt> <events.jsonl> --equations --refs
//!          --write-ir --tagged-code --jobs N --ir-dir <unused> --serve
//!
//!   stdin   <modules.txt> <events.jsonl> <ir-dir>      one request per line
//!   stdout  ready <ns> <envModules> <targets>          once, at start-up
//!           ok <exit code> <ns>                        one per request
//! ```
//!
//! # A pipe, not a FIFO and a holder process 【判断】
//!
//! The server's loop ends on EOF (`Extract.lean:2747`). In shell that is a
//! problem: a request that opens the FIFO, writes and closes it looks like EOF
//! and the server exits after the *first* one, so `serve-ctl.sh` has to park a
//! long-lived `sleep` on the FIFO as a holder and count `^ok ` lines by prefix
//! because separate invocations share no file descriptor (`serve-ctl.sh:17-30`).
//!
//! Here the driver is one process, so the request channel is simply the child's
//! **stdin pipe**, held open in [`Server::stdin`] for as long as the server is
//! wanted. That deletes the holder, the two pid files, the `mkfifo`, the polling
//! loop and the prefix counting — and it buys the property those pieces exist to
//! fake:
//!
//! > **A pipe cannot outlive its writer.** However this process dies — an error
//! > path, a panic, `SIGINT`, `SIGKILL` — the kernel closes the write end, the
//! > server reads EOF and returns 0 through its own exit path. The 3 GB process
//! > cannot be orphaned by a failure of the driver, because the thing that keeps
//! > it alive *is* the driver.
//!
//! The FIFO's holder is a separate `sleep` process and has no such property: kill
//! the driver and the holder keeps the server alive with nobody to stop it, which
//! is why the prototype needs `trap cleanup EXIT INT TERM` (`incremental.sh:186`)
//! and why that trap cannot cover `SIGKILL`.
//!
//! [`Drop`] is still implemented, and it is what makes the *ordinary* stop clean
//! rather than merely eventual: it closes the pipe, waits [`STOP_GRACE`] for the
//! server to leave through its own exit path, and only then signals. `main`
//! returns an [`std::process::ExitCode`] and this crate calls
//! `std::process::exit` nowhere, so every error path unwinds through here.
//!
//! # There is no `--serve-dir`: a server this process does not own 【判断】
//!
//! **Correctness comes from the server's olean generation, never from the round
//! number** 【実測, stage 6a】. A server imported *before* an edit answers with
//! the pre-edit owner of every name that moved — the exact stale pair L3-1 exists
//! to repair — and the site it produces is wrong; with such a server *no* round
//! is safe, including the round 2 that stage 5h had assumed was. So a server is
//! only usable by a caller who can show that its environment is the environment
//! the rest of the run is reasoning about.
//!
//! The prototype's `--serve-dir` hands that obligation to the caller and can only
//! read the server's self-reported `info.txt` (`incremental.sh:74-80`). The
//! product does not offer it. Residency is owned by the run: the pipeline starts
//! the server, and it starts it *inside* a run that begins after `lake build`, so
//! the shape stage 6a measured as wrong is not expressible.
//!
//! # The generation, which is checked and not assumed
//!
//! "Cannot be expressed" is still an argument, and this project's rule is that an
//! argument gets a measurement. [`Generation`] is the world the run is reasoning
//! about, taken as **Lake's own content hash of every olean of every module in
//! `--modules`** — the same files, in the same order, that the ledger hashes, so
//! the guard and `detect` cannot disagree about what "the world" is. It is taken
//! once at the head of the run and compared:
//!
//! * before every request — which, for the first one, is also the check before
//!   the server is spawned,
//! * again as soon as the server reports `ready`, closing the window in which the
//!   import itself runs,
//! * and after every request, so that the interval a request ran in is an
//!   interval the world provably did not move in.
//!
//! A difference stops the run with **exit 3** and names the modules that moved.
//! What that catches is the one way the pipeline can still be handed a stale
//! environment: a `lake build` that lands *while* a run is in flight. It costs
//! three `stat`s and one 64-byte read per module — 432 modules, so far below the
//! import it is guarding that measuring it would be measuring the filesystem.
//!
//! # `--jobs` is start-up configuration, not a request parameter
//!
//! `Extract.lean:2751` builds each request's `cfg` from the start-up one, so the
//! job count the server was started with applies to every request and there is no
//! per-request spelling (plan §6, constraint 6). That is why `--jobs` is a flag of
//! `incremental` only together with `--serve`: with `--extractor` it is the
//! extraction program's business and reaches it through `--extractor-arg`.
//!
//! # Two traps, both measured
//!
//! 1. **`--ir-dir` is required at start-up even though every request overrides
//!    it** 【実測 2026-08-15】. `serve-ctl.sh:120-121` starts the server without
//!    one, which worked against stage 7d's binary because it had a default. M4-a
//!    removed that default and refuses `--write-ir` without `--ir-dir` in
//!    `parseArgs` (`Extract.lean:2778-2782`), so the prototype's exact start-up
//!    command line now exits 1 before importing anything. The value passed here
//!    is a directory under `--work` that **no request ever names**, because a
//!    request that carries a third field replaces it (`Extract.lean:2753`).
//! 2. **stderr is inherited, exactly as the one-shot path inherits it**
//!    (`extract.rs`). It is not captured to a `serve.err` as the prototype
//!    captures it: a Lean error has to reach the caller, and the two extraction
//!    paths have to be diagnosable the same way. It is measurable, too — `lake`
//!    warns about the instrumented `doc-gen4` checkout on every start, and that
//!    warning is what puts `yes` in 5 of the 7 scenarios' `<s>-complained.txt`
//!    in the M3-d3 recording 【実測】. Capturing it would have changed an answer
//!    the gate compares.

use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use lean_doc_incr::{Algorithm, hash_module, sha256_text};

use crate::Failure;
use crate::extract::{EXIT_EXTRACTOR, FIXED_FLAGS, fold_timings};

/// How long a stopped server is given to leave through its own exit path before
/// it is signalled.
///
/// The prototype's bound (`serve-ctl.sh:170-174`, 100 × 0.1 s). Reaching it means
/// the server did not act on EOF, which has not been observed; the signal is
/// there so that a stop is bounded rather than hopeful.
const STOP_GRACE: Duration = Duration::from_secs(10);

/// How often the stop polls while it waits.
const STOP_POLL: Duration = Duration::from_millis(50);

/// How many moved modules a generation refusal names before it stops listing.
const MOVED_IN_MESSAGE: usize = 5;

/// Everything the server is started with.
///
/// The four that are not paths are the ones a caller can get wrong in a way the
/// server cannot report: `jobs` is fixed for the process (see the heading), and
/// `modules` is the **superset** it imports — whatever the requests ask for later
/// has to be inside it, because a resident environment is never grown
/// (`Extract.lean:2707-2714`).
pub struct Serve {
    /// The Lean extractor, `extractor/build.sh`'s output.
    pub bin: PathBuf,
    /// The `lake` to borrow the target's environment with.
    pub lake: PathBuf,
    /// The package being documented, already canonicalised. `lake env` runs in
    /// it and nothing is ever written into it.
    pub target: PathBuf,
    pub jobs: usize,
    /// The module list file, as the server's own command line spells it.
    pub modules_file: PathBuf,
    /// The same list, parsed — [`Generation`] hashes exactly these.
    pub modules: Vec<String>,
    /// `--work`. The start-up events file and the unused `--ir-dir` go here.
    pub work: PathBuf,
    /// Where the server writes the dependency closure's `name -> module` map
    /// (M5-a's `--link-index`, wired up in M5-b), or `None` to write none.
    ///
    /// **It is start-up configuration, not a request field, and that is the
    /// extractor's shape rather than a choice here**: the serve loop builds every
    /// request's `cfg` from the start-up one and the request line carries three
    /// paths (`Extract.lean:2882-2884`), so a map path given here is written once
    /// per request. Two things make that acceptable rather than merely tolerable.
    /// The map is derived from the *environment*, which a resident server imports
    /// once and never reloads, so every round writes the same bytes — M5-a
    /// measured five runs of the same environment at 8,465,776 B and 5/5
    /// byte-identical. And the cost is the marginal one: 0.9 s warm on top of an
    /// extraction that is already several seconds, paid only in rounds that
    /// extract at all — a run with nothing to extract never starts the server, so
    /// it writes no map and pays nothing.
    pub link_index: Option<PathBuf>,
}

/// A resident extractor and the world it is valid for.
///
/// **Started lazily, at the first request** 【判断】. The prototype starts it at
/// the head of the run so that the start-up cost sits inside the measured total
/// (`incremental.sh:175-195`); nothing is hidden by starting later, because a run
/// that extracts anything still pays for it inside the same clock, and a run that
/// extracts *nothing* — the commonest answer this pipeline gives — no longer pays
/// 3 GB and an import for a server it never speaks to. The generation is taken at
/// the head of the run either way, so the guard covers the delay.
pub struct Resident {
    serve: Serve,
    /// The world as it stood at the head of the run, before `detect` looked at
    /// it. Every later check is against this one value.
    generation: Generation,
    server: Option<Server>,
    requests: usize,
}

impl Resident {
    /// Records the world; starts nothing.
    pub fn new(serve: Serve) -> Result<Self, Failure> {
        if !serve.bin.is_file() {
            return Err(Failure::Refused {
                code: 3,
                message: format!(
                    "--extractor-bin {}: not a file. It is `extractor/build.sh`'s output, 171 MB, \
                     built against the target's toolchain and therefore not committed",
                    serve.bin.display(),
                ),
            });
        }
        let generation = Generation::take(&serve.target, &serve.modules)?;
        Ok(Self {
            serve,
            generation,
            server: None,
            requests: 0,
        })
    }

    /// One extraction round, on the same interface as a `--extractor` program:
    /// a module list in, an IR tree and a timings record out.
    pub fn extract(
        &mut self,
        modules: &Path,
        ir_dir: &Path,
        timings: &Path,
    ) -> Result<(), Failure> {
        // The one-shot path's default, kept: `incremental` passes only
        // `--timings`, and both extraction paths have to leave the events file
        // in the same place under the same name or two records of the same run
        // stop being comparable.
        let events = events_beside(timings);
        // The extractor appends, so a file an earlier round left behind would be
        // folded into this round's timings (`extract.rs`).
        let _ = fs::remove_file(&events);
        fs::create_dir_all(ir_dir)
            .map_err(|source| Failure::Failed(format!("{}: {source}", ir_dir.display())))?;
        guard_target(&self.serve.target, ir_dir)?;

        let line = request_line(&[modules, &events, ir_dir])?;
        self.check_generation("before the request")?;
        let started = Instant::now();
        let reply = self.server()?.request(&line)?;
        self.requests += 1;
        // After, as well as before: the interval a request ran in is then an
        // interval the world provably did not move in, which is the property the
        // IR it just wrote depends on.
        self.check_generation("after the request")?;

        let code = reply.split_whitespace().nth(1).unwrap_or("");
        if code != "0" {
            return Err(Failure::Refused {
                code: EXIT_EXTRACTOR,
                message: format!(
                    "the resident extractor answered `{reply}` for {}; the IR tree at {} is \
                     incomplete",
                    modules.display(),
                    ir_dir.display(),
                ),
            });
        }
        let counted = fold_timings(&events, modules, self.serve.jobs, timings)?;
        println!(
            "        served {counted} module(s) from the resident environment in {:.3}s",
            started.elapsed().as_secs_f64(),
        );
        Ok(())
    }

    /// How many extraction requests this run has sent.
    ///
    /// **The same field the stop line prints** — `serve stopped after N
    /// request(s)` — rather than a second tally: two counters of one thing are
    /// two things that can disagree, and this one is a gate's input (see
    /// `crate::build`'s `work` record).
    pub fn requests(&self) -> usize {
        self.requests
    }

    /// Stops the server, if one was ever started.
    ///
    /// Called on the way out of the round loop rather than at the end of the run
    /// 【判断】: nothing after the loop can extract, and the stages that follow it
    /// read the whole IR, so holding 3 GB across them buys nothing. The prototype
    /// stops at the very end (`incremental.sh:380`) to keep the teardown on the
    /// same side of the clock as the start-up; the teardown is inside
    /// `totalSeconds` here too, only earlier.
    ///
    /// Idempotent, as the prototype's `cleanup` is idempotent, and [`Drop`] is
    /// still the backstop for every path that does not reach here.
    pub fn stop(&mut self) {
        if let Some(server) = self.server.take() {
            let requests = self.requests;
            let status = server.stop();
            println!("serve   stopped after {requests} request(s){status}");
        }
    }

    /// The running server, started on first use.
    fn server(&mut self) -> Result<&mut Server, Failure> {
        if self.server.is_none() {
            // No check before the spawn: [`Self::extract`] has just made one and
            // this is its next statement. A second reading here would be a second
            // answer to the same question, which is how two guards start
            // disagreeing.
            let server = Server::start(&self.serve)?;
            // The import window: the server is holding the oleans as they were
            // at some instant inside it, and this is the check that says which.
            // The server is already in hand, so a refusal here drops it — which
            // is the stop.
            let generation = Generation::take(&self.serve.target, &self.serve.modules)?;
            let moved = self.generation.moved(&generation);
            if !moved.is_empty() {
                return Err(stale(&moved, "while the server was importing"));
            }
            println!(
                "serve   {} ({} jobs, generation {})",
                server.ready.trim(),
                self.serve.jobs,
                &self.generation.digest[..16],
            );
            self.server = Some(server);
        }
        Ok(self.server.as_mut().expect("just started"))
    }

    fn check_generation(&self, when: &str) -> Result<(), Failure> {
        let now = Generation::take(&self.serve.target, &self.serve.modules)?;
        let moved = self.generation.moved(&now);
        if moved.is_empty() {
            return Ok(());
        }
        Err(stale(&moved, when))
    }

    /// The generation this run is pinned to, for the record.
    #[must_use]
    pub fn generation(&self) -> &str {
        &self.generation.digest
    }
}

impl Drop for Resident {
    fn drop(&mut self) {
        // Silent, unlike `stop`: a `Drop` that prints is a `Drop` that reports a
        // failure twice. The work is [`Server::drop`]'s either way.
        self.server.take();
    }
}

/// Modules whose oleans moved under a running server.
fn stale(moved: &[String], when: &str) -> Failure {
    let named: Vec<&str> = moved
        .iter()
        .take(MOVED_IN_MESSAGE)
        .map(String::as_str)
        .collect();
    let rest = moved.len().saturating_sub(named.len());
    Failure::Refused {
        code: 3,
        message: format!(
            "the oleans moved {when}: {} module(s) — {}{}. The resident extractor imported the \
             world as it was at the head of this run and Lean cannot swap one module out of an \
             imported environment (`Extract.lean:2716-2721`), so every answer it has given since \
             is about the old world. Re-run after the build that is in flight has finished",
            moved.len(),
            named.join(", "),
            if rest > 0 {
                format!(" and {rest} more")
            } else {
                String::new()
            },
        ),
    }
}

// ------------------------------------------------------------- the generation

/// The oleans of one module list, as Lake's own content hashes.
///
/// `--algorithm lake` rather than `sha256`: it reads the `<file>.hash` Lake
/// already wrote beside every olean, so this is 432 small reads instead of 227 MB
/// (`ledger.rs`). It is the same function the ledger calls, over the same file
/// set in the same order, which is what makes "the world" one thing here rather
/// than two.
struct Generation {
    /// `(module, hash)`, in the module list's order. `-` is a module with no
    /// olean at all, which is a real state: it is what `detect` reports as
    /// removed.
    entries: Vec<(String, String)>,
    digest: String,
}

impl Generation {
    fn take(target: &Path, modules: &[String]) -> Result<Self, Failure> {
        let target = target.to_string_lossy();
        let target = target.trim_end_matches('/');
        let lib_dir = format!("{target}/.lake/build/lib/lean");
        let mut entries: Vec<(String, String)> = Vec::with_capacity(modules.len());
        for module in modules {
            let entry =
                hash_module(target, &lib_dir, module, &Algorithm::lake()).map_err(|source| {
                    Failure::Refused {
                        code: 3,
                        message: format!(
                            "the resident extractor's generation over {lib_dir}: {source}"
                        ),
                    }
                })?;
            entries.push((
                module.clone(),
                entry.map_or_else(|| "-".to_owned(), |entry| entry.hash),
            ));
        }
        let joined: Vec<String> = entries
            .iter()
            .map(|(module, hash)| format!("{module} {hash}"))
            .collect();
        Ok(Self {
            digest: sha256_text(&joined.join("\n")),
            entries,
        })
    }

    /// The modules that differ between two takes, by name.
    ///
    /// The lists are the same module list in the same order, so this is a walk
    /// rather than a join — and if it ever is not (a `--modules` that changed
    /// under the run, which nothing here can do), the length difference itself is
    /// reported rather than silently zipped away.
    fn moved(&self, other: &Self) -> Vec<String> {
        if self.digest == other.digest && self.entries.len() == other.entries.len() {
            return Vec::new();
        }
        let mut moved: Vec<String> = Vec::new();
        for (before, after) in self.entries.iter().zip(&other.entries) {
            if before != after {
                moved.push(before.0.clone());
            }
        }
        for extra in self
            .entries
            .iter()
            .skip(other.entries.len())
            .chain(other.entries.iter().skip(self.entries.len()))
        {
            moved.push(extra.0.clone());
        }
        moved
    }
}

// ----------------------------------------------------------------- the server

/// The child process and the two ends of the protocol.
struct Server {
    child: Child,
    /// The request channel. `Option` because stopping is "close this".
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    /// `<work>/serve.out`, the prototype's name for the same thing: every line
    /// the server printed, protocol and phase report alike, in order. Written and
    /// never read back — a diagnostic, like everything else in `--work`.
    log: File,
    /// The `ready` line, as reported.
    ready: String,
}

impl Server {
    fn start(serve: &Serve) -> Result<Self, Failure> {
        fs::create_dir_all(&serve.work)
            .map_err(|source| Failure::Failed(format!("{}: {source}", serve.work.display())))?;
        let events = serve.work.join("serve-events.jsonl");
        let _ = fs::remove_file(&events);
        // Trap 1 in the heading: `--write-ir` without `--ir-dir` is refused by
        // the extractor's own argument check, and a request's third field
        // replaces whatever is passed here — so this directory is named and
        // never written to.
        let unused_ir = serve.work.join("serve-ir-unused");
        let log_path = serve.work.join("serve.out");
        let log = File::create(&log_path)
            .map_err(|source| Failure::Failed(format!("{}: {source}", log_path.display())))?;

        let mut command = Command::new(&serve.lake);
        command
            .current_dir(&serve.target)
            .arg("env")
            .arg(&serve.bin)
            .arg(&serve.modules_file)
            .arg(&events)
            .args(FIXED_FLAGS)
            .arg("--jobs")
            .arg(serve.jobs.to_string())
            .arg("--ir-dir")
            .arg(&unused_ir);
        if let Some(link_index) = &serve.link_index {
            command.arg("--link-index").arg(link_index);
        }
        command
            .arg("--serve")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // Trap 2 in the heading: inherited, as the one-shot path inherits it.
            .stderr(Stdio::inherit());
        let mut child = command.spawn().map_err(|source| Failure::Refused {
            code: EXIT_EXTRACTOR,
            message: format!(
                "{} env {} --serve: {source}",
                serve.lake.display(),
                serve.bin.display(),
            ),
        })?;
        let stdin = child.stdin.take().expect("stdin was piped");
        let stdout = child.stdout.take().expect("stdout was piped");
        let mut server = Self {
            child,
            stdin: Some(stdin),
            stdout: BufReader::new(stdout),
            log,
            ready: String::new(),
        };
        server.ready = server.wait_for("ready ")?;
        Ok(server)
    }

    /// Writes one request and returns its `ok` line.
    fn request(&mut self, line: &str) -> Result<String, Failure> {
        let stdin = self.stdin.as_mut().ok_or_else(|| {
            Failure::Failed("the resident extractor was already stopped".to_owned())
        })?;
        stdin
            .write_all(line.as_bytes())
            .and_then(|()| stdin.flush())
            .map_err(|source| Failure::Refused {
                code: EXIT_EXTRACTOR,
                message: format!("writing to the resident extractor: {source}"),
            })?;
        self.wait_for("ok ")
    }

    /// The next line with this prefix, with everything before it logged.
    ///
    /// **Located by prefix, not by position** — `run` prints a human-readable
    /// phase report to the same stdout as the protocol (`serve-ctl.sh:25-30`), so
    /// counting lines would be counting the report.
    fn wait_for(&mut self, prefix: &str) -> Result<String, Failure> {
        loop {
            let mut line = String::new();
            let read = self
                .stdout
                .read_line(&mut line)
                .map_err(|source| Failure::Refused {
                    code: EXIT_EXTRACTOR,
                    message: format!("reading from the resident extractor: {source}"),
                })?;
            if read == 0 {
                // EOF: the server is gone. Its stderr already reached the
                // caller's (see the heading), so what is added here is what was
                // being waited for and what the process exited with.
                let status = self.child.wait().ok().and_then(|status| status.code());
                return Err(Failure::Refused {
                    code: EXIT_EXTRACTOR,
                    message: format!(
                        "the resident extractor exited {} before `{}`",
                        status.map_or_else(|| "on a signal".to_owned(), |code| code.to_string()),
                        prefix.trim(),
                    ),
                });
            }
            let _ = self.log.write_all(line.as_bytes());
            let text = line.trim_end_matches(['\n', '\r']).to_owned();
            if text.starts_with(prefix) {
                return Ok(text);
            }
            if text.starts_with("err ") {
                return Err(Failure::Refused {
                    code: EXIT_EXTRACTOR,
                    message: format!("the resident extractor rejected the request: {text}"),
                });
            }
        }
    }

    /// Closes the pipe and waits, then signals. Returns what to say about it.
    fn stop(mut self) -> String {
        let status = self.shut_down();
        match status {
            Some(0) => String::new(),
            Some(code) => format!(" (the server exited {code})"),
            None => " (the server had to be signalled)".to_owned(),
        }
    }

    /// The whole of the teardown, so that [`Drop`] and [`Self::stop`] cannot
    /// drift apart. `None` means it did not leave on its own and was signalled.
    fn shut_down(&mut self) -> Option<i32> {
        // EOF. The server's loop ends on it and returns 0 through its own exit
        // path rather than being signalled (`serve-ctl.sh:17-23`).
        drop(self.stdin.take());
        let deadline = Instant::now() + STOP_GRACE;
        loop {
            match self.child.try_wait() {
                Ok(Some(status)) => return Some(status.code().unwrap_or(-1)),
                Ok(None) => {}
                Err(_) => break,
            }
            if Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(STOP_POLL);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        None
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if self.stdin.is_some() {
            self.shut_down();
        }
    }
}

// ---------------------------------------------------------------- the plumbing

/// `<timings without .json>-events.jsonl` (`extract-once.sh:52`).
fn events_beside(timings: &Path) -> PathBuf {
    let text = timings.to_string_lossy();
    PathBuf::from(format!(
        "{}-events.jsonl",
        text.strip_suffix(".json").unwrap_or(&text)
    ))
}

/// One request line, or a refusal naming the path that cannot be sent.
///
/// The protocol splits on spaces and tabs (`Extract.lean:2748`), so a path with
/// whitespace in it does not fail — it arrives as two shorter paths and the
/// server writes an IR tree somewhere nobody named. That is the `defaultIrDir`
/// failure shape again (M4-a), so it is refused before anything is written.
///
/// The paths are absolute because the server's working directory is the target
/// (`serve-ctl.sh:68-77`): a relative one resolves against the package being
/// documented, which is the one directory this project never writes into.
fn request_line(paths: &[&Path]) -> Result<String, Failure> {
    let mut line = String::new();
    for path in paths {
        let text = crate::extract::absolute(path)
            .to_string_lossy()
            .into_owned();
        if text.chars().any(char::is_whitespace) {
            return Err(Failure::Refused {
                code: 3,
                message: format!(
                    "the resident extractor's protocol is one space-separated line per request \
                     (`Extract.lean:2748`), so a path with whitespace in it cannot be sent: {text}"
                ),
            });
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(&text);
    }
    line.push('\n');
    Ok(line)
}

/// The same rule `lean-doc extract` states, for the same reason: the package
/// being documented is opened read-only and nothing is ever written into it.
fn guard_target(target: &Path, ir_dir: &Path) -> Result<(), Failure> {
    let resolved = crate::extract::resolve(ir_dir);
    if resolved.starts_with(target) {
        return Err(Failure::Refused {
            code: 3,
            message: format!(
                "the round's --ir-dir {} is inside the target {}: the package being documented is \
                 opened read-only and nothing is ever written into it",
                resolved.display(),
                target.display(),
            ),
        });
    }
    Ok(())
}

// --------------------------------------------------------------------- tests

/// The protocol, the guard and the teardown, against a fake extractor.
///
/// **The oracle for the extraction itself is elsewhere**: the resident path and
/// the one-shot path write the same 436 IR files over the same module list, which
/// needs a Lean toolchain and a built package and is therefore the M4-c gate, not
/// a test. What is here is what the gate cannot reach — the failure shapes. Every
/// one of them ends with a 3 GB process either stopped or never started, so they
/// are also the only place "no process is leaked" is asserted rather than
/// observed once by hand.
#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    /// A fake target package, a fake `lake` and a fake resident extractor.
    struct World {
        root: PathBuf,
        target: PathBuf,
        modules: Vec<String>,
        modules_file: PathBuf,
        work: PathBuf,
        lake: PathBuf,
        bin: PathBuf,
    }

    impl World {
        fn new(what: &str, code: &str) -> Self {
            Self::with_hooks(what, code, ":", ":")
        }

        /// The two hooks are `sh`, run at the two moments a real build could
        /// land: `importing` before the server reports `ready`, `serving` inside
        /// the request loop before the reply. They are how a test makes the world
        /// move under a server that is already running.
        fn with_hooks(what: &str, code: &str, importing: &str, serving: &str) -> Self {
            let root = scratch(what);
            let target = root.join("target");
            let modules: Vec<String> = vec!["Pkg.A".to_owned(), "Pkg.B".to_owned()];
            for module in &modules {
                write_olean(&target, module, &format!("{module} v1"));
            }
            let modules_file = root.join("modules.txt");
            fs::write(&modules_file, modules.join("\n") + "\n").expect("writable");

            let lake = root.join("lake");
            fs::write(
                &lake,
                "#!/bin/sh\n[ \"$1\" = env ] || exit 9\nshift\nexec \"$@\"\n",
            )
            .expect("writable");
            executable(&lake);

            // The server: records its argv and its pid once, answers every
            // request, and leaves on EOF the way `Extract.lean:2747` leaves.
            // `served` is a report line, printed to the same stdout as the
            // protocol — a reply found by position rather than by prefix would
            // find that instead (`serve-ctl.sh:25-30`).
            let bin = root.join("extract");
            fs::write(
                &bin,
                format!(
                    "#!/bin/sh\n\
                     printf '%s\\n' \"$$\" > {pid}\n\
                     : > {argv}\n\
                     for a in \"$@\"; do printf '%s\\n' \"$a\" >> {argv}; done\n\
                     {importing}\n\
                     printf 'ready 2500000 99 2\\n'\n\
                     while read -r mods events irdir; do\n\
                     \x20 [ -z \"$mods\" ] && exit 0\n\
                     \x20 printf '%s %s %s\\n' \"$mods\" \"$events\" \"$irdir\" >> {reqs}\n\
                     \x20 {serving}\n\
                     \x20 mkdir -p \"$irdir\"\n\
                     \x20 printf '{{\"phase\":\"stage4b.importModules\",\"pid\":1,\"us\":7,\"resident\":1}}\\n' > \"$events\"\n\
                     \x20 printf 'served, and this line is not a reply\\n'\n\
                     \x20 printf 'ok {code} 4242\\n'\n\
                     done\n",
                    pid = quoted(&root.join("server.pid")),
                    argv = quoted(&root.join("argv")),
                    reqs = quoted(&root.join("requests")),
                ),
            )
            .expect("writable");
            executable(&bin);

            Self {
                work: root.join("work"),
                target,
                modules,
                modules_file,
                lake,
                bin,
                root,
            }
        }

        fn resident(&self) -> Result<Resident, Failure> {
            Resident::new(Serve {
                link_index: None,
                bin: self.bin.clone(),
                lake: self.lake.clone(),
                target: fs::canonicalize(&self.target).expect("a real directory"),
                jobs: 3,
                modules_file: self.modules_file.clone(),
                modules: self.modules.clone(),
                work: self.work.clone(),
            })
        }

        /// One round, with the paths a round would use.
        fn extract(&self, resident: &mut Resident, round: usize) -> Result<(), Failure> {
            resident.extract(
                &self.modules_file,
                &self.root.join(format!("inc-ir-{round}")),
                &self.work.join(format!("extract-timings-{round}.json")),
            )
        }

        fn argv(&self) -> Vec<String> {
            read_lines(&self.root.join("argv"))
        }

        fn requests(&self) -> Vec<String> {
            read_lines(&self.root.join("requests"))
        }

        fn server_pid(&self) -> String {
            fs::read_to_string(self.root.join("server.pid"))
                .expect("the server recorded its pid")
                .trim()
                .to_owned()
        }
    }

    impl Drop for World {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn one_environment_answers_every_round_and_the_reply_is_found_by_prefix() {
        let world = World::new("rounds", "0");
        let mut resident = world.resident().expect("a resident");
        world.extract(&mut resident, 1).expect("round 1");
        let pid = world.server_pid();
        world.extract(&mut resident, 2).expect("round 2");

        assert_eq!(world.requests().len(), 2, "two requests");
        assert_eq!(
            world.server_pid(),
            pid,
            "one process answered both: the whole point of residency",
        );
        // The reply was located by prefix, not by position — the fake prints a
        // report line before it.
        for round in [1, 2] {
            let path = world.work.join(format!("extract-timings-{round}.json"));
            let record: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(&path).expect("a timings record"))
                    .expect("valid JSON");
            assert_eq!(record["importModules"], serde_json::json!(0.000_007));
            assert_eq!(record["importModules:resident"], serde_json::json!(1));
            assert_eq!(record["targetModules"], serde_json::json!(2));
            // The server's, not a request's: `Extract.lean:2751` carries the
            // start-up `cfg` into every request (plan §6, constraint 6).
            assert_eq!(record["jobsRequested"], serde_json::json!(3));
        }
        assert!(
            world.root.join("inc-ir-2").is_dir(),
            "the IR directory is created before the request",
        );
        drop(resident);
        assert!(
            !alive(&pid),
            "the server is gone once nobody holds the pipe"
        );
    }

    #[test]
    fn the_start_up_command_line_carries_an_ir_dir_that_no_request_ever_names() {
        let world = World::new("start-up", "0");
        let mut resident = world.resident().expect("a resident");
        world.extract(&mut resident, 1).expect("round 1");

        let argv = world.argv();
        assert_eq!(argv[0], world.modules_file.display().to_string());
        assert_eq!(
            argv[1],
            world.work.join("serve-events.jsonl").display().to_string()
        );
        for flag in FIXED_FLAGS {
            assert!(argv.iter().any(|arg| arg == flag), "{flag}: {argv:?}");
        }
        assert_eq!(argv.last().map(String::as_str), Some("--serve"));
        let at = argv.iter().position(|arg| arg == "--jobs").expect("--jobs");
        assert_eq!(argv[at + 1], "3");
        // Trap 1 in this file's heading, asserted: M4-a refuses `--write-ir`
        // without `--ir-dir` in `parseArgs`, so the prototype's start-up command
        // line no longer starts. The value is a directory a request always
        // replaces (`Extract.lean:2753`), and it stays unwritten.
        let at = argv
            .iter()
            .position(|arg| arg == "--ir-dir")
            .expect("--ir-dir is passed at start-up");
        let unused = PathBuf::from(&argv[at + 1]);
        assert_eq!(unused, world.work.join("serve-ir-unused"));
        assert!(!unused.exists(), "no request ever named it");
        assert!(
            world.requests()[0].ends_with(&world.root.join("inc-ir-1").display().to_string()),
            "every request carries its own IR directory: {:?}",
            world.requests(),
        );
    }

    #[test]
    fn a_request_the_server_failed_is_exit_four() {
        let world = World::new("failed-request", "7");
        let mut resident = world.resident().expect("a resident");
        let failure = world.extract(&mut resident, 1).expect_err("exit 4");
        let (code, message) = refusal(&failure);
        assert_eq!(code, EXIT_EXTRACTOR);
        assert!(message.contains("ok 7 4242"), "{message}");
        assert!(message.contains("incomplete"), "{message}");
    }

    #[test]
    fn oleans_that_move_under_a_running_server_stop_the_run() {
        // The hook rewrites one olean's Lake hash while the server is answering
        // — which is what a `lake build` landing mid-run looks like to this
        // guard, and is the *only* way the pipeline can still be handed the stale
        // environment stage 6a measured.
        let world = World::with_hooks(
            "stale",
            "0",
            ":",
            "printf 'moved\\n' > \"$(dirname \"$0\")/target/.lake/build/lib/lean/Pkg/B.olean.hash\"",
        );
        let mut resident = world.resident().expect("a resident");
        let pid_before = world.requests();
        assert!(pid_before.is_empty());
        let failure = world.extract(&mut resident, 1).expect_err("exit 3");
        let (code, message) = refusal(&failure);
        assert_eq!(code, 3, "{message}");
        assert!(message.contains("Pkg.B"), "the module is named: {message}");
        assert!(
            !message.contains("Pkg.A"),
            "only the one that moved: {message}"
        );
        assert!(message.contains("after the request"), "{message}");
        let pid = world.server_pid();
        drop(resident);
        assert!(!alive(&pid), "a refusal does not leak a 3 GB process");
    }

    #[test]
    fn oleans_that_move_while_the_server_imports_stop_the_run() {
        // The import window — between the stamp taken before the spawn and the
        // one taken when `ready` lands. A real `lake build` landing there is the
        // worst case of all: the environment is a mixture, and nothing the
        // server says afterwards names which half a given answer came from.
        let world = World::with_hooks(
            "stale-import",
            "0",
            "printf 'moved\\n' > \"$(dirname \"$0\")/target/.lake/build/lib/lean/Pkg/A.olean.hash\"",
            ":",
        );
        let mut resident = world.resident().expect("a resident");
        let failure = failed(world.extract(&mut resident, 1));
        let (code, message) = refusal(&failure);
        assert_eq!(code, 3, "{message}");
        assert!(message.contains("Pkg.A"), "{message}");
        assert!(
            message.contains("while the server was importing"),
            "the check that closes the import window caught it: {message}",
        );
        let pid = world.server_pid();
        drop(resident);
        assert!(!alive(&pid), "the half-built server is stopped, not leaked");
        assert!(
            world.requests().is_empty(),
            "and it was never asked anything",
        );
    }

    #[test]
    fn a_server_that_dies_is_reported_rather_than_waited_for() {
        let world = World::new("dead", "0");
        // A `lake` that exits without ever printing `ready`: the read gets EOF
        // rather than a reply, which is the one failure the prototype needs a
        // polling loop and a pid file to notice (`serve-ctl.sh:82-102`).
        fs::write(&world.lake, "#!/bin/sh\nexit 3\n").expect("writable");
        executable(&world.lake);
        let mut resident = world.resident().expect("a resident");
        let failure = world.extract(&mut resident, 1).expect_err("no server");
        let (code, message) = refusal(&failure);
        assert_eq!(code, EXIT_EXTRACTOR);
        assert!(message.contains("exited 3"), "{message}");
        assert!(
            message.contains("ready"),
            "what it was waiting for: {message}"
        );
    }

    #[test]
    fn a_path_with_whitespace_is_refused_before_the_server_is_started() {
        let world = World::new("whitespace", "0");
        let mut resident = world.resident().expect("a resident");
        let failure = resident
            .extract(
                &world.modules_file,
                &world.root.join("inc ir 1"),
                &world.work.join("extract-timings-1.json"),
            )
            .expect_err("refused");
        let (code, message) = refusal(&failure);
        assert_eq!(code, 3);
        assert!(message.contains("space-separated"), "{message}");
        assert!(
            !world.root.join("server.pid").exists(),
            "nothing was started for a request that cannot be sent",
        );
    }

    #[test]
    fn an_ir_dir_inside_the_target_is_refused() {
        let world = World::new("ir-in-target", "0");
        let mut resident = world.resident().expect("a resident");
        let failure = resident
            .extract(
                &world.modules_file,
                &world.target.join("inc-ir-1"),
                &world.work.join("extract-timings-1.json"),
            )
            .expect_err("refused");
        let (code, message) = refusal(&failure);
        assert_eq!(code, 3);
        assert!(message.contains("read-only"), "{message}");
    }

    #[test]
    fn a_missing_extractor_binary_is_refused_before_anything_runs() {
        let world = World::new("no-bin", "0");
        fs::remove_file(&world.bin).expect("removable");
        let failure = failed(world.resident());
        let (code, message) = refusal(&failure);
        assert_eq!(code, 3);
        assert!(message.contains("extractor/build.sh"), "{message}");
    }

    // ---------------------------------------------------------------- plumbing

    /// The failure of a `Result` whose `Ok` side is not printable.
    fn failed<T>(result: Result<T, Failure>) -> Failure {
        match result {
            Ok(_) => panic!("expected a refusal"),
            Err(failure) => failure,
        }
    }

    fn refusal(failure: &Failure) -> (u8, String) {
        match failure {
            Failure::Refused { code, message } => (*code, message.clone()),
            Failure::Usage(message) | Failure::Failed(message) => {
                panic!("expected a refusal, got: {message}")
            }
            Failure::Answered(code) => panic!("expected a refusal, got exit {code}"),
        }
    }

    /// One module's olean and the `<file>.hash` Lake writes beside it, which is
    /// what `--algorithm lake` — and therefore [`Generation`] — reads.
    fn write_olean(target: &Path, module: &str, body: &str) {
        let path = target
            .join(".lake/build/lib/lean")
            .join(format!("{}.olean", module.replace('.', "/")));
        fs::create_dir_all(path.parent().expect("a parent")).expect("writable");
        fs::write(&path, body).expect("writable");
        // Lake's `.hash` is a content hash; here it is the content, which has
        // the one property this fixture needs — it moves when the olean moves.
        fs::write(path.with_extension("olean.hash"), format!("{body}\n")).expect("writable");
    }

    fn alive(pid: &str) -> bool {
        Command::new("kill")
            .args(["-0", pid])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }

    fn executable(path: &Path) {
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("chmod");
    }

    fn read_lines(path: &Path) -> Vec<String> {
        fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// The temporary paths hold a pid and a counter, never a quote — but a path
    /// that reaches a shell script is quoted anyway, so the fixture cannot be the
    /// thing that breaks.
    fn quoted(path: &Path) -> String {
        format!("'{}'", path.display().to_string().replace('\'', "'\\''"))
    }

    fn scratch(what: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "lean-doc-resident-{}-{}-{what}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("the temporary directory is creatable");
        path
    }
}
