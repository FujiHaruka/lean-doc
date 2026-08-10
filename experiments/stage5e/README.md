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

* **A** = `InformationTheory.Shannon.Huffman.Length` (27 declarations)
* **X** = `InformationTheory.Shannon.Huffman.LengthCore` (new)
* **the referrers** = the 4 modules that name one of A's declarations in a
  *printed signature*, read out of the base IR rather than assumed:
  `Huffman.KraftSum` (23 references), `Huffman.ExpectedLength` (7),
  `Huffman.Optimality` (7), `Huffman.StrongForm` (1)

A's contents move to X; A becomes a shim that imports X. **No referrer's source
is touched.** Full names are unchanged — a namespace comes from the `namespace`
command, not the file path — so the only thing that changes is *which module
defines the name*, which is exactly L3-1's subject.

Run inside an APFS clonefile copy of the target (`cp -Rc`, ~34 s, ~0 real disk),
so `lake build` may run and the original is not written to.

**Stage 5c's A could not be used, and finding out why cost a run.** Stage 5c
moved `Shannon.GaussianPDFVarianceDerivative` and called
`Shannon.FisherDeBruijnGaussian` a module that "references it in two places".
Those two places are backticks in a **module docstring**: that module has **zero
declarations**, and *no module in the package names anything of that A's in a
printed signature*. For stage 5c's olean-level question that was irrelevant. For
a question about what the IR's `refs` say, it makes the move unobservable by
construction — both trees came out byte-identical and the experiment said
nothing. A is now chosen from the IR, and `run.sh` refuses to run on a module
with no referrers.

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
| `setup-clone.sh` | clone / apply the E-B move (two shim styles) / reset |
| `rebuild-own.sh` | rebuild the package's own oleans **inside** the clone — required before any baseline |
| `probe-shim.sh` | isolate whether the shim style is what moves a referring module |
| `run.sh` | D1–D5: the three trees (base, incremental, full rebuild) and their comparison |

---

## The first run was wrong, and how that was caught

The first attempt appeared to **refute D1**: the referring module C showed up in
L2's changed set, contradicting stage 5c's P3. It was an artefact of this
experiment's own setup, and the mechanism is worth keeping because it will bite
anything else that measures on a clone.

`cp -Rc` copies the *build tree* along with the sources, so the clone begins
with oleans that were produced at the **original** path. Stage 5c had already
measured that Mathlib's style linter stores its log — with absolute source paths
— in an environment extension that is serialized into the olean (429/432
modules). So the first rebuild *inside* the clone rewrites the path and the
olean changes by the path-length difference alone.

What gave it away was **resetting the edit and checking again**: with the source
back to its original bytes, the ledger still called A and C changed. A real
consequence of the move could not survive undoing the move. Direct confirmation
followed — C's olean went 5,968 → 6,048 B (**+80**, the exact signature stage 5c
recorded), and the clone's path is present as a string inside it.

**The fix is `rebuild-own.sh`**: rebuild the package's own modules once inside
the clone before taking the baseline, so the path is constant from then on.
Mathlib is deliberately left alone — it is never edited here, so its oleans are
never rebuilt and their stale paths never matter.

**The general rule this adds:** on a cloned tree, a baseline is only valid after
everything that can be rebuilt has been rebuilt *in the clone*. Otherwise the
first rebuild of any module is indistinguishable from a real change.
`run.sh` now checks this rather than trusting it, and also requires the baseline
to be a **fixed point** (`check` must report 0/0/0 before anything is edited).

---

## Results (2026-08-10)

**All numbers are 実測.** Source: `benchmarks/results/stage5e-ownership.txt`.

| # | prediction | result |
|---|---|---|
| D1 | true — no referrer in L2's changed set | **partly refuted**: **3 of the 4** referrers *were* in it; one (`StrongForm`) was not |
| D2 | true | **true** — without L3-1, exactly **1 of 433** pages differs from the reference, and it is that referrer's |
| D3 | true | **true** — with L3-1, **0 of 433** pages differ. Byte-identical to a from-scratch build |
| D4 | true, 2 rounds | **true** — round 2 re-extracted the one stale module and found 0 new stale ones |
| D5 | true, ≲ 0.3 s | **true** — 0.1604 s + 0.0011 s = **0.162 s** total |

### D1 is the interesting one: L2 catches referrers, but not reliably

Three of the four referring modules' oleans really did change, so L2 asked for
them. The fourth's did not, and no ledger-side rule could have reached it —
that is stage 5c's P3. **The lesson is not "L2 misses referrers" but "L2's
coverage of referrers is incidental":** it depends on whether the referring
module's olean happens to move, which is a property of Lean's serialization, not
of the documentation's dependency structure. A layer you cannot state the
guarantee of is not a layer you can rely on.

### D2 — exactly what went stale

Without L3-1, `Huffman/StrongForm.html` keeps a link into the module that no
longer defines the name:

```
incremental:  .../Huffman/Length.html#InformationTheory.Shannon.Huffman.huffmanLength
reference:    .../Huffman/LengthCore.html#InformationTheory.Shannon.Huffman.huffmanLength
```

A broken link, not a cosmetic difference: `Length.html` no longer contains that
anchor. This is the failure judgement point 3 predicted and could not previously
demonstrate end-to-end.

### The cost, and where it lands

| | INC-NOL3 | INC-L3 |
|---|---:|---:|
| rounds | 1 | 2 |
| detect | 0.150 | 0.114 |
| extract | 3.199 | **6.170** |
| ownership (L3-1) | 0.000 | 0.265 |
| merge | 0.212 | 0.319 |
| impact | 0.159 | 0.158 |
| render | 0.225 | 0.212 |
| **total** | **4.173** | **7.777** |
| pages rendered | 5 | 6 |

**L3-1's own work is 0.265 s; the correctness costs 3.6 s.** Almost all of it is
the second round paying a whole extractor start — one module's worth of analysis
on top of the ~3.1 s environment load. That is a direct argument for the resident
process (stage 6) that increment 3 had demoted: **every extra round costs one
environment load**, and L3-1 makes extra rounds the normal case rather than an
exotic one.

### A number that fell out: the package's own from-scratch build

Deleting the package's own oleans and rebuilding with Mathlib warm:
**624.88 s wall / 2,899 s user / 975 s sys**, 431 own modules, peak RSS 3.42 GB,
981,955 page faults (Apple M1 / 16 GiB, Lake's default parallelism). Useful as
the denominator the doc side is compared against, and as the cost of preparing a
clone for any future move experiment.
