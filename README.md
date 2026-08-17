# lean-doc

**Fast HTML documentation for Lean 4 packages that depend on Mathlib.**

`lean-doc` documents **your** package. Declarations that come from dependencies —
Mathlib, Batteries, Lean core — are not re-documented: every reference to them links
straight to that dependency's **version-pinned source on GitHub**, read out of your
`lake-manifest.json`.

```
https://github.com/leanprover-community/mathlib4/blob/<rev>/Mathlib/Order/Basic.lean#L67-L67
```

The result is a self-contained static site — no server, no build step for the reader,
and no requests to any external host while a page is being displayed.

Live example: <https://fujiharuka.github.io/information-theory/> — 422 modules,
produced by a single `lean-doc build` in **24.51 s** (measured, warm page cache).

```sh
lean-doc build --root <your package> --out <output dir> --extractor-bin <extractor>
```

---

## Performance

Numbers on this page carry one of two labels: **measured** (a log exists in
[`benchmarks/`](benchmarks/)) or **extrapolated** (a measured fraction, scaled up —
the fraction is always stated). Ratios are only meaningful with their denominator, so
each comparison below is a *separate* claim; do not merge them into one number.

Measurement target for every row: a Lean 4 package of **432 modules** depending on all
of Mathlib. Hardware: Apple M1 / 8 cores / 16 GB, macOS. Lean, Mathlib and doc-gen4 all
at v4.31.0.

### Compared with doc-gen4

| What is being compared | doc-gen4 | lean-doc | Ratio | Label |
|---|---:|---:|---:|---|
| **(a)** Extracting your 432 modules + persisting the IR | 1,076 s | **14.08 s** | **76×** | both measured, warm, single-threaded |
| **(b)** Building the whole site from scratch, dependencies included | ≈ 32,600 s (9.1 h) | **14.97 s** | **≈ 2,180×** | numerator extrapolated / denominator measured, warm |
| **(c)** Rendering the same 432 pages of HTML, nothing else | 3.17 s | **0.885 s** | **3.6×** | numerator extrapolated / denominator measured, warm |

- **(b) is mostly a difference in scope, not in implementation.** doc-gen4 documents the
  entire import closure (~8,600 modules); lean-doc documents your 432 and links to the
  rest. The numerator is extrapolated from a build that was stopped at 3,590 of 8,600
  modules (42%, 13,611 s measured) and scaled to the whole.
- **(c)**'s numerator is doc-gen4's measured render time over 6,072 modules, scaled down
  by module count.
- **No ratio is given for a one-module edit.** doc-gen4's incremental run does not
  regenerate HTML at all, and includes `lake build`; lean-doc's updates the HTML and does
  not. They are not the same job, so the ratio would not mean anything.

### `lean-doc build` on its own

432 modules, `--jobs 4`, all **measured**:

| Scenario | Time |
|---|---:|
| Full build, dependency map supplied from outside | cold **21.53 s** / warm **9.79 s** |
| Full build, one command, map built too (the default) | **29.50 s** (page cache cold-ish) |
| Run again with nothing changed | **0.30–0.40 s** (site is byte-identical) |
| A module moved, + `lake build` (35 re-extractions, 36 pages) | **5.80 s** |
| One declaration added to one module, + `lake build` (1 re-extraction, 1 page) | warm **3.96–6.22 s**, median 4.35 s (n=6) |
| Only the git revision changed (Lean never starts) | **0.65 s** for 433 pages |

**Do not compare a cold number against a warm one.** Lean's oleans are read through
`mmap`, so the same work moves by more than 2× with the page cache: loading the
environment alone is 2.5 s warm against 13 s cold (measured). The one-declaration row
above shows the same effect inside one binary — its floor is Lean's environment load
(2.4–3.8 s), not lean-doc.

On a GitHub Actions runner (4 cores / 15.6 GiB), `lean-doc build` over 422 modules took
**11.5–20.7 s** (measured, n=5).

### Why it is fast

Not because of the language it is written in. The cost that dominates a doc-gen4 run is
not the documentation work itself (all measured, see
[`benchmarks/doc-gen4-report.md`](benchmarks/doc-gen4-report.md)): 85.0% of the extraction
phase is spent loading the Lean environment, because it starts one process per module;
0.032% of the constants it walks are ones it actually documents; and the HTML phase is 0%
incremental — it regenerates every page on every run.

lean-doc drops four pieces of that work:

1. extracts everything in **one** Lean process;
2. looks declarations up in an index instead of scanning the environment;
3. does not regenerate dependencies at all — it keeps a name → module → pinned-source map
   and links;
4. generates HTML **without Lean**, from the IR, and skips pages whose inputs did not move.

---

## Requirements

- **Rust** — the pinned toolchain in `rust-toolchain.toml` is resolved by `rustup`
  automatically.
- **A C compiler** — CommonMark rendering links against a vendored copy of md4c.
- **elan / lake**, and a Lean package that `lake build` already passes. The extractor is
  compiled against *your* package's toolchain.
- **A GitHub `origin` remote**, if you want `--source-url` derived for you. Any other host
  works too, but you have to pass `--source-url` explicitly (see [Limitations](#limitations)).

Tested with **Lean / Mathlib v4.31.0** on macOS (Apple Silicon) and `ubuntu-latest`.

## Install

```sh
git clone https://github.com/FujiHaruka/lean-doc && cd lean-doc

# 1. The Rust side is self-contained  ->  target/release/lean-doc
cargo build --release

# 2. The extractor borrows your package's Lean environment  ->  extractor/build/extract
TARGET_REPO=/path/to/your-package extractor/build.sh
```

lean-doc ships no toolchain, no lakefile and no Mathlib of its own — the Lean environment
is borrowed from your package through `lake env`. The extractor binary is 171 MB and is
built **against your package's toolchain**, so rebuild it when the toolchain changes.
There is no prebuilt download and no default path for it, for the same reason.

## Usage

```sh
./target/release/lean-doc build \
  --root /path/to/your-package \
  --out  /path/to/docs \
  --extractor-bin ./extractor/build/extract \
  --jobs 4
```

The site is written to **`<out>/site`**. Point a static host at that directory, or open
`index.html` locally.

### What you do not have to pass

| Inferred | Where it comes from |
|---|---|
| `--lib` | the `[[lean_lib]]` blocks in `<root>/lakefile.toml`. A `lakefile.lean` is refused rather than guessed (exit 3) |
| the module list | globbed from your sources (not from `.lake/build`, which can hold orphaned oleans) |
| `--source-url` | git `HEAD` + the `origin` remote of `--root` |
| the dependency map | written by the extractor from the environment it imported; no upstream documentation site is needed |
| where dependency links point | `lake-manifest.json` (url + 40-hex revision), plus `lean --githash` for Lean core. **Entirely offline** — no network access at build time |
| full vs. incremental | whatever is already under `--out` |

### Options for `build`

| Flag | Meaning |
|---|---|
| `--root <dir>` | the Lean package to document (required) |
| `--out <dir>` | the directory lean-doc owns (required, no default) |
| `--extractor-bin <path>` | the extractor built by `extractor/build.sh`, or `$EXTRACT_BIN` |
| `--jobs <n>` | extractor threads (default 1; 4 is a good starting point) |
| `--lib <Name>` | a library root, repeatable — overrides the lakefile |
| `--source-url <url>` | override the derived `https://host/owner/repo/blob/<rev>` prefix |
| `--lake <path>` | the `lake` executable, or `$LAKE` (default `lake`). The `lean` used for `--githash` is looked up next to it |
| `--full` | rebuild everything, ignoring the state under `--out` |
| `--link-index <file>` | use an externally supplied dependency map instead of building one |
| `--timings <file>` | write one JSON line of counts and durations |

### About `--out`

`--out` is **required and has no default**. The obvious default, `<root>/.lake/build/doc`,
is doc-gen4's own output tree, and making it the default would mean deleting someone
else's output. lean-doc therefore refuses (exit 3):

- an `--out` inside `--root`;
- a non-empty directory that does not carry lean-doc's `lean-doc-build.json` marker.

It only ever deletes files inside a tree it can confirm it created.

### Incremental builds

Run the same command against the same `--out` and the build is incremental: lean-doc
hashes your oleans against the ledger it wrote last time, re-extracts only the modules
that moved, and re-renders only the pages affected by the result. `<out>/{ir,state,work}`
and `<out>/ledger.json` hold that state — keep them between runs (in CI: cache them).

Rerunning with nothing changed rewrites nothing: the site stays byte-identical
(measured — a 438-line sha256 manifest matches exactly). Use `--full` to start over.

### Other subcommands

`lean-doc build` is the whole pipeline. Each stage is also exposed on its own
(`modules`, `extract`, `site`, `render`, `global`, `incremental`, `ledger`, `merge`,
`ownership`, `impact`, `prune`) for scripting and debugging. Run `lean-doc` with no
arguments for the full usage text.

### Exit codes

| Code | Meaning |
|---:|---|
| 0 | success |
| 1 | the run failed |
| 2 | the command line is wrong (usage is printed) |
| 3 | refused: the inputs and the files on disk disagree, or an unsafe `--out` |
| 4 | the extractor exited non-zero (its own code is in the message) |
| 5 | the incremental loop hit `--max-rounds` with modules still stale |

---

## What the generated site contains

`<out>/site` holds **one page per module**, plus 7 whole-package files and 3 static
assets. For a 432-module package that is 442 files.

| File | Purpose |
|---|---|
| `index.html` | entry point and full module listing |
| `404.html` | GitHub Pages custom 404; suggests near-miss names |
| `search.html` | search results page |
| `foundational_types.html` | `Sort` / `Type` / `Prop` — types with no source to link to |
| `modules.json` | module tree and "Imported by", read by every page |
| `search-index.json` | search and instance lists, fetched on demand |
| `declarations/name-map.json` | consumed by the incremental pipeline |
| `style.css`, `app.js`, `favicon.svg` | embedded in the binary and written on every build |

In the browser you get the module tree, incremental search over your package
(`/` focuses it, arrow keys and Enter navigate), instance lists resolved per declaration,
"Imported by", a light/dark/auto theme toggle that persists, and pages that stay readable
with JavaScript disabled. The site loads **nothing** from an external host; the only
outbound links are the pinned dependency source URLs.

## GitHub Actions

Copy [`.github/workflow-templates/lean-doc-docs.yml`](.github/workflow-templates/lean-doc-docs.yml)
into your package's repository as `.github/workflows/docs.yml`. It is a thin wrapper
around [`tools/ci-build.sh`](tools/ci-build.sh), which runs the whole thing in one job:

```sh
tools/ci-build.sh --root <your package> --out <dir> --cache-get --jobs 4
```

`ci-build.sh` runs `lake exe cache get` (with `--cache-get`), `lake build`, the extractor
build, `cargo build`, and finally `lean-doc build`, reporting the wall-clock time of each
phase. Because the commands live in a script rather than inline YAML, you can run exactly
what CI runs on your laptop.

**Keep `lake build` and the documentation build in the same job.** The extractor's floor
is loading the Lean environment, and that is I/O: what decides it is whether the oleans
are in the runner's page cache. Measured on Linux runners (n=5):

| | Environment load |
|---|---|
| Same job vs. split job (4 cores / 15.6 GiB) | 2.4–2.6 s vs. 2.5–2.9 s = **1.08–1.58×** |
| Same runner with the page cache dropped | **13.5–22.3 s = 5.2–11.9×** |
| Split job (2 cores / 7.75 GiB) | 20–89 s = 8–34× |

A split job is not *reliably* cold — it writes the oleans itself and they may stay
resident if the runner has room. One job simply removes the question, and the cold
penalty (5–12×) is large enough to be worth removing.

Four caches are worth setting up, each restored into the job that uses it:
`~/.cache/mathlib` (keyed on `lake-manifest.json`), `extractor/build` (keyed on
`lean-toolchain` + the extractor source hash), cargo + `target` (keyed on `Cargo.lock`),
and **`--out` itself** — the last one is what makes the incremental path reachable at
all, since without the previous ledger every module is re-extracted. The template
documents all four inline.

## Limitations

Things lean-doc deliberately does not do:

- **No math typesetting.** Neither MathJax nor KaTeX is bundled; `$…$` is rendered as
  text. On the measurement target, exactly 1 of 348 pages contained TeX.
- **Search covers your package only.** Declarations from dependencies are linked, but not
  indexed for search.
- **Over `file://`, the module tree does not render** — the browser blocks the `fetch`
  for `modules.json` at a non-HTTP origin. Page content is fully readable, and a
  `<noscript>` fallback points at `index.html`. Serve over HTTP for the full experience.
- **`lakefile.lean` is not read.** Pass `--lib` explicitly, or use `lakefile.toml`.
- **A non-GitHub `origin` is refused** rather than guessed (exit 3): only GitHub's
  `/blob/<rev>/<path>` shape is known, and a guessed one would 404 on every declaration of
  every page. Pass `--source-url` yourself if your host uses a different shape.
- **No backwards compatibility across Lean versions.** v4.31.0 is what is tested; other
  versions may or may not work.
- **Windows is untested.** macOS and Linux are.

## License

**Apache License 2.0** ([`LICENSE`](LICENSE)).

lean-doc is a **derivative work of doc-gen4** (Apache-2.0, © 2021 Henrik Böving): its
rendering and extraction decisions were rewritten from doc-gen4's, and parts of the
extractor are literal copies. The whole project is therefore distributed under the same
license. Third-party attribution is in [`NOTICE`](NOTICE); the file-by-file breakdown is
in [`docs/provenance.md`](docs/provenance.md).

lean-doc does not link against doc-gen4 — the extractor only does `import Lean`, and the
Rust side has no Lean dependency at all.

## Repository layout

```
crates/        the Rust side: IR, markdown, rendering, whole-package artifacts, incremental, CLI
extractor/     the Lean extractor; build.sh borrows your package's lake env
e2e/micro/     end-to-end fixtures (Lean, no Mathlib)
tools/         the CI script and the quality gates
benchmarks/    measurement reports, instrumentation and raw logs — where the numbers come from
docs/          design and verification records
```
