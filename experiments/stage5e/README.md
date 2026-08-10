# Stage 5e — L3-1 (name ownership) implemented and tested against a real move

**Question.** Stage 5c established that a declaration moving from module A to
module X leaves a referring module C's `.olean` **byte identical**, so L2 cannot
see it and no widening of the *render* set can repair it. L3-1 is the layer that
is supposed to close that hole. Does it — and is closing it enough to make the
incremental output equal to a from-scratch build?

**The oracle is byte equality with a full rebuild.** Not "no stale pages
reported", not "the changed set looks right": the incremental page tree must
equal the page tree produced by extracting all modules from the moved state and
rendering everything. That is the only check that cannot be satisfied by a
mistake in the same code that produced the answer.

## The change under test

Stage 5c's E-B, reused unchanged because it is already known to be invisible to
L2:

* **A** = `InformationTheory.Shannon.GaussianPDFVarianceDerivative`
* **X** = `InformationTheory.Shannon.GaussianPDFVarianceDerivativeCore` (new)
* **C** = `InformationTheory.Shannon.FisherDeBruijnGaussian`, which references
  A's `isHeatTimeDerivHyp_gaussian` in two places

A's contents move to X; A becomes a shim that imports X. **C's source is not
touched.** Full names are unchanged — a namespace comes from the `namespace`
command, not the file path — so the only thing that changes is *which module
defines the name*, which is exactly L3-1's subject.

Run inside an APFS clonefile copy of the target (`cp -Rc`, ~34 s, ~0 real disk),
so `lake build` may run and the original is not written to.

## Criteria — declared before measuring

| # | proposition | prediction |
|---|---|---|
| **D1** | After `lake build`, L2's changed set contains A, X and the root module, and **does not** contain C. | true (this is stage 5c P3 restated at the pipeline level) |
| **D2** | Without L3-1, the incremental page tree **differs** from the full-rebuild tree, and C's page is among the differing ones. | true |
| **D3** | With L3-1, `ownership.ts` names C as stale, and after the extra round the incremental page tree is **byte-identical** to the full-rebuild tree. | true |
| **D4** | The round loop reaches a fixpoint in **2 rounds**: round 2 re-extracts C and produces no new stale module. | true |
| **D5** | L3-1's own cost is small — one pass over the base IR, well under the extraction it triggers. | true, ≲ 0.3 s |

**Retreat lines.**

* If **D3** is false, L3-1 as specified is insufficient: something else also goes
  stale on a move, and the difference between the two trees names it. Record what
  it is; do not patch until it is understood.
* If **D4** is false — the loop needs three or more rounds — then re-extraction
  is genuinely iterative and `approach.md` §5.5 has to say so, because the cost
  model of the incremental path changes.

## Why `gained` is checked as well as `lost`

`ownership.ts` flags a reference `(O, n)` when `n ∈ lost(O)` (O no longer defines
what the reference points at) **or** when `n ∈ gained(M)` for some `M ≠ O` (the
name now lives elsewhere). The second rule should be redundant for a move —
removing a declaration from A changes A's olean, so A is in the changed set and
`lost(A)` fires — but "should be" is an argument, not a measurement, so both
rules are counted separately and the summary reports which one fired.

## Files

| | |
|---|---|
| `../stage5/ownership.ts` | L3-1 itself: the ownership diff and the reference scan |
| `setup-clone.sh` | make the APFS clone and apply the E-B move |
| `run.sh` | D1–D5: the three trees (base, incremental, full rebuild) and their comparison |
