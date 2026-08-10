# stage 5 — incremental generation, increment 1

`docs/approach.md` §7 stage 5: *"only the changed modules are reprocessed"*.
This increment does the **first half**: hash-based change detection, re-extraction
of one module, and re-rendering the pages that change reach. The other half —
proving that no stale page survives — is leg 8 (judgement point 3), and the
concrete holes this increment found for it are listed at the bottom.

> **Outcome of leg 8 (2026-08-10): REFUTED.** All five propositions behind
> "no stale page survives" were refuted by measurement; see
> `experiments/stage5b/README.md` and
> `benchmarks/results/stage5b-stale-summary.txt`. Criterion 3 below therefore
> ends up **failed**, not merely undecided, and `approach.md` §5.5 was reworked
> into three layers. The *speed* numbers in this README still stand.
> One claim below is now known to be **wrong**: the reverse transitive import
> closure is **not** a sound upper bound on the impact set.

Everything here is throwaway experiment code (CLAUDE.md), and it deliberately
does not modify `experiments/stage4b` or `experiments/stage4c`: the extractor
is run unchanged with a one-line module list, and the renderer is run unchanged
with its existing repeatable `--only`.

**Numbers**: `benchmarks/results/stage5-incremental-summary.txt` (conditions,
per-stage seconds, verdict) and the raw `benchmarks/results/stage5-*.jsonl`.

---

## 1. Judgement criteria, declared before measuring

| # | criterion | verdict |
|---|---|---|
| 1 | the IR from re-extracting one module is **identical** to the IR the full extraction produces for it | **PASS** — byte-identical for all three modules measured, and the merged 432-module tree is byte-identical to the full one |
| 2 | the incremental total is **significantly faster** than full generation | **PASS** — 3.30–4.39 s against 17.07 s, same session, distributions do not overlap. 3.9–5.2x, not the 480x the 0.03 s figure implied |
| 3 | the impact set is **correctly closed** (nothing stale, nothing gratuitous) | **NOT DECIDED HERE.** A necessary condition holds (after six incremental runs the live tree is identical to a full rebuild), but two channels through which a change can reach a page *without* changing that page's olean are identified below and neither is closed. This is leg 8's question and it is left open, not declared passed. |

Criterion 2 is met and criterion 1 is met, so stage 5 continues to increment 2.
**The 0.03 s theoretical figure in `approach.md` §6.5 is refuted** — see §6.

---

## 2. What is hashed, and why

Two hashes with two jobs. `approach.md` §5.5 says "the module-granular hash is
the single source of truth" — that is a *role*, and there are two of them; a
single number cannot fill both, because the two questions are asked at different
times.

| | input side | output side |
|---|---|---|
| question | which modules must be **re-extracted**? | which pages must be **re-rendered**? |
| hashed | the module's `.olean` file(s) | the module's IR JSON |
| computed by | `ledger.ts` (no Lean) | the extractor, already (`index.json.contentHash`) |
| when | before extraction | after extraction |

**Why the olean and not the `.lean` source.** The extractor's only view of a
module is what `importModules` gives it, and that comes exclusively from the
olean. Hashing the source is wrong in both directions: it reports a change for
edits the olean does not carry (whitespace inside a proof body), and it misses
changes the olean does carry (a rebuild triggered by a dependency). The olean is
the extractor's actual input, so it is what the ledger hashes.

**Why not the IR.** Computing the IR hash requires running the extraction that
the ledger exists to skip. It is the right hash for the *second* question, and
the extractor already writes it.

**The three-way olean split.** Modules built with Lean's module system have
`.olean` / `.olean.server` / `.olean.private`. All three are hashed when
present, because the extractor reads declaration ranges and docstrings (server
data) and private declarations (private data); hashing only the public part
would miss a docstring edit. On this measurement target **the package's own 432
modules have one `.olean` each** — the split appears only in the dependency
packages (8,297 of their 8,360 modules have all three). So the ledger here
covers 432 files / 237,909,832 B.

**Lake has already computed this hash.** `<file>.olean.hash` next to every olean
is `computeBinFileHash` of that olean — a content hash, written by
`cacheFileHash` (`Lake/Build/Common.lean`). Reading the 432 `.hash` files is
6.9 KB instead of 227 MB. Both paths are measured (§5); SHA-256 over the bytes is
the reference, the Lake hash is the free alternative. Two caveats keep it from
being simply "the answer": it is a 64-bit non-cryptographic hash, and it is an
undocumented implementation detail of the build tool.

**What is *not* per-module.** Lean version, Mathlib revision, extractor version,
IR schema version and the renderer's own configuration invalidate everything at
once, so they live in the ledger as global keys, not as 432 copies of the same
string. There are **two** such keys, because they do not invalidate the same
thing (`ledgerSchema: 2`):

| key | contents | changed ⇒ |
|---|---|---|
| `extractKey` | `lean-toolchain` text, SHA-256 of `lake-manifest.json`, extractor id, IR schema version, IR generator id | re-extract all 432; which pages are stale still follows from the IR diff |
| `renderKey` | renderer id, `--source-url` | re-extract **nothing**, re-render **all** |

The split is not cosmetic. `--source-url` carries a 40-hex git revision, so it
changes on **every commit** — precisely when an incremental build runs. Under
one key that meant every real incremental build paid a full re-extraction
(Lean started, 27 s) for an input Lean cannot even see: the IR does not carry
the source URL, which is why `render.ts` refuses `--pages` without it. The test
for which side an input belongs on is "can it change the IR", not "does it
change the output" — both do.

`check` reports them separately: the re-extract set goes to `--changed-out`,
and a changed render key goes to `--render-all-out` as a reason line. An empty
`--render-all-out` means "the render set follows from the IR diff as usual".
Both keys are compared over the **union** of the stored and current key sets, so
a key that vanished counts as a change; forgetting `--source-url` therefore
over-renders and says so, rather than silently under-rendering.

## 3. Where the ledger lives

`<ledger>.json`, a separate file from the IR's `index.json`.

The IR index is written by Lean as part of the IR; the ledger is written and
read by the driver with Lean never started. Keeping them apart means (a) the
detection step touches one small file rather than parsing the 15.8 MB IR, and
(b) `index.json` stays byte-reproducible, which matters because stage 4's
15,867,059-byte figure is quoted in `docs/approach.md`.

## 4. Dependency propagation

The question is: M changed — whose IR can change?

**The sound bound is the reverse transitive import closure.** A module that does
not transitively import M cannot observe anything M declares. Nothing in Lean
reaches further than imports.

**The olean ledger alone does not reach that bound, and it is worth being exact
about where it falls short.** If M's *type-level* content changes, every
importer has to be rebuilt and its olean changes with it, so the ledger catches
those. Two channels escape:

1. **Printing.** `notation`, `syntax`, `@[app_unexpander]`, `@[delab]`, `export`
   and friends live in M's environment extensions. They change how *N's* already
   elaborated terms print without touching N's olean at all.
2. **Ownership.** The IR stores a reference as a `(defining module, name)` pair.
   Move a declaration from M to M′ and N's `refs` become wrong — its link points
   at the old page — while N's olean is unchanged.

Both are real. Neither is closed by this increment. What *is* measured is the
size of the first channel's surface on this target, by reading the package's own
sources (read-only; the target is never modified):

| construct | occurrences in the package's 432 `.lean` files |
|---|---:|
| `notation` / `notation3` declarations | 6, in **2** modules (`InformationTheory.Asymptotic`, `…Shannon.TypedRV`) — **all `scoped`** |
| `syntax` / `macro` / `macro_rules` / `elab` / `declare_syntax_cat` | 0 |
| `@[app_unexpander]` / `@[delab]` / unexpanders | 0 |
| `export` | 0 |
| `@[pp_dot]` / `@[pp_nodot]` / `pp_using_anonymous_constructor` | 0 |

This is a syntactic count over the sources (`rg`), not a query on the
environment: the six lines matching `export` are all the English word in prose,
and comments are not excluded. It bounds the surface, it does not certify it.

Both notation-declaring modules use `scoped`, and the extractor runs with
`--open` off by default (three-axes.md pre-decision #4), so on this target and in
this configuration the printing channel is inactive. **That is a property of this
target, not a theorem**, and it is exactly the kind of statement leg 8 has to
attack rather than inherit.

**The two closures, measured on the IR's own graphs** (`impact.ts`, over 432
modules):

| | median | mean | p90 | max | modules with 0 |
|---|---:|---:|---:|---:|---:|
| `IMPORTERS(M)` transitive, own package | 13 (3.0%) | 31.3 (7.2%) | 63 | 414 | 1 |
| `REFERRERS(M)` direct, from `refs` | 0 | 2.6 (0.60%) | 7 | 80 | 259 |

`REFERRERS ⊆ IMPORTERS` (to name M's constant you must import M). The sound
bound costs 7.2% of the package on average; the reference bound 0.60%. The
package's root module `InformationTheory` imports all 430 others, which is why
only one module has no importer — 55 modules are imported by nothing but the
root.

## 5. Choosing the module to change

Three, at the ends of the graph, so the spread is visible rather than assumed:

| name | module | declarations | import closure | transitive importers | direct referrers |
|---|---|---:|---:|---:|---:|
| `hub` | `InformationTheory.Meta.EntryPoint` | 1 | **2,259** | **414** (95.8% of the package) | 0 |
| `leaf` | `…Shannon.Kolmogorov.OmegaNoncomputable` | 52 | 4,258 | 1 (the root only) | 0 |
| `mid` | `…Shannon.ChannelCoding.Basic` | 32 | 5,267 | 159 | **80** (the maximum) |

`hub` is the cheapest to re-extract and the most expensive to re-render; `leaf`
is the opposite; `mid` is the realistic edit. The full package's closure is
6,021 modules, so a single module's environment load is 38–87% of the full one —
the incremental saving on the environment is real but small.

## 6. What is faked, and what that costs

**The measurement target is never modified.** `/Users/haruka/dev/lean-projects`
holds `.lake/build/doc` (stage 4's ground truth) and `.lake/build`'s oleans
(the basis of every number in this repository), so no `.lean` file is edited and
`lake build` is never run. The fact "module M changed" is injected by
invalidating M's ledger entry (`ledger.ts touch`). Everything downstream is real:
the extractor really re-reads M's import closure and re-analyses M, the merge
really rewrites the IR, the renderer really rewrites the pages, and the clocks
are ordinary clocks.

Two consequences, both stated rather than smoothed over:

* **`lake build` is not in these numbers.** It is outside lean-doc but on the
  critical path of a real edit-to-preview loop. Measuring it needs a *copy* of
  the target repository; that is a separate piece of work.
* **Nothing actually changed, so the re-extracted IR is byte-identical and the
  honest render set is empty.** The pipeline reports exactly that
  (`irChanged: 0`), which is the correct answer and is recorded. To measure the
  cost of rendering, `--mode` forces a page set: `self` (M alone), `referrers`
  (M + REFERRERS), `importers` (M + IMPORTERS). The render times are therefore
  real times for real page sets, driven by a set that was chosen rather than
  derived.

## 7. The programs

| file | what it does |
|---|---|
| `ledger.ts` | build / check / touch the input-side hash ledger. `--algorithm sha256\|lake`, `--concurrency N` |
| `impact.ts` | import and reference graphs from the IR; per-module census; the page set for a changed set under `--mode self\|referrers\|importers` |
| `extract-once.sh` | one `lake env extract` over an arbitrary module list, phase timers flattened to JSON |
| `merge-ir.ts` | fold a partial extraction into the package IR; `--verify` compares two IR trees |
| `incremental.sh` | the whole pipeline, one run, per-stage stamps |
| `block-ledger.sh` / `block-extract.sh` / `block-render.sh` / `block-incremental.sh` | the measurement blocks |
| `summarize.py` / `time-step.sh` | generic harness on top of `../stage4c/merge-timing.py` |

**`merge-ir.ts` exists because a partial run cannot produce a correct
`deps/*.json`.** The extractor calls a reference a "dependency" when its defining
module is not in the *target list*, so a one-module run misfiles the package's
own other modules as dependencies — the one-module run of `leaf` writes a
`deps/InformationTheory.json` that the full run does not have. That is an
artefact of the extractor's interface (target list doubling as package list),
not a property of incrementality; the merge recomputes the slice from the merged
module files. It costs 0.16 s because it re-reads all 432 module files, which an
incrementally maintained slice would not.

## 8. Measurement notes

* **The extraction series are not interleaved, and that is a finding.** The first
  attempt used the round-robin the earlier legs used. Every series came out
  cold: the full run's working set (6,021 modules of olean plus 2.8 GB of
  anonymous memory) evicts what the one-module runs just paged in, and after six
  rounds the one-module runs were still at 72,000 major faults. Interleaving is
  right when the variants have comparable working sets; here they differ by 3x.
  Each series therefore runs consecutively, small working sets first, and the
  block ends with a re-take of the small series to measure the drift the full
  run caused (it was within the spread: `hub` 1.926 → 1.833 s, `leaf`
  3.161 → 2.993 s, both *faster*, i.e. no adverse drift).
* **Warm is judged on major page faults**, not on (user+sys)/wall: the render
  series run a multi-threaded runtime whose ratio sits at 1.3 when warm.
* **Cold and warm are not mixed.** The cold ledger series evict exactly the 432
  target oleans with `benchmarks/tools/olean-evict` (no sudo) and run *after*
  the warm series, never interleaved with them — the eviction that makes one
  series cold would make the next warm run cold too.

## 9. What leg 8 has to attack

1. **The printing channel** (§4.1). Add a notation to a module and check whether
   an importer's page changes while its olean does not. On this target the two
   candidate modules are `InformationTheory.Asymptotic` and
   `…Shannon.TypedRV`, and both are `scoped`, so the experiment may need
   `--open` on.
2. **The ownership channel** (§4.2). Move a declaration between modules and check
   the `refs` of the modules that name it. A cheap fix exists — after
   re-extracting M, diff the set of names M defines and re-extract every module
   whose `refs` mention a name that moved — but it is unimplemented and
   unmeasured.
3. **The package-global outputs.** `render.ts` writes module pages only. The
   module index, the search index (`declaration-data-*.bmp`) and `backrefs-*`
   are not produced at all, so "no stale page" is currently a statement about
   432 of an unknown larger number of files.
4. **Deletion.** A module that disappears leaves its page and its ledger entry
   behind. Nothing in this increment removes anything.
