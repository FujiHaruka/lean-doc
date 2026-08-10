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
| `census.ts` | pick the module to move for each kind out of the base IR; refuse when there is no witness |
| `run.sh` | N0–N6, one scenario per kind, each reset to a clean clone first |
