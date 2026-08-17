# fuzz — the exploration behind the corpus gate

`crates/lean-doc-md/tests/data/fuzz/` is the gate: every file in it goes through
`parse` and the HTML assembly on every push, on stable, with no extra toolchain
(`docs/plans/quality-gates.md` 決定 5). **This directory is how entries get into
that gate** — libFuzzer plus AddressSanitizer, run by hand, out of band.

The subject is the C. `crates/lean-doc-md/vendor/md4c` is compiled by `build.rs`
and called across FFI, so the failure being hunted is a memory-safety failure
that no amount of Rust-side review would show.

## Running it

```
rustup toolchain install nightly --profile minimal
cargo +nightly install cargo-fuzz

export CXXFLAGS="-isystem $(xcrun --show-sdk-path)/usr/include/c++/v1"   # macOS only, see below
export CFLAGS="-fsanitize=address -fsanitize=fuzzer-no-link -mllvm -asan-guard-against-version-mismatch=0 -fno-omit-frame-pointer -g"

mkdir -p fuzz/corpus/docstring
cp crates/lean-doc-md/tests/data/fuzz/*.md fuzz/corpus/docstring/
cargo +nightly fuzz run docstring -- -max_total_time=600 -max_len=16384
```

A crash lands in `fuzz/artifacts/docstring/`. **It does not stay there**: copy
the input to `crates/lean-doc-md/tests/data/fuzz/` with a name saying what shape
it is, and raise the `seen >=` floor in `tests/fuzz_corpus.rs` so a corpus that
silently empties still fails.

## `CFLAGS` is not optional — it is what puts md4c under the fuzzer

`cargo-fuzz` passes `-Zsanitizer=address` and the sancov flags through
`RUSTFLAGS`, and **`RUSTFLAGS` reaches Rust only**. The C that `build.rs`
compiles is untouched by it, so without `CFLAGS` the run explores a binary in
which md4c is neither coverage-instrumented nor ASan-checked: the fuzzer gets no
feedback from inside the parser, and an overflow there is caught only if it
happens to hit an unmapped page.

The difference is measurable, and was measured before the exploration was
trusted — same seeds, same `-seed=1`, same `-runs=20000`【実測 2026-08-17】:

| md4c | libFuzzer's `cov:` |
|---|---|
| not instrumented (`RUSTFLAGS` only) | **1023** |
| instrumented (`CFLAGS` as above) | **2487** |

`-mllvm -asan-guard-against-version-mismatch=0` is there because Apple clang
emits `___asan_version_mismatch_check_apple_clang_1700` into every ASan module
constructor and Rust links its own ASan runtime, which has no such symbol; the
link fails without it. `CXXFLAGS` is a separate macOS problem — the Command Line
Tools' `usr/include/c++/v1` is missing the headers libFuzzer's own C++ needs,
and the SDK's copy has them.

## What has been run

All of it on one machine (Apple M1, aarch64-apple-darwin, Apple clang 17.0.0,
nightly + `cargo-fuzz` 0.13.2):

| when | what | execs | new corpus units | crashes |
|---|---|---|---|---|
| 2026-08-17 | `-max_len=16384`, 600 s, seeded with the committed corpus | **1,146,450** | 19,434 | **0** |
| 2026-08-17 | `-max_len=200001`, 300 s, same corpus | **388,027** | 4,145 | **0** |
| 2026-08-17 | `-max_len=4096`, 600 s, **empty corpus** (QV6, below) | **7,404,720** | 7,988 | **0** |

**8,939,197 executions, no crash and no ASan report**【実測】. That is a
statement about this machine, this build and this input distribution — not a
proof, and not a reason to stop adding corpus entries when a real docstring
surprises the parser.

### QV6 — the fuzzer finds the two shapes on its own

`docs/plans/quality-gates.md` §7 asks whether a fuzzer reaches the two inputs
that kill MD4Lean (a NUL inside a fenced code block, a GFM table with no body
row) without being told about them. **It does** — the third run above started
from an **empty** corpus, on purpose, because an input descended from a seed
proves nothing about discovery. Of the 7,988 units it kept:

| shape | units |
|---|---|
| a NUL between an opening fence and its close | **200** |
| a header row and a delimiter row with no body row after them | **43** |

They are not crashes here — this crate was written to survive both — so what
this measures is **reach**, not safety.

**Leak detection is not part of this.** LeakSanitizer does not run on macOS, so
a buffer md4c allocates and never frees would not show here.
