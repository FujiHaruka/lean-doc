# lean-doc

**Fast HTML documentation for Lean 4 packages — built for the ones standing on Mathlib.**

lean-doc documents **your** package only. Declarations from your dependencies — Mathlib,
Batteries, Lean core, whatever your `lake-manifest.json` pins — are never regenerated: every
reference to them links to that dependency's **version-pinned source on GitHub**. The output is a
self-contained static site.

Live example: <https://fujiharuka.github.io/information-theory/> — 422 modules, one command,
**24.5 s**.

## Is this for you?

**Yes**, if doc-gen4 is too slow for your CI and your package uses `lakefile.toml`, lives on
GitHub, and is on Lean 4.31.x. Any Lean 4 package works — the payoff just scales with how large
your dependencies are next to your own code, and Mathlib is as large as that gets. You get a
module tree, search, instance lists, "Imported by", hyperlinked signatures, and a dark theme.

**No**, if:

- **your docstrings need typeset math** — `$…$` is rendered as plain text; no MathJax, no KaTeX
- **you want to search dependency declarations** — search covers your package only
- **you are far from Lean 4.31.0** — nothing else is tested, and the extractor is compiled
  against your toolchain, so a mismatch surfaces as a build failure, not as bad output
- **you use `lakefile.lean`** — library names cannot be derived from it; pass `--lib` by hand
- **you are on Windows, Intel macOS, or Linux/arm64** — releases carry Linux/x86-64 and
  Apple Silicon; anything else builds from source, which needs Rust and a C compiler

Already hosting doc-gen4 output? Page paths (`Foo/Bar.html`) and declaration anchors
(`#Foo.bar`) keep doc-gen4's shape, so existing links into your own docs survive.

## Speed

432-module package on an Apple M1 / 16 GB, warm page cache, wall clock:

| | doc-gen4 | lean-doc |
|---|---:|---:|
| Extract all 432 modules, single-threaded | 1,076 s | **14.08 s** |
| Full site from nothing, `--jobs 4` | — | **29.5 s** |
| Rebuild after one added declaration | — | **~4 s** |
| Rebuild with nothing changed | — | **0.3 s** |

**The dashes are the point**: doc-gen4 is not doing these jobs. It documents your entire import
closure — ~8,600 modules here against your 432 — and regenerates every page on every run. That
build has never been finished on this machine (aborted at 42%, memory-bound, after 13,611 s of
CPU time, which extrapolates to ~9 h of CPU for the closure), so there is no wall-clock number to
put next to 29.5 s.

On a 4-core GitHub runner `lean-doc build` took **11.5–20.7 s** for 422 modules. A first run also
builds the tools (~16 s extractor, ~24 s cargo, cached afterwards); the rest of the job is your
usual `lake exe cache get` + `lake build`. Peak memory ≈3.3 GB. Raw logs: [`benchmarks/`](benchmarks/).

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
      - uses: FujiHaruka/lean-doc@v0.1.1
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
Outputs: `site`, `out`, `timings`.

To archive instead of publish, swap the last two steps for `actions/upload-artifact` and drop
the `permissions` / `environment` lines.

## Running it locally

You need `elan`/`lake` and a package that `lake build` passes. The `lean-doc` binary can be
downloaded; the Lean extractor is always built here, because it is compiled against **your**
toolchain.

```sh
git clone https://github.com/FujiHaruka/lean-doc && cd lean-doc

# the extractor — always built here, against your package's toolchain (~16 s)
TARGET_REPO=/path/to/your-package extractor/build.sh     # -> extractor/build/extract

# the binary — download it (Linux/x86-64 shown; aarch64-apple-darwin is the
# other one), which unpacks to lean-doc-<version>-<target>/lean-doc …
curl -sSfL https://github.com/FujiHaruka/lean-doc/releases/latest/download/lean-doc-x86_64-unknown-linux-musl.tar.gz \
  | tar xz
# … or build it, which needs Rust (via rustup) and a C compiler
cargo build --release                                    # -> target/release/lean-doc

# then, with whichever of the two you have:
./target/release/lean-doc build --root /path/to/your-package --out /path/to/docs \
  --extractor-bin ./extractor/build/extract --jobs 4
```

The site is `<out>/site`; `--out` itself must live outside `--root`. Running the same command is
incremental, so keep `<out>` between runs; `--full` starts over. The extractor is built against
your package's toolchain — rebuild it (~15 s) when that changes.

Run `lean-doc` with no arguments for the full flag list. The two you may need:
`--lib <Name>` if your libraries are not in `lakefile.toml`, `--source-url <url>` if `origin` is
not GitHub (another host is refused rather than guessed).

## Status

`v0.1.1` — the action and the released binaries. Tested on macOS (Apple Silicon) and
`ubuntu-latest` with Lean/Mathlib v4.31.0. Pin the action to a tag (`@v0.1.1`); `@main` moves.

The extractor is not distributed as a binary. It **could** be — it is decided by the toolchain
alone, it is portable, and against the wrong toolchain it fails loudly rather than writing a
wrong IR (all three measured, `benchmarks/results/extractor-uniqueness-2026-08-18.txt`). It is
not shipped because at 226 MB it is not worth replacing a 16 s build that CI caches anyway.

## License

Apache-2.0 ([`LICENSE`](LICENSE)). lean-doc is a derivative work of **doc-gen4**
(Apache-2.0, © 2021 Henrik Böving); attribution is in [`NOTICE`](NOTICE).
