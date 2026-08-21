//! `litedoc4 watch` — keep the site current while the package is being edited,
//! and serve it.
//!
//! Feature sweep **A-2**, from doc-gen4 #389 ("local build does not refresh
//! after code change", where a commenter writes that coming from Rust one could
//! expect the edit-to-page cycle to be *below one second, not above ten
//! minutes*) and #404 ("hangs indefinitely", which turned out to be Lean core's
//! documentation still being generated in the background). **The algorithm
//! existed already**: `litedoc4 build` rebuilds a no-op package in 0.31 s and a
//! one-declaration edit in 4.35 s 【実測, `docs/plans/feature-sweep.md` §4】.
//! What was missing is the loop, the server, and saying out loud what is
//! happening.
//!
//! ```text
//!   ask the ledger  ──no work──>  idle
//!         │
//!    work │
//!         v
//!   is the answer the same as last tick?  ──no──>  a build is in flight; say so
//!         │ yes
//!         v
//!   did the last pass already act on it?  ──yes──> nothing new; stay quiet
//!         │ no
//!         v
//!   `litedoc4 build`, and one line saying what it cost
//! ```
//!
//! # It does not run `lake build` 【判断】
//!
//! The division of labour everywhere else in this project is that **the user (or
//! the GitHub action) builds the package and litedoc4 reads the oleans**;
//! `watch` keeps it. Building inside the loop would give this one command a job
//! no other command has, and would put a Lean compile — with its own errors, its
//! own output and its own idea of what to do when it fails — inside a loop whose
//! whole purpose is to be predictable. So: run `lake build` in another window.
//! This says so in `--help`, in the start-up banner and in the README, because
//! it is the first thing anyone will ask.
//!
//! # The trigger is the ledger, not a timestamp 【判断】
//!
//! "Which modules are stale" already has an answer in this tree —
//! [`litedoc4_incr::check_ledger`], the same call `build` makes — and a second
//! answer derived from mtimes would be a second definition of *changed* that
//! could disagree with the first. It cannot be allowed to: the disagreement's
//! quiet direction is a watch loop that never rebuilds. So the loop **asks**, and
//! what it asks is the product's own question.
//!
//! # Asking on a timer, and why there is no file-system watcher 【判断】
//!
//! There is no `notify` crate and no `FSEvents`/`inotify`/`ReadDirectoryChanges`
//! of our own. Three reasons, in order of weight:
//!
//! 1. **An event is only ever a hint.** Whatever wakes the loop up, the answer
//!    still comes from the ledger — so an event-driven build would be *this*
//!    loop with a different alarm clock, and the alarm clock is the part that
//!    behaves differently on each operating system.
//! 2. **One path, not two.** `docs/plans/feature-sweep.md` A-2 says a watcher
//!    that splits into "events where available, polling otherwise" is two code
//!    paths that behave differently on different machines, which is exactly what
//!    this project refuses elsewhere. Polling is not the fallback here; it is the
//!    path.
//! 3. **The cost is measured and small** — `benchmarks/results/watch-2026-08-21.txt`.
//!    An idle pass is one `check_ledger` over the target's 422 modules — 228 MB of
//!    oleans, 0.13 s warm.
//!
//! The price is stated rather than hidden: an edit is noticed up to
//! [`Flags::interval`] late, on top of the `lake build` the user is waiting for
//! anyway.
//!
//! # One quiet interval before extracting
//!
//! A pass acts only when the ledger's answer is **the same as the previous
//! pass's**. That is what "a build is in flight" looks like from here: while
//! `lake build` is writing oleans the answer keeps moving, and extracting into
//! that would either produce a site made of two half-worlds or be stopped by the
//! resident extractor's own generation guard (`crate::resident`, exit 3). One
//! interval of quiet is the cost; the alternative is starting a 3 GB Lean import
//! against a directory that is still being written.
//!
//! # The resident extractor does **not** survive a pass 【実測, stage 6a】
//!
//! This is the one place where what was built differs from what A-2 planned, and
//! the reason is not an implementation shortcut. A resident extractor is one
//! imported Lean environment, and **Lean cannot swap a module out of an imported
//! environment** (`Extract.lean:2716-2721`, quoted in `crate::resident`). A pass
//! only ever extracts when the oleans have moved — that is what made it a pass —
//! so a server held over from the previous pass is, by construction, a server
//! whose environment is exactly the one that just went stale. Keeping it alive
//! across passes would buy nothing on the passes that do work, and the passes
//! that do no work never start it at all ([`crate::resident::Resident`] starts
//! lazily). What residency does buy is unchanged and still bought: **within** a
//! pass, the ownership/merge rounds share one import.
//!
//! # What a failed pass does
//!
//! It says what failed and then **waits for the world to move again** before
//! trying the same thing. A loop that retried immediately would start a 3 GB
//! import every interval for as long as the failure lasted, which is the shape
//! #404 was mistaken for. The first pass is the exception: it fails the command,
//! because until it succeeds there is no site to serve and no state to continue
//! from.

use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use litedoc4_incr::{CheckOptions, CheckSummary, check_ledger, sha256_text};

use crate::{Failure, USAGE, refused, usage};

/// How long a pass waits before asking again, unless `--interval` says
/// otherwise.
///
/// One second, because that is the number doc-gen4 #389 is written in ("below 1
/// second, not above 10 minutes") and because an idle pass costs a fraction of
/// it on the measurement target 【実測,
/// `benchmarks/results/watch-2026-08-21.txt`】. It is a floor on how late an
/// edit can be noticed, not on how long a rebuild takes.
const DEFAULT_INTERVAL: Duration = Duration::from_millis(1000);

/// The shortest `--interval` that is accepted.
///
/// An idle pass reads every olean of the package, so an interval below this asks
/// the question again before the answer to the last one has finished being
/// useful — it would spend a core to save less than the interval it saves.
const MIN_INTERVAL: Duration = Duration::from_millis(100);

/// How often a settling report repeats while a build is in flight.
///
/// **Nothing may look like a hang** — #404 was a ten-minute silence — so a wait
/// says what it is waiting for, again, at a rate a person reads as progress
/// rather than as noise.
const SETTLE_REPORT: Duration = Duration::from_secs(5);

/// How often an idle loop says it is still there.
///
/// A silent terminal cannot be told from a dead one. One line a minute is what
/// that costs.
const HEARTBEAT: Duration = Duration::from_secs(60);

/// `watch`'s own two flags, filled in by [`crate::build::parse`].
///
/// They live here rather than in [`crate::build`] because they are this
/// command's, and they arrive as text rather than as numbers so that the refusal
/// for `--port banana` is written next to what a port means.
#[derive(Default)]
pub(crate) struct Flags {
    pub(crate) port: Option<String>,
    pub(crate) interval: Option<String>,
}

impl Flags {
    /// Both of them, for a caller that wants the refusal before it does any
    /// work. [`crate::build::parse`] calls this before it starts a `lake`.
    pub(crate) fn check(&self) -> Result<(), Failure> {
        self.port()?;
        self.interval()?;
        Ok(())
    }

    /// The port to bind, or a refusal naming what was passed.
    fn port(&self) -> Result<u16, Failure> {
        let Some(raw) = &self.port else {
            return Ok(crate::httpd::DEFAULT_PORT);
        };
        let port: u16 = raw
            .parse()
            .map_err(|_| Failure::Usage(format!("--port wants a number 1-65535, not `{raw}`")))?;
        if port == 0 {
            return usage(
                "--port 0 asks the kernel for whatever is free, which is an address nobody can \
                 type. Name the port, or leave the flag out for the default",
            );
        }
        Ok(port)
    }

    /// How long a pass sleeps, or a refusal.
    fn interval(&self) -> Result<Duration, Failure> {
        let Some(raw) = &self.interval else {
            return Ok(DEFAULT_INTERVAL);
        };
        let millis: u64 = raw.parse().map_err(|_| {
            Failure::Usage(format!(
                "--interval wants a whole number of milliseconds, not `{raw}`"
            ))
        })?;
        let interval = Duration::from_millis(millis);
        if interval < MIN_INTERVAL {
            return usage(format!(
                "--interval {millis} is below {} ms: a pass reads every olean of the package, so a \
                 shorter one spends a core asking a question whose last answer is still current",
                MIN_INTERVAL.as_millis(),
            ));
        }
        Ok(interval)
    }
}

// ------------------------------------------------------------------- the loop

/// `litedoc4 watch`.
pub(crate) fn watch(args: &[String]) -> Result<(), Failure> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{USAGE}");
        return Ok(());
    }
    let mut flags = Flags::default();
    let mut request = crate::build::parse(args, Some(&mut flags))?;
    let port = flags.port()?;
    let interval = flags.interval()?;

    // **Pinned here, once**, and then never derived again — see [`Trigger`].
    // Both are things `build` works out for itself on every run, and both are
    // inputs to the render key, so a loop that let them move between the
    // question and the answer would rebuild for a reason that is not true.
    if request.libs.is_empty() {
        let declared = crate::lakefile::read_libraries(&request.root)?;
        println!(
            "watch   lib    {} (from {})",
            declared.names.join(", "),
            declared.file.display(),
        );
        request.libs = declared.names;
    }
    if request.source_url.is_none() {
        let derived = crate::build::derive_source_url(&request.root)?;
        crate::pipeline::check_source_url(&derived)?;
        println!(
            "watch   source {derived} (derived once, now — a commit during this session does not move it)"
        );
        request.source_url = Some(derived);
    }

    // **Before the first build**: a port that is taken has to be said while the
    // reader is still looking, not after a two-minute full generation.
    let listener = crate::httpd::bind(port)?;
    let site = request.layout.site.clone();
    println!(
        "watch   site   http://127.0.0.1:{port}/  ({})",
        site.display()
    );
    println!(
        "watch   root   {} — this command does **not** run `lake build`. Run it in another \
         window; watch notices the oleans it writes.",
        request.root.display(),
    );
    println!(
        "watch   asks the ledger every {} ms. Ctrl-C stops it.",
        interval.as_millis(),
    );
    // Detached on purpose: it has no exit condition, and the process ending is
    // what closes the listener.
    let _server = thread::Builder::new()
        .name("litedoc4-httpd".to_owned())
        .spawn(move || crate::httpd::serve(&listener, &site))
        .map_err(|source| Failure::Failed(format!("cannot start the server thread: {source}")))?;

    let trigger = Trigger {
        ledger: &request.layout.ledger,
        ir: &request.layout.ir,
        link_index: &request.link_index,
        source_url: request.source_url.clone().unwrap_or_default(),
        external_links: request.external_links.digest(),
        root: &request.root,
        libs: &request.libs,
    };
    run_loop(&request, &trigger, interval, port)
}

/// The loop, once everything it needs exists.
fn run_loop(
    request: &crate::build::Request,
    trigger: &Trigger<'_>,
    interval: Duration,
    port: u16,
) -> Result<(), Failure> {
    let mut previous: Option<String> = None;
    let mut acted: Option<String> = None;
    let mut passes: u64 = 0;
    let mut rebuilds: u64 = 0;
    let mut settling: Option<Instant> = None;
    let mut said: Option<Instant> = None;
    let mut quiet_since = Instant::now();

    loop {
        passes += 1;
        let asked_at = Instant::now();
        let now = match trigger.ask() {
            Ok(now) => now,
            Err(failure) => {
                // The ledger cannot be read: a half-written file, a directory
                // that went away. Said every time, because unlike a failed
                // rebuild this costs nothing to retry and the world moving is
                // what fixes it.
                eprintln!("watch   cannot ask the ledger: {}", describe(&failure));
                thread::sleep(interval);
                continue;
            }
        };
        // **What the poll costs, in the line that says the loop is alive.** The
        // one number a reader can use to choose `--interval`, and the one this
        // design would otherwise hide: asking is not free, and how much it is
        // not free by is a property of their package, not of this file.
        let asked = asked_at.elapsed();
        let step = match &now {
            // No ledger at all: nothing has ever been built here.
            None => Step::Rebuild,
            Some(reading) => decide(previous.as_deref(), acted.as_deref(), reading),
        };
        match step {
            Step::Idle => {
                settling = None;
                said = None;
                if quiet_since.elapsed() >= HEARTBEAT {
                    let up_to_date = now.as_ref().map_or(0, |reading| reading.modules);
                    println!(
                        "watch   idle — {up_to_date} module(s) up to date, {passes} pass(es) so \
                         far, {rebuilds} rebuild(s), {:.3} s to ask",
                        asked.as_secs_f64(),
                    );
                    quiet_since = Instant::now();
                }
            }
            Step::Settling => {
                let reading = now.as_ref().expect("settling implies a reading");
                let stale = reading.what();
                match settling {
                    // The first pass of a settling stretch, which is also the
                    // ordinary case: a `lake build` finished between two passes
                    // and the very next pass rebuilds. Saying "a build is in
                    // flight" here would be a guess, and usually a wrong one.
                    None => {
                        settling = Some(Instant::now());
                        said = Some(Instant::now());
                        println!(
                            "watch   {stale} — waiting one quiet interval in case something \
                             is still writing oleans",
                        );
                    }
                    // Still moving several passes later: now it *is* a build in
                    // flight, and this is the line that must exist. #404 was a
                    // ten-minute silence read as a hang.
                    Some(started) => {
                        if said.is_none_or(|at| at.elapsed() >= SETTLE_REPORT) {
                            println!(
                                "watch   still not quiet after {:.0} s — {stale}, and the \
                                 answer keeps moving, so a build is in flight. Waiting; nothing \
                                 is stuck.",
                                started.elapsed().as_secs_f64(),
                            );
                            said = Some(Instant::now());
                        }
                    }
                }
            }
            // The same answer this loop has already acted on. Nothing is
            // printed: the pass that acted said what it did, and a module whose
            // source exists but whose olean does not is a state that persists
            // until the user builds — one line per second about it would bury
            // everything else.
            Step::Skip => {
                settling = None;
                said = None;
                // **The heartbeat fires here too.** This state can last for
                // hours — a source file that exists with no olean behind it
                // holds it until the user builds — and a terminal that has said
                // nothing for hours cannot be told from a dead one.
                if quiet_since.elapsed() >= HEARTBEAT {
                    let what = now.as_ref().map_or_else(String::new, Reading::what);
                    println!(
                        "watch   waiting — {what}, unchanged since the last pass acted on it, so \
                         there is nothing new to do ({:.3} s to ask)",
                        asked.as_secs_f64(),
                    );
                    quiet_since = Instant::now();
                }
            }
            Step::Rebuild => {
                settling = None;
                said = None;
                rebuilds += 1;
                let first = rebuilds == 1 && acted.is_none();
                announce(rebuilds, now.as_ref());
                match crate::build::run(request) {
                    Ok(ran) => {
                        println!(
                            "watch   #{rebuilds} {} — re-extracted {} module(s), re-rendered {} \
                             page(s), started Lean {} time(s) in {:.3} s{}",
                            ran.what,
                            ran.modules_extracted,
                            ran.pages_rendered,
                            ran.extractor_requests,
                            ran.seconds,
                            if ran.pages_rendered == 0 {
                                " — no page on the site changed"
                            } else {
                                ""
                            },
                        );
                        println!("watch   #{rebuilds} reload http://127.0.0.1:{port}/");
                    }
                    Err(failure) => {
                        // The first one is fatal: there is no site to serve and
                        // no state to continue from, so a loop here would be a
                        // loop over a broken configuration.
                        if first {
                            return Err(failure);
                        }
                        eprintln!(
                            "watch   #{rebuilds} the rebuild stopped: {}",
                            describe(&failure),
                        );
                        eprintln!(
                            "watch   #{rebuilds} not retrying until the oleans move again — a \
                             failing extraction restarted every {} ms would import 3 GB every {} \
                             ms",
                            interval.as_millis(),
                            interval.as_millis(),
                        );
                    }
                }
                acted = now.as_ref().map(|reading| reading.digest.clone());
                quiet_since = Instant::now();
            }
        }
        previous = now.as_ref().map(|reading| reading.digest.clone());
        thread::sleep(interval);
    }
}

/// The line a rebuild opens with: what the ledger said, before the run says what
/// it did about it.
fn announce(rebuilds: u64, now: Option<&Reading>) {
    println!();
    match now {
        // The first run of all, which is the one that can look like a hang: it
        // imports the package's Lean environment and extracts every module, and
        // on a Mathlib-sized package that is tens of seconds during which the
        // only thing on screen is the pipeline's own `detect` line. #404 was
        // exactly this shape — a long wait nobody had described in advance —
        // so it is described in advance.
        None => println!(
            "watch   #{rebuilds} nothing has been built under --out yet — generating the whole \
             site. This imports the package's Lean environment once and extracts every module, \
             which is the slowest thing this command does; the lines below are that run's own",
        ),
        Some(reading) => println!("watch   #{rebuilds} the ledger reports {}", reading.what(),),
    }
}

/// What one pass does.
#[derive(Debug, PartialEq, Eq)]
enum Step {
    /// The ledger says everything is up to date.
    Idle,
    /// There is work, but the answer moved since the last pass: something is
    /// still writing oleans.
    Settling,
    /// There is work, the world is quiet — and this is **bit for bit the answer
    /// the last pass already acted on**: the same olean hashes and the same
    /// lists.
    ///
    /// Two states reach here and both need it. A pass that *failed* must not be
    /// retried until something changes, or a broken extraction becomes a 3 GB
    /// Lean import per interval. And a source file whose olean does not exist
    /// yet is reported stale by every pass for as long as that is true — the
    /// rebuild cannot fix it, because what would fix it is `lake build` — so
    /// without this the loop would rebuild once a second for ever.
    ///
    /// It is also why `litedoc4 ledger touch` on the *same* module twice
    /// produces one rebuild and not two: the second reading is identical to the
    /// first. A real edit cannot be identical, because `lake build` moves the
    /// olean's content hash and the hashes are inside the digest.
    Skip,
    Rebuild,
}

/// The whole of the loop's judgement, as a function of three values.
///
/// Written as a pure function so that the four answers can be asserted without a
/// Lean toolchain, a package or a clock — the loop above is the part that cannot
/// be tested, so the part that decides is not in it.
fn decide(previous: Option<&str>, acted: Option<&str>, now: &Reading) -> Step {
    if !now.work() {
        return Step::Idle;
    }
    if previous != Some(now.digest.as_str()) {
        return Step::Settling;
    }
    if acted == Some(now.digest.as_str()) {
        return Step::Skip;
    }
    Step::Rebuild
}

// ---------------------------------------------------------------- the trigger

/// Everything one pass needs to ask the ledger the question `build` asks.
///
/// **Every field is fixed for the session, and that is the point.** The ledger
/// records a render key made of the source URL, the dependency map and the
/// documentation map; if the loop computed any of them differently from the run
/// it triggers, the two would take turns telling each other the key had moved
/// and the loop would re-render the whole site on every pass, for ever. So they
/// are resolved once, in [`watch`], and both sides read the same values —
/// `--deps-docs-url` is refused for the same reason (`crate::build`'s
/// `DEPS_DOCS_IN_WATCH`).
///
/// The module list is the exception: it is re-globbed every pass, because a
/// source file that appeared or vanished is one of the things this loop exists
/// to notice.
struct Trigger<'a> {
    ledger: &'a Path,
    ir: &'a Path,
    link_index: &'a Path,
    source_url: String,
    external_links: String,
    root: &'a Path,
    libs: &'a [String],
}

impl Trigger<'_> {
    /// One reading, or `None` when nothing has ever been built here.
    fn ask(&self) -> Result<Option<Reading>, Failure> {
        if !self.ledger.is_file() {
            return Ok(None);
        }
        let modules = crate::pipeline::module_names(self.root, self.libs)?;
        let check = check_ledger(&CheckOptions {
            ledger: self.ledger,
            // The ledger's own, never an override: two algorithms produce
            // incomparable hashes and every module would read as changed.
            algorithm: None,
            modules: Some(&modules),
            ir: Some(self.ir),
            source_url: &self.source_url,
            link_index: Some(self.link_index),
            external_links: Some(&self.external_links),
            concurrency: crate::pipeline::hash_concurrency(),
            // Nothing is written. A question that moves the state its answer is
            // about is one a caller cannot stop on (`litedoc4_incr::detect`).
            changed_out: None,
            removed_out: None,
            render_all_out: None,
            timings: None,
        })
        .map_err(refused)?;
        Ok(Some(Reading::of(&check)))
    }
}

/// One answer from the ledger, reduced to what the loop compares.
struct Reading {
    /// **The hashes and the answer together.** The hashes are what make "the
    /// world stopped moving" answerable: two passes during one `lake build` can
    /// report the same *list* of changed modules while the bytes underneath are
    /// still moving, and a digest over the list alone would call that quiet.
    digest: String,
    modules: usize,
    re_extract: usize,
    removed: usize,
    render_all: Vec<String>,
}

impl Reading {
    fn of(check: &CheckSummary) -> Self {
        let mut lines: Vec<String> = check
            .fresh
            .modules
            .iter()
            .map(|entry| format!("{} {}", entry.module, entry.hash))
            .collect();
        lines.push(format!("re-extract {}", check.re_extract.join(",")));
        lines.push(format!("removed {}", check.removed.join(",")));
        lines.push(format!("renderKey {}", check.render_key_changed.join(",")));
        lines.push(format!(
            "extractKey {}",
            check.extract_key_changed.join(",")
        ));
        Self {
            digest: sha256_text(&lines.join("\n")),
            modules: check.modules,
            re_extract: check.re_extract.len(),
            removed: check.removed.len(),
            render_all: check.render_key_changed.clone(),
        }
    }

    /// Whether a run would do anything at all.
    fn work(&self) -> bool {
        self.re_extract > 0 || self.removed > 0 || !self.render_all.is_empty()
    }

    /// The work in words, for a line that has to be true when the only thing
    /// stale is the render key and no module is.
    fn what(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.re_extract > 0 {
            parts.push(format!("{} module(s) to re-extract", self.re_extract));
        }
        if self.removed > 0 {
            parts.push(format!("{} removed", self.removed));
        }
        if !self.render_all.is_empty() {
            parts.push(format!(
                "the render key moved ({})",
                self.render_all.join(",")
            ));
        }
        if parts.is_empty() {
            return "nothing stale".to_owned();
        }
        parts.join(", ")
    }
}

/// A failure as one line, for a loop that reports it and carries on.
///
/// `main` prints these and exits; here they have to be printable without being
/// final, which is the whole difference between a command and a loop.
fn describe(failure: &Failure) -> String {
    match failure {
        Failure::Usage(message) | Failure::Failed(message) => message.clone(),
        Failure::Answered(code) => format!("exit {code}"),
        Failure::Refused { code, message } => format!("{message} (exit {code})"),
    }
}

// --------------------------------------------------------------------- tests

/// The command line and the decision, which are the two halves of this file that
/// own their input. The loop itself needs a package, a toolchain and an
/// extractor, so it is `tools/watch-gate.sh`.
#[cfg(test)]
mod tests {
    use super::*;

    fn reading(re_extract: usize, removed: usize, render_all: &[&str], digest: &str) -> Reading {
        Reading {
            digest: digest.to_owned(),
            modules: 422,
            re_extract,
            removed,
            render_all: render_all.iter().map(|name| (*name).to_owned()).collect(),
        }
    }

    #[test]
    fn nothing_stale_is_idle_whatever_the_history() {
        let quiet = reading(0, 0, &[], "a");
        assert_eq!(decide(None, None, &quiet), Step::Idle);
        assert_eq!(decide(Some("a"), Some("a"), &quiet), Step::Idle);
        assert_eq!(decide(Some("b"), Some("c"), &quiet), Step::Idle);
    }

    #[test]
    fn an_answer_that_moved_since_the_last_pass_is_a_build_in_flight() {
        let stale = reading(37, 0, &[], "a");
        assert_eq!(decide(None, None, &stale), Step::Settling);
        assert_eq!(decide(Some("earlier"), None, &stale), Step::Settling);
    }

    #[test]
    fn a_quiet_world_with_new_work_rebuilds_and_the_same_work_does_not_twice() {
        let stale = reading(1, 0, &[], "a");
        assert_eq!(decide(Some("a"), None, &stale), Step::Rebuild);
        assert_eq!(decide(Some("a"), Some("older"), &stale), Step::Rebuild);
        // The shape a source file with no olean produces: the answer is the same
        // every pass and stays that way until the user builds.
        assert_eq!(decide(Some("a"), Some("a"), &stale), Step::Skip);
    }

    #[test]
    fn a_moved_render_key_is_work_even_with_nothing_to_re_extract() {
        let keyed = reading(0, 0, &["sourceUrl"], "a");
        assert!(keyed.work());
        assert_eq!(decide(Some("a"), None, &keyed), Step::Rebuild);
        // …and a deletion is work with nothing to re-extract either.
        let deleted = reading(0, 1, &[], "a");
        assert!(deleted.work());
        assert_eq!(decide(Some("a"), None, &deleted), Step::Rebuild);
    }

    /// The one line every waiting message is made of, including the shape that
    /// has no stale module in it at all.
    #[test]
    fn the_work_is_described_by_what_is_actually_stale() {
        assert_eq!(reading(0, 0, &[], "a").what(), "nothing stale");
        assert_eq!(reading(1, 0, &[], "a").what(), "1 module(s) to re-extract");
        assert_eq!(reading(0, 2, &[], "a").what(), "2 removed");
        assert_eq!(
            reading(0, 0, &["sourceUrl"], "a").what(),
            "the render key moved (sourceUrl)",
            "a pass with nothing to re-extract must not report `0 module(s) stale`",
        );
        assert_eq!(
            reading(3, 1, &["linkIndex"], "a").what(),
            "3 module(s) to re-extract, 1 removed, the render key moved (linkIndex)",
        );
    }

    #[test]
    fn the_port_is_a_number_in_range_or_a_refusal_that_names_it() {
        let flags = Flags::default();
        assert_eq!(
            flags.port().expect("the default"),
            crate::httpd::DEFAULT_PORT
        );
        for (raw, needle) in [
            ("banana", "not `banana`"),
            ("70000", "not `70000`"),
            ("-1", "not `-1`"),
            ("0", "asks the kernel"),
        ] {
            let flags = Flags {
                port: Some(raw.to_owned()),
                interval: None,
            };
            let message = match flags.port() {
                Ok(port) => panic!("--port {raw} was accepted as {port}"),
                Err(failure) => describe(&failure),
            };
            assert!(message.contains(needle), "--port {raw}: {message}");
        }
        assert_eq!(
            Flags {
                port: Some("8484".to_owned()),
                interval: None,
            }
            .port()
            .expect("in range"),
            8484,
        );
    }

    #[test]
    fn the_interval_has_a_floor_and_it_says_what_the_floor_buys() {
        let flags = Flags::default();
        assert_eq!(flags.interval().expect("the default"), DEFAULT_INTERVAL);
        let too_fast = Flags {
            port: None,
            interval: Some("10".to_owned()),
        };
        let message = match too_fast.interval() {
            Ok(interval) => panic!("--interval 10 was accepted as {interval:?}"),
            Err(failure) => describe(&failure),
        };
        assert!(message.contains("reads every olean"), "{message}");
        assert!(message.contains("100 ms"), "{message}");
        assert_eq!(
            Flags {
                port: None,
                interval: Some("2500".to_owned()),
            }
            .interval()
            .expect("above the floor"),
            Duration::from_millis(2500),
        );
    }

    /// The flags each command refuses on behalf of the other, by name.
    ///
    /// Both directions, because a one-directional check would pass on the day
    /// `--port` silently became a `build` flag that does nothing.
    #[test]
    fn each_command_refuses_the_others_flags_by_name() {
        let build = |args: &[&str]| {
            let args: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
            crate::build::parse(&args, None).err().map(|f| describe(&f))
        };
        let watch = |args: &[&str]| {
            let args: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
            let mut flags = Flags::default();
            crate::build::parse(&args, Some(&mut flags))
                .err()
                .map(|f| describe(&f))
        };

        for (args, needle) in [
            (vec!["--port", "8484"], "is a `watch` flag"),
            (vec!["--interval", "500"], "is a `watch` flag"),
        ] {
            let message = build(&args).expect("refused");
            assert!(message.contains(needle), "{args:?}: {message}");
        }
        for (args, needle) in [
            (vec!["--full"], "is not a `watch` flag"),
            (
                vec!["--deps-docs-url", "Mathlib=https://example.invalid"],
                "over the network",
            ),
            (
                vec!["--deps-docs-index", "Mathlib=/tmp/table.bmp"],
                "over the network",
            ),
        ] {
            let message = watch(&args).expect("refused");
            assert!(message.contains(needle), "{args:?}: {message}");
        }
        // And the shared refusals still land on both: `--root` is required for
        // either, and the parser is one parser.
        assert!(
            watch(&["--out", "/tmp/x"])
                .expect("refused")
                .contains("--root"),
        );
    }
}
