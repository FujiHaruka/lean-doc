# experiments/stage1 — minimal Lean extractor

Verification stage 1 of [`docs/approach.md`](../../docs/approach.md) §7: *can we
extract a whole package's semantic information from a single process, using
lean-doc's own code rather than a patched doc-gen4?*

The measured baseline this is compared against comes from the instrumented
doc-gen4 `batch` command (`benchmarks/results/batch.jsonl`).

## What this measures — and what it does not

`Extract.lean` is a single-file program whose only dependency is `import Lean`.
It loads every module of the target package into **one** environment with a
single `importModules` call, then enumerates the declarations of those modules
twice, by two different routes:

| route | how | corresponds to |
|---|---|---|
| **index lookup** | for each target module, read `env.header.moduleData[i].constNames` | approach.md §5.2, the proposed direction |
| **scan lookup** | walk `env.constants`, keep constants whose defining module is a target | what doc-gen4 does today |

Both routes apply the same filter (`Name.isInternal`, `Name.isInternalDetail`)
and the program **exits non-zero if the two declaration sets differ**. That check
is the point: the index route is only interesting if it is also correct.

**It does no pretty printing and no semantic analysis.** No `MetaM`, no type
rendering, no equation generation, no docstring extraction. Therefore:

- Its `stage1.importModules` **is** comparable with doc-gen4's
  `load.importModules` (12.91 s in `benchmarks/results/batch.jsonl`): same work,
  same 432 direct imports.
- Its `stage1.total` is **not** comparable with doc-gen4's `batch.total`
  (45.93 s). It is a *floor* — "environment load plus declaration enumeration" —
  measured to show how much of the budget is spent before any real work starts.
  Reporting it as a speedup over 45.93 s would be comparing different amounts of
  work.

The scan-vs-index comparison is the honest one: identical inputs, identical
outputs, one process, same run.

## Build

```sh
./build.sh
```

lean-doc has no `lean-toolchain` and no lakefile on purpose — it must not carry a
Mathlib checkout. The build borrows the Lean environment from the measurement
target (`/Users/haruka/dev/lean-projects`) via `lake env`, compiles
`Extract.lean` to C with `lean -c`, and links a native binary with
`leanc -rdynamic`. Output goes to `build/` (gitignored).

`-rdynamic` is required: `importModules (loadExts := true)` runs module
initializers through the Lean interpreter, which needs Lean's symbols to stay
visible in the final executable (Lake spells this `supportInterpreter := true`).

The measurement must run on this compiled binary. Under `lean --run` the
enumeration loops are interpreted, which inflates `indexLookup` / `scanLookup`
and makes the numbers meaningless.

## Run

```sh
./run.sh                     # -> benchmarks/results/stage1-extract.jsonl
./run.sh stage1-extract-2nd  # warm re-run
```

`run.sh` records the run conditions (host, RAM, toolchain, `LEAN_NUM_THREADS`,
module count) and peak RSS into `benchmarks/results/<name>-summary.txt`, next to
the JSONL. Run it twice: the first/second difference is the olean page-cache
effect, which for this workload is large enough to change conclusions.

Manual invocation is `extract <modules.txt> <out.jsonl>`; the module list is one
module name per line (`benchmarks/results/it-modules.txt`, 432 lines).

## Output

The JSONL uses the same record shape as the doc-gen4 instrumentation, so
`benchmarks/tools/analyze.ts` reads it directly:

```sh
deno run -A ../../benchmarks/tools/analyze.ts ../../benchmarks/results/stage1-extract.jsonl
```

| phase | meaning | extra fields |
|---|---|---|
| `stage1.initSearchPath` | `initSearchPath (← findSysroot)` | |
| `stage1.importModules` | the single batched import | `directImports` |
| `stage1.envStats` | not a duration (`us` is 0) | `loadedModules` — size of the import closure |
| `stage1.indexLookup` | module → declarations | `targetModules`, `enumerated`, `kept` |
| `stage1.scanLookup` | environment → declarations | `scanned`, `relevant`, `kept` |
| `stage1.compare` | not a duration | `onlyIndex`, `onlyScan`, `agree` |
| `stage1.total` | whole program | `modules` |

Reading it:

- `stage1.importModules` vs doc-gen4's `load.importModules` — the environment
  load floor of approach.md §3. This is the only cross-tool time comparison this
  experiment supports.
- `scanned` vs `relevant` — the hit rate of the scan route. `relevant` counts
  constants belonging to a target module *before* the internal-name filter;
  `kept` counts what survives it.
- `stage1.scanLookup` vs `stage1.indexLookup` — the cost §5.2 proposes to
  delete, for one batched process. Note this is per *process*, not per module:
  the per-module scan cost doc-gen4 pays 432 times does not appear here.
- `agree` — must be `true`. If it is `false` the index route is unsound as
  written and §5.2 needs revisiting, not the numbers.

### Known caveat: `enumerated` > `relevant`

The two counters differ (8,849 vs 8,824 on the 432-module target) even though the
declaration *sets* agree. The gap is exactly the constant names that appear in
more than one module's `.olean` — auto-generated equation and congruence lemmas
such as `Foo.eq_1` and `Foo.congr_simp`, emitted again in each module that
elaborates them.

The scan route sees each such name once, because `const2ModIdx` records only the
first module that defined it; the index route enumerates it once per module. So
the routes agree on *which declarations exist* but not on *which module owns*
those names. On the measured target this is 25 occurrences, 5 of them
user-visible (non-internal) names, and every module involved is inside the
package — nothing escapes to a dependency.

This matters for page placement, not for coverage: an index-based extractor has
to pick an owner explicitly instead of inheriting doc-gen4's implicit
"first module wins" rule.
