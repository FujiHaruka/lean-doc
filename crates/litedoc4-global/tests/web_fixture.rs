//! The `search-index.bin` the TypeScript decoder is tested against.
//!
//! `crates/litedoc4-render/web/test/` has a decoder for this format written in
//! TypeScript, and something has to say what the right answer is. **It is this
//! encoder** — not a second encoder written in TypeScript, which is the shape
//! CLAUDE.md rules out ("オラクルを同じ言語・同じ設計で書き直さない"): two
//! implementations of one design make the same mistake and agree about it.
//!
//! So the bytes are produced here, committed, and read there. This test exists
//! to keep the committed copy from drifting away from the encoder that made it:
//! it rebuilds them from [`CASES`] and compares.
//!
//! Regenerate with `UPDATE_WEB_FIXTURE=1 cargo test -p litedoc4-global`.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use litedoc4_global::search_index::{Entry, RESTART, encode};

/// Where the TypeScript tests look, relative to this crate.
const FIXTURE_DIR: &str = "../litedoc4-render/web/test/fixtures";

/// The kind vocabulary, in the order the labels table carries it.
const KINDS: [&str; 4] = ["def", "theorem", "structure", "instance"];

/// One declaration: name, kind subscript, module subscript.
type Case = (&'static str, usize, usize);

/// The corpus, in the order the format requires (UTF-16 code unit order over
/// the original names — `litedoc4_global::search_index` explains why).
///
/// Every entry is here for a reason the decoder can get wrong:
///
/// - `Pkg.a` … `Pkg.alphabet` — front coding with long shared prefixes.
/// - twenty `Pkg.block.n**` — more than [`RESTART`], so the second block starts
///   with a name written out whole and the restart table has to be consulted.
/// - `Pkg.script𝒜` — **astral**. One code point, two UTF-16 units, four UTF-8
///   bytes: the length the ranking counts is 2, not 1 (U1, 実測 2026-08-19).
/// - `Pkg.Γamma` — `toLowerCase()` and "add 32 to A-Z" disagree, so this one
///   lands in the fold section and the reader has to substitute it.
/// - `Pkg.` + 400 × `x` — past 254 bytes, where the suffix length escapes to a
///   u16.
/// - `NoDot` — a name with no components at all, whose "last component" is the
///   whole name.
static CASES: &[Case] = &[
    ("NoDot", 0, 0),
    ("Pkg.a", 0, 1),
    ("Pkg.alpha", 1, 1),
    ("Pkg.alphabet", 1, 2),
    ("Pkg.b", 0, 2),
    ("Pkg.block.n00", 0, 3),
    ("Pkg.block.n01", 0, 3),
    ("Pkg.block.n02", 0, 3),
    ("Pkg.block.n03", 0, 3),
    ("Pkg.block.n04", 0, 3),
    ("Pkg.block.n05", 0, 3),
    ("Pkg.block.n06", 0, 3),
    ("Pkg.block.n07", 0, 3),
    ("Pkg.block.n08", 0, 3),
    ("Pkg.block.n09", 0, 3),
    ("Pkg.block.n10", 1, 3),
    ("Pkg.block.n11", 1, 3),
    ("Pkg.block.n12", 1, 3),
    ("Pkg.block.n13", 1, 3),
    ("Pkg.block.n14", 1, 3),
    ("Pkg.block.n15", 1, 3),
    ("Pkg.block.n16", 1, 3),
    ("Pkg.block.n17", 1, 3),
    ("Pkg.block.n18", 1, 3),
    ("Pkg.block.n19", 1, 3),
    ("Pkg.Nested.deep.name", 2, 4),
    ("Pkg.script\u{1D49C}", 2, 4),
    ("Pkg.\u{393}amma", 3, 5),
    ("Pkg.\u{3B2}eta", 3, 5),
];

/// The 400-`x` name, built rather than typed.
fn long_name() -> String {
    format!("Pkg.{}", "x".repeat(400))
}

fn corpus() -> (Vec<String>, Vec<usize>, Vec<usize>) {
    let mut names: Vec<String> = CASES.iter().map(|(n, _, _)| (*n).to_owned()).collect();
    let mut kinds: Vec<usize> = CASES.iter().map(|(_, k, _)| *k).collect();
    let mut modules: Vec<usize> = CASES.iter().map(|(_, _, m)| *m).collect();
    names.push(long_name());
    kinds.push(0);
    modules.push(5);
    (names, kinds, modules)
}

fn build() -> (Vec<u8>, String) {
    let (names, kinds, modules) = corpus();
    let entries: Vec<Entry<'_>> = names
        .iter()
        .zip(&kinds)
        .zip(&modules)
        .map(|((name, &kind), &module)| Entry { name, kind, module })
        .collect();
    let bytes = encode(&entries, &KINDS);

    // A hand-rolled writer rather than serde_json: this file is read by one
    // test in another language, and its shape is three parallel arrays. Pulling
    // in a dependency to write eight lines of JSON is not worth the coupling.
    let mut json = String::from("{\n  \"kinds\": [");
    for (i, kind) in KINDS.iter().enumerate() {
        if i > 0 {
            json.push_str(", ");
        }
        json.push('"');
        json.push_str(kind);
        json.push('"');
    }
    json.push_str("],\n  \"names\": [\n");
    for (i, name) in names.iter().enumerate() {
        json.push_str("    ");
        json.push_str(&escape(name));
        if i + 1 < names.len() {
            json.push(',');
        }
        json.push('\n');
    }
    json.push_str("  ],\n  \"kindOf\": [");
    push_numbers(&mut json, &kinds);
    json.push_str("],\n  \"moduleOf\": [");
    push_numbers(&mut json, &modules);
    json.push_str("]\n}\n");
    (bytes, json)
}

fn push_numbers(out: &mut String, values: &[usize]) {
    for (i, v) in values.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&v.to_string());
    }
}

/// A JSON string literal, with every non-ASCII character escaped as `\u`.
///
/// Escaped rather than emitted raw so that the file is ASCII and a decoder that
/// mangles UTF-8 on the way in cannot make the comparison pass by mangling both
/// sides the same way — the astral name is the point of the fixture.
fn escape(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_ascii_graphic() || c == ' ' => out.push(c),
            c => {
                let mut buf = [0u16; 2];
                for unit in c.encode_utf16(&mut buf) {
                    write!(out, "\\u{unit:04x}").expect("a String never fails to write");
                }
            }
        }
    }
    out.push('"');
    out
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(FIXTURE_DIR)
        .join(name)
}

/// The corpus is only worth committing if it still exercises what it claims to.
#[test]
fn the_corpus_covers_what_its_comment_says() {
    let (names, _, _) = corpus();
    assert!(
        names.len() > RESTART + 1,
        "fewer than {} names never crosses a restart block",
        RESTART + 1,
    );
    assert!(
        names.iter().any(|n| n.chars().any(|c| c as u32 > 0xFFFF)),
        "no astral character — the UTF-16 length regression cannot recur here",
    );
    assert!(
        names
            .iter()
            .any(|n| n.to_lowercase() != n.chars().map(fold_ascii).collect::<String>()),
        "no name that ASCII folding is wrong for — the fold section stays empty",
    );
    assert!(
        names.iter().any(|n| n.len() > 254),
        "no name past the one-byte suffix length",
    );
    assert!(names.iter().any(|n| !n.contains('.')), "no dotless name");
}

fn fold_ascii(c: char) -> char {
    if c.is_ascii_uppercase() {
        c.to_ascii_lowercase()
    } else {
        c
    }
}

/// The committed bytes are the ones this encoder produces today.
///
/// Not `#[ignore]`: the input is in this file and the encoder is in this
/// workspace, so it needs no toolchain, no target repository and no network.
#[test]
fn the_committed_fixture_is_what_the_encoder_writes() {
    let (bytes, json) = build();
    let bin = fixture("search-index.bin");
    let expected = fixture("expected.json");

    if std::env::var_os("UPDATE_WEB_FIXTURE").is_some() {
        fs::create_dir_all(bin.parent().expect("a parent")).expect("create the fixture directory");
        fs::write(&bin, &bytes).expect("write the index");
        fs::write(&expected, &json).expect("write the expectations");
        return;
    }

    let on_disk = fs::read(&bin).unwrap_or_else(|e| {
        panic!(
            "{}: {e}\nRegenerate with `UPDATE_WEB_FIXTURE=1 cargo test -p litedoc4-global`",
            bin.display(),
        )
    });
    assert_eq!(
        on_disk,
        bytes,
        "{} is not what this encoder writes today. The TypeScript decoder is \
         tested against it, so a stale copy tests nothing. Regenerate with \
         `UPDATE_WEB_FIXTURE=1 cargo test -p litedoc4-global` and read the diff.",
        bin.display(),
    );

    let on_disk = fs::read_to_string(&expected).unwrap_or_else(|e| {
        panic!(
            "{}: {e}\nRegenerate with `UPDATE_WEB_FIXTURE=1 cargo test -p litedoc4-global`",
            expected.display(),
        )
    });
    assert_eq!(on_disk, json, "{} is stale", expected.display());
}
