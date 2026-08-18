# litedoc4

**Fast HTML documentation for Lean 4 packages — built for the ones standing on Mathlib.**

litedoc4 documents **your** package only. Declarations from your dependencies — Mathlib,
Batteries, Lean core, whatever your `lake-manifest.json` pins — are never regenerated: every
reference to them links to that dependency's **version-pinned source on GitHub**. The output is a
self-contained static site.

Live example: <https://fujiharuka.github.io/information-theory/> — 422 modules, one command,
**24.5 s**.

## Is this for you?

**Yes**, if doc-gen4 is too slow for your CI and your package lives on GitHub and is on Lean
4.31.x. Any Lean 4 package works — the payoff just scales with how large your dependencies are
next to your own code, and Mathlib is as large as that gets. You get a module tree, search,
instance lists, "Imported by", hyperlinked signatures, and a dark theme.

**No**, if:

- **your docstrings need typeset math** — `$…$` is rendered as plain text; no MathJax, no KaTeX
- **you want to search dependency declarations** — search covers your package only
- **you are far from Lean 4.31.0** — nothing else is tested, and the extractor is compiled
  against your toolchain, so a mismatch surfaces as a build failure, not as bad output
- **you are on Windows, Intel macOS, or Linux/arm64** — releases carry Linux/x86-64 and
  Apple Silicon; anything else builds from source, which needs Rust and a C compiler

A `lakefile.lean` package is fine — the live example is one. The CLI will not guess library names
out of Lean code, so pass `--lib` by hand; used as a Lake dependency (below) it is read for you.

Already hosting doc-gen4 output? Page paths (`Foo/Bar.html`) and declaration anchors
(`#Foo.bar`) keep doc-gen4's shape, so existing links into your own docs survive.

## Speed

Apple M1 / 16 GB, warm page cache, wall clock. One Mathlib-dependent package throughout, measured
at two of its revisions — 432 modules then, 422 later — so every row names the one it ran on:

| | doc-gen4 | litedoc4 |
|---|---:|---:|
| Extract every module, single-threaded (432 modules) | 1,076 s | **14.08 s** |
| Full site from nothing, `--jobs 4` (422) | — | **24.5 s** |
| Rebuild after one added declaration (422) | — | **4.35 s** |
| Rebuild with nothing changed (422) | — | **0.31 s** |

The rebuild row is the median of 6 runs spread over 3.96–6.22 s. What moves between them is Lean's
environment load, not the work, so that wall clock is not something to gate on.

**The dashes are the point**: doc-gen4 is not doing these jobs. It documents your entire import
closure — ~8,600 modules here against your 432 — and regenerates every page on every run. That
build has never been finished on this machine (aborted at 42%, memory-bound, after 13,611 s of
CPU time, which extrapolates to ~9 h of CPU for the closure), so there is no wall-clock number to
put next to 24.5 s.

On a 4-core GitHub runner `litedoc4 build` took **11.5–20.7 s** for 422 modules. A first run also
builds the tools (~16 s extractor, ~24 s cargo, cached afterwards); the rest of the job is your
usual `lake exe cache get` + `lake build`. Peak resident memory ≈4.0 GB (3.88–4.03 GB across 50
runs). Raw logs: [`benchmarks/`](benchmarks/).

## Documentation on GitHub Pages

Copy this into your package as `.github/workflows/docs.yml`, and enable Pages in your
repository settings (Settings → Pages → Source: GitHub Actions).

```yaml
name: docs
on: { push: { branches: [main] } }
jobs:
  docs:
    runs-on: ubuntu-latest
    permissions: { contents: read, pages: write, id-token: write }
    environment: { name: github-pages }
    steps:
      - uses: actions/checkout@v4
      - uses: FujiHaruka/litedoc4@v0.1.4
        id: docs
        with:
          cache-get: true             # `lake exe cache get` — drop it if you have no Mathlib
      - uses: actions/upload-pages-artifact@v3
        with: { path: "${{ steps.docs.outputs.site }}" }
      - uses: actions/deploy-pages@v4
```

That is the whole thing: the action installs elan if it is missing, runs `lake build` and the
docs **in one job** (split across jobs the oleans fall out of the page cache and it runs 5–12×
slower), and keeps four caches for you — Mathlib's oleans, the Lean extractor, the Rust build,
and last run's state, which is what makes the second run incremental.

Inputs you may need: `root` if your package is not at the repository root, `lib` if you use
`lakefile.lean`, `lake-build: false` if you already built the package **earlier in the same
job** (from another job the page cache is cold), `full: true` to ignore previous state.
Outputs: `site`, `out`, `timings`, and `binary-source` — where the `litedoc4` binary came from
(`release`, `cached` or `cargo`), so a caller can assert on it.

To archive instead of publish, swap the last two steps for `actions/upload-artifact` and drop
the `permissions` / `environment` lines.

## Running it locally

You need `elan`/`lake` and a package that `lake build` passes. The `litedoc4` binary can be
downloaded; the Lean extractor is always built here, because it is compiled against **your**
toolchain.

### As a Lake dependency

Add it to your `lakefile.lean` (or the `[[require]]` equivalent in `lakefile.toml`):

```lean
require «litedoc4» from git "https://github.com/FujiHaruka/litedoc4" @ "v0.1.4"
```

```sh
lake run docs -- --out ../mypkg-docs
```

Lake builds the extractor against your toolchain and the script fills in `--root`,
`--extractor-bin` and `--lib` for you — including for a `lakefile.lean` package, where `--lib`
otherwise has to be written by hand.

`--out` is required and must be outside the package (`litedoc4 build` refuses one inside), and a
relative path resolves against the package directory, so `../mypkg-docs` is the shortest spelling
that works.

The `litedoc4` binary itself is fetched for you. The script looks, in order, at `$LITEDOC4_BIN`,
`${XDG_CACHE_HOME:-~/.cache}/litedoc4/v<version>/<target>/litedoc4`, the GitHub release matching
the version in the revision you required, `litedoc4` on `PATH`, and finally `cargo build`. When
none of them answers, it says what it looked for and where. A download announces its URL before
the first request and is used only if its SHA-256 matches the `checksums.txt` published beside it
— a mismatch, or no way to compute a SHA-256 at all, is a refusal rather than a warning, and
leaves the cache empty.

Releases carry `x86_64-unknown-linux-musl` and `aarch64-apple-darwin` only. On anything else —
Intel macOS, arm Linux — the script says there is no asset for your target and falls through to
`PATH` and `cargo build`; that is a normal path, not a failure.

```sh
LITEDOC4_NO_DOWNLOAD=1 lake run docs -- --out ../mypkg-docs   # never reach the network
```

`LITEDOC4_NO_DOWNLOAD` set to anything non-empty takes the same path, and still uses a binary
that is already in the cache — turning downloads off does not throw away one you already have.

There is deliberately no `lean-toolchain` in this package: one that named a *higher* version
would make your `lake update` rewrite **your** `lean-toolchain`, and a *lower* one would be
ignored without a warning (both measured, `benchmarks/results/lake-package-probe-2026-08-18.txt`).
Without the file, Lake builds the extractor with your toolchain and says nothing.

### From a checkout

```sh
git clone https://github.com/FujiHaruka/litedoc4 && cd litedoc4

# the extractor — always built here, against your package's toolchain (~16 s)
TARGET_REPO=/path/to/your-package extractor/build.sh     # -> extractor/build/extract

# the binary — download it (Linux/x86-64 shown; aarch64-apple-darwin is the
# other one), which unpacks to litedoc4-<version>-<target>/litedoc4 …
curl -sSfL https://github.com/FujiHaruka/litedoc4/releases/latest/download/litedoc4-x86_64-unknown-linux-musl.tar.gz \
  | tar xz
# … or build it, which needs Rust (via rustup) and a C compiler
cargo build --release                                    # -> target/release/litedoc4

# then, with whichever of the two you have:
./target/release/litedoc4 build --root /path/to/your-package --out /path/to/docs \
  --extractor-bin ./extractor/build/extract --jobs 4
```

The site is `<out>/site`; `--out` itself must live outside `--root`. Running the same command is
incremental, so keep `<out>` between runs; `--full` starts over. The extractor is built against
your package's toolchain — rebuild it (~15 s) when that changes.

Run `litedoc4` with no arguments for the full flag list. The two you may need:
`--lib <Name>` if your libraries are not in `lakefile.toml`, `--source-url <url>` if `origin` is
not GitHub (another host is refused rather than guessed).

## Status

`v0.1.4` — the action and the released binaries. Tested on macOS (Apple Silicon) and
`ubuntu-latest` with Lean/Mathlib v4.31.0. Pin the action to a tag (`@v0.1.4`); `@main` moves.

**Renamed from `lean-doc` on 2026-08-18** — repository, crates, CLI, and Lake package name.
**`v0.1.4` is the first release under the new name**, and the first tag whose tree carries
`lakefile.lean`. `v0.1.3` and earlier ship `lean-doc-*.tar.gz` and cannot be `require`d, so
pin to `v0.1.4` or later. What was deliberately *not* renamed, and why, is in
[`docs/plans/rename.md`](docs/plans/rename.md).

The extractor is not distributed as a binary. It **could** be — it is decided by the toolchain
alone, it is portable, and against the wrong toolchain it fails loudly rather than writing a
wrong IR (all three measured, `benchmarks/results/extractor-uniqueness-2026-08-18.txt`). It is
not shipped because at 226 MB it is not worth replacing a 16 s build that CI caches anyway.

## License

Apache-2.0 ([`LICENSE`](LICENSE)). litedoc4 is a derivative work of **doc-gen4**
(Apache-2.0, © 2021 Henrik Böving); attribution is in [`NOTICE`](NOTICE).
