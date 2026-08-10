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
