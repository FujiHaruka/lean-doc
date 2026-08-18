//! A target that leaks on purpose, so that a green leak check can be believed.
//!
//! LeakSanitizer does not run on macOS, so the leak question was left open with
//! the machine that could answer it — `ubuntu-latest` — free all along
//! (`docs/plans/unverified-sweep.md` U4). Running LSan there is easy; knowing
//! that it *ran* is the part that needs this file.
//!
//! `.github/workflows/ci-leak.yml` runs this target first and **fails if it
//! passes**. A leak check that reports nothing looks identical whether the
//! subject is clean or the sanitizer is not linked in, and this is the only way
//! to tell those apart — the same shape as Q6's `CFLAGS` finding, where a fuzzer
//! that reached none of the C looked exactly like a fuzzer that found nothing.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Allocated, made unreachable, never freed: what LSan calls a definite leak.
    // `Box::leak` alone would keep a live reference, which LSan tolerates as
    // still-reachable, so the pointer is dropped on the floor instead.
    let boxed: Box<[u8]> = data.to_vec().into_boxed_slice();
    std::mem::forget(boxed);
});
