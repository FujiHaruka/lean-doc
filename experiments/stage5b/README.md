# stage 5 — incremental generation, increment 2 (judgement point 3)

`docs/approach.md` §7 stage 5, second half: **prove that no stale page survives.**
Increment 1 (`experiments/stage5/`) measured the speed and left criterion 3
undecided. This increment decides it.

Throwaway experiment code (CLAUDE.md). Nothing in
`/Users/haruka/dev/lean-projects` is modified: no `.lean` edit, no `lake build`,
no `lake update`, no `fromDb`.

**Numbers**: `benchmarks/results/stage5b-stale-summary.txt`.

---

## 1. The claim under test, decomposed

Increment 1's criterion 3 was stated as *"the impact set is correctly closed
(nothing stale, nothing gratuitous)"*. That is one sentence carrying five
independent claims. Each is stated below as a falsifiable proposition together
with **the observation that refutes it**, and with **the prediction made before
running anything** — so that a result agreeing with the prediction is still a
measurement and a result disagreeing with it is still recorded.

| # | proposition | refuted by | prediction (declared 2026-08-10, before measuring) |
|---|---|---|---|
| **S1** | Every input that changes a page's bytes is observed by the ledger | one input that changes ≥1 page while `ledger check` reports 0 changed modules | **refuted** — `--source-url` is a CLI argument and is not in `envKey` |
| **S2** | The computed impact set contains every page whose bytes actually change | one injected change where (pages differing between a full render before and after) ⊄ (pages the mechanism re-renders) | **refuted** — docstring autolinks resolve through a global name→module map, which is not bounded by imports |
| **S3** | Re-extracting one module gives the same IR as full extraction, under every supported extractor configuration | one configuration where the one-module IR and the full IR differ for the same module | **refuted** under `--open`: a single-module run does not import the module that declares the scoped notation |
| **S4** | A module that disappears leaves nothing behind | a surviving page / ledger entry / IR file, or a pipeline error | **refuted** — no delete path exists in `render.ts` or `merge-ir.ts`; `ledger check` is expected to throw |
| **S5** | The artifacts the pipeline maintains are the whole site | one artifact that the site needs, that depends on the changed module, and that the pipeline never writes | **refuted** — `render.ts` writes module pages only |

**Judgement point 3 is met iff S1–S5 all hold.** If any is refuted, criterion 3
is **not met**, the refutation goes into `docs/verification-log.md`, and
`docs/approach.md` §5.5 ("the module-granular hash is the single source of
truth") gets a revision proposal. Per `docs/plans/three-axes.md` §3 that is
outcome (B), and (B) is a completion, not a failure.

**The predictions above are not results.** They exist so that the experiments
cannot be quietly re-aimed at whatever they happen to find. Every row is filled
in with a measured verdict in §3, and any row where the measurement disagrees
with the prediction is called out explicitly.

## 2. What each experiment does

| exp | proposition | Lean? | what it does |
|---|---|---|---|
| **E1** | S1 | no | render 432 pages twice, changing only `--source-url`; count changed pages and what `ledger check` says |
| **E2** | S2 | no | inject one declaration into one module's IR, render before/after, compare the true changed-page set against `impact.ts --mode self\|referrers\|importers` |
| **E3** | S3 | yes | full extraction with `--open`, and single-module extraction with `--open`, byte-compared against each other and against the `--open`-off IR |
| **E4** | S4 | no | run the pipeline against a module tree with one olean removed (a copy; the target is untouched) |
| **E5** | S5 | no | enumerate the artifacts doc-gen4 produces that the pipeline does not, and classify each by what it is a function of |
| **E6** | (the ownership channel, `experiments/stage5/README.md` §4.2) | no | move one declaration from M to M′ inside a copy of the IR, render before/after, and check where the referring pages' links end up |

E1, E2, E4, E5, E6 do not start Lean and cost seconds. E3 costs one full
extraction plus three single-module runs.

E6 does not carry a proposition of its own: leg 7 named the ownership channel
alongside the printing channel, and S2's refutation is about docstrings, not
about ownership. It is here because judgement point 3 cannot be closed while one
of the two named channels is unmeasured.

## 3. Results

Measured 2026-08-10, one session, no Lean started, nothing in
`/Users/haruka/dev/lean-projects` written. Numbers and the five kinds of faking
they rest on: `benchmarks/results/stage5b-stale-summary.txt`.

| # | verdict | the observation | agrees with the prediction? |
|---|---|---|---|
| **S1** | **REFUTED** | `--source-url`'s revision changes **432 / 432 pages** (4,992 occurrences) while `ledger check` reports **0 changed** | yes |
| **S2** | **REFUTED** | injecting one declaration named `log` changes **142 pages**; `--mode importers` leaves **141 stale** (leaf N) / **3 stale + 276 gratuitous** (hub N) | yes |
| **S3** | **REFUTED** | under `--open`, re-extracting one module gives **12,694 B** where the full extraction gives **12,961 B** for the same module — the single-module IR is byte-identical to the `--open`-*off* baseline, i.e. the flag had no effect at all | yes, including the reason (the notation-declaring module is not in a single-module run's import closure) |
| **S4** | **REFUTED** | one olean removed → `ledger check` **throws** (`ledger.ts:121`, exit 1); one added → **invisible**; the deleted module's page **survives byte-identical** and 4 live pages keep linking to it | yes |
| **S5** | **REFUTED** | **23 / 23** non-module artifacts are never written; 5 of them are a function of every module, **38,581,208 B** | yes |

**Judgement point 3 is not met.** It required all five to hold; **all five are
refuted**. That is outcome (B) of `docs/plans/three-axes.md` §3.

All five measured rows agree with the predictions declared in §1. Seven things
inside those rows did not, and they are the part worth carrying forward:

1. `--mode importers` is not merely incomplete, it is **both** incomplete and
   wasteful — 3 stale and 276 gratuitous on the same run.
2. Deleting a module leaves **0 other pages changed**: `render.ts` fills `known`
   from `refs` too (`render.ts:1357`), so links into the deleted module keep
   being generated. The damage is not "an old page lingers" but "live pages point
   at a module that no longer exists".
3. `envChanged` is not only computed-and-discarded (`ledger.ts:222-224` vs
   `231-234`); `incremental.sh:75-76` also omits `--ir`, so `irSchemaVersion`
   and `irGenerator` are never compared at all.
4. The per-module class-b artifacts (`declaration-data-<module>.bmp`,
   `backrefs-<module>.json`) are **absent from this doc tree** although doc-gen4's
   source writes them (`Output.lean:169,179`). "0 files" here is a property of
   how this tree was built, not of doc-gen4.
5. For the largest candidate (`log`), **no** choice of module to inject into
   avoids stale pages (0 / 432), even though across all 1,095 candidates the
   luckiest module would avoid them 94.5% of the time.
6. `--open` turns out to be **two** mechanisms, not one, and only one of them
   fails to reproduce in a single-module run. `activateScoped` needs the
   notation-declaring module in the import closure; `openDecls` is a printer
   option and needs nothing. 97 of the 263 changed modules are the first, 166
   the second. Whoever reads S3's refutation as "`--open` is unreproducible"
   is reading it too broadly.
7. E6's ownership channel is invisible to a byte comparison of the site: the 48
   referring pages come out **byte-identical**, and what is broken is the anchor
   at the far end of their links. Widening `--mode` fixes **0** of them, because
   `--mode` selects pages to re-render and the fix requires re-*extraction*.

## 3a. Running it

`E1`, `E2`, `E4`, `E5` are one script. It needs a schema-2 IR tree (the one
`experiments/stage5` produced) and a scratch directory; it writes nothing
outside them and nothing into the measurement target.

```
experiments/stage5b/run-all.sh --ir <schema-2 ir> --work <scratch> [--out <results>]
```

`E3` and `E6` are one script each. `run-e3.sh` is the only one that starts Lean;
it reuses any IR tree already present under `--work`, so a second run costs no
Lean time.

```
experiments/stage5b/run-e3.sh --ir <schema-2 ir, --open OFF> --work <scratch>
experiments/stage5b/run-e6.sh --ir <schema-2 ir>             --work <scratch>
```

The pieces, if one is wanted on its own:

| file | what it does |
|---|---|
| `instrument-render.py` | writes an instrumented copy of `stage4c/render.ts` that dumps every failing docstring autolink token. The copy's 432 pages are byte-identical to the pristine renderer's — that is the check that it changed nothing |
| `inject-decl.py` | copies an IR tree and adds one declaration to one module (E2) |
| `drop-module.py` | copies an IR tree with one module removed (E4) |
| `fake-target.py` | a symlink stand-in for the target's olean tree, so a module can be made to disappear without touching the target (E4) |
| `compare-pages.py` | byte-diff two page trees; score an impact set against the difference (`stale` / `gratuitous`); print a diff excerpt. **Refuses to run** on a missing/empty tree or on two trees holding different page sets (`--allow-asymmetric` counts those as differing) — a stale counter must not be able to answer "0 changed" by accident |
| `autolink-analysis.py` | the candidate census, and the extension of the two measured injections to all 432 modules and all 1,095 candidates |
| `site-inventory.py` | doc-gen4's output classified by what each file is a function of, and the broken links our pages emit (E5) |
| `revdep.py` | leg 7's reverse-dependency check over a rendered tree (kept; not used by `run-all.sh`) |
| `notation-surface.py` | leg 7's syntactic count of printing-channel constructs (kept; superseded by `notation-reach.py`) |
| `extract-open.sh` | `stage5/extract-once.sh` plus a `--open` flag. A copy rather than an edit, because increment 1's numbers were taken with that script (E3) |
| `notation-reach.py` | the notation declarations in the target's sources, their *heads*, who names a head in `refs`, and the IMPORTERS closures they are supposed to be bounded by (E3) |
| `open-diff.py` | `classify`: which modules the `--open` IR differs in, split into "a scoped notation fired" and "names got shorter"; `bytecmp`: the byte comparison that decides S3 (E3) |
| `pick-move.py` | the declaration to move and the two destinations, by the rule stated in its docstring (E6) |
| `move-decl.py` | copies an IR tree twice: once with the move and stale `refs`, once with the move and the `refs` a full re-extraction would have written (E6) |
| `link-target.py` | where the referring pages' links point after the move, and whether the anchor at the other end still exists (E6) |

## 4. Correction to increment 1

`experiments/stage5/README.md` §4 states:

> **The sound bound is the reverse transitive import closure.** A module that
> does not transitively import M cannot observe anything M declares. Nothing in
> Lean reaches further than imports.

That is true of *elaboration* and false of *the generated page*. Both doc-gen4
and `render.ts` resolve names inside docstrings through a map built from the
whole environment, which is not restricted to the importing modules. The
correction is measured in E2 and recorded in `docs/verification-log.md`; the
increment-1 README keeps its original text with a pointer here, because it is
the record of what was believed at that time.

E3 refutes the same sentence a second time and for an unrelated reason. The
printing channel §4.1 names is bounded by `IMPORTERS(the module that declares
the notation)`, and on this target that closure has **one** member. The
notation's *heads* — the constants it prints instead of — live in three other
modules, and 95 modules name a head without importing the notation's module at
all. When the extractor runs with `--open`, those 95 modules' IR changes.
`IMPORTERS` is not an upper bound for the printing channel either.
