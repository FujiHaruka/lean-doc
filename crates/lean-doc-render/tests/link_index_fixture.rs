//! Reads the real `.lidx` of the target package's dependency closure.
//!
//! 8.5 MB derived from Mathlib's `declaration-data.bmp`
//! (`experiments/stage7d/build-link-index.ts`). It lives outside the repository,
//! so this test **skips** when it is absent rather than failing: `cargo test`
//! has to pass on a machine that has never run the pipeline. Point
//! `LEAN_DOC_LINK_INDEX` at another file to run the structural half against it.
//!
//! The expected counts come from the TypeScript side, not from this reader:
//!
//! | | value | source |
//! |---|---:|---|
//! | entries (`linkIndex.size`) | 258,760 | `benchmarks/results/stage7c-render-timings.jsonl` |
//! | file, in UTF-16 code units | 8,494,819 | same |
//! | modules that define an entry | 5,775 | `docs/verification-log.md` (段階 7c) |
//! | `@` module names | 6,115 | `benchmarks/results/stage5b-stale-summary.txt` (`declaration-data.bmp`'s `modules`) |

use std::path::{Path, PathBuf};
use std::time::Instant;

use lean_doc_render::LinkIndex;
use lean_doc_render::link_index::FORMAT_MARKER;

const DEFAULT_LINK_INDEX: &str = "/private/tmp/lean-doc-relay/w7c/linkindex/link-index.lidx";

/// 実測, `stage7c-render-timings.jsonl`: what the prototype's renderer counted
/// after parsing this same file.
const TS_ENTRIES: usize = 258_760;
const TS_CODE_UNITS: usize = 8_494_819;
const TS_MODULES: usize = 5_775;
const TS_KNOWN_MODULES: usize = 6_115;

fn fixture() -> Option<PathBuf> {
    let path = PathBuf::from(
        std::env::var("LEAN_DOC_LINK_INDEX").unwrap_or_else(|_| DEFAULT_LINK_INDEX.to_owned()),
    );
    if path.is_file() {
        Some(path)
    } else {
        eprintln!(
            "skipping: no link index at {} (set LEAN_DOC_LINK_INDEX to point at one)",
            path.display()
        );
        None
    }
}

#[test]
fn reads_the_dependency_closure_of_the_target_package() {
    let Some(path) = fixture() else { return };
    let text = std::fs::read_to_string(&path).expect("readable");

    let start = Instant::now();
    let index = LinkIndex::parse(&text);
    let elapsed = start.elapsed();
    eprintln!(
        "{}: {} entries / {} modules / {} module names in {:.3} s",
        path.display(),
        index.len(),
        index.module_count(),
        index.known_modules().len(),
        elapsed.as_secs_f64(),
    );

    assert!(text.starts_with(FORMAT_MARKER), "format marker");
    // Every entry resolves, and no module name leaked into the entry table.
    for name in ["Nat.succ", "Nat.add_comm"] {
        let module = index.module_of(name).unwrap_or_else(|| panic!("{name}"));
        assert!(!module.is_empty(), "{name} landed in the empty module");
    }

    if path != Path::new(DEFAULT_LINK_INDEX) {
        eprintln!("structural checks only: not the file the counts were measured on");
        return;
    }

    assert_eq!(index.len(), TS_ENTRIES);
    assert_eq!(index.module_count(), TS_MODULES);
    assert_eq!(index.known_modules().len(), TS_KNOWN_MODULES);
    assert_eq!(
        text.encode_utf16().count(),
        TS_CODE_UNITS,
        "the prototype measured the file in UTF-16 code units, not bytes"
    );

    // Spot check against the file itself rather than against this reader: the
    // last entry in the text, and the header it sits under.
    let (module, name) = last_entry(&text);
    assert_eq!(index.module_of(&name), Some(module.as_str()));
    assert!(
        index.is_known_module("Mathlib.Order.Basic"),
        "a module that is a link target in its own right"
    );
}

/// The final `\t` line of the file, and the group header above it.
fn last_entry(text: &str) -> (String, String) {
    let mut current = String::new();
    let mut last = (String::new(), String::new());
    for line in text.split('\n') {
        match line.as_bytes().first() {
            Some(b'\t') => last = (current.clone(), line[1..].to_owned()),
            Some(b'@' | b'#') | None => {}
            Some(_) => current = line.to_owned(),
        }
    }
    last
}
