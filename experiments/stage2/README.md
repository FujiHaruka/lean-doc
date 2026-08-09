# experiments/stage2 — semantic extraction

Verification stage 2 of [`docs/approach.md`](../../docs/approach.md) §7: *now
that declarations can be reached through the module index (stage 1), how long
does it take to produce everything a documentation page needs — and does custom
syntax survive it?*

Stage 1 (`../stage1`) is left untouched; it remains the "environment load +
enumeration" floor. This directory adds what stage 1 deliberately left out, in
two increments:

1. the **semantic analysis** — signature, docstring, kind, range, equations;
2. the **per-module collection** — module docstrings, direct imports, tactic
   docs; doc-gen4's `getAllModuleDocs`, which is 54% of its batch run.

## What it does

For every declaration reached through `moduleData[i].constNames`:

| | how | doc-gen4 counterpart |
|---|---|---|
| blacklist | `isProjFn`, `findDeclarationRanges?`, `isInternal`, `isAuxRecursor`, `isNoConfusion`, `isInternalDetail`, `isRec`, `isMatcher` | `DocInfo.isBlackListed` |
| signature | `delabCore` + `delabForallParamsWithSignature` → `sanitizeSyntax` → `parenthesize` → `format` per binder and for the type | `Info.ofTypedName` |
| docstring | `findDocString?` | `getDocString?` |
| kind | `axiom` / `theorem` / `opaque` / `definition` / `instance` / `inductive` / `structure` / `class` / `constructor` | `DocInfo.getKind` |
| range | `findDeclarationRanges?` | same |
| equations | `getEqnsFor?`, else `valueToEq`; **off by default**, `--equations` turns it on | `computeEquations?` |
| structure fields / parents / ctor, inductive ctors | `withFields`-equivalent + one signature pp each | `getFieldTypes`, `InductiveInfo.ofInductiveVal` |

And per module:

| | how | doc-gen4 counterpart |
|---|---|---|
| module docstrings | `getModuleDoc?` | same |
| direct imports | `moduleData[i].imports` | same |
| tactic docs | `allTacticDocs` **once**, bucketed by defining module | `collectTactics` per module |

The blacklist is transcribed rather than approximated on purpose: a different
exclusion granularity changes the declaration count, and a time comparison over
different declaration counts is meaningless. On the fixed target it reproduces
doc-gen4's output exactly — 4,750 declarations with the same kind histogram
(see "Counts" below).

Each declaration is analyzed in its own `MetaM.toIO` with a fresh `Core.State`
and `maxHeartbeats := 5000000`, the same way doc-gen4's `process` loop does it,
so neither side accumulates elaborator state across declarations.

Only `import Lean`. lean-doc does not depend on doc-gen4.

### What it deliberately does not do

These are omitted from this increment, so the measured time is a *lower* bound
on doc-gen4-equivalent work, not an upper one:

- **`renderTagged` / `tagCodeInfos`** — doc-gen4 turns the formatted signature
  into `RenderedCode` so that identifiers become links. `--tag` runs the
  `tagCodeInfos` half of that to measure what it costs.
- **attributes** (`getAllAttributes`), **instance class/type indices**
  (`getInstanceTypes`), **`sorried`** (`Expr.hasSorry` over the proof term).
- **IR persistence.** `--dump` / `--dump-modules` write JSON Lines so results can
  be eyeballed and diffed; that is not the IR format. doc-gen4's database write
  (0.68 s on this target) therefore has no counterpart here.

## Build

```sh
./build.sh
```

Same constraints as stage 1: no toolchain and no lakefile here, the environment
is borrowed from the measurement target via `lake env`; `--root` is needed
because the source lives outside that repository; `leanc -rdynamic` is required
or module initializers cannot be interpreted. Output goes to `build/`
(gitignored). Measure on the compiled binary, never `lean --run`.

## Run

```sh
./run.sh stage2-noeq                       # equations off (default)
./run.sh stage2-eq   -- --equations        # equations on
./run.sh stage2-open -- --open InformationTheory.Shannon --dump /tmp/d.jsonl
```

`run.sh` records the conditions and peak RSS into
`benchmarks/results/<name>-summary.txt` next to the JSONL. Run the same
measurement at least five times: the first run is page-cache cold and inflates
`importModules` roughly 5×.

Binary usage:

```
extract <modules.txt> <out.jsonl> [--equations] [--dump <p>] [--dump-modules <p>]
        [--only <p>] [--open <ns,..>] [--tag] [--skip-analyze]
        [--tactics-emulate] [--tactics-probe] [--dump-tactics <p>]
```

- `--only <p>` restricts processing to the declaration names listed in `<p>`;
  used with `--dump` to inspect individual signatures.
- `--dump-modules <p>` writes one JSON object per module (docstrings, imports,
  tactics); this is what the doc-gen4 database is diffed against.
- `--skip-analyze` skips the semantic analysis, so the per-module collection can
  be measured and diffed on its own.
- `--tactics-emulate` additionally runs the collection doc-gen4's way (one
  `allTacticDocs` per module) in the same process, split into the call itself and
  the filter loop over its result. `--tactics-probe` breaks a single
  `allTacticDocs` call down further. Both are diagnosis only — they inflate
  `stage2.total`, so never measure with them on.
- `--dump-tactics <p>` writes every tactic in the environment with its defining
  module, ignoring the target list. Used to build the module list below.
- `--open <ns,..>` activates the scoped-notation extensions of those namespaces
  (`Lean.activateScoped`) and puts them in `openDecls` before pretty printing.
  See "Scoped notation" below.

## Output

JSONL with the same record shape as the doc-gen4 instrumentation, so
`benchmarks/tools/analyze.ts` reads it directly.

| phase | meaning | extra fields |
|---|---|---|
| `stage2.initSearchPath` | | |
| `stage2.importModules` | the single batched import | `directImports` |
| `stage2.envStats` | not a duration | `loadedModules` |
| `stage2.indexLookup` | module → declaration names | `targetModules`, `enumerated`, `candidates` |
| `stage2.moduleDocs` | module docstrings + direct imports | `modules`, `moduleDocs`, `modulesWithDocs`, `imports` |
| `stage2.tactics` | one `allTacticDocs` + bucketing | `tacticsInEnv`, `tacticsAssigned` |
| `stage2.analyze` | the semantic analysis | `considered`, `produced`, `blacklisted`, `failed`, `ppUs`, `eqUs`, `docUs`, `equations`, `eqFailures`, `genEquations`, `tagCode` |
| `stage2.dump` | JSON serialization (only with `--dump`) | `records` |
| `stage2.dumpModules` | ditto for the module records (`--dump-modules`) | `records` |
| `stage2.tacticsPerModule` | diagnosis only (`--tactics-emulate`) | `calls`, `allTacticDocsUs`, `filterLoopUs` |
| `stage2.tacticsProbe` | diagnosis only (`--tactics-probe`) | see `Extract.lean` |
| `stage2.total` | whole program | `modules` |

`stage2.analyze` is the number to compare with doc-gen4's
`process.constantLoop.ofConstantUs`; `stage2.moduleDocs + stage2.tactics` with
`process.getAllModuleDocs`; `stage2.importModules` with `load.importModules`.
**`stage2.total` is still not comparable with `batch.total`**: doc-gen4's total
also contains the database write, which this program does not do.

## Counts — how the equivalence with doc-gen4 was checked

Timing claims are only worth something if both tools do the same work, so the
output was compared against the database doc-gen4 writes for the same 432
modules (`batch ... bench-stage2cmp.db`, deleted afterwards). All **実測**:

| | stage2 `--dump` | doc-gen4 DB | |
|---|---:|---:|---|
| declarations | 4,750 | `name_info` 4,750 | ✓ |
| theorem / definition / instance / structure / constructor / opaque | 3742 / 842 / 91 / 37 / 37 / 1 | same | ✓ |
| binders | 49,658 | `declaration_args` 49,338 | see below |
| structure fields / parents / ctors | 156 / 1 / 37 | 156 / 1 / 37 | ✓ |
| docstrings | 3,394 | `declaration_markdown_docstrings` 3,394 | ✓ |
| equation lemmas (`--equations`) | 863 | `definition_equations` 863 | ✓ |

The 320-binder gap is exactly the binders of the 37 `constructor` declarations,
which doc-gen4 computes but does not persist (they are `render := false` and get
re-rendered from their parent structure). Per kind the binder counts are
identical: theorem 42,914, definition 5,545, instance 711, structure 168.

For the per-module half, `--dump-modules` is diffed against the same database by
`benchmarks/tools/compare-modules.py`; the output is
`benchmarks/results/stage2-modcmp.txt`. Every set is **identical**, not merely
equal in count:

| | lean-doc | doc-gen4 DB | |
|---|---:|---:|---|
| modules | 432 | `modules` 432 | ✓ |
| (importer, imported) | 3,503 | `module_imports` 3,503 | ✓ |
| (module, line, text) | 1,515 | `module_docs_markdown` ⋈ `declaration_ranges` 1,515 | ✓ |
| tactics | 0 | `tactics` 0 | ✓ |

**0 = 0 proves nothing about the bucketing**, so the check was repeated on a
second list: the 432 target modules *plus* the 141 modules that define the
environment's 421 tactics (`benchmarks/results/stage2-tacmods.txt`, built from
`--dump-tactics`). The environment is the same 6,021 modules either way, so this
only changes which modules the tactics get assigned to.

| | lean-doc | doc-gen4 DB | |
|---|---:|---:|---|
| modules | 573 | 573 | ✓ |
| (importer, imported) | 4,096 | 4,096 | ✓ |
| (module, line, text) | 1,740 | 1,740 | ✓ |
| (module, internal name, user name, docstring) | 421 | `tactics` 421 | ✓ |
| (module, internal name, tag) | 0 | `tactic_tags` 0 | ✓ |

The numbers themselves live in `benchmarks/results/stage2-*.jsonl` with
conditions in the matching `*-summary.txt`; `stage2-dg4-*.jsonl` is the
same-session doc-gen4 baseline (`stage2-dg4-summary.txt`), and
`stage2-notation-samples.txt` is the notation evidence below.

## Where doc-gen4's `getAllModuleDocs` spends 16.7 seconds

`--tactics-emulate` runs the collection doc-gen4's way inside this process and
reproduces the number: **16.91 s** against doc-gen4's 16.64 s (warm, both 実測,
`benchmarks/results/stage2-tacdiag*`). Split:

| | | |
|---|---:|---|
| `allTacticDocs` | 3.76 s (22%) | 432 calls × 8.7 ms — rebuilding the tactic table |
| the filter loop over its result | **13.15 s (78%)** | 432 × 421 iterations × 72 µs |

72 µs is far too slow for the two hash lookups the loop appears to contain, and
`--tactics-probe` shows why. Isolating the loop body, 5 passes over the 421
tactics each:

```
touch d.internalName                        0.000s
  + env.getModuleIdxFor?                    0.000s
  + env.header.moduleNames[modIdx]!         0.151s   <- all of it
  same, with moduleNames hoisted out        0.000s   <- the fix
```

`EnvironmentHeader.moduleNames` is a **`def`, not a field**
(`Lean/Environment.lean`: `header.modules.map (·.module)`), so every call
allocates a fresh 6,021-element array. doc-gen4's `collectTactics` calls it once
per tactic per module. The real complexity is therefore

> O(target modules × tactics in the environment × **imported modules**)
> = 432 × 421 × 6,021 ≈ 1.1 × 10⁹

not O(modules × tactics): the third factor is the one that hurts, and it is
invisible at the call site. Hoisting `moduleNames` out of the loop is what
removes the 13 s; enumerating the table once removes the remaining 3.8 s.
Together: **0.011 s** (`stage2.moduleDocs` 0.001 s + `stage2.tactics` 0.010 s,
warm medians over 5 runs) against doc-gen4's 16.64 s — a factor of ~1,500.

The whole pipeline with this increment in it, warm medians of runs 2-6 of
`stage2-full-eq-{1..6}` (conditions in `stage2-full-eq-summary.txt`), all **実測**:

| phase | | approach.md §6.1 "最適化後" |
|---|---:|---:|
| `importModules` | 2.512s | 2.60s |
| `indexLookup` | 0.002s | ~0s |
| `moduleDocs` + `tactics` | 0.011s | 0.04s (仮定) |
| `analyze` (equations on) | 9.261s | 11.22s |
| IR write | — (not implemented) | 0.68s |
| **`stage2.total`** | **11.815s** | **14.5s** |

Wall 11.82–11.97 s against user+sys 12.04–12.17 s, i.e. warm. Peak RSS
3.29–3.30 GB.

**Is 0.011 s real, or is work being deferred?** Lean builds structure fields
lazily, so `ModuleOut.docs` (an `Array.map`) and `TacticOut.docString` (a string
concatenation) are thunks that `stage2.moduleDocs` / `stage2.tactics` never
force. `--dump-modules` forces all of them and serialises them to JSON;
`stage2.dumpModules` is **0.009 s** for the 432 target modules and **0.015 s**
for the 573-module list with all 421 tactics assigned. That bounds the deferred
work: the honest worst case for the whole per-module collection is 0.02 s, still
half of §6.1's assumed 0.04 s. doc-gen4 defers exactly the same fields and pays
for them in its database write.

## Scoped notation

The measurement target defines `notation3` scoped notation
(`InformationTheory/Shannon/TypedRV.lean`: `H(μ; X)`, `H(μ; X | Y)`,
`I(μ; X ; Y)`, `D(μ; X ∥ Y)`; `InformationTheory/Asymptotic.lean`: `≐`).

An imported environment has **not** activated any `ScopedEnvExtension`, so by
default the delaborators those notations install never fire. doc-gen4 does not
activate them either, and consequently prints `DotEq a a`, not `a ≐ a`. This
program reproduces that byte for byte by default.

`--open <ns>` calls `Lean.activateScoped` on the environment (putting the
namespace in `Core.Context.openDecls` alone is *not* enough — that affects name
resolution, not extension activation). With it, the same declarations print with
the package's own notation. That is a behaviour difference from doc-gen4, which
is why it is a flag and not the default.
