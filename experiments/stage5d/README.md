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
| `compare-detect.sh` | C6: before/after/before on the unchanged-key path |
