# experiments/stage4 — IR persistence (writing the extraction result out)

Preparation for verification stage 4 of [`docs/approach.md`](../../docs/approach.md)
§7, and leg 4 of [`docs/plans/three-axes.md`](../../docs/plans/three-axes.md):
*what does it cost to persist the extraction result as a module-granular IR, and
what does it cost to read that IR back without starting Lean?*

§6.1 of `approach.md` carried the write as a **仮定** — doc-gen4's 0.68 s, assumed
to apply unchanged to lean-doc. This directory is what replaces it with a
measurement, and the read side is the precondition for stage 4 proper (HTML built
from the IR, no Lean process involved).

This directory is a copy of `../stage3` plus the IR writer. `../stage1`,
`../stage2` and `../stage3` are frozen so their numbers stay reproducible.
Everything not described below behaves exactly as in stage 3 — same blacklist,
same signature path, same reference collector, same `Core.Context` options, same
JSONL record shape. Read [`../stage3/README.md`](../stage3/README.md) and
[`../stage2/README.md`](../stage2/README.md) for those.

## What is new

| flag | default | what it does |
|---|---|---|
| `--write-ir` | off | persist the result: one JSON file per module, a package index, a dependency-side map slice |
| `--ir-dir <p>` | see below | where to write it. Does **not** imply `--write-ir` |

`--ir-dir` deliberately does not switch the writer on: "the IR is off unless
`--write-ir`" is the single rule that keeps the stage-3 baseline reproducible from
this tree (see "Baseline identity").

Output directory precedence: `--ir-dir`, then `$IR_DIR`, then the built-in
`defaultIrDir`. All three must stay **outside the measurement target** — nothing
is ever written to `/Users/haruka/dev/lean-projects`.

## The IR

```
<irDir>/index.json                     package index
<irDir>/modules/<Module.Name>.json     one file per module — the IR proper
<irDir>/deps/<Root>.json               dependency-side map slice, one per package
```

Granularity is the module, per `approach.md` §5.4: doc-gen4 splits a row per
declaration and then spends 66-78% of its HTML-generation CPU reassembling them
with N+1 queries. Reads and writes here are both whole-module.

### A module file

```json
{"schemaVersion":1,
 "module":"InformationTheory.Asymptotic",
 "imports":["Init","Mathlib.Analysis.Asymptotics.Defs", …],
 "moduleDocs":[{"line":9,"col":0,"text":"# Asymptotic / exponent framework…"}],
 "tactics":[{"internalName":…,"userName":…,"tags":[…],"docString":…}],
 "declarations":[
   {"name":"InformationTheory.Asymptotic.DotEq.refl","kind":"theorem",
    "binders":["(a : ℕ → ℝ)"],"implicits":[false],"type":"DotEq a a",
    "doc":"`DotEq` is reflexive: …","line":56,"col":0,
    "equations":[],"members":[],
    "refs":[["Init.Prelude","Nat"],
            ["Mathlib.Data.Real.Basic","Real"],
            ["InformationTheory.Asymptotic","InformationTheory.Asymptotic.DotEq"]]}]}
```

Three properties are conclusions of stage 3, not free choices:

* **Absolute identifiers only.** A reference is a `(defining module, name)` pair.
  **No URL — relative or absolute — appears anywhere in the IR.** Stage 3
  increment 3 established that the href is
  `"../"×(depth−1) + "./" + module.replace(".","/") + ".html#" + name`, i.e. it
  depends on the *page being rendered*, not on the target. Relativisation is an
  output-time concern (§5.6).
* **The dependency slice is two columns** (name → module) and holds only the
  constants this package actually refers to. Stage 3 increment 2 measured that
  slice at 53 KB against 34.3 MB for doc-gen4's whole `declaration-data.bmp`, and
  established that `kind` is not needed for linking.
* **Every module carries a content hash** (in the index), which §5.5 wants as the
  single source of truth for "what has to be re-extracted / re-rendered". Leg 7
  is what actually uses it; stage 4 only pays for it and reports the cost.

### What is *not* in the IR (and why it matters for leg 5)

* **Attributes, instance `className`/`typeNames`, `sorried`** — stage 2 left them
  out and stage 4 does not add them. They are part of the ~13 s estimate in §6.1,
  not of the 12.73 s measured.
* **Positional rendered code.** `refs` is a *set* of `(module, name)` pairs per
  declaration; the order and the position of each occurrence inside the printed
  signature are dropped. doc-gen4 keeps them (its `name_info.type` column is a
  `RenderedCode = TaggedText Tag` blob), and a renderer that wants to turn
  `Real.log` inside a signature into an `<a>` at exactly that offset needs them.
  **The set is not enough — measured, stage 4 increment 1.** 44.0% of doc-gen4's
  72,421 signature anchors have no textual relation between the printed token and
  the constant (`ℕ`→`Nat`, `≤`→`LE.le`); reconstructing links from plain text plus
  the set tops out at 56% recall (51.7% measured, 4.0% of declarations exactly
  right), and signatures are 71.1% of the rendered bytes. So the IR grows and the
  write/read numbers below are superseded by whatever the positional variant
  measures. **That redo is the plan, not a regression** — see
  `benchmarks/results/stage4-html-inventory.txt` and the verification log.
  The set *is* enough for resolving link targets (name → module): 0 misses across
  all 72,421 signature anchors.
* **`_private.` handling.** Stage 3 measured 8 private names whose naive URL is
  dead. The IR still stores them; the filtering decision belongs to the output
  side and is still open (`approach.md` §8).

### The hash

`hashHex` is 16 hex digits of Lean's `String.hash` over the module file's exact
bytes — a **64-bit, non-cryptographic** digest. Lean core v4.31.0 ships no
SHA-256, and transcribing one would put an unrelated implementation inside a
number this experiment is trying to measure. For 432 modules the collision
probability is ~5e-15, which is fine for change detection and not fine as a
tamper-evident content address. `lean_string_hash` is also only stable within a
Lean version — harmless, because §5.4 already puts the Lean version in the cache
key (and `index.json` records it).

The hash covers the file exactly as written, so a consumer can verify it by
hashing the file; `schemaVersion` is inside the body, so a schema bump changes
every hash, which is the desired invalidation.

## Build and run

```sh
./build.sh
./run.sh stage4-ir -- --equations --refs --write-ir --ir-dir "$IR_DIR"
```

Same constraints as stages 2 and 3 (no toolchain and no lakefile here, `lake env`
borrows the measurement target's, `--root` because the source is outside that
repository, `leanc -rdynamic` for the interpreted module initializers). Never
measure with `lean --run`.

Reading it back, without Lean:

```sh
deno run --allow-read --allow-env ../../benchmarks/tools/read-ir.ts \
  --ir "$IR_DIR" --runs 7 --standalone 6
```

For smoke runs, set `MODULES` to a short list and `RESULTS_DIR` to a scratch
directory so nothing lands next to the committed measurements.

## Numbers

Conditions: Apple M1 / 8 cores / 16 GB, Lean, Mathlib and doc-gen4 all v4.31.0,
`LEAN_NUM_THREADS` unset, oleans built, one process, **warm** (wall ≈ user+sys on
every run). 432 target modules, 4,750 declarations, `--equations --refs`. Six runs
per series in one session; run 1 is discarded as page-cache cold and the **median
of runs 2-6** is reported, with the min-max of those five. Peak RSS 3.06 GB
(unchanged from stage 3). All 実測.

Four series, because the first two disagreed (see "Pitfalls"):

| series | what it is | logs |
|---|---|---|
| `stage4-noir-{1..6}` | IR off, six runs, then | `../../benchmarks/results/` |
| `stage4-ir-{1..6}` | IR on, six runs (blocked ordering) | ″ |
| `stage4-alt-{noir,ir}-{1..6}` | the same two, **alternating** | ″ |
| `stage4-irkeep-{1..6}` | IR on, overwriting the tree instead of recreating it | ″ |

Conditions per run are in the matching `*-summary.txt`.

### What the IR write costs

`stage4.writeIR`, pooled over the ten write-enabled runs of the first three
series:

| | seconds | range |
|---|---:|---|
| build the `Json` and compress it to a `String` | 0.147 | 0.146-0.157 |
| content hash (432 × `String.hash`) | **0.0023** | 0.0023-0.0024 |
| `IO.FS.writeFile` × 436 | 0.046 | 0.041-0.050 |
| **`stage4.writeIR`** | **0.198** | **0.193-0.211** |

`stage4-irkeep` (overwriting an existing tree rather than a fresh one) gives
0.196 s [0.186-0.199], i.e. creating and overwriting cost the same.

Serialisation dominates: three quarters of the write is Lean's `Json.compress`
producing 8.6 MB of text, and only a quarter is the 436 `write(2)`s. **The hash is
1.2% of the write phase and 0.02% of the run** — §5.5 can lean on a per-module
hash without it showing up anywhere.

### What gets written

| | |
|---|---:|
| files | 436 (432 modules + 3 dependency maps + 1 index) |
| module bytes | 8,627,735 |
| dependency map bytes | 27,208 |
| index bytes | 88,452 |
| **total** | **8,743,395 (8.34 MiB)** |
| declarations | 4,750 |
| deduplicated (declaration, reference) pairs | 56,552 |
| dependency map entries | 533 |
| references whose module could not be resolved | 0 |

The 533 is exactly stage 3 increment 1's dependency-side count, and the 3 files
are `Init`, `Lean` and `Mathlib`. The IR is **not committed** (8.34 MiB,
regenerable); it is written to a scratch directory outside both repositories.

### Reading it back, without Lean

`../../benchmarks/tools/read-ir.ts`, seven runs, run 1 dropped. Full output:
`../../benchmarks/results/stage4-readir.txt` (machine-readable: `.json`).

| | seconds |
|---|---:|
| in-process, median of runs 2-7 | **0.0563** (0.0548-0.0643) |
| one fresh process per read, median of runs 2-6 | **0.0588** (0.0588-0.0590) |

That is **2.3% of the 2.5 s warm `importModules` floor**, at 148 MiB/s. The
cross-check with a new process each time exists because in-process repetition can
be flattered by a warm V8 heap; it is not, the two agree within 4%. This is a
Deno/V8 number and the implementation language is undecided (§5.6), so it bounds
one runtime, not the design.

The read is also a consistency check on the IR, and every count lands on a number
some earlier stage measured independently:

| read back | matches |
|---|---|
| 4,750 declarations | stage 2 (= doc-gen4's DB) |
| 1,446 distinct `(module, name)` references | stage 3 increment 1's unique set |
| 533 dependency map entries | stage 3 increment 2's dependency side |
| 863 equations | stage 2 (= doc-gen4's DB) |
| 1,515 module docstrings | stage 2 (= doc-gen4's DB) |
| 194 structure members | stage 2's 156 fields + 1 parent + 37 ctors |
| 3,935 direct imports | stage 2's 3,503 **plus 432** — see "Pitfalls" |

### Against doc-gen4

doc-gen4's counterpart is `updateModuleDb` (`DocGen4/DB.lean:494`), 0.68 s for the
same 432 modules (実測, `../../benchmarks/results/batch-rerun.jsonl`). **Do not
turn that into a ratio**: it is a different amount of work in a different format.

| | doc-gen4 `updateModuleDb` | stage-4 IR |
|---|---|---|
| target | one SQLite file, already holding 6,072 modules, `deleteModule` + insert | 436 fresh JSON files |
| signature | `RenderedCode = TaggedText Tag` binary blob — **positional**, every tag kept | plain text + a *set* of `(module, name)` pairs |
| also writes | attributes, instance `className`/`typeNames`, `sorried`, recursor internal names | content hashes, the dependency map slice |
| size, same 432 modules | 22.18 MB (`bench.db`) / 22.68 MB (`bench-a.db`) — includes indices and free pages | 8.74 MB |

The overlap that *is* comparable is "persist the analysis result of 432 modules",
and on that both are a fraction of a second against a ~12 s run. Neither the
0.68 s nor the 0.198 s is where this pipeline spends its time.

## Baseline identity

Same idea as stage 3's: with the new flag off, this binary must still be stage 3.

* **Byte identity.** On a 10-module smoke list, stage 4 with `--write-ir` off
  produces `--dump`, `--dump-modules` and `--dump-refs` output byte-identical to
  stage 3's, both with `--refs` on and with it off.
* **`--write-ir` is purely additive.** With it on, `--dump` is still byte-identical
  to the run with it off.
* **The IR is deterministic.** Two runs into two different directories compare
  equal under `diff -r`.
* **Same session, same time.** Stage 3 was re-measured six times in this session
  (`stage4-s3ref-{1..6}`): `analyze` 9.808 s, `total` 12.506 s, against stage 4's
  IR-off 9.804 s / 12.466 s. `produced` (4,750) and `refOccurrences` (143,121) are
  identical on both sides, so the two are doing the same work, not just taking the
  same time.

Re-run all of this whenever `Extract.lean` changes: it is what keeps stage 4's
timings comparable with stages 2 and 3.

## Pitfalls

**The hash timer read 0 µs at first, and that was wrong.** `let h := hashHex text`
has no consumer before the next clock read — its result is used further down, when
the index entry is built — so the compiler sank the call past the timer and
`hashUs` came out at 34 µs for 8.6 MB, i.e. 253 GB/s. The fix is a branch the
compiler cannot drop (`if h.length != 16 then throw …`). Same class of mistake as
stage 2's `--tactics-probe` and stage 3's `refUs`; **all three read low, in the
flattering direction.** Note the *total* `writeIR` was right both times (0.192 s
before, 0.194 s after) — only the split between "serialize" and "write" was wrong,
because whatever the timer skipped, `writeFile` paid for later.

Sanity check to keep: serialize + hash + write must be ~8.6 MB divided by a
plausible per-byte rate. 253 GB/s is not one.

**Do not read the write cost off `stage4.total`.** The three orderings disagree:

| series | `importModules` | `total` |
|---|---:|---:|
| IR off (blocked) | 2.597 [2.551-2.714] | 12.466 [12.411-12.700] |
| IR off (alternating) | 2.628 [2.548-2.696] | 12.685 [12.472-12.769] |
| IR on, tree deleted first (blocked) | 3.169 [2.610-4.429] | 13.447 [12.790-14.677] |
| IR on, tree deleted first (alternating) | 3.230 [2.563-5.852] | 13.251 [12.536-15.963] |
| IR on, tree overwritten | 2.816 [2.659-3.712] | 12.888 [12.638-14.017] |

`total` moves by up to 1.0 s while `writeIR` stays at 0.196-0.198 s in every one of them.
The excess is in **`importModules`, a phase that finishes before the writer
starts** and therefore cannot be caused by it: what moves it is the file churn
around the run — deleting 436 files immediately beforehand costs the most, and the
series that only overwrites is halfway back to the baseline. On a workload that
pages in 3 GB of oleans through `mmap`, a few thousand filesystem operations
nearby are visible. The phase timer is the number; the difference of totals is
not.

The alternating series exists for the same reason: run the two configurations in
blocks and whichever block goes second inherits whatever the machine was doing,
which is how the blocked ordering produced a +0.23 s difference in `analyze` — a
phase `--write-ir` cannot touch at all.

**432 duplicate imports.** `ModuleOut.imports` is the raw
`header.moduleData[i].imports`, which lists `Init` twice for every module, so the
IR holds 3,935 import entries where doc-gen4's `module_imports` has 3,503.
Stage 2's comparison against doc-gen4 was set-based and is unaffected. Dropping
the duplicate is an output-side concern; it is noted here so the 3,935 is not read
as a discrepancy.
