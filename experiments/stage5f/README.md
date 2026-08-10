# Stage 5f — the deletion path

**Question.** A module disappears. Three things still hold it: the ledger, the
IR, and the rendered page. Until stage 5e the pipeline detected this and
**stopped** (`exit 4`), on the grounds that failing is better than silently
leaving a page behind. Does the implemented path actually erase all three, and
does the result equal a from-scratch build?

**Why the renderer cannot do this by itself.** `render.ts` only ever writes. With
`--only` it writes the pages it is asked for and never looks at what else is in
the tree, so a deleted module's page survives every later run and is
indistinguishable from a live one. Deletion has to be an explicit step; it is
not the absence of one.

**Why the ledger needs no incremental path.** `ledger build` costs the same
~0.05 s as `ledger check` (both are dominated by reading 432 `.olean.hash`
files). So the ledger third of the deletion path is "rebuild it", and there is
no reason to write anything cleverer. This is worth stating because the obvious
design — an incremental ledger update keyed on the removed set — would be
strictly more code for no measurable gain.

## The change under test

Delete a **leaf** module: one no other module of the package imports except the
root, and one whose declarations nothing references. Both conditions are read
off `impact.ts --census` rather than assumed, and the root's import of it is
removed so the package still compiles.

**A deletion that keeps the package compiling cannot leave a dangling
reference** — anything naming a deleted declaration would fail to typecheck. So
L3-1 should have nothing to do here, and F2 predicts exactly that. The case
where deletion *does* feed L3-1 is a move whose old file is deleted rather than
shimmed; that is stage 5e's mechanism with an empty "gained" side, and
`ownership.ts --removed` implements it as one computation rather than two.

Run inside the same APFS clone as stage 5e, reset to its pre-move state first.

## Criteria — declared before measuring

| # | proposition | prediction |
|---|---|---|
| **F1** | `check` reports the module under `removed`, and reports the root as `changed` (its import list moved). | true |
| **F2** | `ownership.ts --removed` folds the deleted module's whole name set into `lost` and finds **0** stale modules — a compiling deletion has no referrers. | true, 0 |
| **F3** | After the run the IR has no index entry for the module and its module file is gone; the dependency slice no longer carries names only it referenced. | true |
| **F4** | The module's page is deleted, no orphan page is left, and no empty directory remains. | true |
| **F5** | The incremental page tree is **byte-identical** to a from-scratch build of the post-deletion state. | true |

**Retreat lines.**

* If **F5** fails on pages other than the deleted one, something global depends
  on the module set in a way the per-module IR does not capture — that is the
  §5.5 L3-3 (whole-package post-processing) hole showing up, and it is a finding
  about the *output* contract, not about deletion.
* If **F2** finds stale modules, the "a compiling deletion has no referrers"
  argument is wrong and the reason has to be understood before the path is
  trusted — most likely a reference that survives typechecking, such as one that
  only appears in a docstring.

## Files

| | |
|---|---|
| `../stage5/prune-pages.ts` | the page third: delete removed modules' pages, report orphans |
| `../stage5/merge-ir.ts --remove` | the IR third |
| `run.sh` | F1–F5 against a from-scratch reference |

---

## Results (2026-08-10)

**All numbers are 実測.** Source: `benchmarks/results/stage5f-deletion.txt`.
Deleted: `InformationTheory.Shannon.Kolmogorov.OmegaNoncomputable` (52 declarations,
imported only by the root, no referrers) — chosen from `impact.ts --census`.

| # | prediction | result |
|---|---|---|
| F1 | true | **true** — `removed` names it, `changed` names the root |
| F2 | true, 0 | **true** — 52 names folded into `lost`, **0** stale, 0.153 s |
| F3 | true | **true** — 431 index entries both sides, module sets equal, module file gone, dependency maps equal (527 = 527) |
| F4 | true | **true** — page gone, no orphan, directory sets equal to the from-scratch tree |
| F5 | true | **refuted** — 431 pages, none missing or extra, but **1 differs** |

**The deletion path itself works.** All three thirds — ledger, IR, pages — come
out equal to a from-scratch build, including the emptied directory. Deletion
cost 0.066 s of pruning inside a 4.909 s run.

### F5, and why it is not a deletion bug

The one differing page is `Shannon/LZ78/GreedyLongestPrefix.html`, which has
nothing to do with the deleted module. The difference:

```
incremental:  <code>bAbsorbed = <a href=".../Mathlib/Data/Nat/Find.html#Nat.find">Nat.find</a></code>
reference:    <code>bAbsorbed = Nat.find</code>
```

The **incremental tree has a link the from-scratch build does not**. The
mechanism, traced end to end:

1. The deleted module was the only one in the package that referenced `Nat.find`
   (3 times, in printed signatures).
2. So `Nat.find` was in the package's **global name → module map**, which is
   assembled from the dependency slice. Deleting the module removed it, along
   with 5 other names: the map went **533 → 527** entries.
3. `GreedyLongestPrefix`'s *docstring* mentions `` `Nat.find` `` in backticks.
   Autolink resolves backticked tokens against that global map, so before the
   deletion it produced a link and after it does not.
4. Nothing re-rendered that page: its own IR did not change and no rule points
   at it. The stale link survives.

This is exactly **§5.5's L3-2 (the global map)**, and this is a sharper
demonstration of it than judgement point 3 had:

* **The failure is bidirectional.** Judgement point 3 showed pages that should
  have *gained* a link. This shows a page that should have *lost* one — a link
  to an anchor that is no longer generated anywhere.
* **The affected page is arbitrarily far away.** LZ78 does not import Kolmogorov,
  is not imported by it, and shares no reference with it. Only the global map
  connects them, which is what "does not close under imports" means.
* **The delta is small and computable.** Six names left the map. The set of pages
  that can possibly be affected is "pages whose docstrings mention one of those
  six", which is a scan, not a guess — see stage 5g.

**The retreat line declared before measuring said this outcome means "a finding
about the output contract, not about deletion".** That is what it is, and it is
recorded as such: F1–F4 are the deletion path and they hold.
