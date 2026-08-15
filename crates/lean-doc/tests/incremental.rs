//! Milestone **M3-d2**: `lean-doc incremental` and `lean-doc modules`.
//!
//! Three oracles, none of which is this file's own opinion:
//!
//! - **Full generation.** `lean-doc site` over the world as it now is, compared
//!   with the tree an incremental round left behind, file by file, byte by byte.
//!   That is the composition of every gate M1 to M3-d1 passed, and it is the only
//!   statement that catches a stage that ran with the wrong input rather than one
//!   that crashed.
//! - **`experiments/stage7h/oracle.sh`, lifted one level.** Its seven states —
//!   base / rerun / modified / removed / added / restored / stale-state — are a
//!   sequence over IR trees carrying one cache; here they are a sequence over
//!   **source** states carrying an IR, a page tree, a ledger and a cache, with
//!   the comparison above made at every one.
//!   `lean-doc-global/tests/state_and_delta.rs` is the same seven states one
//!   layer down.
//! - **A fake extractor.** `--extractor` has no default (see `src/pipeline.rs`),
//!   so the extraction step is a `/bin/sh` script that copies a baked IR tree.
//!   Every other stage is the real one. Without this seam every test here would
//!   need a Lean toolchain, which in practice means none of them would exist.
//!
//! # Byte equality is not branch coverage (plan §7)
//!
//! Of the [`BRANCHES`] this milestone added:
//!
//! | exercise | reaches |
//! |---|---:|
//! | one ordinary round — one changed module, nothing else ([`ONE_RUN`]) | **18** |
//! | the seven states ([`SEVEN_STATES`]) | **32** |
//! | curated cases only ([`NO_SEVEN_STATE_RUN_REACHES`]) | **27** |
//!
//! Of 59. The dependency is asserted rather than commented:
//! [`the_curated_cases_cover_what_the_seven_states_do_not`] runs every curated
//! case and checks that together they reach all 27.

#![cfg(unix)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};

const BIN: &str = env!("CARGO_BIN_EXE_lean-doc");

/// Plan 決定 1: 40 lower-case hex digits after `/blob/`, or `coverage.ts:512`'s
/// revless normalisation misses and the acceptance score drops 3.1103 points
/// 【実測】. `incremental` is the one command that checks it.
const URL: &str =
    "https://example.invalid/owner/repo/blob/0123456789abcdef0123456789abcdef01234567";
/// A second revision, for the `renderKey` moved case: same everything, new
/// commit.
const URL_NEXT: &str =
    "https://example.invalid/owner/repo/blob/fedcba9876543210fedcba9876543210fedcba98";

/// The six whole-package artifacts. Three are `.html`, which is what makes plan
/// §7 debt 4 — `prune --ir` pointed at a site calls them orphans — a thing that
/// can happen at all.
const ARTIFACTS: [&str; 6] = [
    "declarations/declaration-data.bmp",
    "declarations/name-map.json",
    "navbar.html",
    "tactics.html",
    "references.html",
    "references.bib",
];

// --------------------------------------------------------------- the branches

/// Every branch M3-d2 added, named by an event of the run rather than by a line
/// of the code.
///
/// Each is decided by [`observe`] and [`observe_modules`] from what a run was
/// given and what it left on disk — never by asking the code under test what it
/// decided. Where a decision needs a rule (is this URL acceptable? which of the
/// two derivations produced the render set?) the rule is written out a second
/// time here.
const BRANCHES: [&str; 59] = [
    // the command line
    "sourceUrlAccepted",
    "sourceUrlNoBlobRefused",
    "sourceUrlNotHexRefused",
    "modeDefaulted",
    "modeGiven",
    "modeUnrecognisedRefused",
    "maxRoundsDefaulted",
    "maxRoundsGiven",
    "maxRoundsZeroRefused",
    "requiredFlagMissing",
    "retiredFlagRefused",
    "timingsWritten",
    "timingsOmitted",
    "extractorArgsGiven",
    "extractorArgsEmpty",
    "extractorFailed",
    // detect, and the snapshot that has to precede it
    "mapBeforeSnapshotted",
    "mapBeforeAbsent",
    "renderAllFired",
    "renderAllQuiet",
    "detectChangedEmpty",
    "detectChangedNonEmpty",
    "detectRemovedEmpty",
    "detectRemovedNonEmpty",
    // the round loop
    "roundsZero",
    "roundsOne",
    "roundsMany",
    "roundsExceeded",
    "roundExtracted",
    "roundExtractionSkipped",
    "deletionsInFirstRound",
    "deletionsNotRepeated",
    "irChangedEmpty",
    "irChangedNonEmpty",
    // prune
    "pruneRan",
    "pruneSkipped",
    "globalArtifactsSurvivedPrune",
    // global
    "deltaOn",
    "deltaOff",
    "globalSetEmpty",
    "globalSetNonEmpty",
    // impact, the union, render
    "impactSelectionMade",
    "impactSelectionSkipped",
    "renderSetFromImpactOnly",
    "renderSetFromGlobalOnly",
    "renderSetFromBoth",
    "renderSetEmpty",
    "renderSkipped",
    "renderRan",
    // the module glob
    "modulesLibFileOnly",
    "modulesLibDirOnly",
    "modulesLibBoth",
    "modulesLibMissingRefused",
    "modulesNested",
    "modulesNonLeanSkipped",
    "modulesRepeatedLib",
    "modulesToStdout",
    "modulesToFile",
    "modulesRequiredFlagMissing",
];

/// What one ordinary round reaches: one module's olean and docstring moved,
/// nothing added, nothing deleted, the default mode and the default round
/// bound.
///
/// **Eighteen of fifty-nine**, and measured by [`case_one_ordinary_round`]
/// rather than asserted here. Every deletion branch, both round-loop boundaries,
/// the whole global-only half of the render set and every refusal are invisible
/// to it — and that half is the one plan §7's debt 1 is about.
const ONE_RUN: [&str; 18] = [
    "deltaOn",
    "detectChangedNonEmpty",
    "detectRemovedEmpty",
    "extractorArgsGiven",
    "globalSetEmpty",
    "impactSelectionMade",
    "irChangedNonEmpty",
    "mapBeforeSnapshotted",
    "maxRoundsDefaulted",
    "modeDefaulted",
    "pruneSkipped",
    "renderAllQuiet",
    "renderRan",
    "renderSetFromImpactOnly",
    "roundExtracted",
    "roundsOne",
    "sourceUrlAccepted",
    "timingsWritten",
];

/// Thirty-two of fifty-nine: what the seven states reach, together —
/// **measured** by [`the_seven_states_match_full_generation`], not assumed.
const SEVEN_STATES: [&str; 32] = [
    "deletionsInFirstRound",
    "deltaOn",
    "detectChangedEmpty",
    "detectChangedNonEmpty",
    "detectRemovedEmpty",
    "detectRemovedNonEmpty",
    "extractorArgsGiven",
    "globalArtifactsSurvivedPrune",
    "globalSetEmpty",
    "globalSetNonEmpty",
    "impactSelectionMade",
    "impactSelectionSkipped",
    "irChangedEmpty",
    "irChangedNonEmpty",
    "mapBeforeSnapshotted",
    "maxRoundsDefaulted",
    "modeDefaulted",
    "pruneRan",
    "pruneSkipped",
    "renderAllQuiet",
    "renderRan",
    "renderSetEmpty",
    "renderSetFromBoth",
    "renderSetFromGlobalOnly",
    "renderSetFromImpactOnly",
    "renderSkipped",
    "roundExtracted",
    "roundExtractionSkipped",
    "roundsOne",
    "roundsZero",
    "sourceUrlAccepted",
    "timingsWritten",
];

/// The branches **no run of the seven-state sequence reaches**, whatever the
/// data. Twenty-seven of fifty-nine, in four groups.
///
/// - **Eleven are refusals** (`sourceUrl*Refused`, `modeUnrecognisedRefused`,
///   `maxRoundsZeroRefused`, `requiredFlagMissing`, `retiredFlagRefused`,
///   `extractorFailed`, `roundsExceeded`, `modulesLibMissingRefused`,
///   `modulesRequiredFlagMissing`). A sequence that passes asks for none of
///   them, which is the point: the whole surface of "this command line is wrong"
///   is invisible to any run that works.
/// - **Four are flags the pipeline always passes the same way**
///   (`timingsOmitted`, `extractorArgsEmpty`, `modeGiven`, `maxRoundsGiven`).
/// - **Five are shapes the seven states do not contain.** `roundsMany` and
///   `deletionsNotRepeated` need a *declaration moving between modules*, which
///   is `run.sh`'s end-to-end scenario rather than `oracle.sh`'s;
///   `renderAllFired` needs a new revision; `mapBeforeAbsent` and `deltaOff`
///   need a page tree with no `declarations/name-map.json`, which after a full
///   generation never happens.
/// - **Seven are `lean-doc modules`**, which the sequence calls once per state
///   and never varies.
const NO_SEVEN_STATE_RUN_REACHES: [&str; 27] = [
    "deletionsNotRepeated",
    "deltaOff",
    "extractorArgsEmpty",
    "extractorFailed",
    "mapBeforeAbsent",
    "maxRoundsGiven",
    "maxRoundsZeroRefused",
    "modeGiven",
    "modeUnrecognisedRefused",
    "modulesLibBoth",
    "modulesLibDirOnly",
    "modulesLibFileOnly",
    "modulesLibMissingRefused",
    "modulesNested",
    "modulesNonLeanSkipped",
    "modulesRepeatedLib",
    "modulesRequiredFlagMissing",
    "modulesToFile",
    "modulesToStdout",
    "renderAllFired",
    "requiredFlagMissing",
    "retiredFlagRefused",
    "roundsExceeded",
    "roundsMany",
    "sourceUrlNoBlobRefused",
    "sourceUrlNotHexRefused",
    "timingsOmitted",
];

// -------------------------------------------------------------- the observer

type Files = BTreeMap<PathBuf, Vec<u8>>;

/// One run of `lean-doc incremental`, with everything needed to say what it
/// reached.
struct Report {
    args: Vec<String>,
    code: i32,
    stderr: String,
    work: PathBuf,
    /// The page tree **before** the run: a run that skipped the renderer is only
    /// distinguishable from one that re-rendered identical bytes by this.
    pages_before: Files,
    pages_after: Files,
}

impl Report {
    fn flag(&self, name: &str) -> bool {
        self.args.iter().any(|arg| arg == name)
    }

    /// The **last** occurrence: a repeated flag is a later assignment
    /// overwriting an earlier one, which is what the harness does when it
    /// appends an override to a complete command line.
    fn value_of(&self, name: &str) -> Option<&str> {
        let at = self.args.iter().rposition(|arg| arg == name)?;
        self.args.get(at + 1).map(String::as_str)
    }

    fn count(&self, name: &str) -> usize {
        self.args.iter().filter(|arg| *arg == name).count()
    }

    fn work_file(&self, name: &str) -> Option<String> {
        fs::read_to_string(self.work.join(name)).ok()
    }

    fn work_set(&self, name: &str) -> Option<BTreeSet<String>> {
        self.work_file(name)
            .map(|text| lines(&text).into_iter().collect())
    }

    fn work_json(&self, name: &str) -> Option<Value> {
        serde_json::from_str(&self.work_file(name)?).ok()
    }

    /// How many rounds ran, counted from the files a round leaves behind rather
    /// than from the number the run reports.
    fn rounds(&self) -> usize {
        (1..)
            .take_while(|round| self.work.join(format!("round-in-{round}.txt")).is_file())
            .count()
    }
}

fn lines(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Plan 決定 1's rule, written a second time: `/blob/`, then 40 lower-case hex
/// digits, then the end of the string or a `/`.
fn revision_is_forty_hex(url: &str) -> bool {
    let Some((_, rest)) = url.split_once("/blob/") else {
        return false;
    };
    let rev = rest.split('/').next().unwrap_or(rest);
    rev.len() == 40
        && rev
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Whether the hand-written assertions run, as against the byte comparison
/// alone.
///
/// **This exists for the mutation survey and for nothing else.** Plan §7's
/// standing rule is that a whole-tree byte comparison is not branch coverage, and
/// the only way to put a number on that is to run the gate — the site and IR
/// comparisons against full generation — with every other check switched off and
/// see which mutants it still catches. `LEAN_DOC_GATE_ONLY=1` does that. It can
/// only ever *remove* checks, so an ordinary run (and CI, which sets nothing) is
/// the strict one.
fn beyond_the_gate() -> bool {
    std::env::var_os("LEAN_DOC_GATE_ONLY").is_none()
}

fn fire_into(fired: &mut BTreeSet<&'static str>, branch: &'static str) {
    assert!(
        BRANCHES.contains(&branch),
        "{branch} is not a counted branch"
    );
    fired.insert(branch);
}

/// Which branches one `incremental` run reached.
fn observe(run: &Report) -> BTreeSet<&'static str> {
    let mut fired: BTreeSet<&'static str> = BTreeSet::new();
    let mut fire = |branch: &'static str| fire_into(&mut fired, branch);

    // The command line, judged by this file's own reading of it.
    match run.value_of("--source-url") {
        Some(url) if revision_is_forty_hex(url) => fire("sourceUrlAccepted"),
        Some(url) if !url.contains("/blob/") => fire("sourceUrlNoBlobRefused"),
        Some(_) => fire("sourceUrlNotHexRefused"),
        None => {}
    }
    match run.value_of("--mode") {
        None => fire("modeDefaulted"),
        Some(text) if ["self", "referrers", "importers", "all"].contains(&text) => {
            fire("modeGiven")
        }
        Some(_) => fire("modeUnrecognisedRefused"),
    }
    match run.value_of("--max-rounds") {
        None => fire("maxRoundsDefaulted"),
        Some("0") => fire("maxRoundsZeroRefused"),
        Some(_) => fire("maxRoundsGiven"),
    }
    // **`--serve` is not among them any more** (M4-c): it is a live flag, and
    // `tests/resident.rs` is where it is judged. `--serve-dir` and `--serve-from`
    // stayed retired — see `src/resident.rs` for why a server this run did not
    // start is one it cannot vouch for.
    for retired in [
        "--jobs",
        "--l3-1",
        "--global",
        "--serve-dir",
        "--serve-from",
        "--count-reads",
        "--module",
        "--no-link-index",
    ] {
        if run.flag(retired) {
            fire("retiredFlagRefused");
        }
    }
    for required in [
        "--ir",
        "--pages",
        "--ledger",
        "--work",
        "--modules",
        "--source-url",
        "--link-index",
        "--state",
        "--extractor",
    ] {
        if !run.flag(required) {
            fire("requiredFlagMissing");
        }
    }
    if run.count("--extractor-arg") > 0 {
        fire("extractorArgsGiven");
    } else if run.flag("--extractor") {
        fire("extractorArgsEmpty");
    }
    match run.value_of("--timings") {
        Some(path) if Path::new(path).is_file() => fire("timingsWritten"),
        _ => fire("timingsOmitted"),
    }
    if run.code == 4 {
        fire("extractorFailed");
    }
    if run.code == 5 {
        fire("roundsExceeded");
    }

    // Everything below needs a run that got as far as `detect`.
    let Some(changed) = run.work_set("changed.txt") else {
        return fired;
    };
    fire(if run.work.join("name-map-before.json").is_file() {
        "mapBeforeSnapshotted"
    } else {
        "mapBeforeAbsent"
    });
    fire(if changed.is_empty() {
        "detectChangedEmpty"
    } else {
        "detectChangedNonEmpty"
    });
    let removed = run.work_set("removed.txt").unwrap_or_default();
    fire(if removed.is_empty() {
        "detectRemovedEmpty"
    } else {
        "detectRemovedNonEmpty"
    });
    fire(
        if run
            .work_set("render-all.txt")
            .unwrap_or_default()
            .is_empty()
        {
            "renderAllQuiet"
        } else {
            "renderAllFired"
        },
    );

    let rounds = run.rounds();
    fire(match rounds {
        0 => "roundsZero",
        1 => "roundsOne",
        _ => "roundsMany",
    });
    for round in 1..=rounds {
        if run
            .work
            .join(format!("extract-timings-{round}.json"))
            .is_file()
        {
            fire("roundExtracted");
        }
        if run
            .work_set(&format!("round-in-{round}.txt"))
            .unwrap_or_default()
            .is_empty()
        {
            fire("roundExtractionSkipped");
        }
        let dropped = run
            .work_json(&format!("merge-timings-{round}.json"))
            .and_then(|record| record["removed"].as_u64())
            .unwrap_or(0);
        if dropped > 0 {
            assert!(
                round == 1 || !beyond_the_gate(),
                "a deletion was folded into a round after the first",
            );
            fire("deletionsInFirstRound");
        } else if round > 1 && !removed.is_empty() {
            fire("deletionsNotRepeated");
        }
    }
    if rounds > 0 && run.work.join("ir-changed.txt").is_file() {
        fire(
            if run
                .work_set("ir-changed.txt")
                .unwrap_or_default()
                .is_empty()
            {
                "irChangedEmpty"
            } else {
                "irChangedNonEmpty"
            },
        );
    }
    // A run that stopped — the round bound, a failed extractor — reached none of
    // the stages below, and reading their absent files as "empty" would report
    // decisions nobody took.
    if run.code != 0 {
        return fired;
    }

    if run.work.join("prune.json").is_file() {
        fire("pruneRan");
        if ARTIFACTS
            .iter()
            .all(|path| run.pages_after.contains_key(&PathBuf::from(path)))
        {
            fire("globalArtifactsSurvivedPrune");
        }
    } else {
        fire("pruneSkipped");
    }

    let global_set = run.work_set("global-set.txt").unwrap_or_default();
    fire(if run.work.join("name-map-before.json").is_file() {
        "deltaOn"
    } else {
        "deltaOff"
    });
    fire(if global_set.is_empty() {
        "globalSetEmpty"
    } else {
        "globalSetNonEmpty"
    });

    let impact_set = run.work_set("impact-set.txt");
    fire(if impact_set.is_some() {
        "impactSelectionMade"
    } else {
        "impactSelectionSkipped"
    });
    let impact_set = impact_set.unwrap_or_default();
    let render_set = run.work_set("render-set.txt").unwrap_or_default();
    // The union, recomputed here from the two halves the stages wrote.
    let union: BTreeSet<String> = impact_set.union(&global_set).cloned().collect();
    assert!(
        render_set == union || !beyond_the_gate(),
        "the render set is not the union of the two derivations",
    );
    fire(match (impact_set.is_empty(), global_set.is_empty()) {
        (true, true) => "renderSetEmpty",
        (false, true) => "renderSetFromImpactOnly",
        (true, false) => "renderSetFromGlobalOnly",
        (false, false) => "renderSetFromBoth",
    });
    // **Every module whose IR bytes moved is in the render set.** Not a branch:
    // an invariant, and the one a pipeline breaks when it hands `impact` the
    // ledger's changed set instead of the round loop's `seen` — the modules L3-1
    // found are exactly the ones whose IR moved without their olean moving, and
    // dropping them under-renders silently.
    let ir_changed = run.work_set("ir-changed.txt").unwrap_or_default();
    assert!(
        ir_changed.is_subset(&render_set) || !beyond_the_gate(),
        "the IR moved for {:?} and they are not in the render set {render_set:?}",
        ir_changed.difference(&render_set).collect::<Vec<_>>(),
    );
    let skipped = run
        .work_file("render-timings.json")
        .is_some_and(|text| text.contains("skipped"));
    fire(if skipped {
        "renderSkipped"
    } else {
        "renderRan"
    });
    if skipped {
        // The **module pages** only: the six whole-package artifacts are
        // `global`'s, and step 6 rewrites them on every run whatever the render
        // set is.
        let module_pages = |files: &Files| -> Files {
            files
                .iter()
                .filter(|(path, _)| !ARTIFACTS.contains(&path.to_string_lossy().as_ref()))
                .map(|(path, bytes)| (path.clone(), bytes.clone()))
                .collect()
        };
        assert!(
            module_pages(&run.pages_before) == module_pages(&run.pages_after) || !beyond_the_gate(),
            "the renderer was skipped and a module page moved anyway",
        );
    }
    fired
}

/// One run of `lean-doc modules`, and the library layout it was pointed at.
struct ModulesReport {
    args: Vec<String>,
    code: i32,
    stdout: String,
    /// Which of `<Lib>.lean` / `<Lib>/` existed, per `--lib`, decided here from
    /// the filesystem.
    shapes: Vec<(bool, bool)>,
    nested: bool,
    other_files: bool,
    out: Option<PathBuf>,
}

fn observe_modules(run: &ModulesReport) -> BTreeSet<&'static str> {
    let mut fired: BTreeSet<&'static str> = BTreeSet::new();
    let mut fire = |branch: &'static str| fire_into(&mut fired, branch);
    if !run.args.iter().any(|arg| arg == "--root") || !run.args.iter().any(|arg| arg == "--lib") {
        fire("modulesRequiredFlagMissing");
        return fired;
    }
    if run.args.iter().filter(|arg| *arg == "--lib").count() > 1 {
        fire("modulesRepeatedLib");
    }
    for (file, dir) in &run.shapes {
        match (file, dir) {
            (true, true) => fire("modulesLibBoth"),
            (true, false) => fire("modulesLibFileOnly"),
            (false, true) => fire("modulesLibDirOnly"),
            (false, false) => fire("modulesLibMissingRefused"),
        }
    }
    if run.nested {
        fire("modulesNested");
    }
    if run.other_files {
        fire("modulesNonLeanSkipped");
    }
    if run.code == 0 {
        match &run.out {
            Some(path) if path.is_file() => fire("modulesToFile"),
            _ => {
                assert!(!run.stdout.is_empty(), "no --out and nothing on stdout");
                fire("modulesToStdout");
            }
        }
    }
    fired
}

// ------------------------------------------------------------------ the world

/// One module of the synthetic package.
#[derive(Clone)]
struct ModuleSpec {
    name: String,
    /// The compiled artifact's bytes. **Separate from the IR on purpose**: stage
    /// 5c measured that moving a declaration out of A leaves the referring
    /// module B's olean byte-identical while B's IR changes, which is the whole
    /// reason L3-1 exists. A fixture that derived one from the other could not
    /// contain that case.
    olean: String,
    imports: Vec<String>,
    decls: Vec<DeclSpec>,
}

#[derive(Clone)]
struct DeclSpec {
    name: String,
    doc: Option<String>,
    /// `(defining module, name)`, as the IR stores them.
    refs: Vec<(String, String)>,
}

impl DeclSpec {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            doc: None,
            refs: Vec::new(),
        }
    }

    fn doc(mut self, doc: &str) -> Self {
        self.doc = Some(doc.to_owned());
        self
    }

    fn reference(mut self, module: &str, name: &str) -> Self {
        self.refs.push((module.to_owned(), name.to_owned()));
        self
    }
}

/// The package, in the order a source glob produces (plan §7, U1 — every name
/// here is ASCII, where every order agrees).
#[derive(Clone)]
struct World(Vec<ModuleSpec>);

impl World {
    fn module(&mut self, name: &str) -> &mut ModuleSpec {
        self.0
            .iter_mut()
            .find(|module| module.name == name)
            .unwrap_or_else(|| panic!("no module {name} in the world"))
    }

    fn drop_module(&mut self, name: &str) {
        let before = self.0.len();
        self.0.retain(|module| module.name != name);
        assert_eq!(before - 1, self.0.len(), "no module {name} to drop");
    }

    fn insert(&mut self, module: ModuleSpec) {
        self.0.push(module);
        self.0.sort_by(|a, b| a.name.cmp(&b.name));
    }

    fn names(&self) -> Vec<String> {
        self.0.iter().map(|module| module.name.clone()).collect()
    }

    /// `Pkg.A.moved` leaves `Pkg.A` for `Pkg.X`: both oleans move, `Pkg.B`'s
    /// does not, and `Pkg.B`'s IR reference now names the wrong module. The one
    /// edit that needs a second round.
    fn move_a_declaration(&mut self) {
        self.module("Pkg.A").olean = "olean:Pkg.A:1".to_owned();
        self.module("Pkg.A").decls = vec![DeclSpec::new("Pkg.A.stay")];
        self.module("Pkg.X").olean = "olean:Pkg.X:1".to_owned();
        self.module("Pkg.X").decls = vec![
            DeclSpec::new("Pkg.X.seed").reference("Pkg.X", "Pkg.A.moved"),
            DeclSpec::new("Pkg.A.moved"),
        ];
        self.module("Pkg.B").decls = vec![
            DeclSpec::new("Pkg.B.b")
                .doc("Uses `Pkg.A.moved`, and one day `Pkg.Added.a`.")
                .reference("Pkg.X", "Pkg.A.moved"),
        ];
        // Only when it is still in the package: one case deletes it and moves a
        // declaration in the same step.
        if self.0.iter().any(|module| module.name == "Pkg.Leaf") {
            self.module("Pkg.Leaf").decls = vec![
                DeclSpec::new("Pkg.Leaf.l")
                    .doc("A leaf nobody imports.")
                    .reference("Pkg.X", "Pkg.A.moved"),
            ];
        }
    }

    /// One module's olean and docstring move together: the ordinary edit.
    fn edit_a_docstring(&mut self, tag: &str) {
        self.module("Pkg.C").olean = format!("olean:Pkg.C:{tag}");
        self.module("Pkg.C").decls[0].doc = Some(format!(
            "Revision {tag}. Mentions `Pkg.Leaf.l` and `Dep.Home.other`."
        ));
    }
}

/// The base package.
///
/// Five relationships are load-bearing and each is used by exactly one state:
///
/// - `Pkg.C`'s docstring mentions `Pkg.Leaf.l` **without a `refs` entry**, so
///   deleting `Pkg.Leaf` moves the whole-package name map and moves *no*
///   module's IR. That is the shape plan §7 debt 1 loses.
/// - `Pkg.B`'s docstring mentions `Pkg.Added.a`, a name nothing defines yet, so
///   adding `Pkg.Added` makes `Pkg.B` stale through the map and `Pkg.Added`
///   stale through the changed set — both halves of the render set at once.
/// - `Pkg.B` **refers to** `Pkg.A.moved`, so moving that declaration to `Pkg.X`
///   leaves `Pkg.B`'s olean alone and its IR wrong: the second round.
/// - `Pkg.C` refers to `Dep.elsewhere`, so `deps/Dep.json` is non-empty and the
///   merge has to rebuild it to the from-scratch bytes.
/// - `Pkg` imports nothing and `Pkg.Leaf` is imported by nothing, which keeps
///   the deletion a leaf deletion.
fn base_world() -> World {
    World(vec![
        ModuleSpec {
            name: "Pkg".to_owned(),
            olean: "olean:Pkg:0".to_owned(),
            imports: vec![],
            decls: vec![DeclSpec::new("Pkg.core").doc("The root. See `Pkg.A.moved`.")],
        },
        ModuleSpec {
            name: "Pkg.A".to_owned(),
            olean: "olean:Pkg.A:0".to_owned(),
            imports: vec!["Pkg".to_owned()],
            decls: vec![DeclSpec::new("Pkg.A.moved"), DeclSpec::new("Pkg.A.stay")],
        },
        ModuleSpec {
            name: "Pkg.B".to_owned(),
            olean: "olean:Pkg.B:0".to_owned(),
            imports: vec!["Pkg".to_owned(), "Pkg.A".to_owned()],
            decls: vec![
                DeclSpec::new("Pkg.B.b")
                    .doc("Uses `Pkg.A.moved`, and one day `Pkg.Added.a`.")
                    .reference("Pkg.A", "Pkg.A.moved"),
            ],
        },
        ModuleSpec {
            name: "Pkg.C".to_owned(),
            olean: "olean:Pkg.C:0".to_owned(),
            imports: vec!["Pkg".to_owned()],
            decls: vec![
                DeclSpec::new("Pkg.C.c")
                    .doc("Mentions `Pkg.Leaf.l` and `Dep.Home.other`.")
                    .reference("Dep.Home", "Dep.elsewhere"),
            ],
        },
        ModuleSpec {
            name: "Pkg.Leaf".to_owned(),
            olean: "olean:Pkg.Leaf:0".to_owned(),
            imports: vec!["Pkg".to_owned()],
            decls: vec![
                DeclSpec::new("Pkg.Leaf.l")
                    .doc("A leaf nobody imports.")
                    .reference("Pkg.A", "Pkg.A.moved"),
            ],
        },
        ModuleSpec {
            name: "Pkg.X".to_owned(),
            olean: "olean:Pkg.X:0".to_owned(),
            imports: vec!["Pkg".to_owned()],
            decls: vec![DeclSpec::new("Pkg.X.seed").reference("Pkg.A", "Pkg.A.moved")],
        },
    ])
}

/// Lean's `String.hash` stands in for FNV-1a here: 16 hex digits, derived from
/// the module's bytes and from nothing else, which is the only property the
/// `contentHash` cache and the merge's staleness test rely on.
fn content_hash(body: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in body.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn decl_json(decl: &DeclSpec) -> Value {
    json!({
        "binderCode": [], "binders": [], "col": 0, "doc": decl.doc,
        "endCol": 1, "endLine": 1, "equationCode": [], "equations": [],
        "implicits": [], "index": 0, "kind": "def", "line": 1, "members": [],
        "modifiers": [], "name": decl.name,
        "refs": decl.refs.iter().map(|(module, name)| json!([module, name])).collect::<Vec<_>>(),
        "type": "Prop", "typeCode": [],
    })
}

/// Writes the IR a full extraction of `world` would produce, plus one file per
/// index entry for the fake extractor to splice.
///
/// The three orders that decide the bytes are written out here a second time
/// (plan §7, M3-b): `deps/<Root>.json` has its top-level keys and its
/// declaration names in **code point** order, `index.json`'s `dependencyMaps`
/// entries are keyed `bytes` / `entries` / `file` / `package`, and that array is
/// in code point order of the root. If `merge` ever stops agreeing, the seven
/// states say so at `index.json` rather than somewhere downstream.
fn write_world(root: &Path, world: &World) {
    let ir = root.join("ir");
    let entries_dir = root.join("entries");
    let _ = fs::remove_dir_all(&ir);
    let _ = fs::remove_dir_all(&entries_dir);

    let own: BTreeSet<&str> = world.0.iter().map(|m| m.name.as_str()).collect();
    let mut index_entries: Vec<Value> = Vec::new();
    let mut declarations = 0usize;
    let mut dep: BTreeMap<String, String> = BTreeMap::new();
    for module in &world.0 {
        let file = format!("modules/{}.json", module.name);
        let body = serde_json::to_string(&json!({
            "declarations": module.decls.iter().map(decl_json).collect::<Vec<_>>(),
            "imports": module.imports,
            "module": module.name,
            "moduleDocs": [],
            "schemaVersion": 4,
            "tactics": [],
        }))
        .expect("serialises");
        write(&ir.join(&file), body.as_bytes());
        declarations += module.decls.len();
        let entry = json!({
            "bytes": body.len(),
            "contentHash": content_hash(&body),
            "declarations": module.decls.len(),
            "file": file,
            "module": module.name,
        });
        write(
            &entries_dir.join(format!("{}.json", module.name)),
            serde_json::to_string(&entry)
                .expect("serialises")
                .as_bytes(),
        );
        index_entries.push(entry);
        for decl in &module.decls {
            for (owner, name) in &decl.refs {
                if !own.contains(owner.as_str()) {
                    dep.insert(name.clone(), owner.clone());
                }
            }
        }
    }

    let mut by_root: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for (name, module) in &dep {
        by_root
            .entry(module.split('.').next().unwrap_or("").to_owned())
            .or_default()
            .insert(name.clone(), module.clone());
    }
    let mut dependency_maps: Vec<Value> = Vec::new();
    for (package, declarations) in &by_root {
        let body = serde_json::to_string(&json!({
            "declarations": declarations, "package": package, "schemaVersion": 4,
        }))
        .expect("serialises");
        let file = format!("deps/{package}.json");
        write(&ir.join(&file), body.as_bytes());
        dependency_maps.push(json!({
            "bytes": body.len(), "entries": declarations.len(),
            "file": file, "package": package,
        }));
    }
    fs::create_dir_all(ir.join("deps")).expect("writable");

    write(
        &ir.join("index.json"),
        serde_json::to_string(&json!({
            "declarationCount": declarations,
            "dependencyMaps": dependency_maps,
            "generator": "lean-doc/crates/lean-doc/tests/incremental.rs",
            "hashAlgorithm": "lean-string-hash-64/hex16",
            "leanVersion": "4.31.0",
            "moduleCount": world.0.len(),
            "modules": index_entries,
            "schemaVersion": 4,
        }))
        .expect("serialises")
        .as_bytes(),
    );
}

/// Writes the repository the ledger hashes: the sources the glob finds and the
/// oleans `detect` reads.
fn write_target(repo: &Path, world: &World) {
    let _ = fs::remove_dir_all(repo.join("Pkg"));
    let _ = fs::remove_file(repo.join("Pkg.lean"));
    let _ = fs::remove_dir_all(repo.join(".lake"));
    write(&repo.join("lean-toolchain"), b"leanprover/lean4:v4.31.0\n");
    write(
        &repo.join("lake-manifest.json"),
        br#"{"version":"1.1.0","packages":[]}"#,
    );
    for module in &world.0 {
        let path = module.name.replace('.', "/");
        write(&repo.join(format!("{path}.lean")), b"-- a source file\n");
        write(
            &repo.join(format!(".lake/build/lib/lean/{path}.olean")),
            module.olean.as_bytes(),
        );
    }
}

/// A dependency closure holding the one name no module defines.
fn write_lidx(path: &Path) {
    write(
        path,
        b"#lidx1\n@Dep.Home\nDep.Home\n\tDep.elsewhere\n\tDep.Home.other\n",
    );
}

/// The fake extractor: `--modules` in, a partial IR tree out.
///
/// A `/bin/sh` script rather than a second Rust binary, because a bin target
/// would ship with the product. It is passed as `--extractor /bin/sh
/// --extractor-arg <script>`, which also exercises the argument pass-through,
/// and it is deterministic: every byte it writes is copied from the baked tree,
/// including the `index.json` entries, so an incrementally merged IR can be
/// compared with a from-scratch one byte for byte.
///
/// It appends its whole command line to `<work>/extractor-calls.txt`, which is
/// how [`case_extractor_contract`] checks the three flags without owning the
/// code that passes them.
fn write_fake_extractor(path: &Path) {
    write(
        path,
        br#"#!/bin/sh
# The fake extractor of crates/lean-doc/tests/incremental.rs.
set -eu
WORLD=""; MODULES=""; IRDIR=""; TIMINGS=""; FAIL=0
while [ $# -gt 0 ]; do
  case "$1" in
    --world) WORLD="$2"; shift 2 ;;
    --modules) MODULES="$2"; shift 2 ;;
    --ir-dir) IRDIR="$2"; shift 2 ;;
    --timings) TIMINGS="$2"; shift 2 ;;
    --fail) FAIL=1; shift ;;
    *) echo "fake extractor: unknown option: $1" >&2; exit 2 ;;
  esac
done
[ -n "$MODULES" ] && [ -n "$IRDIR" ] && [ -n "$TIMINGS" ] || {
  echo "fake extractor: --modules, --ir-dir and --timings are all required" >&2; exit 2; }
WORK=$(dirname "$TIMINGS")
echo "--world $WORLD --modules $MODULES --ir-dir $IRDIR --timings $TIMINGS" \
  >> "$WORK/extractor-calls.txt"
[ "$FAIL" = 0 ] || { echo "fake extractor: asked to fail" >&2; exit 3; }
mkdir -p "$IRDIR/modules"
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
  printf '{"declarationCount":0,"dependencyMaps":[],'
  printf '"generator":"fake-extractor","hashAlgorithm":"lean-string-hash-64/hex16",'
  printf '"leanVersion":"4.31.0","moduleCount":%s,"modules":[' "$n"
  cat "$ENTRIES"
  printf '],"schemaVersion":4}'
} > "$IRDIR/index.json"
rm -f "$ENTRIES"
printf '{"targetModules":%s,"extractor":"fake"}\n' "$n" > "$TIMINGS"
"#,
    );
    let mut perms = fs::metadata(path).expect("the script exists").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).expect("the script is chmod-able");
}

// -------------------------------------------------------------- the harness

/// One live tree: the IR, the pages, the ledger and the cache an incremental
/// round carries forward, plus the repository they describe.
struct Live {
    trees: TempDir,
    repo: PathBuf,
    ir: PathBuf,
    pages: PathBuf,
    ledger: PathBuf,
    state: PathBuf,
    lidx: PathBuf,
    script: PathBuf,
    modules: PathBuf,
    world_root: PathBuf,
    runs: usize,
}

impl Live {
    /// Full generation over `world`, then a ledger over the repository that
    /// produced it.
    ///
    /// **This is where the cache comes from** (plan §7 debt 8): an incremental
    /// round's premise is that the previous run left a `--state` behind, and
    /// `lean-doc site --state` is the run that does it.
    fn setup(what: &str, world: &World) -> Self {
        let trees = TempDir::new(what);
        let live = Self {
            repo: trees.path.join("repo"),
            ir: trees.path.join("live/ir"),
            pages: trees.path.join("live/pages"),
            ledger: trees.path.join("live/ledger.json"),
            state: trees.path.join("live/state"),
            lidx: trees.path.join("link-index.lidx"),
            script: trees.path.join("fake-extractor.sh"),
            modules: trees.path.join("live/modules.txt"),
            world_root: trees.path.join("world"),
            trees,
            runs: 0,
        };
        write_lidx(&live.lidx);
        write_fake_extractor(&live.script);
        write_target(&live.repo, world);
        write_world(&live.world_root, world);
        copy_tree(&live.world_root.join("ir"), &live.ir);

        let ok = lean_doc(&[
            "site",
            "--ir",
            &live.ir.display().to_string(),
            "--out",
            &live.pages.display().to_string(),
            "--source-url",
            URL,
            "--link-index",
            &live.lidx.display().to_string(),
            "--state",
            &live.state.display().to_string(),
        ]);
        assert_eq!(code(&ok), 0, "{}", stderr(&ok));
        live.write_module_list();
        live.refresh_ledger();
        live
    }

    fn write_module_list(&self) {
        let ok = lean_doc(&[
            "modules",
            "--root",
            &self.repo.display().to_string(),
            "--lib",
            "Pkg",
            "--out",
            &self.modules.display().to_string(),
        ]);
        assert_eq!(code(&ok), 0, "{}", stderr(&ok));
    }

    /// The ledger, as the repository is now.
    ///
    /// **The caller's job, not the pipeline's.** `incremental` reads the ledger
    /// and never rewrites it, and neither does `incremental.sh` — `run.sh:167`
    /// re-seeds `base-ledger.json` for every variant it measures. Who owns this
    /// between two real runs is M4's question; here it is done explicitly, so
    /// that the seven states are seven states and not one repeated.
    fn refresh_ledger(&self) {
        let ok = lean_doc(&[
            "ledger",
            "build",
            "--modules",
            &self.modules.display().to_string(),
            "--target",
            &self.repo.display().to_string(),
            "--ir",
            &self.ir.display().to_string(),
            "--source-url",
            URL,
            "--out",
            &self.ledger.display().to_string(),
        ]);
        assert_eq!(code(&ok), 0, "{}", stderr(&ok));
    }

    /// Puts the repository and the baked world into `world`'s state, then
    /// re-reads the module list. The ledger is **not** touched: it still
    /// describes the previous state, which is what makes the change visible.
    fn advance(&self, world: &World) {
        write_target(&self.repo, world);
        write_world(&self.world_root, world);
        self.write_module_list();
    }

    /// One incremental round, with `extra` appended to the command line.
    fn round(&mut self, what: &str, extra: &[&str]) -> Report {
        self.runs += 1;
        let work = self.trees.path.join(format!("work-{}-{what}", self.runs));
        let timings = work.join("timings.json");
        let mut args: Vec<String> = [
            "incremental",
            "--ir",
            &self.ir.display().to_string(),
            "--pages",
            &self.pages.display().to_string(),
            "--ledger",
            &self.ledger.display().to_string(),
            "--work",
            &work.display().to_string(),
            "--modules",
            &self.modules.display().to_string(),
            "--source-url",
            URL,
            "--link-index",
            &self.lidx.display().to_string(),
            "--state",
            &self.state.display().to_string(),
            "--extractor",
            "/bin/sh",
            "--extractor-arg",
            &self.script.display().to_string(),
            "--extractor-arg",
            "--world",
            "--extractor-arg",
            &self.world_root.display().to_string(),
            "--timings",
            &timings.display().to_string(),
        ]
        .iter()
        .map(|arg| (*arg).to_owned())
        .collect();
        args.extend(extra.iter().map(|arg| (*arg).to_owned()));
        self.invoke(&args, &work)
    }

    /// Runs whatever command line it is handed, so that a refusal is observed by
    /// the same code path as a success.
    fn invoke(&self, args: &[String], work: &Path) -> Report {
        let pages_before = tree(&self.pages);
        let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
        let output = lean_doc(&borrowed);
        Report {
            args: args.to_vec(),
            code: code(&output),
            stderr: stderr(&output),
            work: work.to_owned(),
            pages_before,
            pages_after: tree(&self.pages),
        }
    }

    /// Full generation over `world` into a fresh tree — the oracle every state
    /// is compared against. Returns the root, holding `ir/` and `site/`.
    fn full_generation(&self, what: &str, world: &World, url: &str) -> PathBuf {
        let root = self.trees.path.join(format!("full-{what}"));
        let _ = fs::remove_dir_all(&root);
        write_world(&root.join("baked"), world);
        let ir = root.join("ir");
        copy_tree(&root.join("baked").join("ir"), &ir);
        let ok = lean_doc(&[
            "site",
            "--ir",
            &ir.display().to_string(),
            "--out",
            &root.join("site").display().to_string(),
            "--source-url",
            url,
            "--link-index",
            &self.lidx.display().to_string(),
        ]);
        assert_eq!(code(&ok), 0, "{what}: {}", stderr(&ok));
        root
    }

    /// The pages this round produced against the pages full generation
    /// produces. Returns the denominator — the number of files compared — and
    /// panics with the differing paths, which is the only failure message worth
    /// reading here.
    fn assert_site_matches(&self, what: &str, full: &Path) -> usize {
        let site = tree(&full.join("site"));
        let pages = tree(&self.pages);
        assert_eq!(
            pages.keys().collect::<Vec<_>>(),
            site.keys().collect::<Vec<_>>(),
            "{what}: the incremental tree and the full one hold different files",
        );
        let differing: Vec<&PathBuf> = pages
            .keys()
            .filter(|path| pages[*path] != site[*path])
            .collect();
        assert!(
            differing.is_empty(),
            "{what}: {} of {} page(s) differ from full generation: {differing:?}",
            differing.len(),
            site.len(),
        );
        site.len()
    }
}

/// What comparing the live IR against a from-scratch one found.
struct IrVerdict {
    same: usize,
    differing: Vec<String>,
    /// True when the only difference is `index.json`, and that file holds the
    /// same entries in a different sequence.
    ///
    /// **Nothing may reach this any more** (M3-d2b): it is kept so that a
    /// failure can say *which* divergence came back — the module order, or
    /// something that changed the data — instead of only that one did.
    index_order_only: bool,
}

/// Compares two IR trees file by file.
///
/// **This used to have a registered exception, and no longer does** (M3-d2b).
/// `merge` appended a module the base index did not have, while a from-scratch
/// extraction lists modules in the order the extractor was handed them — the
/// glob's — so a run that *added* a module produced an `index.json` with the
/// same entries, the same counts and the same dependency maps in a different
/// sequence (`added` / `restored` / `stale-state`, 54 of 57 files). The pipeline
/// now hands `merge` the same module list it hands `detect`, so the two orders
/// are one and every file of every state is compared without an excuse.
fn compare_ir(live: &Path, scratch: &Path) -> IrVerdict {
    let a = tree(live);
    let b = tree(scratch);
    assert_eq!(
        a.keys().collect::<Vec<_>>(),
        b.keys().collect::<Vec<_>>(),
        "the two IR trees hold different files",
    );
    let mut verdict = IrVerdict {
        same: 0,
        differing: Vec::new(),
        index_order_only: false,
    };
    for (path, bytes) in &a {
        if bytes == &b[path] {
            verdict.same += 1;
        } else {
            verdict.differing.push(path.display().to_string());
        }
    }
    verdict.differing.sort();
    if verdict.differing == ["index.json"] {
        let read = |root: &Path| -> Value {
            serde_json::from_str(&fs::read_to_string(root.join("index.json")).expect("an index"))
                .expect("the index is JSON")
        };
        let (mut left, mut right) = (read(live), read(scratch));
        let take_modules = |value: &mut Value| -> BTreeSet<String> {
            value["modules"]
                .take()
                .as_array()
                .expect("an array")
                .iter()
                .map(|entry| serde_json::to_string(entry).expect("serialises"))
                .collect()
        };
        verdict.index_order_only =
            take_modules(&mut left) == take_modules(&mut right) && left == right;
    }
    verdict
}

// ------------------------------------------------------------- the sequence

/// One state: advance the world, run the round, compare against full
/// generation, then hand the ledger on to the next state.
fn step(live: &mut Live, what: &str, world: &World) -> (BTreeSet<&'static str>, IrVerdict) {
    live.advance(world);
    let report = live.round(what, &[]);
    assert_eq!(report.code, 0, "{what}: {}", report.stderr);
    let covered = observe(&report);
    let full = live.full_generation(what, world, URL);
    let site = live.assert_site_matches(what, &full);
    let ir = compare_ir(&live.ir, &full.join("ir"));
    // The denominators, printed rather than only asserted: a gate whose母数 is
    // not in the log is one nobody can quote (`cargo test -- --nocapture`).
    println!(
        "  {what:<12} site {site}/{site} byte-identical   IR {}/{} byte-identical{}",
        ir.same,
        ir.same + ir.differing.len(),
        if ir.differing.is_empty() {
            String::new()
        } else {
            format!("   (differing: {})", ir.differing.join(", "))
        },
    );
    live.refresh_ledger();
    (covered, ir)
}

/// **The gate.** The seven states of `stage7h/oracle.sh`, over the whole
/// pipeline, each compared with `lean-doc site` over the same world.
///
/// The **site** is identical in all seven, which is the statement plan §1's gate
/// is about. The IR is compared too, and since M3-d2b that answer is "identical"
/// everywhere as well — see [`compare_ir`] for the exception that used to be
/// registered here.
#[test]
fn the_seven_states_match_full_generation() {
    let mut world = base_world();
    let mut live = Live::setup("seven-states", &world);
    let mut covered: BTreeSet<&'static str> = BTreeSet::new();
    let mut verdicts: Vec<(&str, IrVerdict)> = Vec::new();

    // 1. the unchanged world. Nothing was extracted, nothing was rendered, and
    //    the site is still the one full generation writes.
    let (fired, ir) = step(&mut live, "base", &world);
    if beyond_the_gate() {
        assert!(fired.contains("roundsZero"), "base ran a round");
        assert!(fired.contains("renderSkipped"));
    }
    covered.extend(&fired);
    verdicts.push(("base", ir));

    // 2. the same world again. A pipeline that is not idempotent fails here and
    //    nowhere earlier.
    let (fired, ir) = step(&mut live, "rerun", &world);
    if beyond_the_gate() {
        assert!(fired.contains("renderSetEmpty"));
    }
    covered.extend(&fired);
    verdicts.push(("rerun", ir));

    // 3. one module's docstring changes: its olean moves, its IR moves, its page
    //    moves and nothing else does.
    world.edit_a_docstring("1");
    let (fired, ir) = step(&mut live, "modified", &world);
    if beyond_the_gate() {
        assert!(fired.contains("renderSetFromImpactOnly"), "{fired:?}");
    }
    covered.extend(&fired);
    verdicts.push(("modified", ir));

    // 4. a leaf module disappears. **Plan §7's debt 1**: no module changed, so
    //    `impact` writes no selection at all, and the only stale page —
    //    `Pkg.C`, whose docstring links a name that has just left the package —
    //    is named by the global map delta alone.
    world.drop_module("Pkg.Leaf");
    let (fired, ir) = step(&mut live, "removed", &world);
    if beyond_the_gate() {
        assert!(
            fired.contains("renderSetFromGlobalOnly"),
            "the deletion was visible to `impact`, so this state is not debt 1's: {fired:?}",
        );
        assert!(fired.contains("impactSelectionSkipped"), "{fired:?}");
        assert!(
            fired.contains("globalArtifactsSurvivedPrune"),
            "plan §7 debt 4: the whole-package artifacts did not survive the prune",
        );
    }
    covered.extend(&fired);
    verdicts.push(("removed", ir));

    // 5. a module appears, and an existing docstring has been waiting for one of
    //    its names — so this state needs both halves of the render set.
    world.insert(ModuleSpec {
        name: "Pkg.Added".to_owned(),
        olean: "olean:Pkg.Added:0".to_owned(),
        imports: vec!["Pkg".to_owned()],
        decls: vec![DeclSpec::new("Pkg.Added.a").doc("The new one.")],
    });
    let (fired, ir) = step(&mut live, "added", &world);
    if beyond_the_gate() {
        assert!(fired.contains("renderSetFromBoth"), "{fired:?}");
    }
    covered.extend(&fired);
    verdicts.push(("added", ir));

    // 6. back to the world of state 1, from a tree that has seen all of the
    //    above: one module returns, one goes, one changes back.
    world = base_world();
    let (fired, ir) = step(&mut live, "restored", &world);
    if beyond_the_gate() {
        assert!(fired.contains("deletionsInFirstRound"), "{fired:?}");
    }
    covered.extend(&fired);
    verdicts.push(("restored", ir));

    // 7. a cache written by a different derivation is a guess, not a cache. The
    //    world does not move, so a run that trusted it would render nothing and
    //    leave the artifacts stale.
    let state_file = live.state.join("global-state.json");
    let mut state: Value =
        serde_json::from_str(&fs::read_to_string(&state_file).expect("a state file"))
            .expect("the state is JSON");
    state["derivation"] = json!("some older rule");
    write(
        &state_file,
        serde_json::to_string(&state)
            .expect("serialises")
            .as_bytes(),
    );
    let (fired, ir) = step(&mut live, "stale-state", &world);
    covered.extend(&fired);
    verdicts.push(("stale-state", ir));
    let rebuilt: Value =
        serde_json::from_str(&fs::read_to_string(&state_file).expect("a state file"))
            .expect("the state is JSON");
    assert!(
        rebuilt["derivation"] != json!("some older rule") || !beyond_the_gate(),
        "the foreign state survived the run",
    );

    // The IR verdicts, all seven, stated rather than summarised.
    let report: Vec<String> = verdicts
        .iter()
        .map(|(what, verdict)| {
            format!(
                "{what}: {} same, {} differing {:?}",
                verdict.same,
                verdict.differing.len(),
                verdict.differing,
            )
        })
        .collect();
    let joined = report.join("\n");
    for (what, verdict) in &verdicts {
        assert!(
            verdict.differing.is_empty(),
            "{what}: the IR is not the one a full extraction writes{}\n{joined}",
            if verdict.index_order_only {
                " — same entries, different module order, so `merge` stopped following \
                 `--modules` (M3-d2b)"
            } else {
                ""
            },
        );
    }
    // **Empty, and pinned as empty** (M3-d2b). Before the pipeline handed `merge`
    // the package's module list this held `["added", "restored", "stale-state"]`
    // — the three states that add a module — and the difference was `index.json`
    // alone, which no page byte follows. Asserting the set rather than deleting
    // it is what makes a divergence that comes back say so.
    let diverged: Vec<&str> = verdicts
        .iter()
        .filter(|(_, verdict)| !verdict.differing.is_empty())
        .map(|(what, _)| *what)
        .collect();
    assert!(
        diverged.is_empty(),
        "the IR of {diverged:?} is not a from-scratch one\n{joined}",
    );

    if beyond_the_gate() {
        assert_eq!(
            covered,
            SEVEN_STATES.iter().copied().collect::<BTreeSet<_>>(),
            "the seven states reach a different set of branches than they did",
        );
    }
}

// ------------------------------------------------------- the curated cases
//
// Each is a function that returns the branches it reached, so that the named
// test below it reads as a statement and
// `the_curated_cases_cover_what_the_seven_states_do_not` can run all of them and
// check the coverage claim rather than repeat it.

/// One ordinary round, for [`ONE_RUN`]'s count.
fn case_one_ordinary_round() -> BTreeSet<&'static str> {
    let mut world = base_world();
    let mut live = Live::setup("one-round", &world);
    world.edit_a_docstring("1");
    live.advance(&world);
    let report = live.round("ordinary", &[]);
    assert_eq!(report.code, 0, "{}", report.stderr);
    let full = live.full_generation("ordinary", &world, URL);
    live.assert_site_matches("ordinary", &full);
    observe(&report)
}

#[test]
fn one_ordinary_round_reaches_eighteen_branches() {
    assert_eq!(
        case_one_ordinary_round(),
        ONE_RUN.iter().copied().collect::<BTreeSet<_>>(),
    );
}

/// **Debt 1, on its own.** A `name-map.json` one state behind — what a run
/// interrupted between `merge` and `global` leaves — with nothing changed and
/// nothing deleted.
///
/// The prototype loses this run entirely: `impact` writes no `impact-set.txt`,
/// `sort -u` fails on the missing file, and `|| : > "$RENDERSET"` empties the
/// render set. Here the two halves meet in memory, so the global map's answer
/// reaches the renderer.
fn case_stale_name_map() -> BTreeSet<&'static str> {
    let world = base_world();
    let mut live = Live::setup("stale-name-map", &world);

    // The map as it would have been before `Pkg.Leaf` existed: `Pkg.Leaf.l` is
    // missing from it, so the delta says that name moved into the package.
    let map_path = live.pages.join("declarations/name-map.json");
    let mut map: BTreeMap<String, String> =
        serde_json::from_str(&fs::read_to_string(&map_path).expect("a name map"))
            .expect("the map is a flat string map");
    assert!(map.remove("Pkg.Leaf.l").is_some(), "the fixture changed");
    write(
        &map_path,
        serde_json::to_string(&map).expect("serialises").as_bytes(),
    );

    let report = live.round("stale-map", &[]);
    assert_eq!(report.code, 0, "{}", report.stderr);
    let covered = observe(&report);
    assert!(covered.contains("detectChangedEmpty"), "{covered:?}");
    assert!(covered.contains("detectRemovedEmpty"), "{covered:?}");
    assert!(covered.contains("roundsZero"), "{covered:?}");
    // The prototype's two halves, as they are on disk: one file missing, the
    // other naming the page that has to be re-rendered.
    assert!(
        !report.work.join("impact-set.txt").exists(),
        "`impact` wrote a selection, so this is not the case debt 1 is about",
    );
    let global_set = report.work_set("global-set.txt").unwrap_or_default();
    assert_eq!(
        global_set,
        ["Pkg.C".to_owned()].into_iter().collect::<BTreeSet<_>>(),
    );
    assert_eq!(
        report.work_set("render-set.txt").unwrap_or_default(),
        global_set,
        "the global map's half of the render set was dropped — this is debt 1",
    );
    assert!(covered.contains("renderRan"), "{covered:?}");
    assert!(covered.contains("renderSetFromGlobalOnly"), "{covered:?}");
    covered
}

#[test]
fn a_stale_name_map_alone_still_reaches_the_renderer() {
    case_stale_name_map();
}

/// **The snapshot debt 6 is about.** `name-map.json` is both the "before" side
/// of the delta and the file `global` overwrites in place, so the snapshot has
/// to be taken before anything runs.
///
/// The counterfactual is run rather than described: `lean-doc global --before`
/// against the map **as it is after the round** produces an empty print set,
/// which is what a pipeline that snapshotted late would have handed the
/// renderer.
#[test]
fn the_name_map_is_snapshotted_before_the_round_overwrites_it() {
    let world = base_world();
    let mut live = Live::setup("name-map-snapshot", &world);
    let map_path = live.pages.join("declarations/name-map.json");
    let mut map: BTreeMap<String, String> =
        serde_json::from_str(&fs::read_to_string(&map_path).expect("a name map"))
            .expect("the map is a flat string map");
    map.remove("Pkg.Leaf.l");
    let stale_bytes = serde_json::to_string(&map).expect("serialises");
    write(&map_path, stale_bytes.as_bytes());

    let report = live.round("snapshot", &[]);
    assert_eq!(report.code, 0, "{}", report.stderr);

    let snapshot = fs::read_to_string(report.work.join("name-map-before.json"))
        .expect("the snapshot was taken");
    assert_eq!(snapshot, stale_bytes, "the snapshot is not the before map");
    let after = fs::read_to_string(&map_path).expect("a name map");
    assert_ne!(
        after, stale_bytes,
        "`global` did not overwrite the map, so there was nothing to snapshot",
    );
    let delta: Value = report.work_json("global-delta.json").expect("a delta");
    assert_eq!(delta["changedNames"], json!(1), "{delta}");
    assert_eq!(delta["affectedModules"], json!(["Pkg.C"]), "{delta}");

    // What the same delta says against the map the round just wrote.
    let late = live.trees.path.join("late-print-set.txt");
    let ok = lean_doc(&[
        "global",
        "--ir",
        &live.ir.display().to_string(),
        "--out",
        &live.trees.path.join("late-site").display().to_string(),
        "--before",
        &map_path.display().to_string(),
        "--print-set",
        &late.display().to_string(),
    ]);
    assert_eq!(code(&ok), 0, "{}", stderr(&ok));
    assert_eq!(
        fs::read_to_string(&late).expect("a print set"),
        "",
        "snapshotting after `global` would still have found a delta, so this proves nothing",
    );
}

/// A page tree with no `declarations/name-map.json`: the delta is off, and the
/// run still writes a `global-set.txt` — a **0-byte file, not a missing one**.
fn case_no_map_yet() -> BTreeSet<&'static str> {
    let world = base_world();
    let mut live = Live::setup("no-map-yet", &world);
    fs::remove_file(live.pages.join("declarations/name-map.json")).expect("a name map to remove");
    let report = live.round("no-map", &[]);
    assert_eq!(report.code, 0, "{}", report.stderr);
    let covered = observe(&report);
    assert!(covered.contains("mapBeforeAbsent"), "{covered:?}");
    assert!(covered.contains("deltaOff"), "{covered:?}");
    assert_eq!(
        fs::read(report.work.join("global-set.txt")).expect("a global set"),
        Vec::<u8>::new(),
        "a delta that never ran left something other than an empty file",
    );
    assert!(!report.work.join("global-delta.json").exists());
    // The map is back, written by this run's `global`.
    assert!(live.pages.join("declarations/name-map.json").is_file());
    covered
}

#[test]
fn a_page_tree_with_no_name_map_runs_with_the_delta_off() {
    case_no_map_yet();
}

/// A declaration that moves between modules takes **two rounds**, and the second
/// one is the whole reason the loop is a loop: `Pkg.B`'s olean does not move, so
/// no ledger rule can reach it, and only the fresh IR of the module the name
/// left says so.
fn case_moved_declaration() -> BTreeSet<&'static str> {
    let mut world = base_world();
    let mut live = Live::setup("moved-declaration", &world);
    world.move_a_declaration();
    live.advance(&world);

    let report = live.round("moved", &[]);
    assert_eq!(report.code, 0, "{}", report.stderr);
    let covered = observe(&report);
    assert!(covered.contains("roundsMany"), "{covered:?}");
    assert_eq!(report.rounds(), 2);
    assert_eq!(
        report
            .work_set("round-in-1.txt")
            .expect("round 1 has an input"),
        ["Pkg.A".to_owned(), "Pkg.X".to_owned()]
            .into_iter()
            .collect::<BTreeSet<_>>(),
    );
    assert_eq!(
        report
            .work_set("round-in-2.txt")
            .expect("round 2 has an input"),
        ["Pkg.B".to_owned(), "Pkg.Leaf".to_owned()]
            .into_iter()
            .collect::<BTreeSet<_>>(),
        "L3-1 did not reach exactly the modules whose olean never moved — a module it \
         re-extracted in round 1 came back, or one that refers to the moved name did not",
    );

    // **Why round 2 is always the last one, and why `--exclude` cannot be
    // observed.** A module reaches round 2 only because its *references* went
    // stale; its own olean did not move, so its declaration names are the base
    // IR's — and `ownership` derives "lost" and "gained" from declaration names
    // alone. Round 2 therefore watches nothing, scans no base module, and can
    // report nobody. The mutation survey found `--exclude: None` unkillable for
    // exactly this reason; if this assertion ever fails, a third round became
    // possible and that mutant became killable.
    let second = report
        .work_json("ownership-2.json")
        .expect("round 2 wrote a summary");
    assert_eq!(second["lostNames"], json!(0), "{second}");
    assert_eq!(second["gainedNames"], json!(0), "{second}");
    assert_eq!(second["scannedBaseModules"], json!(0), "{second}");

    let full = live.full_generation("moved", &world, URL);
    live.assert_site_matches("moved", &full);
    let verdict = compare_ir(&live.ir, &full.join("ir"));
    assert!(
        verdict.differing.is_empty(),
        "the merged IR is not a from-scratch one: {:?}",
        verdict.differing,
    );
    covered
}

#[test]
fn a_moved_declaration_takes_two_rounds() {
    case_moved_declaration();
}

/// `--max-rounds` reached with modules still stale is **exit 5**, and nothing
/// downstream of the loop ran.
fn case_round_bound() -> BTreeSet<&'static str> {
    let mut world = base_world();
    let mut live = Live::setup("round-bound", &world);
    world.move_a_declaration();
    live.advance(&world);

    let report = live.round("bounded", &["--max-rounds", "1"]);
    assert_eq!(report.code, 5, "{}", report.stderr);
    assert!(report.stderr.contains("Pkg.B"), "{}", report.stderr);
    assert!(report.stderr.contains("1 round"), "{}", report.stderr);
    let covered = observe(&report);
    assert!(covered.contains("roundsExceeded"), "{covered:?}");
    assert!(covered.contains("maxRoundsGiven"), "{covered:?}");
    assert_eq!(report.pages_before, report.pages_after);
    assert!(!report.work.join("render-set.txt").exists());
    covered
}

#[test]
fn the_round_bound_is_exit_five() {
    case_round_bound();
}

/// A moved `renderKey` overrides `--mode` and re-renders every page — including
/// the ones whose IR nobody touched — and the result is still full generation's
/// bytes at the new revision.
fn case_render_key() -> BTreeSet<&'static str> {
    let world = base_world();
    let mut live = Live::setup("render-key", &world);
    let report = live.round("render-all", &["--source-url", URL_NEXT, "--mode", "self"]);
    assert_eq!(report.code, 0, "{}", report.stderr);
    let covered = observe(&report);
    assert!(covered.contains("renderAllFired"), "{covered:?}");
    assert!(covered.contains("modeGiven"), "{covered:?}");
    assert_eq!(
        report.work_json("timings.json").expect("a timings record")["mode"],
        json!("all"),
        "`--mode self` survived a moved render key",
    );
    assert_eq!(
        report.work_set("render-set.txt").unwrap_or_default(),
        world.names().into_iter().collect::<BTreeSet<_>>(),
    );
    // Nothing was re-extracted: the render key invalidates no IR.
    assert_eq!(report.rounds(), 0);
    let full = live.full_generation("render-key", &world, URL_NEXT);
    live.assert_site_matches("render-key", &full);
    covered
}

#[test]
fn a_moved_render_key_renders_every_page() {
    case_render_key();
}

/// The renderer is skipped on an empty set, **and** it would render nothing if
/// it were not.
///
/// The second half is what plan §5 calls "the type replaced the guard": the
/// prototype's `if [ ${#ONLY[@]} -eq 0 ]` was the only thing between an empty
/// regeneration set and 432 re-rendered pages, because `render.ts` reads no
/// `--only` as "all". Here `--only-from` an empty file is an empty set.
#[test]
fn an_empty_regeneration_set_renders_nothing_twice_over() {
    let world = base_world();
    let mut live = Live::setup("empty-set", &world);
    let report = live.round("empty", &[]);
    assert_eq!(report.code, 0, "{}", report.stderr);
    assert_eq!(
        report.work_file("render-timings.json").as_deref(),
        Some("{\"skipped\":\"empty render set\"}\n"),
    );
    assert_eq!(
        report.pages_before, report.pages_after,
        "the skipped renderer changed the pages",
    );

    // The same set, handed to the renderer on purpose.
    let empty = live.trees.path.join("empty-set.txt");
    write(&empty, b"");
    let pages = tree(&live.pages);
    let ok = lean_doc(&[
        "render",
        "--ir",
        &live.ir.display().to_string(),
        "--pages",
        &live.pages.display().to_string(),
        "--source-url",
        URL,
        "--link-index",
        &live.lidx.display().to_string(),
        "--only-from",
        &empty.display().to_string(),
    ]);
    assert_eq!(code(&ok), 0, "{}", stderr(&ok));
    let log = String::from_utf8_lossy(&ok.stdout).into_owned();
    assert!(log.contains("modules 0/"), "{log}");
    assert_eq!(
        pages,
        tree(&live.pages),
        "an explicitly empty set rendered something",
    );
}

/// The three flags the extractor is called with are `stage7g/extract-once.sh`'s
/// required arguments, in its order, with every `--extractor-arg` in front of
/// them.
fn case_extractor_contract() -> BTreeSet<&'static str> {
    let mut world = base_world();
    let mut live = Live::setup("extractor-contract", &world);
    world.edit_a_docstring("1");
    live.advance(&world);
    let report = live.round("contract", &[]);
    assert_eq!(report.code, 0, "{}", report.stderr);

    let calls = fs::read_to_string(report.work.join("extractor-calls.txt"))
        .expect("the extractor recorded its command line");
    let calls = lines(&calls);
    assert_eq!(calls.len(), 1, "{calls:?}");
    let call = &calls[0];
    assert!(call.starts_with("--world "), "{call}");
    let modules_at = call.find(" --modules ").expect("--modules");
    let ir_dir_at = call.find(" --ir-dir ").expect("--ir-dir");
    let timings_at = call.find(" --timings ").expect("--timings");
    assert!(modules_at < ir_dir_at && ir_dir_at < timings_at, "{call}");
    assert!(
        call.contains("round-in-1.txt"),
        "the round's input list is not what was passed: {call}",
    );
    assert!(call.contains("inc-ir-1"), "{call}");
    assert!(call.contains("extract-timings-1.json"), "{call}");
    // And the extractor's own timings reached the record.
    let record = report.work_json("timings.json").expect("a timings record");
    assert_eq!(record["extract"]["extractor"], json!("fake"));
    assert_eq!(record["extract"]["targetModules"], json!(1));
    observe(&report)
}

#[test]
fn the_extractor_is_called_the_way_extract_once_expects() {
    case_extractor_contract();
}

/// An extractor that fails stops the run with **exit 4**, names the child's own
/// exit code and leaves the pages alone.
fn case_failing_extractor() -> BTreeSet<&'static str> {
    let mut world = base_world();
    let mut live = Live::setup("extractor-failure", &world);
    world.edit_a_docstring("1");
    live.advance(&world);
    let report = live.round("failing", &["--extractor-arg", "--fail"]);
    assert_eq!(report.code, 4, "{}", report.stderr);
    assert!(report.stderr.contains("exited 3"), "{}", report.stderr);
    assert!(
        report.stderr.contains("nothing was rendered"),
        "{}",
        report.stderr,
    );
    assert_eq!(report.pages_before, report.pages_after);
    let covered = observe(&report);
    assert!(covered.contains("extractorFailed"), "{covered:?}");

    // A program that does not exist is the same refusal, not a panic.
    let mut args = report.args.clone();
    let at = args
        .iter()
        .position(|arg| arg == "/bin/sh")
        .expect("--extractor's value");
    args[at] = "/nonexistent/extractor".to_owned();
    let missing = live.invoke(&args, &report.work);
    assert_eq!(missing.code, 4, "{}", missing.stderr);
    covered
}

#[test]
fn a_failing_extractor_stops_the_run() {
    case_failing_extractor();
}

/// The timings record's field names are `incremental.sh:393-424`'s, so the
/// aggregation and the JSONL already in `benchmarks/results/` keep reading.
fn case_timings() -> BTreeSet<&'static str> {
    let mut world = base_world();
    let mut live = Live::setup("timings", &world);
    world.edit_a_docstring("1");
    live.advance(&world);
    let report = live.round("timings", &[]);
    assert_eq!(report.code, 0, "{}", report.stderr);
    let record = report.work_json("timings.json").expect("a timings record");

    for phase in [
        "detectSeconds",
        "extractSeconds",
        "ownershipSeconds",
        "mergeSeconds",
        "roundsSeconds",
        "pruneSeconds",
        "globalSeconds",
        "impactSeconds",
        "renderSeconds",
        "totalSeconds",
    ] {
        assert!(record[phase].is_number(), "{phase}: {record}");
    }
    assert_eq!(record["mode"], json!("self"));
    assert_eq!(record["rounds"], json!(1));
    assert_eq!(record["staleFound"], json!(0));
    assert_eq!(record["changed"], json!(1));
    assert_eq!(record["removed"], json!(0));
    assert_eq!(record["irChanged"], json!(1));
    assert_eq!(record["globalStale"], json!(0));
    assert_eq!(record["pagesRendered"], json!(1));
    // The prototype's own fields that this command does not have.
    for gone in ["module", "l3_1", "global_impl"] {
        assert!(record.get(gone).is_none(), "{gone} came back: {record}");
    }
    // M4-c: `serve` is written on both paths, so a resident run and a fresh run
    // are told apart in the record — which is what the prototype's comment says
    // the field is for. `jobs` and `serveGeneration` are the resident path's, and
    // this run is not one: behind `--extractor` the job count is inside somebody
    // else's argument list.
    assert_eq!(record["serve"], json!(false));
    for absent in ["jobs", "serveGeneration"] {
        assert!(
            record.get(absent).is_none(),
            "{absent} on the one-shot path: {record}",
        );
    }
    // The nested per-stage records, as the prototype embeds them.
    assert_eq!(record["merge"]["command"], json!("merge"));
    assert!(record["global"]["cacheHits"].is_number(), "{record}");
    assert_eq!(record["render"]["pagesWritten"], json!(1));

    // The same run without `--timings` writes no record and still works.
    let mut args = report.args.clone();
    let at = args
        .iter()
        .position(|arg| arg == "--timings")
        .expect("--timings");
    args.drain(at..=at + 1);
    let quiet = live.invoke(&args, &report.work);
    assert_eq!(quiet.code, 0, "{}", quiet.stderr);
    let mut covered = observe(&report);
    covered.extend(observe(&quiet));
    covered
}

#[test]
fn the_timings_record_keeps_the_prototypes_field_names() {
    case_timings();
}

/// Plan §7 debt 7: the revision has to be 40 lower-case hex digits, and the
/// refusal has to say what breaks.
fn case_source_url() -> BTreeSet<&'static str> {
    let world = base_world();
    let mut live = Live::setup("source-url", &world);
    let mut covered: BTreeSet<&'static str> = BTreeSet::new();
    let cases: [(&str, &str); 5] = [
        (
            "https://example.invalid/owner/repo/blob/v1.2.3",
            "sourceUrlNotHexRefused",
        ),
        (
            "https://example.invalid/owner/repo/blob/main",
            "sourceUrlNotHexRefused",
        ),
        (
            // Upper case: `coverage.ts`'s character class is `[0-9a-f]`.
            "https://example.invalid/owner/repo/blob/0123456789ABCDEF0123456789abcdef01234567",
            "sourceUrlNotHexRefused",
        ),
        (
            // Thirty-nine digits.
            "https://example.invalid/owner/repo/blob/0123456789abcdef0123456789abcdef0123456",
            "sourceUrlNotHexRefused",
        ),
        (
            "https://example.invalid/owner/repo/tree/main",
            "sourceUrlNoBlobRefused",
        ),
    ];
    for (url, branch) in cases {
        let report = live.round("bad-url", &["--source-url", url]);
        assert_eq!(report.code, 2, "{url}: {}", report.stderr);
        assert!(report.stderr.contains("3.1103"), "{url}: {}", report.stderr);
        assert!(
            report.stderr.contains("coverage.ts"),
            "{url}: {}",
            report.stderr,
        );
        let fired = observe(&report);
        assert!(fired.contains(branch), "{url}: {fired:?}");
        assert_eq!(report.pages_before, report.pages_after, "{url}");
        covered.extend(fired);
    }
    // A trailing path after the revision is the ordinary shape and is accepted.
    let ok = live.round(
        "url-with-path",
        &[
            "--source-url",
            "https://example.invalid/owner/repo/blob/0123456789abcdef0123456789abcdef01234567/",
        ],
    );
    assert_eq!(ok.code, 0, "{}", ok.stderr);
    covered
}

#[test]
fn a_source_url_without_a_forty_hex_revision_is_refused() {
    case_source_url();
}

/// Every flag the prototype has and this command refuses, refused **by name**
/// with the reason — and every flag it requires, missing.
fn case_command_line() -> BTreeSet<&'static str> {
    let world = base_world();
    let mut live = Live::setup("command-line", &world);
    let mut covered: BTreeSet<&'static str> = BTreeSet::new();

    let retired: [(&[&str], &str); 8] = [
        (&["--jobs", "4"], "--extractor-arg"),
        (&["--l3-1", "off"], "wrong site"),
        (&["--global", "old"], "--state is required"),
        (&["--serve-dir", "/tmp/x"], "stage 6a"),
        (&["--serve-from", "2"], "round number"),
        (&["--count-reads", "/tmp/x.jsonl"], "measurement tool"),
        (&["--module", "Pkg.A"], "label"),
        (&["--no-link-index"], "150"),
    ];
    for (extra, expected) in retired {
        let report = live.round("retired", extra);
        assert_eq!(report.code, 2, "{extra:?}: {}", report.stderr);
        assert!(
            report.stderr.contains(expected),
            "{extra:?}: {}",
            report.stderr,
        );
        let fired = observe(&report);
        assert!(fired.contains("retiredFlagRefused"), "{extra:?}");
        covered.extend(fired);
    }

    let report = live.round("bad-mode", &["--mode", "nonsens"]);
    assert_eq!(report.code, 2, "{}", report.stderr);
    assert!(
        report.stderr.contains("self|referrers"),
        "{}",
        report.stderr
    );
    covered.extend(observe(&report));
    let report = live.round("zero-rounds", &["--max-rounds", "0"]);
    assert_eq!(report.code, 2, "{}", report.stderr);
    covered.extend(observe(&report));

    // Each required flag, dropped in turn.
    let full = live.round("full", &[]).args;
    for flag in [
        "--ir",
        "--pages",
        "--ledger",
        "--work",
        "--modules",
        "--source-url",
        "--link-index",
        "--state",
        "--extractor",
    ] {
        let mut args = full.clone();
        let at = args.iter().position(|arg| arg == flag).expect(flag);
        args.drain(at..=at + 1);
        let report = live.invoke(&args, &live.trees.path.join("nowhere"));
        assert_eq!(report.code, 2, "{flag}: {}", report.stderr);
        assert!(report.stderr.contains(flag), "{flag}: {}", report.stderr);
        let fired = observe(&report);
        assert!(
            fired.contains("requiredFlagMissing"),
            "{flag}: {}",
            report.stderr,
        );
        covered.extend(fired);
    }

    // `--extractor` with no `--extractor-arg` is a legal command line: the fake
    // extractor refuses, which is its own business.
    let mut args = full.clone();
    while let Some(at) = args.iter().position(|arg| arg == "--extractor-arg") {
        args.drain(at..=at + 1);
    }
    let report = live.invoke(&args, &live.trees.path.join("no-extractor-args"));
    let fired = observe(&report);
    assert!(fired.contains("extractorArgsEmpty"), "{fired:?}");
    covered.extend(fired);
    covered
}

#[test]
fn the_command_line_is_checked() {
    case_command_line();
}

/// The deletion path is the first round's, and the second round does not ask
/// again.
fn case_deletions_first_round_only() -> BTreeSet<&'static str> {
    let mut world = base_world();
    let mut live = Live::setup("deletions", &world);
    world.drop_module("Pkg.Leaf");
    world.move_a_declaration();
    live.advance(&world);

    let report = live.round("deleted-and-moved", &[]);
    assert_eq!(report.code, 0, "{}", report.stderr);
    assert_eq!(report.rounds(), 2);
    let covered = observe(&report);
    assert!(covered.contains("deletionsInFirstRound"), "{covered:?}");
    assert!(covered.contains("deletionsNotRepeated"), "{covered:?}");
    assert_eq!(
        report.work_json("merge-timings-1.json").expect("round 1")["removed"],
        json!(1),
    );
    assert_eq!(
        report.work_json("merge-timings-2.json").expect("round 2")["removed"],
        json!(0),
        "the second round asked about a module that is already gone",
    );
    assert!(!live.pages.join("Pkg/Leaf.html").exists());
    for artifact in ARTIFACTS {
        assert!(
            live.pages.join(artifact).exists(),
            "plan §7 debt 4: {artifact} was swept away as an orphan",
        );
    }
    let full = live.full_generation("deleted-and-moved", &world, URL);
    live.assert_site_matches("deleted-and-moved", &full);
    covered
}

#[test]
fn deletions_are_folded_into_the_first_round_only() {
    case_deletions_first_round_only();
}

/// A re-extraction that lands on the same bytes rewrites nothing: what is
/// *stale* follows the IR's `contentHash`, not the ledger's olean hash.
#[test]
fn a_re_extraction_that_changes_nothing_rewrites_nothing() {
    let mut world = base_world();
    let mut live = Live::setup("no-op-extraction", &world);
    // The olean moves — a rebuild of a dependency is enough — and the IR does
    // not.
    world.module("Pkg.C").olean = "olean:Pkg.C:rebuilt".to_owned();
    live.advance(&world);

    let report = live.round("no-op", &[]);
    assert_eq!(report.code, 0, "{}", report.stderr);
    let covered = observe(&report);
    assert!(covered.contains("detectChangedNonEmpty"), "{covered:?}");
    assert!(covered.contains("irChangedEmpty"), "{covered:?}");
    // `--mode self` still renders the module the ledger named: the IR hash
    // decides what is stale, the mode decides what is asked for.
    assert_eq!(
        report.work_set("render-set.txt").unwrap_or_default(),
        ["Pkg.C".to_owned()].into_iter().collect::<BTreeSet<_>>(),
    );
    assert_eq!(
        report.pages_before, report.pages_after,
        "re-rendering the same IR produced different bytes",
    );
}

/// `lean-doc merge --modules <file>` — the one stage flag M3-d2b added, checked
/// at the **process** boundary.
///
/// The pipeline reaches the same code as a library call, so nothing above proves
/// that the subcommand parses the flag and hands it on; a CLI that accepted it
/// and dropped it would leave every test here green. The refusal is checked in
/// the same run, because a caller's only sign that the list is stale is the exit
/// code.
#[test]
fn the_merge_command_takes_a_module_list() {
    let world = base_world();
    let live = Live::setup("merge-cli", &world);
    let nothing = live.trees.path.join("nothing-removed.txt");
    write(&nothing, b"");

    // A list naming a module the IR has nothing behind: exit 3, and the name is
    // in the message rather than only in a count.
    let ghosts = live.trees.path.join("ghost-modules.txt");
    let mut names = world.names();
    names.push("Pkg.Ghost".to_owned());
    write(&ghosts, (names.join("\n") + "\n").as_bytes());
    let out = live.trees.path.join("merged-ir");
    let refused = lean_doc(&[
        "merge",
        "--base",
        &live.ir.display().to_string(),
        "--out",
        &out.display().to_string(),
        "--remove",
        &nothing.display().to_string(),
        "--modules",
        &ghosts.display().to_string(),
    ]);
    assert_eq!(code(&refused), 3, "{}", stderr(&refused));
    assert!(
        stderr(&refused).contains("Pkg.Ghost"),
        "{}",
        stderr(&refused)
    );
    assert!(!out.exists(), "the refused merge wrote a tree");

    // The package's own list — `lean-doc modules` wrote it — is accepted, and
    // the index comes out in it.
    let ok = lean_doc(&[
        "merge",
        "--base",
        &live.ir.display().to_string(),
        "--out",
        &out.display().to_string(),
        "--remove",
        &nothing.display().to_string(),
        "--modules",
        &live.modules.display().to_string(),
    ]);
    assert_eq!(code(&ok), 0, "{}", stderr(&ok));
    let index: Value =
        serde_json::from_str(&fs::read_to_string(out.join("index.json")).expect("an index"))
            .expect("the index is JSON");
    let merged: Vec<String> = index["modules"]
        .as_array()
        .expect("an array")
        .iter()
        .map(|entry| entry["module"].as_str().expect("a name").to_owned())
        .collect();
    assert_eq!(
        merged,
        lines(&fs::read_to_string(&live.modules).expect("the module list")),
    );
}

// ------------------------------------------------------------ the module glob

/// `lean-doc modules`: the source glob, in every shape a library root comes in.
fn case_module_glob() -> BTreeSet<&'static str> {
    let trees = TempDir::new("module-glob");
    let repo = trees.path.join("repo");
    // `Lib.lean` and `Lib/`, with a nested directory and a file that is not
    // Lean's.
    write(&repo.join("Lib.lean"), b"-- root\n");
    write(&repo.join("Lib/A.lean"), b"");
    write(&repo.join("Lib/Deep/B.lean"), b"");
    write(&repo.join("Lib/Deep/C.lean"), b"");
    write(&repo.join("Lib/notes.md"), b"not a module");
    // A second library that is only a file, and a third that is only a
    // directory.
    write(&repo.join("Solo.lean"), b"");
    write(&repo.join("Dir/Only.lean"), b"");
    let mut covered: BTreeSet<&'static str> = BTreeSet::new();

    let both = run_modules(&repo, &["Lib"], None);
    assert_eq!(
        lines(&both.stdout),
        ["Lib", "Lib.A", "Lib.Deep.B", "Lib.Deep.C"],
    );
    covered.extend(observe_modules(&both));

    let file_only = run_modules(&repo, &["Solo"], None);
    assert_eq!(lines(&file_only.stdout), ["Solo"]);
    covered.extend(observe_modules(&file_only));

    let dir_only = run_modules(&repo, &["Dir"], None);
    assert_eq!(lines(&dir_only.stdout), ["Dir.Only"]);
    covered.extend(observe_modules(&dir_only));

    // Two libraries, one of them named twice: the union, deduplicated, in the
    // project's UTF-16 order rather than the caller's locale collation.
    let out = trees.path.join("modules.txt");
    let repeated = run_modules(&repo, &["Solo", "Solo", "Dir"], Some(&out));
    assert_eq!(repeated.code, 0);
    assert_eq!(
        fs::read_to_string(&out).expect("the list was written"),
        "Dir.Only\nSolo\n",
    );
    covered.extend(observe_modules(&repeated));

    // A library that is neither a file nor a directory: exit 3, not an empty
    // list — an empty list is indistinguishable from a package whose every
    // module was deleted, and `check` would report all of them removed.
    let missing = run_modules(&repo, &["Ghost"], None);
    assert_eq!(missing.code, 3, "{}", missing.stdout);
    covered.extend(observe_modules(&missing));

    // The two required flags.
    for args in [
        vec!["modules".to_owned(), "--lib".to_owned(), "Lib".to_owned()],
        vec![
            "modules".to_owned(),
            "--root".to_owned(),
            repo.display().to_string(),
        ],
    ] {
        let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
        let output = lean_doc(&borrowed);
        assert_eq!(code(&output), 2, "{args:?}: {}", stderr(&output));
        covered.extend(observe_modules(&ModulesReport {
            args,
            code: code(&output),
            stdout: String::new(),
            shapes: Vec::new(),
            nested: false,
            other_files: false,
            out: None,
        }));
    }
    covered
}

#[test]
fn the_module_glob_reads_the_sources() {
    case_module_glob();
}

fn run_modules(repo: &Path, libs: &[&str], out: Option<&Path>) -> ModulesReport {
    let mut args: Vec<String> = vec![
        "modules".to_owned(),
        "--root".to_owned(),
        repo.display().to_string(),
    ];
    for lib in libs {
        args.push("--lib".to_owned());
        args.push((*lib).to_owned());
    }
    if let Some(path) = out {
        args.push("--out".to_owned());
        args.push(path.display().to_string());
    }
    let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = lean_doc(&borrowed);
    let shapes: Vec<(bool, bool)> = libs
        .iter()
        .map(|lib| {
            (
                repo.join(format!("{lib}.lean")).is_file(),
                repo.join(lib).is_dir(),
            )
        })
        .collect();
    let mut nested = false;
    let mut other_files = false;
    for lib in libs {
        if let Ok(listing) = fs::read_dir(repo.join(lib)) {
            for entry in listing.flatten() {
                let kind = entry.file_type().expect("a file type");
                if kind.is_dir() {
                    nested = true;
                } else if !entry.file_name().to_string_lossy().ends_with(".lean") {
                    other_files = true;
                }
            }
        }
    }
    ModulesReport {
        args,
        code: code(&output),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        shapes,
        nested,
        other_files,
        out: out.map(Path::to_owned),
    }
}

// ----------------------------------------------------------- the branch ledger

/// The three lists are a partition, and the curated cases really do reach
/// everything the seven states cannot.
///
/// **This is the test that fails when somebody deletes a curated case because
/// "the seven states pass anyway".** It runs every one of them.
#[test]
fn the_curated_cases_cover_what_the_seven_states_do_not() {
    let states: BTreeSet<&str> = SEVEN_STATES.iter().copied().collect();
    let one: BTreeSet<&str> = ONE_RUN.iter().copied().collect();
    let unreached: BTreeSet<&str> = NO_SEVEN_STATE_RUN_REACHES.iter().copied().collect();
    let all: BTreeSet<&str> = BRANCHES.iter().copied().collect();

    assert_eq!(BRANCHES.len(), all.len(), "a branch is listed twice");
    assert!(
        one.is_subset(&states),
        "one run reaches something the seven states do not: {:?}",
        one.difference(&states).collect::<Vec<_>>(),
    );
    let counted: BTreeSet<&str> = states.union(&unreached).copied().collect();
    assert_eq!(
        counted,
        all,
        "these branches are in neither list: {:?}",
        all.difference(&counted).collect::<Vec<_>>(),
    );
    assert_eq!(
        states.intersection(&unreached).count(),
        0,
        "a branch is both reached and unreached: {:?}",
        states.intersection(&unreached).collect::<Vec<_>>(),
    );

    let mut curated: BTreeSet<&'static str> = BTreeSet::new();
    curated.extend(case_one_ordinary_round());
    curated.extend(case_stale_name_map());
    curated.extend(case_no_map_yet());
    curated.extend(case_moved_declaration());
    curated.extend(case_round_bound());
    curated.extend(case_render_key());
    curated.extend(case_extractor_contract());
    curated.extend(case_failing_extractor());
    curated.extend(case_timings());
    curated.extend(case_source_url());
    curated.extend(case_command_line());
    curated.extend(case_deletions_first_round_only());
    curated.extend(case_module_glob());

    let missed: Vec<&&str> = unreached
        .iter()
        .filter(|branch| !curated.contains(*branch))
        .collect();
    assert!(
        missed.is_empty(),
        "no real-data exercise and no curated case reaches: {missed:?}",
    );
    let nowhere: Vec<&&str> = all
        .iter()
        .filter(|branch| !curated.contains(*branch) && !states.contains(*branch))
        .collect();
    assert!(
        nowhere.is_empty(),
        "these branches fire nowhere: {nowhere:?}"
    );
}

// ------------------------------------------------------------------- plumbing

fn lean_doc(args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .output()
        .expect("the binary under test runs")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn code(output: &Output) -> i32 {
    output
        .status
        .code()
        .unwrap_or_else(|| panic!("the process was killed by a signal: {}", stderr(output)))
}

/// Every file under `root`, keyed by its path relative to it. A missing root is
/// an empty tree, which is what a run that has not written yet leaves.
fn tree(root: &Path) -> Files {
    let mut files = Files::new();
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

fn copy_tree(from: &Path, to: &Path) {
    let _ = fs::remove_dir_all(to);
    for (path, bytes) in tree(from) {
        write(&to.join(path), &bytes);
    }
    fs::create_dir_all(to.join("deps")).expect("writable");
}

fn write(path: &Path, body: &[u8]) {
    fs::create_dir_all(path.parent().expect("a parent")).expect("writable");
    fs::write(path, body).expect("writable");
}

/// A directory that removes itself, as in `tests/site.rs`.
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
            "lean-doc-incr-{}-{}-{slug}",
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
