# Stage 6d — is preview mode the receptacle for residency?

**Where this question comes from.** Stage 5h's own conclusion (verbatim):

> The real fit is a long-lived preview mode, where the server outlives many edits
> and answers everything except the just-edited modules.

and `approach.md` §8 keeps "does it have a preview mode" open, with increment 3's
measurement raising its weight: `lake build` is 74–98% of a real edit's critical
path, so nothing on the doc side can put a fresh page in front of the user.

**Stage 6a removed the ground under the sentence above.** A pre-edit resident
environment answers the L3-1 rounds *wrongly* (W1), because a reference's owner is
resolved from the environment and the environment's snapshot predates the move.
The same mechanism applies to preview: a server that outlives an edit can only
report the world as it was before the edit.

So the question this stage settles is narrow and it decides whether residency has
any home at all:

> Is there any page a pre-edit resident server can produce that is **both** correct
> **and** not already sitting on disk from the previous build?

If the answer is none, then preview cannot be residency's receptacle, and §8's
preview question reduces to the two options it already lists — a syntactic
approximation, or serving the previous page and marking it not-updated.

## Criteria — declared before measuring

| # | proposition | prediction |
|---|---|---|
| **U1** | For the stage 5e move, the set of pages a **pre-edit** server can produce that differ from what the previous build already wrote is **empty**. | **true** — a pre-edit environment is a function of the pre-edit oleans, so anything derived from it equals what was derived from them last time |
| **U2** | The pages a user actually needs after an edit are exactly the ones **only a post-build extraction can produce**: the render set (6 of 433 for the stage 5e move). | true |
| **U3** | A "serve the previous page, marked not-updated" preview is wrong on **≤2%** of the site, and the marker has to be **per page** rather than site-wide, because the stale set is small and known. | true — 6/433 = 1.4% |
| **U4** | The stale set is **derivable without Lean**: it is the changed-module set from L2 plus L3-1's stale set plus L3-2's, all of which come from the ledger and the IR. So a preview can mark exactly the right pages before the build finishes. | uncertain — L3-1's set needs the *fresh* IR of the changed module, which needs the build. What is derivable pre-build is L2's set plus the pages naming the changed modules |

**Retreat lines.**

* If **U1 is false** — there is some page a pre-edit server can produce that is
  both correct and new — then residency has a use in preview and the design
  question is which pages qualify and how to test for them cheaply.
* If **U4 is true**, a preview can be honest for free: mark exactly the pages that
  the pending build may change and leave the rest alone. If it is false, the
  marker has to be conservative (mark the importers' closure), and the honest
  cost of that is how many pages get marked for nothing.

## What follows either way

This stage does **not** attempt a syntactic approximation. It bounds what the
alternatives are worth so that §8's entry can stop citing residency as the
answer. The syntactic option stays open, and its cost is a Lean parser without
elaboration — out of scope here and named as such.

## Files

| | |
|---|---|
| `run.sh` | U1–U4, reusing stage 6a's clone, servers, and page trees |

---

## Results (2026-08-10)

**All numbers are 実測.** Source: `benchmarks/results/stage6d-preview.txt`,
computed from stage 6a's trees (`stage6a-resident-wiring.txt` for the run they
came from). Nothing was started and nothing was timed here — these are byte
comparisons.

| # | prediction | result |
|---|---|---|
| U1 | true | **true — the set is empty** |
| U2 | true | **true** — 6 of 433 module pages, and that is exactly the render set |
| U3 | true, ≤2% | **true, 1.39%**, and the marker must be per page |
| U4 | uncertain | **mostly derivable: 5 of 6 pre-build (83%)**; the missing one is L3-1's |

### U1 — the answer that closes the question

Server P was asked for the one L3-1 round-2 module. Its answer was **byte-identical
to what the previous build already had on disk**, and **incorrect** against the
post-build truth. So:

> modules served by P: 1 — of those, both correct **and** different from the
> previous build: **0**

**A pre-edit resident environment is a function of the pre-edit oleans, so
everything derived from it was already derived from them last time.** There is no
page it can produce that is both new and right. Residency cannot be what makes a
preview fast, because a preview's whole job is to say something about the edit.

### U2 — the shape of an edit's blast radius

| | |
|---|---:|
| module pages the edit reached | **6 of 433 (1.39%)** — 5 changed, 1 added |
| the pipeline's render set | **6 — the same set** |
| whole-package artefacts that changed | 4 (`declaration-data.bmp`, `name-map.json`, `navbar.html`, `tactics.html`) |
| module pages already correct on disk | **427** |

The four artefacts are L3-3's: rebuilt on every run whatever changed, so they are
never *stale* in the sense a preview cares about. Counting them with the module
pages would inflate the number a preview gets wrong from 1.39% to 2.28%, which is
why they are counted apart.

### U3/U4 — what an honest "not updated" preview costs

* **1.39% of the site would be stale**, and the stale set is *named*, so the
  marker belongs on those pages. A site-wide "may be out of date" banner would
  mark **427 correct pages for nothing** — worse than useless, because it trains
  the reader to ignore it.
* **Before the build finishes, 5 of the 6 can be named exactly** from the ledger
  alone (olean hashes, no Lean). The sixth is the L3-1 referrer, and it is
  undiscoverable pre-build **by construction**: it is stale precisely because its
  own olean did not change, so no hash can betray it.
* So a pre-build marker is 83% precise here and needs a conservative rule for the
  rest. The cheap conservative rule is "also mark everything whose IR names a
  declaration of a changed module" — the referrer set, which the IR already has
  (4 modules for this change) and which needs no Lean.

### What this leaves in §8

Preview mode has **two** options, not three. Residency is out:

1. a syntactic approximation that does not need the environment — accurate for
   names and signatures as written, wrong wherever elaboration matters;
2. serve the previous page and mark it — **wrong on 1.39% of the site, 83% of
   which is markable before the build even starts**.

Option 2 now has numbers and option 1 still does not; its cost is a Lean parser
without elaboration, which is out of scope here and named as such.
