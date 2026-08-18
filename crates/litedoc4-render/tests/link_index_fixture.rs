//! Reads the real `.lidx` of the target package's dependency closure.
//!
//! ~10 MB, written by `litedoc4 build` itself. It lives outside the repository,
//! so this test is `#[ignore]`d rather than silently skipped: `cargo test` has
//! to pass on a machine that has never run the pipeline, and a run that reports
//! it as ignored says out loud that it did not run.
//!
//! # What it checks — the reader against the writer, on real data
//!
//! The format marker; that every entry resolves and no module name leaked into
//! the entry table; and — read out of the **text**, not out of this reader — the
//! last entry with the group header above it, and a module that is a link target
//! in its own right.
//!
//! # What it stopped checking【判断 2026-08-16】
//!
//! Four counts the **prototype's** reader measured over the **prototype's**
//! `.lidx`: 258,760 entries / 8,494,819 UTF-16 code units / 5,775 modules /
//! 6,115 `@` names (`benchmarks/results/stage7c-render-timings.jsonl`).
//!
//! **The question stopped being askable, and not because the numbers were lost.**
//! That file was derived from Mathlib's `declaration-data.bmp` by
//! `experiments/stage7d/build-link-index.ts`, which left with `experiments/`
//! (tag `experiments-frozen`). The product derives the same map **by walking the
//! environment** instead (V1, M5-a) — a different construction, so a different
//! tally: **255,809 entries over the same target**【実測 2026-08-16】. Holding a
//! walk-derived map to a bmp-derived count was never the same question; keeping
//! it only meant the test ran on a file nobody can rebuild, which is where the
//! four counts had already left it (the default path was empty, so the gate read
//! this as "no input").

use std::path::PathBuf;
use std::time::Instant;

use litedoc4_render::LinkIndex;
use litedoc4_render::link_index::FORMAT_MARKER;

/// Where `benchmarks/tools/check-lidx-urls.sh` leaves the `.lidx` it drives
/// (its `WORK_DIR`), which is also where `crates/litedoc4/src/packages.rs`'s
/// corpus test looks. Any `litedoc4 build --out <dir>` writes one at
/// `<dir>/link-index.lidx`; copy it here.
const DEFAULT_LINK_INDEX: &str = "/private/tmp/litedoc4-m7a/link-index.lidx";

/// The fixture, or a panic naming what to set.
///
/// The only caller is `#[ignore]`d, so reaching this function at all means the
/// corpus gate asked for the test by name. Returning "not here, never mind"
/// there would be a green result for a comparison that never ran.
fn fixture() -> PathBuf {
    let path = PathBuf::from(
        std::env::var("LITEDOC4_LINK_INDEX").unwrap_or_else(|_| DEFAULT_LINK_INDEX.to_owned()),
    );
    assert!(
        path.is_file(),
        "no link index at {}: set LITEDOC4_LINK_INDEX, or run this test through \
         tools/corpus-gate.sh, which is the only thing that should be asking for it",
        path.display()
    );
    path
}

#[test]
#[ignore = "corpus: needs LITEDOC4_LINK_INDEX (tools/corpus-gate.sh)"]
fn reads_the_dependency_closure_of_the_target_package() {
    let path = fixture();
    // The two halves are timed apart because they scale differently: the read
    // is the file's bytes and the parse is its entries, and M7-a moved the
    // first by 23.6% while adding a field to every one of the second.
    let start = Instant::now();
    let text = std::fs::read_to_string(&path).expect("readable");
    let read = start.elapsed();

    let start = Instant::now();
    let index = LinkIndex::parse(&text);
    let elapsed = start.elapsed();
    eprintln!(
        "{}: {} B / {} entries ({} with a line range) / {} modules / {} module names; \
         read {:.3} s, parse {:.3} s",
        path.display(),
        text.len(),
        index.len(),
        index.ranged_len(),
        index.module_count(),
        index.known_modules().len(),
        read.as_secs_f64(),
        elapsed.as_secs_f64(),
    );

    // **Either marker**: `#lidx1` is what the prototype and M5-a wrote, and
    // `#lidx2` is what M7-a writes. The reader branches on neither (see
    // [`FORMAT_MARKER`]), so what this checks is that the file has one at all.
    let marker = text.split('\n').next().unwrap_or_default();
    assert!(
        marker == FORMAT_MARKER || marker == "#lidx1",
        "format marker: {marker:?}"
    );
    // Every entry resolves, and no module name leaked into the entry table.
    for name in ["Nat.succ", "Nat.add_comm"] {
        let module = index.module_of(name).unwrap_or_else(|| panic!("{name}"));
        assert!(!module.is_empty(), "{name} landed in the empty module");
    }

    // Spot check against the file itself rather than against this reader: the
    // last entry in the text, and the header it sits under.
    let (module, name) = last_entry(&text);
    assert_eq!(index.module_of(&name), Some(module.as_str()));
    assert!(
        index.is_known_module("Mathlib.Order.Basic"),
        "a module that is a link target in its own right"
    );
}

/// The final `\t` line of the file, and the group header above it. The name is
/// the first field, since M7-a a line can carry two more.
fn last_entry(text: &str) -> (String, String) {
    let mut current = String::new();
    let mut last = (String::new(), String::new());
    for line in text.split('\n') {
        match line.as_bytes().first() {
            Some(b'\t') => {
                let name = line[1..].split('\t').next().unwrap_or_default();
                last = (current.clone(), name.to_owned());
            }
            Some(b'@' | b'#') | None => {}
            Some(_) => current = line.to_owned(),
        }
    }
    last
}
