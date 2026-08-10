# Stage 6c — how general is L3-1?

Stage 5e verified L3-1 against **one** change: a whole module's body moved to a
new module, where the moved names were plain theorems referenced in printed
signatures. Two things were left open, and this stage closes both.

## Open question 1 — other kinds of moved declaration

The suspicion recorded in the handoff is that **instances are different**, and it
is a specific suspicion rather than a vague one. L3-1 finds a stale reference by
scanning the IR's `refs`, and `refs` is populated from the *printed* signature
(`Extract.lean:397-426`). An instance is normally **not named in any printed
signature** — typeclass resolution finds it, the pretty printer does not print it.
So the link that goes stale when an instance moves is not in a signature at all:
it is in the **instances index**, a whole-package artifact (`global.ts`), and
whether that comes out right is L3-2's and L3-3's business, not L3-1's.

If that is so, then "L3-1 handles moves" is true and **insufficient**, and the
thing that saves an instance move is the global-map diff that increment 7 built
for a different reason. That is worth knowing either way.

`@[simp]` lemmas and `abbrev` are the other two kinds named as unverified.
`abbrev` is the interesting one: it is reducible, so the delaborator may print a
*different* name than the one written, which changes which name lands in `refs`.

## Open question 2 — can L3-1 ever need three rounds?

`--max-rounds` is 5 and no change has produced more than 2. That reads like a gap
in coverage. **It may instead be a structural bound, and if it is, the round loop
is over-engineered.** The argument:

1. A module enters round *n* ≥ 2 only from `ownership.ts`'s stale set, and that
   set excludes everything already seen — in particular everything L2 reported.
   So **a round-≥2 module's own olean did not change**; that is precisely why L2
   could not see it (stage 5c).
2. `ownership.ts` computes `lost`/`gained` by diffing the **declaration name sets**
   of the re-extracted modules against the base IR (`ownership.ts:112-134`).
3. A module's declaration set comes from its own olean's `constNames`
   (`Extract.lean:1553`). Same olean bytes ⇒ same `constNames`.
4. So round-≥2 modules contribute nothing to `lost`/`gained`, `watching` is false,
   and the loop stops. **Two rounds is the maximum.**

**Where that argument has a hole, stated precisely.** Step 3 is not quite "same
olean ⇒ same declaration set": the set is `constNames` *filtered* by
`isBlackListed`, and that predicate **consults the environment** —
`isAuxRecursor env`, `isNoConfusion env`, `getStructureInfo? env parent`,
`findDeclarationRanges?` (`Extract.lean:168-189`). A change that flips the
predicate for some declaration of an unchanged module would change that module's
declaration set without changing its olean, and *that* would produce a round 3.
So the bound is not a theorem about the design; it is a claim about whether any
realistic change flips the blacklist. This stage tries to break it.

## Criteria — declared before measuring

Oracle throughout: **byte equality of the whole page tree with a from-scratch
post-change build**, the same oracle as stages 5e/5f. `--l3-1 off` is run
alongside so that "L3-1 was necessary" is measured and not assumed.

| # | proposition | prediction |
|---|---|---|
| **N0** | The census can find, in this target, a module to move for each kind: one defining an **instance** that the instances index links to, one defining a **`@[simp]` lemma** referenced elsewhere, one defining an **`abbrev`** referenced in a printed signature. | uncertain — stage 5c wasted a run on a module nobody referenced, so the census refuses rather than substitutes. A kind with no witness is reported as **untestable on this target**, not as passing |
| **N1** | **instance move**: with L3-1 on, the tree is byte-identical to REFERENCE. | true |
| **N2** | **instance move**: L3-1 alone is *not* what saves it — with `--l3-1 off` the tree is **also** byte-identical, because the stale link lives in the instances index and L3-2/L3-3 rebuild it. | **true** — this is the suspicion, stated so it can be refuted |
| **N3** | **`@[simp]` lemma move**: byte-identical with L3-1 on; and L3-1 **is** needed (differs with it off), because a simp lemma referenced in a signature is an ordinary reference. | true |
| **N4** | **`abbrev` move**: byte-identical with L3-1 on. | uncertain — if the delaborator prints the unfolded form, the `refs` entry names something else and L3-1 may look at the wrong name |
| **N5** | Every change above reaches its fixed point in **2 rounds**. | true |
| **N6** | No change above produces a **round 3**, and the reason is the structural argument above rather than luck: in every run, `ownership.ts`'s round-2 report has `lostNames = 0` and `gainedNames = 0`. | true — this is the measurable form of the bound |

**Retreat lines.**

* If **N2 is false** (with L3-1 off the instance move is wrong), then instances
  *are* reached through `refs` after all and the suspicion was wrong — good news
  for L3-1's generality, and the reason has to be found.
* If **N4 is false**, L3-1's key is the wrong one for reducible definitions and
  §5.5's L3-1 needs a second rule keyed on the unfolded name.
* If **N6 is false** — some round-2 report shows non-zero `lost`/`gained` — the
  two-round bound is not structural, `--max-rounds` earns its keep, and the
  blacklist's environment dependence is a live hazard rather than a footnote.
* If **N5 is false while N6 is true**, the round counter and the ownership report
  disagree, which would mean `incremental.sh` and `ownership.ts` do not agree on
  what a round is.

## What this stage does not claim

* Nothing about **renames**. `ownership.ts:16-17` argues a rename changes the
  referring module's olean, so L2 sees it and L3-1 is not involved. That is an
  argument, not a measurement, but it is a different question from generality of
  the *move* path and is left where it is.
* Nothing about several simultaneous moves, or a move plus a deletion.

## Files

| | |
|---|---|
| `run.sh` | N0–N6, one scenario per kind, each reset to a clean clone first. The census is inline rather than a separate `census.ts`: it reads the IR *and* greps the sources for `@[simp]` (attributes are not in the IR — `Extract.lean:730` drops them on purpose), so it is not a pure IR tool |

`KINDS=<kinds>` and `A_<KIND>=<module>` pin a scenario, so a stronger witness can
be run without re-deriving it. `SUFFIX` keeps its report file distinct.

---

## Results (2026-08-10)

**All numbers are 実測.** Source: `benchmarks/results/stage6c-l31-generality.txt`
(+ `-abbrev-strong.txt`). Darwin 25.6.0 arm64 / Apple M1 / 16 GB,
`lean4:v4.31.0`, APFS clone reset before each scenario, base re-extracted and
confirmed a fixed point before every move. Oracle: byte equality of all 439 files
with a from-scratch post-move build.

| # | prediction | result |
|---|---|---|
| N0 | uncertain | **2 of 3 kinds have a witness.** `@[simp]` is **untestable on this target**, for a reason worth recording (below) |
| N1 | true | **true** — instance move with L3-1 on: **439/439 byte-identical** |
| N2 | true | **true** — with L3-1 **off** it is *also* 439/439. L3-1 found **0** stale modules and the run took **1 round** |
| N3 | true | **untestable** — no `@[simp]` witness exists on this target |
| N4 | uncertain | **true** — abbrev move with L3-1 on: **439/439 byte-identical**, and **L3-1 is necessary**: with it off, **3 pages are wrong** |
| N5 | true | **true** — 1 round (instance), 2 rounds (abbrev). Never 3 |
| N6 | true | **true** — the abbrev run's round-2 ownership report is `lostNames 0, gainedNames 0` |

### N0/N2 — instances are not reached through `refs`, and the census says so target-wide

The census does not just pick one module; it prints the same column for every
candidate, and that column is the finding:

```
...Superposition.TimeShare      9 instance(s), 0 of them named in a signature elsewhere, 2 referring module(s)
...OuterBoundUV.Region          7 instance(s), 0 of them named in a signature elsewhere, 8 referring module(s)
...OuterBoundUV.Bridge          5 instance(s), 0 of them named in a signature elsewhere, 9 referring module(s)
...TimeSharingConverse.Bridge   4 instance(s), 0 of them named in a signature elsewhere, 1 referring module(s)
...TimeBandLimiting.Operator    4 instance(s), 0 of them named in a signature elsewhere, 7 referring module(s)
```

**Not one instance on this target is named in another module's printed signature.**
Typeclass resolution finds instances; the pretty printer does not print them. So
`refs` — and therefore L3-1 — cannot see an instance move at all.

What *does* save it: the instances list is **not in the page bytes**. `render.ts:1142`
emits an empty `<details class="instances-for-list">` that the browser fills from
`declarations/declaration-data.bmp`, and `global.ts` rebuilds that file on **every**
run (L3-3). So the instance move is correct because a whole-package artefact is
never trusted to be current — the cheapest of the three layers, doing the work
L3-1 was suspected of.

Measured shape of the instance run: 4 changed modules (L2 caught all of them,
including both referring modules), `staleFound 0`, 1 round, 4 pages, and the
ownership pass reports **46 names lost and 46 gained** with **0** modules stale.

### N3 — why `@[simp]` has no witness, and why that is a result

Attributes are not in the IR, so the census greps the sources. On this target:

| | |
|---|---:|
| `@[…simp…]` attribute occurrences in the package's 432 `.lean` files | **76** |
| of those, declarations whose name the census can resolve | **73** |
| the same names matched back onto IR declarations (suffix match) | **70** |
| **of those, named in another module's printed signature** | **0** |

So there is no witness because **a `@[simp]` lemma is a proof-side fact and a
printed signature is a type**. The same mechanism as instances, reached by a
different route. It is reported as untestable rather than as passing, because
"L3-1 handles simp lemmas" is not something this target can show either way — but
"nothing on this target references one in a way L3-1 looks at" is shown.

**The census's name matching was wrong at first** and has been fixed: the source
name is namespace-relative (`Channel.smooth_apply`) while the IR name is fully
qualified, so matching on either the whole name or the last component alone
under-detects. The corrected suffix match finds 70 where the first version found
fewer — **and still 0 are referenced**, so the conclusion did not depend on the
bug. Verified separately before trusting it.

### N4 — abbrev works, and it is a harder case than stage 5e's

| | L3-1 on | L3-1 off |
|---|---|---|
| result | **439/439 byte-identical** | **WRONG — 3 pages differ** |
| rounds | 2 | 1 |
| changed / staleFound | 2 / **3** | 2 / 0 |
| pages rendered | 5 | 2 |
| wrong pages | — | `Fano/Core.html`, `Fano/DPI.html`, `Fano/Measure.html` |

Stage 5e's move produced **1** stale module and **1** wrong page. This one produces
**3 and 3**, so the abbrev case exercises L3-1 harder than the case it was built
against, and it comes out exact. **A reducible definition does not defeat the
`(module, name)` key** — the delaborator prints the abbrev's own name, not its
unfolding, at least here.

### N5/N6 — two rounds is the bound, and the reason is structural

The abbrev run is the only one that reached a round 2, and its ownership report
reads `lostNames 0, gainedNames 0` — exactly what the structural argument
predicts, because a round-≥2 module's olean did not change and a module's
declaration set comes from its own olean. **So the loop stops for a reason, not by
luck.**

**What this does *not* license.** The argument still has the hole named above:
the declaration set is `constNames` filtered by `isBlackListed`, which consults
the environment. Nothing here flipped that predicate. `--max-rounds` therefore
stays — it costs nothing and it is the only thing standing between an unforeseen
blacklist flip and an unbounded loop — but the round loop can be *documented* as
"extract, then extract the referrers, done", which is what it has always done.
