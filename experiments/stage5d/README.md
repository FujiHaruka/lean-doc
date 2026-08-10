# Stage 5d — splitting L1 into an extract key and a render key

**Question.** The global key L1 was one record: any change to it re-extracted all
432 modules. Is that the right blast radius for every input in it — and what does
getting it wrong cost?

**Why now.** `--source-url` carries a 40-hex git revision, so it changes on
**every commit**, which is exactly when an incremental build runs. And it was not
in the key at all: `envKey` held five items and none of them was the source URL
(this is judgement point 3's S1, still open after stage 5c closed the *other*
three holes). So the choice was between two wrong answers:

* leave it out → `check` reports 0 changed while all 432 pages are stale (S1);
* put it in the single key → every commit re-extracts 432 modules with Lean
  started, to serve an input Lean cannot see.

The claim under test is that there is a third answer: **two keys with different
blast radii**, split by "can this input change the IR" rather than "does this
input change the output" (both do).

| key | contents | changed ⇒ |
|---|---|---|
| `extractKey` | toolchain, manifest, extractor id, IR schema, IR generator | re-extract all 432; the stale page set still follows from the IR diff |
| `renderKey` | renderer id, `--source-url` | re-extract **nothing**, re-render **all** |

## Criteria — declared before measuring

These are committed before the first run (git is the witness). Each has a
prediction; a prediction that is wrong is recorded as wrong and the plan is
corrected, not the result.

| # | proposition | prediction |
|---|---|---|
| **C1** | Changing only the `--source-url` revision makes `check` report `reExtract 0` and a non-empty `--render-all-out`. | true |
| **C2** | That run never starts Lean: `extractSeconds` is 0 and no extractor event is emitted. | true |
| **C3** | That run rewrites **432/432** pages, and every page's bytes really change (the old revision string survives nowhere). | true |
| **C4** | The IR is byte-identical across that run — the source URL is a renderer input and is not in the IR. | true |
| **C5** | Faking the toolchain in `extractKey` still re-extracts all 432, and leaves `--render-all-out` **empty** (an extract-key change does not by itself force a render; the IR diff decides). | true |
| **C6** | The split costs nothing measurable in `detect`: the before/after gap is within the before/before drift. | true |
| **C7** | The single-key behaviour (what a `--source-url` change would have cost with the key unsplit) is materially more expensive than the split behaviour, and the difference is the whole extraction. | true, ≈ 25–30 s saved |

**C7 is the number the split exists for.** It is measured, not argued: the
single-key counterfactual is produced by faking the *extract* key and forcing
`--mode all`, which yields exactly "re-extract 432 + render 432" — the work one
key would have ordered for a new git revision. The split variant is the real
`--source-url` change. Both run in the same session at the same warm state.

**Retreat line.** If C4 is false — if the source URL does reach the IR — the
split is unsound as specified and the render key has to be re-derived, because
then a revision change *does* need Lean.

## What is faked, and what is not

The measurement target must not be modified, so:

* the `--source-url` change is real (it is a CLI argument, nothing on disk);
* the toolchain change for C5/C7 is faked **in the ledger**, which is ours and
  lives in scratch — the target's `lean-toolchain` is not touched;
* everything downstream is real work on real inputs: the extractor really runs,
  the renderer really rewrites the pages.

## Files

| | |
|---|---|
| `run.sh` | C1–C5 + C7: the correctness checks and the split-vs-single-key cost |
| `compare-detect.sh` | C6: before vs after on the unchanged-key path, interleaved |

---

## Results (2026-08-10)

**All numbers are 実測.** Sources: `benchmarks/results/stage5d-keysplit.txt`
(raw `stage5d-keysplit.jsonl`) and `stage5d-detect-cost.txt`
(raw `stage5d-detect-cost.jsonl`).

| # | prediction | result | evidence |
|---|---|---|---|
| C1 | true | **true** | `reExtract 0`, `--render-all-out` = one line, `renderKey:sourceUrl` |
| C2 | true | **true** | `extractSeconds` 0.033 (the branch not taken), no `extract-events.jsonl` written |
| C3 | true | **true** | 432 pages rendered; pages still carrying the old revision **0**, carrying the new one **432** |
| C4 | true | **true** | IR before vs after: `diff -r` reports nothing |
| C5 | true | **true** | faked `leanToolchain` → `reExtract 432`, `--render-all-out` empty |
| C6 | true | **true** | detect **+0.0003 s** against a within-variant spread of 0.0068 s |
| C7 | true, ≈25–30 s | **true, but the prediction was wrong** | 18.392 s → 1.341 s wall, i.e. **17.05 s saved, 13.7×** |

### C7 in full

| | split (`--source-url` changed) | onekey (the counterfactual) |
|---|---:|---:|
| wall clock | **1.3414 [1.3359–1.5036]** | **18.3923 [17.2356–21.2470]** |
| user+sys | 1.5200 | 17.3300 |
| major page faults | 190 | 19,565 [5,586–53,544] |
| peak RSS | 218.5 MB | 3,297 MB |
| detect | 0.1441 | 0.1035 |
| extract | 0.0281 (not run) | 16.7658 [15.5857–19.5784] |
| merge | 0.0206 | 0.3635 |
| render | 0.8652 | 0.9245 |
| modules re-extracted | **0** | 432 |
| pages rendered | 432 | 432 |

5 runs each, interleaved, run 1 dropped.

**Where the prediction went wrong.** 25–30 s came from the standing figure for a
full extraction (27 s, `extract-once.sh`). In this series the same 432-module
extraction took **16.77 s** median. The two are not the same measurement: the
27 s figure is a cold-ish standalone run writing a fresh IR tree, this one is
interleaved with renders that keep the page cache churning. The onekey series is
still not converged — its major faults range over an order of magnitude
(5,586–53,544) and its wall clock over 4 s, which is the memory-bound signature
CLAUDE.md warns about. **The saving is reported as 17.05 s from the medians of
this one session; it is not a replacement for the 27 s figure and the two must
not be quoted side by side.**

**The detect column is not a comparison.** split's 0.1441 vs onekey's 0.1035 is
an artifact: the onekey variant rebuilds its ledger immediately before each run,
which warms the 432 `.olean.hash` files. The real cost of the split is C6's
+0.3 ms, measured by interleaving the two *versions* against one workload.

### What this changes beyond the saving

**Every commit re-renders all 432 pages, unavoidably.** The revision string is in
the page bytes (4,992 occurrences across the tree), so a new commit invalidates
every page whatever else it touches. Two consequences:

1. `--mode self|referrers|importers` — the whole "impact set" framing of stage 5
   — is **moot for the revision component of a real commit**: the render set is
   `all` regardless. The mode still decides the *extra* pages a module change
   makes stale, but it is no longer what decides the render bill.
2. The render bill it decides is small anyway: **0.87 s for all 432 pages**. That
   is why the split is worth 13.7× — the expensive half was never the rendering.

   **Corrected 2026-08-10.** This line used to add "most of which is the fixed IR
   read", which the raw log contradicts. Per `stage5d-keysplit.jsonl`
   (`stage5d-split`, run 2): the fixed part (`preMain` + `readIr` + `indexBuild`)
   is **0.144 s = 18%** of the 0.7996 s in-process total; the page-proportional
   part (`renderHeaders` + `renderPage` + `flatten` + `write`) is **0.653 s =
   82%**. The fixed cost dominates only when **one** page is re-rendered (§(g)'s
   `leaf-self`: 0.17 s of 0.1952 s), and that description slid across to the
   432-page case. It matters for the open question below: what a
   no-revision-in-the-bytes design removes is the **82%**, not the 18%.

**An open design question this surfaces** (not measured, out of scope here):
if the source URL were not baked into the page bytes — relative links plus the
revision injected once at serve time — a new commit would invalidate *no page at
all*, and the 0.87 s would go too. That is a change to §5.6's output contract,
not to the ledger, and it is recorded in `approach.md` §8.
