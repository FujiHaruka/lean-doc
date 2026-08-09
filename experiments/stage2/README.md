# experiments/stage2 — semantic extraction

Verification stage 2 of [`docs/approach.md`](../../docs/approach.md) §7, first
increment: *now that declarations can be reached through the module index
(stage 1), how long does it take to produce the semantic information a
documentation page actually needs — and does custom syntax survive it?*

Stage 1 (`../stage1`) is left untouched; it remains the "environment load +
enumeration" floor. This directory adds the semantic analysis that stage 1
deliberately left out.

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
- **module docstrings and tactic collection** — doc-gen4's
  `getAllModuleDocs` (16.6 s on this target, of which nearly all is tactic
  collection) is the separate "1 モジュールのために全体を舐める" problem of
  approach.md §5.2 and is not part of this increment.
- **IR persistence.** `--dump` writes JSON Lines so results can be eyeballed and
  diffed; it is not the IR format.

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
extract <modules.txt> <out.jsonl> [--equations] [--dump <p>] [--only <p>] [--open <ns,..>] [--tag]
```

- `--only <p>` restricts processing to the declaration names listed in `<p>`;
  used with `--dump` to inspect individual signatures.
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
| `stage2.analyze` | the semantic analysis | `considered`, `produced`, `blacklisted`, `failed`, `ppUs`, `eqUs`, `docUs`, `equations`, `eqFailures`, `genEquations`, `tagCode` |
| `stage2.dump` | JSON serialization (only with `--dump`) | `records` |
| `stage2.total` | whole program | `modules` |

`stage2.analyze` is the number to compare with doc-gen4's
`process.constantLoop.ofConstantUs`. `stage2.importModules` is comparable with
`load.importModules`. **`stage2.total` is still not comparable with
`batch.total`**: doc-gen4's total also contains `getAllModuleDocs` and the
database write, neither of which this program does.

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

The numbers themselves live in `benchmarks/results/stage2-*.jsonl` with
conditions in the matching `*-summary.txt`; `stage2-dg4-*.jsonl` is the
same-session doc-gen4 baseline (`stage2-dg4-summary.txt`), and
`stage2-notation-samples.txt` is the notation evidence below.

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
