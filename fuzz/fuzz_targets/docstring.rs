//! Everything this crate does to a docstring, under libFuzzer and ASan.
//!
//! The subject is the C: `crates/litedoc4-md/vendor/md4c` is compiled by
//! `build.rs` and called across FFI, so the failure this hunts for is a
//! memory-safety failure that no amount of Rust-side review would show. A wrong
//! `<p>` is a different test's problem — what is asserted here is only that
//! control comes back and that ASan stays quiet.
//!
//! Crashes found here do not stay here: the input goes into
//! `crates/litedoc4-md/tests/data/fuzz/`, where the stable-toolchain gate runs
//! it on every push.

#![no_main]

use litedoc4_md::{NoLinks, Renderer};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // The extractor hands this crate a Lean `String`, so the input is already
    // valid UTF-8 by the time it arrives. Converting lossily matches that
    // boundary instead of widening it to a shape production never sees — the
    // interesting content (NUL included) survives either way.
    let text = String::from_utf8_lossy(data);
    let renderer = Renderer::new("../", &NoLinks);
    let _ = renderer.docstring(&text);
});
