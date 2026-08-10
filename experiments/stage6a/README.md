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

---

## Results (2026-08-10)

**All numbers are 実測 unless labelled.** Source:
`benchmarks/results/stage6a-resident-wiring.txt` and
`benchmarks/results/stage6a-resident-wiring.jsonl` (30 pipeline runs).

Conditions: Darwin 25.6.0 arm64 / Apple M1 / 16 GB, `leanprover/lean4:v4.31.0`.
APFS clone of the target, its own 431 modules rebuilt in place first (626.75 s
wall / 2762.29 s user / 821.13 s sys, peak RSS 3.42 GB, 1,110,490 page faults).
Warm: both servers were started minutes after that rebuild, so the package's
oleans were in page cache. 6 runs per variant interleaved inside a phase, run 1
discarded, medians below.

**"439 pages" is the whole site**: 433 module pages (432 under
`InformationTheory/` plus `InformationTheory.html`) + `navbar.html` +
`references.html` + `tactics.html` + `declarations/declaration-data.bmp` +
`declarations/name-map.json` + `references.bib`. Stage 5e's "433" counted module
pages only; the two are consistent, not contradictory.

| # | prediction | result |
|---|---|---|
| W0 | uncertain | **survived** — P was alive after the `lake build` that rewrote oleans it had imported, and answered a request in 0.151 s |
| W1 | true | **true** — see the witness below |
| W2 | true | **true** — 1 of 439 pages wrong, and it is the referrer page, the same failure shape stage 5e got from `--l3-1 off` |
| W3 | true | **true** — `post` and `postall` are both 439/439 byte-identical to REFERENCE |
| W4 | true (a loss) | **split: true for serve-from-2, FALSE for serve-from-1** |
| W5 | true | **true** — 5.06 s is not reachable correctly; the best correct number is 6.168 s |

### W1 — the witness, which is four bytes

`pre`'s round-2 IR for the referrer differs from a post-build extraction by
**10,263 B against 10,267 B**, and the difference is one reference's owner:

```
InformationTheory.Shannon.Huffman.huffmanLength_optimal
  server P says the owner is  ...Huffman.Length     :: ...Huffman.huffmanLength
  REFERENCE says the owner is ...Huffman.LengthCore :: ...Huffman.huffmanLength
```

`Length` is 4 bytes shorter than `LengthCore`. **The resident server returned the
exact stale pair L3-1 exists to repair**, which is what `Extract.lean:1352`
resolving the owner from the environment predicts. The site that comes out of it
is wrong in exactly one page.

### W0 — a resident server does survive a rebuild, and that makes W1 worse

**What was measured**: P was still running after the build and answered a request
for a module the build had rewritten, in 0.151 s, without error.

**Why, is inference and not measurement.** The likeliest reason is that
`importModules (leakEnv := true)` has already faulted in everything the
environment needs, so no later access re-reads the file and a truncate-in-place
would go unnoticed. (`lean` is invoked as `-o <path>.olean`, which looks like a
direct write rather than a temp-and-rename, so the "old inode stays alive" story
is probably *not* the mechanism — an earlier draft of this section asserted it as
fact and that was wrong.) **The difference matters**: if survival depends on
everything already being resident, then a server that faults lazily — a larger
environment, memory pressure, a partially-touched module — could still take a
SIGBUS. **Untested.**

What is not in doubt is the consequence: **a long-lived server does not fail
loudly.** W1 is what it serves — a coherent view of a build that no longer exists.
The failure mode is a confident wrong answer, which is the harder one to notice.

### W4 — the prediction was right about the wrong variant

| variant | what is served | median total | [min–max] | of which extract |
|---|---|---:|---:|---:|
| `fresh` | nothing (today's pipeline) | **7.850** | [7.765–7.901] | 5.872 |
| `pre` | round 2, pre-edit env | 5.284 | [5.245–5.320] | 3.335 |
| `fresh2` | nothing (phase 2 control) | **7.940** | [7.846–8.228] | 5.976 |
| `post` | round 2, post-build env | 5.302 | [5.253–5.317] | 3.352 |
| `postall` | **every round**, post-build env | **2.682** | [2.579–2.690] | 0.718 |

Phase comparability: `fresh` 7.850 vs `fresh2` 7.940, **delta 0.090 s** — the two
phases are the same measurement.

Server startup, warm: **P 3.998 s, Q 3.486 s**. (Stage 5h's 6.335 s was a
different condition — the original tree, not a just-rebuilt clone. Not comparable,
and not compared.)

A server started per pipeline run must pay its startup inside the run:

| | pipeline | + startup | vs `fresh2` 7.940 |
|---|---:|---:|---|
| `post` (serve rounds 2+) | 5.302 | **8.788** | **a loss of 0.85 s** |
| `postall` (serve every round) | 2.682 | **6.168** | **a win of 1.77 s (−22%)** |

**Why the split.** Today's pipeline pays one environment load *per round*: 5.872 s
of extraction across two rounds. Serving only round 2 removes one load (≈2.5 s)
but adds a whole startup (3.486 s) — a net loss. Serving **both** rounds removes
both loads (extraction falls 5.872 → 0.718 s) for the same single startup, and
that pays. The break-even is at **two rounds**, not the four stage 5h estimated,
because a round's load here is ~2.5–3 s against a 3.5 s startup on warm oleans.

### W5 — what is actually reachable

* **5.06 s is not reachable.** It assumed the `pre` shape, which is wrong. `pre`
  did land at 5.284 s, so the extrapolation's *clock* was close; its *answer* was
  incorrect. Same speed, different correctness — the cleanest form this result
  could have taken.
* **6.168 s is reachable and correct**: one server per pipeline run, started after
  `lake build`, serving every round. −22% on the two-round case.
* **2.682 s is reachable only without the startup**, i.e. by a server that serves
  several pipeline runs with no `lake build` between them. That is not the edit
  loop; it is re-rendering, re-querying, or a batch of independent requests.

### What this settles about residency

1. **A server must never outlive the build whose oleans it imported.** Not because
   it dies — it does not — but because it answers wrongly (W0 + W1).
2. **The unit of residency is the pipeline run, not the session**, and it is worth
   it only when the run has ≥2 extraction rounds.
3. **`--serve-from 2` is the wrong default.** The correct-and-faster configuration
   is `--serve-from 1` against a post-build server. The flag defaults to 2 because
   that is what the extrapolation assumed and this stage had to be able to run it;
   the measurement says 1.
4. Stage 5h's "the real fit is a long-lived preview mode" does not survive W1
   either, for the same reason. That is stage 6d.
