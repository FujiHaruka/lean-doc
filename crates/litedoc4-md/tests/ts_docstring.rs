//! Where this crate and the frozen TypeScript prototype disagree, and why that
//! is the prototype's limit rather than a regression.
//!
//! The migration's inner loop is "byte diff against the prototype" (plan §1).
//! M1-c is where it loosens: `experiments/stage7d/render.ts` hand-wrote a
//! 594-line CommonMark subset because TypeScript had no byte-compatible parser,
//! and this crate links the real md4c. Plan §5 fixes the judgement in advance —
//! **whichever side matches doc-gen4 is right, and md4c is not to be bent
//! towards the subset.**
//!
//! So this file is not a second expectation. It is the list of inputs on which
//! the prototype and doc-gen4 differ, produced by
//! `tests/oracle/gen-ts-docstring-expected.ts` from the prototype's own code
//! (**a frozen value: the generator and the prototype went with `experiments/`
//! on 2026-08-16 and exist only at tag `experiments-frozen`, so HEAD cannot
//! regenerate it**), and the assertion that on every one of them this crate is
//! on doc-gen4's
//! side. `tests/docgen4.rs` is what says so positively; what is added here is
//! that the difference is *real* — a port that had quietly reproduced the
//! subset's behaviour would fail [`the_prototype_is_the_one_that_differs`].
//!
//! # What the list is for after this milestone step
//!
//! M1-d compares whole pages against the prototype's output. These are the
//! docstrings whose bytes will move, so a page diff that lands on one of them
//! has its suspect already named.
//!
//! # The measurement
//!
//! Over the whole corpus (4,987 inputs = every one of the 4,858 real
//! docstrings + the 129 hand-written cases doc-gen4 survives), the prototype
//! differs from doc-gen4 on **41**,
//! of which **exactly one is a real docstring** 【実測 2026-08-11】. The other
//! 40 are the corners the subset's own comment says it does not implement:
//! tables, task lists, images, hard breaks, entities, permissive autolinks,
//! reference links, strikethrough, backslash escapes, CRLF and NUL.

use std::collections::{BTreeMap, BTreeSet};

use litedoc4_md::{NoLinks, Renderer};
use serde::Deserialize;

const TS: &str = include_str!("data/ts-docstring-expected.json");
const DOCGEN4: &str = include_str!("data/docgen4-expected.json");

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TsExpected {
    /// How the answers were produced, for the report a failure prints.
    oracle: String,
    /// How many cases of the committed doc-gen4 fixture were re-rendered.
    sample_cases: usize,
    /// What the whole corpus said, when the generator was given `--full`.
    corpus: Option<Corpus>,
    /// Only the disagreements; see the module comment.
    cases: Vec<TsCase>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Corpus {
    total: usize,
    differed: usize,
    real_total: usize,
    real_differed: usize,
    real_what: Vec<String>,
}

#[derive(Deserialize)]
struct TsCase {
    what: String,
    root: String,
    /// The prototype's bytes.
    ts: String,
}

#[derive(Deserialize)]
struct DocGen4Expected {
    cases: Vec<DocGen4Case>,
}

#[derive(Deserialize)]
struct DocGen4Case {
    what: String,
    root: String,
    md: String,
    html: String,
}

fn ts_expected() -> TsExpected {
    serde_json::from_str(TS).expect("tests/data/ts-docstring-expected.json is valid")
}

fn doc_gen4_cases() -> Vec<DocGen4Case> {
    serde_json::from_str::<DocGen4Expected>(DOCGEN4)
        .expect("tests/data/docgen4-expected.json is valid")
        .cases
}

#[test]
fn the_fixture_is_the_prototypes_own_output() {
    let e = ts_expected();
    assert!(e.oracle.contains("render.ts"), "{}", e.oracle);
    let cases = doc_gen4_cases();
    assert_eq!(
        e.sample_cases,
        cases.len(),
        "the two fixtures were generated against different samples"
    );
    // Every disagreement has to name a case of the doc-gen4 fixture, or the
    // join below silently checks nothing.
    let known: BTreeSet<&str> = cases.iter().map(|c| c.what.as_str()).collect();
    for case in &e.cases {
        assert!(
            known.contains(case.what.as_str()),
            "{} is not a case",
            case.what
        );
    }
}

/// The measurement this file exists to record, pinned so that it cannot drift
/// without someone re-reading it.
#[test]
fn the_corpus_numbers_are_what_was_measured() {
    let corpus = ts_expected()
        .corpus
        .expect("the committed fixture was generated with --full");
    assert_eq!(corpus.total, 4_987);
    assert_eq!(corpus.differed, 41);
    assert_eq!(corpus.real_total, 4_858);
    assert_eq!(corpus.real_differed, 1);
    assert_eq!(
        corpus.real_what,
        ["InformationTheory.Shannon.TimeBandLimiting.Count module doc 1"],
        "a different real docstring now differs; decide which side is right \
         against doc-gen4 before touching this number"
    );
}

/// On every recorded disagreement, this crate produces doc-gen4's bytes and not
/// the prototype's.
///
/// The second half is the load-bearing one: it fails if the port ever starts
/// agreeing with the subset instead, which is the failure plan §5 names — "TS
/// に合わせて md4c を歪めない".
#[test]
fn the_prototype_is_the_one_that_differs() {
    let e = ts_expected();
    let cases = doc_gen4_cases();
    let by_what: BTreeMap<&str, &DocGen4Case> =
        cases.iter().map(|c| (c.what.as_str(), c)).collect();

    let mut failures = Vec::new();
    for ts_case in &e.cases {
        let case = by_what[ts_case.what.as_str()];
        assert_eq!(case.root, ts_case.root, "{}", ts_case.what);
        let got = Renderer::new(&case.root, &NoLinks).docstring(&case.md);
        if got != case.html {
            failures.push(format!("{}: does not match doc-gen4", ts_case.what));
        } else if got == ts_case.ts {
            failures.push(format!(
                "{}: matches the prototype, so it is no longer a disagreement",
                ts_case.what
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(
        e.cases.len() >= 30,
        "only {} disagreements are recorded; the list stopped covering the \
         subset's gaps",
        e.cases.len()
    );
}

/// Which gaps the recorded disagreements are, named rather than counted.
///
/// The subset's own comment (`render.ts:1091-1096`) lists what it left out
/// because this corpus contains none of it. Each of those has to appear here,
/// or the hand-written cases stopped reaching it.
#[test]
fn the_disagreements_are_the_subsets_declared_gaps() {
    let whats: Vec<String> = ts_expected().cases.into_iter().map(|c| c.what).collect();
    let mentions = |needle: &str| whats.iter().any(|w| w.contains(needle));
    for gap in [
        "table",
        "task list",
        "image",
        "hard break",
        "entity",
        "autolink",
        "reference link",
        "strikethrough",
        "backslash escapes",
        "crlf",
        "nul",
    ] {
        assert!(mentions(gap), "no recorded disagreement mentions {gap:?}");
    }
    // And one real docstring, which is the case for the list existing at all.
    assert!(
        whats
            .iter()
            .any(|w| !w.starts_with("curated: ") && !w.starts_with("html: ")),
        "every recorded disagreement is hand-written; the real one is missing \
         from the sample"
    );
}
