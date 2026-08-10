# Stage 5g — the whole-package artifacts, and the global map's delta

Two things that are the same data seen twice.

**The artifacts.** doc-gen4's output has 23 non-module files, of which 5 are
functions of every module: `declarations/declaration-data.bmp`, `navbar.html`,
`tactics.html`, `references.html`, `references.bib`. `render.ts` generates
**none** of them, which judgement point 3 promoted from "not incremental yet" to
"not implemented". This stage implements what the IR supports and says plainly
what it does not.

**The delta.** The same map that `declaration-data.bmp` serializes — name →
defining module — is what a docstring's autolink resolves against. Stage 5f
showed that deleting one module dropped six names out of it and left a link in an
unrelated page pointing at an anchor that no longer exists (§5.5 L3-2). The map's
**delta** is the thing that decides which extra pages must be re-rendered, and it
is small: six names. So the artifact and the correctness rule are one mechanism.

## What "38.6 MB" is and is not

That figure is doc-gen4's, and 37.5 MB of it is `declaration-data.bmp` holding
**258,760 declarations** — the whole dependency tree, Mathlib included. Ours
covers the package's own declarations (**4,750** across 432 modules), so the size
is not comparable and the 38.6 MB number must not be reused as ours. What
carries over is the *shape* of the problem, not the magnitude.

## Criteria — declared before measuring

| # | proposition | prediction |
|---|---|---|
| **G1** | `tactics.html` is **not** derivable from the package's IR: the tactic docs come from the environment, not from these modules, and the IR records **0** tactics for all 432. | true |
| **G2** | Generating the derivable artifacts from the full IR costs **< 1 s**. | true |
| **G3** | Updating them from a changed/removed module set gives **byte-identical** artifacts to generating them from scratch. | true |
| **G4** | The incremental update is **faster** than the from-scratch generation by enough to justify existing. | **false** — the full generation is expected to be cheap enough that the incremental path is not worth its complexity, exactly as the ledger's `build` turned out to cost the same as its `check` |
| **G5** | Adding the global-map delta rule to the render set closes stage 5f's F5: the deletion run then produces **431/431 byte-identical** pages. | true |

**G4 is predicted false on purpose.** Writing down the prediction that a piece of
work is not worth doing is the only way the measurement can contradict it. If the
full generation turns out to cost seconds, G4 flips and the incremental path is
justified.

**Retreat line.** If G5 fails, the delta rule is not sufficient: something other
than the name → module map reaches a page's bytes, and the remaining difference
names it. Do not widen the rule to "re-render everything" — that would hide
whatever it is.

## Files

| | |
|---|---|
| `../stage5/global.ts` | generate the whole-package artifacts, and emit the map delta |
| `run.sh` | G1–G4 (G5 is settled by stage 5f's end-to-end byte comparison) |

---

## Results (2026-08-10)

**All numbers are 実測.** Sources: `benchmarks/results/stage5g-global.txt`
(raw `stage5g-global.jsonl`) and `stage5f-deletion.txt` for G5.

| # | prediction | result |
|---|---|---|
| G1 | true | **true** — **0** tactic docstrings across all 432 modules |
| G2 | true, < 1 s | **true** — **0.139 s** (read 0.119 + write 0.020), spread 0.138–0.143 |
| G3 | true | **true** — two independent generations are byte-identical, and the artifacts an incremental run produces equal a from-scratch run's (stages 5e and 5f both compare all 6 files) |
| G4 | **false** | **false, confirmed** — see below |
| G5 | true | **true** — stage 5f went from 1 differing page to **437/437 byte-identical** |

### Sizes — and the number that must not be carried over

| file | bytes |
|---|---:|
| `declarations/declaration-data.bmp` | 1,216,017 |
| `declarations/name-map.json` | 602,729 |
| `navbar.html` | 57,949 |
| `tactics.html` | 243 |
| `references.html` | 186 |
| `references.bib` | 0 |
| **total** | **1,877,124** |

**1.88 MB, not 38.6 MB.** doc-gen4's 37.5 MB `declaration-data.bmp` holds
258,760 declarations because it documents the whole dependency tree; ours holds
the package's **4,750 own declarations** plus **533 dependency names** to link
out to. The "38.6 MB" figure from judgement point 3 describes doc-gen4's output
and must not be reused as ours.

### G1 — one of the five is not ours to generate

`tactics.html` is a function of the *environment's* tactic documentation, filled
by core and Mathlib, not by these modules: the IR records **0** tactic
docstrings for all 432. An empty shell is emitted that says so. This is not a
gap in the IR — it is the correct answer for a package that declares no tactics —
but a package that does declare some would need the extractor to carry them,
which it currently does not.

### G4 — the incremental path is not worth writing, as predicted

The most it could remove is the full-IR read, **0.119 s**; it would still have to
read the changed modules, rebuild the derived maps and write every artifact
(0.020 s). Against an incremental run of 4.5–8.0 s the whole step is **1.7–3.1%**
and the savable part is **1.5–2.6%**.

This is the third time the same answer has come back: **the ledger** rebuilds
wholesale because `build` costs what `check` costs, **the dependency slice** is
recomputed in `merge-ir.ts` rather than patched, and now the global artifacts.
The common cause is that reading the whole IR is 0.119 s — the design decision in
§5.4 to keep the IR small enough to re-read is what keeps making the incremental
version of each of these unnecessary.

### G5 — and the bug the first version of the rule had

The delta rule works: 58 names moved in or out of the map (5,283 → 5,225) and the
scan named **exactly one** page, `LZ78.GreedyLongestPrefix`, which mentions
`` `Nat.find` ``. No over-approximation.

Two mistakes were made getting there, both of the same kind — assuming the shape
of something instead of reading it:

1. **The map was built from own declarations only.** The name that mattered,
   `Nat.find`, is a *dependency* name living in the IR's dependency slice. With
   own declarations alone the delta was 52 names, none of them the one that
   mattered, and the rule reported 0 pages while a page was demonstrably wrong.
2. **The scan matched whole code spans.** `render.ts`'s `autoLinkInline` splits
   the text *inside* a code span on whitespace and tries each part; the docstring
   says `` `bAbsorbed = Nat.find` ``, so matching the span as a unit finds
   nothing. The scan now tokenises the same way.

Both failures looked identical from outside — "0 pages to re-render" — and both
would have been invisible without the byte comparison against a from-scratch
build. That is the argument for the oracle, restated by experience.
