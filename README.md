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
- **you cannot build from source** — there is no released binary and no `cargo install` yet

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

Copy this into your package as `.github/workflows/docs.yml`. Keep `lake build` and the docs in
**one job** — split across jobs the oleans fall out of the page cache and it runs 5–12× slower.

```yaml
name: docs
on: { push: { branches: [main] } }
jobs:
  docs:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with: { path: package }
      - uses: actions/checkout@v4                  # add `ref:` to pin lean-doc
        with: { repository: FujiHaruka/lean-doc, path: lean-doc }
      - run: |
          curl -sSfL https://raw.githubusercontent.com/leanprover/elan/master/elan-init.sh \
            | sh -s -- -y --default-toolchain none
          echo "$HOME/.elan/bin" >> "$GITHUB_PATH"
      - uses: actions/cache@v4                     # Mathlib's oleans
        with: { path: ~/.cache/mathlib, key: "mathlib-${{ hashFiles('package/lake-manifest.json') }}" }
      - uses: actions/cache@v4                     # the Lean extractor (171 MB)
        with: { path: lean-doc/extractor/build, key: "extractor-${{ hashFiles('package/lean-toolchain', 'lean-doc/extractor/Extract.lean') }}" }
      - uses: actions/cache@v4                     # the Rust build
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            lean-doc/target
          key: cargo-${{ hashFiles('lean-doc/Cargo.lock') }}
      - uses: actions/cache@v4                     # last run's state — what makes it incremental
        with: { path: docs-out, key: "leandoc-docs-${{ github.sha }}", restore-keys: leandoc-docs- }
      - run: |                                     # no Mathlib? drop --cache-get
          lean-doc/tools/ci-build.sh --root package \
            --out "$GITHUB_WORKSPACE/docs-out" --cache-get --jobs 4
      - uses: actions/upload-artifact@v4
        with: { name: docs, path: docs-out/site }
```

To publish rather than archive: swap the last step for `actions/upload-pages-artifact` +
`actions/deploy-pages`, add `pages: write` and `id-token: write` to `permissions:`, and enable
Pages in your repository settings.

## Running it locally

Needs Rust (via `rustup`), a C compiler, `elan`/`lake`, and a package that `lake build` passes.

```sh
git clone https://github.com/FujiHaruka/lean-doc && cd lean-doc
cargo build --release                                    # -> target/release/lean-doc
TARGET_REPO=/path/to/your-package extractor/build.sh     # -> extractor/build/extract

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

`v0.1.0`, tagged 2026-08-17 — build from source, and pin `ref:` in CI if you want it to hold
still. Tested on macOS (Apple Silicon) and `ubuntu-latest` with Lean/Mathlib v4.31.0.

## License

Apache-2.0 ([`LICENSE`](LICENSE)). lean-doc is a derivative work of **doc-gen4**
(Apache-2.0, © 2021 Henrik Böving); attribution is in [`NOTICE`](NOTICE).
