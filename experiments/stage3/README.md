# experiments/stage3 — referenced constants (the demand side of the link map)

Verification stage 3 of [`docs/approach.md`](../../docs/approach.md) §7, planned
in [`docs/plans/stage3.md`](../../docs/plans/stage3.md): *can links into a
dependency (Mathlib) be resolved from a "declaration name → module → URL" map
alone, without building an IR for the dependency — and how big does that map have
to be?*

This directory is a copy of `../stage2` plus **increment 1** of that plan: the
constants a declaration's rendered signature refers to. Those names are the
**demand** side, i.e. the lower bound on the map. `../stage1` and `../stage2` are
frozen and untouched, so their numbers stay reproducible.

Everything not described below behaves exactly as in stage 2 — same blacklist,
same signature path, same per-module collection, same `Core.Context` options,
same JSONL record shape. Read [`../stage2/README.md`](../stage2/README.md) for
those; this file only covers what stage 3 adds.

## What is new

| flag | default | what it does |
|---|---|---|
| `--refs` | off | collect, per declaration, every constant doc-gen4 would turn into a link |
| `--dump-refs <p>` | — | write the **unique** set of those constants with its defining module (needs `--refs`) |

Both are off by default on purpose: with them off this binary reproduces the
stage-2 output byte for byte (checked, see "Baseline identity"), so the stage-2
baseline can be re-measured from the stage-3 tree.

`--tag` from stage 2 is still there and is **independent** of `--refs`. It stays
as the measured cost of `Widget.tagCodeInfos`, which is the comparison point for
what the new collector costs.

## What "referenced constant" means exactly

doc-gen4 gets from a `Format` to linkable HTML in two steps:

1. `Widget.tagCodeInfos` (Lean core, `Lean/Widget/InteractiveCode.lean`) wraps
   each tag position in a `SubexprInfo`. It **does not name any constant**; it
   only allocates a `WithRpcRef` per tag for the RPC layer.
2. doc-gen4's own `renderTagged` (`DocGen4/RenderedCode.lean`) decides what
   becomes an `<a href>`. This is the step that names constants.

`renderTagged` links a name iff, for a tag position `n`, `infos.get? n` is
`Elab.Info.ofTermInfo ti` **and** `ti.expr.consumeMData` is `Expr.const c _`.
`collectConsts` in `Extract.lean` is that test and nothing else, applied to every
tag of `Widget.TaggedText.prettyTagged fmt`. Step 1 is skipped: it cannot change
which names come out, and skipping it avoids an `IO.Ref` allocation per tag.

Walking every tag is equivalent to `renderTagged`'s recursion — it descends into
the subtree of a `.const` tag unless that subtree is a bare `.text`, and a bare
`.text` subtree has no tags in it.

### Deliberately not collected

These are doc-gen4's omissions, reproduced on purpose. The stage-3 criterion is
"does lean-doc reach *the same* link targets as doc-gen4", not "what could be
linked", so the collector is written to be faithful, not complete:

- **`Elab.Info.ofFieldInfo`** — carries a constant (the projection), but
  `renderTagged` only matches `.ofTermInfo`, so doc-gen4 does not link it. Most
  likely a doc-gen4 oversight; if stage 3 ends up wanting the fixed behaviour it
  has to be an explicit, separately measured change.
- **`Elab.Info.ofDelabTermInfo`** and every other `Elab.Info` constructor — same
  reason.
- **`Expr.sort`** — `renderTagged` gives it its own tag (`Type` / `Prop` links to
  `foundational_types.html`, not to a declaration), so it yields no name.
- **`Expr.fvar`, `Expr.mvar`, …** — fall through to `.otherExpr`.

### Which entry points are covered

| doc-gen4 | here | in scope for increment 1 |
|---|---|---|
| `Info.ofTypedName` (signature) | `ppSignature`, per binder and for the result type | yes |
| `prettyPrintTerm` for equations (`DefinitionInfo`) | `ppEquation` → `ppTerm` | yes (needs `--equations`) |
| `prettyPrintTerm` for structure parent types (`StructureInfo`) | `structureMembers` → `ppTerm` | yes |
| structure fields / ctors, inductive ctors | go through `ppSignature` | yes |
| docstring links (`Output/DocString.lean`) | — | **no** — a different path that never touches `tagCodeInfos`; increment 3 measures it as a delta |

`ppTerm` keeps stage 2's `Meta.ppExpr` when `--refs` is off and switches to
doc-gen4's `PrettyPrinter.ppExprWithInfos` when it is on, because that is the only
way to get the `infos`. `Meta.ppExpr` *is* `ppExprWithInfos` with the map thrown
away, so the printed text is unchanged either way — verified, see below.

## Output

`--dump` gains one field per declaration:

```json
{"name":"...","refs":["LE.le","Real","Fintype.card"], ...}
```

Occurrences, in order of appearance, **not deduplicated** — the unique count and
the occurrence count are both wanted, so deduplication happens at dump time.

`--dump-refs` writes the unique set, one JSON object per line, in order of first
appearance:

```json
{"module":"Mathlib.MeasureTheory.Integral.Lebesgue.Basic","name":"MeasureTheory.lintegral","occurrences":37,"own":false}
```

- `module` — `env.getModuleIdxFor?` + `env.header.moduleNames`, `null` if the
  constant has no defining module. `moduleNames` is a `def`, not a field: it
  allocates a fresh 6,021-element array per call (this cost stage 2 13 s), so it
  is hoisted out of the loop.
- `own` — the defining module is in the target module list (`it-modules.txt`),
  i.e. this constant belongs to the package being documented rather than to a
  dependency. This is the own/dependency split increment 2 needs.

New timing phases and fields, on top of stage 2's:

| phase | meaning | extra fields |
|---|---|---|
| `stage3.analyze` | as `stage2.analyze`, plus | `refUs`, `refOccurrences`, `collectRefs` |
| `stage3.dumpRefs` | writing the unique set (`--dump-refs`) | `records`, `occurrences`, `own`, `unresolved` |

**`refUs` is contained in `ppUs` + `eqUs`, it is not an extra term.** The
collector runs inside the pretty printing it measures, the same way `--tag` does
in stage 2. To get the *added* cost, compare whole `stage3.analyze` values with
and without `--refs`; do not subtract `refUs` from anything.

`refUs` only means something because the walk is forced inside the timed region.
Written the obvious way — a pure `let` above the timer, consumed by the closure
that stores the result — the compiler sinks the walk into that closure and the
timer reads ~0: on the 10-module smoke list the same work measured 11 µs that way
and 4.3 ms once forced. If this code is restructured, re-check that `refUs` still
moves with the number of occurrences.

All other phase names are stage 2's with the `stage3.` prefix, and
`benchmarks/tools/analyze.ts` knows them.

## Build and run

```sh
./build.sh
./run.sh stage3-refs -- --equations --refs --dump-refs /tmp/refs.jsonl
```

Same constraints as stage 2 (no toolchain and no lakefile here, `lake env`
borrows the measurement target's, `--root` because the source is outside that
repository, `leanc -rdynamic` for the interpreted module initializers). Never
measure with `lean --run`.

For smoke runs, set `MODULES` to a short list and `RESULTS_DIR` to a scratch
directory so nothing lands next to the committed measurements:

```sh
MODULES=/tmp/smoke-modules.txt RESULTS_DIR=/tmp ./run.sh smoke -- --refs --dump-refs /tmp/refs.jsonl
```

## Numbers

Conditions: Apple M1 / 8 cores / 16 GB, Lean, Mathlib and doc-gen4 all v4.31.0,
`LEAN_NUM_THREADS` unset, oleans built, one process, **warm** (wall ≈ user+sys on
every run). 432 target modules, 4,750 declarations. Five configurations measured
back to back in one session, six runs each; run 1 is discarded as page-cache cold
and the **median of runs 2-6** is reported. Logs:
`../../benchmarks/results/stage3-{noeq,noeq-tag,noeq-refs,eq,eq-refs}-{1..6}.jsonl`
with conditions in the matching `*-summary.txt`. All 実測.

### Referenced constants

| | |
|---|---:|
| occurrences (equations on) | 143,121 |
| occurrences (equations off) | 130,898 |
| mean per declaration (on) | 30.1 |
| **unique** | **1,446** |
| of those, defined in a target module | 913 |
| **of those, in a dependency** | **533** |
| without a defining module | 0 |

The unique set is `../../benchmarks/results/stage3-refs.jsonl`. For scale, the map
doc-gen4 ships (`declaration-data.bmp`) has 258,760 entries in 37,481,374 bytes
(実測), so the dependency-side demand of this package is 0.21% of it. Whether
that supply actually resolves this demand is increment 2, not measured here.

### What `--refs` costs

| configuration | `stage3.analyze` | vs. baseline |
|---|---:|---:|
| equations off, baseline | 8.058s | — |
| equations off, `--tag` | 8.510s | +0.452s |
| equations off, `--refs` | 8.612s | **+0.554s** |
| equations on, baseline | 9.175s | — |
| equations on, `--refs` | 9.675s | **+0.500s** |

`stage3.total` goes 11.775s → 12.259s (+0.484s); `refUs` (contained in the two
above) is 0.468s. Stage 2 measured `--tag` at +0.405s in a different session; the
same-session re-measurement here is +0.452s, i.e. consistent. `--refs` is 0.10s
above `--tag`: it skips `tagCodeInfos` but walks every tag and accumulates names.

### Against doc-gen4's HTML

`../../benchmarks/tools/compare-links.ts` diffs the collected set against the
links doc-gen4 actually emitted, restricted to signature blocks. Output:
`../../benchmarks/results/stage3-linkcmp.txt` (increment 1) and
`../../benchmarks/results/stage3-urlcmp.txt` (increment 3).

| | increment 1 | increment 3 |
|---|---:|---:|
| HTML link targets, unique | 1,154 | **1,161** |
| collected constants, unique | 1,446 | 1,446 |
| **in the HTML, missing here** | **0** | **0** |
| collected but not linked in the HTML | 292 | 285 |

The two columns differ because increment 3 **fixed a defect in the tool, not in
the extractor**: increment 1 matched structure fields as
`<li class="structure_field...">`, but `DocGen4/Output/Structure.lean` writes
`<li id={name} class="structure_field">` for a direct field, so that regex saw 4
of the 157 field blocks in this corpus. Increment 3 matches
`div.structure_field_info`, which is order-independent and also excludes the
sibling `div.structure_field_doc` (a docstring, out of scope). This *adds* 7
target names and 615 `<a>` occurrences and leaves "missing here" at 0, i.e. it
widens what lean-doc is checked against and lean-doc still contains all of it.
Increment 1's numbers stay reproducible with `--legacy-blocks`
(`../../benchmarks/results/stage3-urlcmp-legacy.txt`).

Containment in this direction is what to expect: doc-gen4 only emits an `<a>` for
names it can resolve. The 285 are increment 3's material, not a result.

**This covers 348 of the 432 target modules.** The HTML tree in
`.lake/build/doc/InformationTheory` has 348 pages (3,477 declarations); 84 target
modules have no page, because that doc build was cut short. The zero above is a
zero over those 348 modules, not over the whole target.

## Increment 3 — URL generation

The per-declaration dump is 7.3 MB, so it is **not committed**. Regenerate it
with:

```sh
MODULES=$PWD/benchmarks/results/it-modules.txt RESULTS_DIR=<dir> \
  ./experiments/stage3/run.sh stage3-decls -- \
  --equations --refs --dump <dir>/stage3-decls-dump.jsonl
```

then compare (from the repository root):

```sh
deno run --allow-read --allow-write --allow-env \
  benchmarks/tools/compare-links.ts benchmarks/results/stage3-refs.jsonl \
  --decls <dir>/stage3-decls-dump.jsonl \
  --mismatches benchmarks/results/stage3-urlcmp-mismatches.jsonl \
  > benchmarks/results/stage3-urlcmp.txt
```

The rule implemented, read off `DocGen4/Output/Base.lean` (`getRoot`,
`moduleNameToLink`, `declNameToLink`):

```
href = "../" * (components(page module) - 1) + "./"
     + module(target).replace(".", "/") + ".html#" + target
```

All 実測, **348 of 432 modules**, population = the 3,477 declarations that are
both in the dump and rendered as a `div.decl`:

| | |
|---|---:|
| (declaration, reference) pairs, HTML | 39,298 |
| (declaration, reference) pairs, lean-doc | 40,936 |
| common | 39,298 (100% of HTML) |
| **exact href match, A. env** | **39,298 / 39,298 = 100.000%** |
| **exact href match, B. map (`.bmp`)** | **39,295 / 39,298 = 99.992%** |
| in the HTML, not in lean-doc | **0** |
| in lean-doc, not linked by doc-gen4 | 1,638 |

A and B are not equally strong evidence. doc-gen4's `name2ModIdx` **is**
`env.const2ModIdx` (`DocGen4/Process/Analyze.lean:243`), the same map the
extractor read with `getModuleIdxFor?`, so A tests the path rule and nothing
else. **B is the independent one**: it never touches the dependency's
environment.

B's 3 misses are `Eq.rec` (×1) and `List.recOn` (×2) — recursors, which doc-gen4
blacklists and therefore never puts in the `.bmp`. `Init/Prelude.html` exists and
has no `id="Eq.rec"`, so **those three links doc-gen4 emitted are dead** (実測).
Strategy B emits nothing there instead of a dead link.

## Baseline identity

Checked on a 10-module smoke list (single run, 参考値 — the point of the check is
the byte comparison, which does not depend on timing):

- `--refs` **off**: the `--dump` output is byte-identical to stage 2's for the
  same module list, except for the added `"refs":[]` field; `--dump-modules` is
  byte-identical with no exception. So the rename to `stage3.*` is the only
  behavioural difference in the baseline configuration.
- `--refs` **on**: every field except `refs` is still identical to stage 2's,
  i.e. switching `ppTerm` to `ppExprWithInfos` does not change the printed text.

Re-run this diff whenever `Extract.lean` changes: it is what keeps stage 3's
timings comparable with stage 2's.
