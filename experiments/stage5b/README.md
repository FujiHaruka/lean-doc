# stage 5 — incremental generation, increment 2 (judgement point 3)

`docs/approach.md` §7 stage 5, second half: **prove that no stale page survives.**
Increment 1 (`experiments/stage5/`) measured the speed and left criterion 3
undecided. This increment decides it.

Throwaway experiment code (CLAUDE.md). Nothing in
`/Users/haruka/dev/lean-projects` is modified: no `.lean` edit, no `lake build`,
no `lake update`, no `fromDb`.

**Numbers**: `benchmarks/results/stage5b-stale-summary.txt`.

---

## 1. The claim under test, decomposed

Increment 1's criterion 3 was stated as *"the impact set is correctly closed
(nothing stale, nothing gratuitous)"*. That is one sentence carrying five
independent claims. Each is stated below as a falsifiable proposition together
with **the observation that refutes it**, and with **the prediction made before
running anything** — so that a result agreeing with the prediction is still a
measurement and a result disagreeing with it is still recorded.

| # | proposition | refuted by | prediction (declared 2026-08-10, before measuring) |
|---|---|---|---|
| **S1** | Every input that changes a page's bytes is observed by the ledger | one input that changes ≥1 page while `ledger check` reports 0 changed modules | **refuted** — `--source-url` is a CLI argument and is not in `envKey` |
| **S2** | The computed impact set contains every page whose bytes actually change | one injected change where (pages differing between a full render before and after) ⊄ (pages the mechanism re-renders) | **refuted** — docstring autolinks resolve through a global name→module map, which is not bounded by imports |
| **S3** | Re-extracting one module gives the same IR as full extraction, under every supported extractor configuration | one configuration where the one-module IR and the full IR differ for the same module | **refuted** under `--open`: a single-module run does not import the module that declares the scoped notation |
| **S4** | A module that disappears leaves nothing behind | a surviving page / ledger entry / IR file, or a pipeline error | **refuted** — no delete path exists in `render.ts` or `merge-ir.ts`; `ledger check` is expected to throw |
| **S5** | The artifacts the pipeline maintains are the whole site | one artifact that the site needs, that depends on the changed module, and that the pipeline never writes | **refuted** — `render.ts` writes module pages only |

**Judgement point 3 is met iff S1–S5 all hold.** If any is refuted, criterion 3
is **not met**, the refutation goes into `docs/verification-log.md`, and
`docs/approach.md` §5.5 ("the module-granular hash is the single source of
truth") gets a revision proposal. Per `docs/plans/three-axes.md` §3 that is
outcome (B), and (B) is a completion, not a failure.

**The predictions above are not results.** They exist so that the experiments
cannot be quietly re-aimed at whatever they happen to find. Every row is filled
in with a measured verdict in §3, and any row where the measurement disagrees
with the prediction is called out explicitly.

## 2. What each experiment does

| exp | proposition | Lean? | what it does |
|---|---|---|---|
| **E1** | S1 | no | render 432 pages twice, changing only `--source-url`; count changed pages and what `ledger check` says |
| **E2** | S2 | no | inject one declaration into one module's IR, render before/after, compare the true changed-page set against `impact.ts --mode self\|referrers\|importers` |
| **E3** | S3 | yes | full extraction with `--open`, and single-module extraction with `--open`, byte-compared against each other and against the `--open`-off IR |
| **E4** | S4 | no | run the pipeline against a module tree with one olean removed (a copy; the target is untouched) |
| **E5** | S5 | no | enumerate the artifacts doc-gen4 produces that the pipeline does not, and classify each by what it is a function of |

E1, E2, E4, E5 do not start Lean and cost seconds. E3 costs one full extraction
(~16 s) plus a few single-module runs (~3 s each).

## 3. Results

(filled in by the measurements; see `benchmarks/results/stage5b-stale-summary.txt`)

## 4. Correction to increment 1

`experiments/stage5/README.md` §4 states:

> **The sound bound is the reverse transitive import closure.** A module that
> does not transitively import M cannot observe anything M declares. Nothing in
> Lean reaches further than imports.

That is true of *elaboration* and false of *the generated page*. Both doc-gen4
and `render.ts` resolve names inside docstrings through a map built from the
whole environment, which is not restricted to the importing modules. The
correction is measured in E2 and recorded in `docs/verification-log.md`; the
increment-1 README keeps its original text with a pointer here, because it is
the record of what was believed at that time.
