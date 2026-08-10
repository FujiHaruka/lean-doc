# Stage 6a — wiring the resident server into `incremental.sh`

**Question.** Stage 5h measured a resident extractor at 0.035–0.088 s per request
against 1.72–4.60 s one-shot, and extrapolated stage 5e's two-round case from
7.99 s to ≈5.06 s by serving **round 2** from a resident process. That
extrapolation was never assembled. This stage assembles it.

## What stage 5h claimed, and the reason to doubt it

Stage 5h's README states the split it believed made residency usable (verbatim):

> * **round 1** re-extracts the changed modules — their oleans moved, so a
>   resident process must not serve them;
> * **rounds 2+** re-extract modules whose oleans did *not* move — that is exactly
>   why L2 could not see them — so a resident process serves them correctly.

The second bullet is an inference from "M's own olean is unchanged" to "M's IR is
correctly derivable from a pre-edit environment". **Reading the code says that
inference does not hold.**

A module's IR is not a function of its own olean. `Extract.lean:1352` resolves
every reference's owner from the *environment*:

```lean
match (env.getModuleIdxFor? n).map (modNames[·]!) with
| some defMod => pairs := pairs.push (defMod, n)
```

And the owner column is precisely what L3-1 exists to repair. `ownership.ts:8-20`:

> That pair is a fact about *where a name lives*, and it goes stale when the name
> moves — even though nothing about the referring module changed.

So round 2 re-extracts R in order to learn that `n` now lives in X rather than A.
A resident environment imported **before** the move still holds A as the owner of
`n`, because that is what its snapshot of the oleans says. Re-extracting R there
should reproduce the stale pair — the same wrong answer as not re-extracting at
all.

**This is a prediction about bytes, so it is measured and not argued.** The
oracle is the one stages 5e/5f used: byte equality of the whole page tree with a
from-scratch post-move build. A report from the code under test is not evidence.

## The two servers, and why both are needed

| | import list | environment reflects | started |
|---|---|---|---|
| **P (pre-edit)** | pre-move module list | oleans as they were **before** the move | before `setup-clone.sh move` |
| **Q (post-build)** | post-move module list | oleans **after** `lake build` | after the move's build |

P is the server the extrapolation assumed: it outlives the edit, so its startup
cost is amortised over many edits and the 6.3 s does not appear in the
per-edit number. Q is the honest alternative: correct by construction, but its
6.3 s startup lands inside the run being measured.

## Criteria — declared before measuring

| # | proposition | prediction |
|---|---|---|
| **W0** | Server **P** survives the `lake build` that rewrites oleans it has already imported, and still answers requests afterwards. | uncertain — the environment's olean data is `mmap`ed, so whether a rebuild is survivable depends on whether Lake replaces files by rename (old inode stays alive, safe) or writes in place (torn pages). **This is a prerequisite for testing W1 through P, and a result either way**: a long-lived preview server that cannot outlive a build is a much weaker thing than one that can |
| **W1** | For an L3-1 round-2 module, the IR that server **P** returns is **not** byte-equal to a fresh post-build process's IR for the same module. | **true** — the differing bytes are the `refs` owner column |
| **W2** | Wiring P into rounds 2+ therefore makes the assembled site **wrong**: its page tree differs from REFERENCE, and it differs **in the same page** the `--l3-1 off` variant of stage 5e got wrong. | true |
| **W3** | Server **Q** serves rounds 2+ with IR byte-equal to a fresh process's, and the assembled site is byte-equal to REFERENCE (433/433). | true |
| **W4** | With Q started inside the run, the two-round case is **slower** than the current fresh-process-per-round pipeline: Q's startup (≈6.3 s) exceeds the ≈3 s of round-2 environment load it removes. | true — a net loss |
| **W5** | Stage 5h's ≈5.06 s is therefore **not reachable** by either server: P is incorrect and Q is slower. | true |

**The two servers are started and stopped in sequence, never together.** The
extractor's peak RSS is 3.27–3.37 GB (verification log, stage 5 §(b)) on a 16 GB
machine, and this workload is memory-bound rather than CPU-bound. Two residents
plus a fresh extraction process would be ~10 GB and any paging would land in the
timings. `fresh` is therefore measured **once per phase**, and the two `fresh`
medians are a check that the phases are comparable at all.

**Retreat lines.**

* If **W1 is false** — P's IR matches — then the reading of `Extract.lean:1352`
  above is wrong and the extrapolation stands. The next question would be *why*
  the owner column survives a stale environment, because that would mean the IR
  does not depend on the environment the way the code appears to say it does.
* If **W4 is false** — Q wins anyway — then the fresh-process-per-round design is
  losing more per round than stage 5h's per-request numbers suggest, and the
  round loop should be measured again rather than the server discarded.
* If **W1 is true and W4 is true**, residency has **no place in the batch
  pipeline** and its only remaining fit is a process that outlives many edits and
  is never asked about anything downstream of one — i.e. §8's preview mode, where
  serving a *deliberately* stale answer is the feature rather than a bug.

## What this stage does not claim

* It does not test more than one shape of change. The move is stage 5e's
  (`Length` → `LengthCore`), because the point is to compare against a number
  taken on that exact case. Other shapes are stage 6c's business.
* It says nothing about a server that is *selectively* trusted (serve M only when
  nothing in M's closure moved). That is a real design, but it needs a closure
  test the pipeline does not have, and W1 decides whether it is worth building.

## Files

| | |
|---|---|
| `serve-ctl.sh` | start / request / stop a resident extractor over a FIFO |
| `../stage5/incremental.sh` | `--serve-dir <d>` `--serve-from <n>`: rounds ≥ n go to the server |
| `run.sh` | W1–W5 |
