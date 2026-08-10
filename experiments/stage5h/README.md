# Stage 5h — stage 6: one process, many extractions

**Question.** The environment load is the extraction's fixed cost — ~3.1 s warm,
81.8% of everything a one-module run does that is not analysis (§6.5). It is paid
**per process**. Can one process serve several extractions, and is the result the
same?

**Why this got its priority back.** Increment 3 demoted stage 6: `lake build` is
74–98% of a real edit's critical path, so removing 3.1 s from the doc side moves
the end-to-end number by little. Increment 5 pushed the other way: **L3-1 turns
re-extraction into rounds**, and every round is another process, so the fixed
cost is paid per round, not per edit. The measured 2-round case went 4.17 s →
7.78 s, and 3.4 s of that increase is a second environment load.

## What a resident process can and cannot do

**Cannot reload.** Lean has no way to swap a changed module out of an imported
environment. A resident server's environment is a snapshot of the oleans as they
were at import time, so it is **wrong** for the module the user just edited.

**Can serve everything else.** L3-1's extra rounds re-extract modules whose own
olean did *not* change — that is precisely why L2 could not see them. Those are
exactly the requests a stale-but-consistent environment answers correctly.

So the honest claim under test is narrow and useful: a resident process is a
correct implementation of *the rounds after the first*, not of the whole
pipeline. `--serve` therefore refuses nothing and reports everything; which
requests are legitimate is the caller's problem, and stating that is part of the
result.

**The direction of the environment difference matters.** Stage 5b's S3 refuted
"a one-module target list reproduces the full extraction" for `--open`, and the
mechanism was that the one-module list left the environment too **small**. A
resident environment is never smaller than a one-shot's — it is the whole
package's — so it cannot fail that way. Whether it is nonetheless byte-identical
is H2, and it is measured, not argued.

## Criteria — declared before measuring

| # | proposition | prediction |
|---|---|---|
| **H1** | After the first, a request's `importModules` time is ~0 and its `initSearchPath` is skipped. | true |
| **H2** | The IR a resident request produces is **byte-identical** to a one-shot run's for the same module, in the default configuration (`--open` off). | true — one-shot-single already matched the full extraction (increment 1 criterion 1, reconfirmed by S3), and the resident environment is the full one |
| **H3** | A single-module request costs materially less than the 3.4 s one-shot. | true, ≲ 0.6 s |
| **H4** | The environment does not degrade: extracting the same module repeatedly, and interleaved with other modules, gives byte-identical IR every time. | true |
| **H5** | Stage 5e's 2-round case (7.78 s) would drop by roughly one environment load. | true, ≈ −3.2 s |

**Retreat lines.**

* If **H2** fails, a resident process is not a drop-in for the one-shot and the
  difference has to be explained before any of this is usable — most likely the
  pretty-printer shortening names differently under a larger environment, which
  would be a finding about the IR's stability, not about residency.
* If **H4** fails, §8's "environment pollution" concern is real and concrete, and
  the retreat is a process per N requests rather than per pipeline.

## Files

| | |
|---|---|
| `../stage4b/Extract.lean` | `--serve`: import once, then one request per stdin line |
| `run.sh` | H1–H5 |

---

## Results (2026-08-10)

**All numbers are 実測 unless labelled.** Source:
`benchmarks/results/stage5h-resident.txt`. Requests were deliberately repeated
and interleaved (`M1 M2 M1 M3 M1 M2`), because H4 asks whether the Nth answer
equals the first and a sequence of distinct modules cannot show that.

| # | prediction | result |
|---|---|---|
| H1 | true | **true** — `importModules` and `initSearchPath` are **0.0000 s** on all six requests |
| H2 | true | **true** — every request's module IR is **byte-identical** to the one-shot's, under a 6,021-module environment |
| H3 | true, ≲ 0.6 s | **true, and by much more than predicted** — **0.035–0.088 s** per request against 1.72–4.60 s one-shot |
| H4 | true | **true** — repeated and interleaved requests are byte-identical to their first answer. No degradation over six requests |
| H5 | true, ≈ −3.2 s | **true** — see below (外挿) |

### The numbers

| | seconds |
|---|---:|
| env load at startup (432-module import list → 6,021 modules) | **6.335** |
| request 1 (StrongForm) | 0.040 |
| request 2 (Length) | 0.088 |
| request 3 (StrongForm again) | 0.035 |
| request 4 (Asymptotic) | 0.041 |
| request 5 (StrongForm again) | 0.036 |
| request 6 (Length again) | 0.081 |
| one-shot StrongForm / Length / Asymptotic | 4.602 / 1.867 / 1.718 |
| of which `importModules` | 4.584 / 1.808 / 1.697 |

**A one-shot run is its environment load.** 98–99% of it, on these modules. The
analysis is 0.02–0.06 s.

### H2 is the result that makes the rest usable

The resident environment holds **6,021 modules**; a one-shot for a single module
holds only that module's closure. The IR came out byte-identical anyway, on every
request. That was not obvious — the pretty-printer shortens names against the
environment, so a larger one could in principle print differently — and it is
the condition under which a resident process is a drop-in rather than a
different generator.

### The catch, stated plainly: a resident environment cannot be reloaded

Lean has no way to swap a changed module out of an imported environment, so the
server's view of the oleans is frozen at import time. That makes it **wrong for
the module the user just edited** and right for everything else.

> **Refuted by stage 6a (2026-08-10).** This section used to continue: "Which is
> precisely the split L3-1 creates: round 1 re-extracts the changed modules …
> rounds 2+ re-extract modules whose oleans did *not* move … so a resident process
> serves them correctly." **The second half is false.** A module's IR is not a
> function of its own olean: `Extract.lean:1352` resolves every reference's owner
> from the *environment*, and repairing that owner column is the only reason L3-1
> re-extracts a round-2 module at all. A pre-edit environment returns the stale
> owner, so serving round 2 from it reproduces exactly the error L3-1 was added to
> fix — measured: the referrer's IR came out 10,263 B against the correct 10,267 B,
> owner `…Huffman.Length` where it should be `…Huffman.LengthCore`, and 1 of 439
> pages was wrong. **"Right for everything else" means everything downstream of no
> change at all, which in an edit loop is nothing.** What is safe is a server
> started *after* `lake build`; see `stage6a/README.md`.

### H5 — what this is worth on stage 5e's two-round case 【外挿、後に否定】

Combining two measurements: stage 5e's L3-1 run (7.989 s total, 6.170 s of
extraction over 2 rounds, of which round 1 was 3.199 s) and this stage's
per-request cost.

| | measured | with a resident server for round 2 |
|---|---:|---:|
| round 1 extraction (5 modules, must be a fresh process) | 3.199 | 3.199 |
| round 2 extraction (1 module, olean unchanged) | ~2.971 | **~0.04** |
| everything else | 1.819 | 1.819 |
| **total** | **7.989** | **≈ 5.06** |

**≈ −2.9 s, i.e. the whole of round 2 but the first.** This is an extrapolation,
not a measurement of the assembled pipeline: the resident server has not been
wired into `incremental.sh`.

> **Assembled in stage 6a, and this row does not survive.** The `pre` shape this
> table describes measured **5.284 s**, so the clock was close — but its output is
> **wrong** (see the refutation above). The reachable-and-correct number is
> **6.168 s**, from a server started after `lake build` and serving *every* round,
> against 7.940 s for the current pipeline. Serving only round 2, as this table
> assumes, is a **net loss** once the server's own startup is inside the run
> (8.788 s).

### What this does *not* say

* **Starting the server is not free: 6.335 s**, which is *more* than a
  single-module one-shot's import (1.7 s) because it imports the whole package.
  Residency wins by serving many requests, not by being cheaper to start. A
  design that restarts the server after every `lake build` would pay 6.3 s to
  save 1.7 s per round — a loss below ~4 rounds.
* **The real fit is a long-lived preview mode**, where the server outlives many
  edits and answers everything except the just-edited modules. That is §8's
  "preview mode" question, and this measurement is an argument for it rather
  than for stage 6 as originally framed.
* **Six requests is not a soak test.** H4 holds over six; it says nothing about
  six hundred, and environment growth over a long-lived session is unmeasured.
* **A request naming a module outside the server's import list is untested.**
