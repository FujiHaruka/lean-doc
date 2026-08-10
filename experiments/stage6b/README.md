# Stage 6b — keeping the revision out of the page bytes

**Question** (`approach.md` §8). The git revision is in the page bytes, so a
commit invalidates every page whatever else it touches. If the pages carried a
*placeholder* instead and the revision were supplied once at serve time, a
revision-only commit would invalidate **no page**. What does that cost, what does
it buy, and what breaks?

## Where the revision actually is

Only two sites, both derived from one prefix (`stage4c/render.ts:1250`):

| | HTML | per page |
|---|---|---|
| `gh_link` (`render.ts:1202-1205`) | `<div class="gh_link"><a href="{SOURCE_URL}/{Mod/Path}.lean#L{line}-L{endLine}">source</a></div>` | one per rendered declaration — **4,560** across the tree |
| `gh_nav_link` (`render.ts:800`) | `<p class="gh_nav_link"><a href="{SOURCE_URL}/{Mod/Path}.lean">source</a></p>` | exactly **1** |

`4,560 + 432 = 4,992`, which is the count in `stage5b-e1-pages.txt:1-5` (all
occurrences of the 40-hex revision across the 432 differing pages; per page min 1
/ median 9 / max 73). **Everything else in the href is IR-derived** — the module
path and the line range — so nothing but the prefix needs the renderer to run
again when the revision moves.

## The design under test

The page bytes carry the token `{{SOURCE_URL}}` where the prefix used to be:

```html
<a href="{{SOURCE_URL}}/InformationTheory/Asymptotic.lean#L44-L52">source</a>
```

so that **one textual substitution reproduces the old bytes exactly**. That makes
the two designs equivalent up to a substitution rather than merely similar, and it
gives the injection two implementations with the same target:

* **serve-time JS** — a site-wide `source-url.js` holds the prefix and a load-time
  pass rewrites the two link classes. render.ts already injects per-page
  `<script>const SITE_ROOT=…</script>` (`render.ts:742-743`), so the mechanism is
  not new to the output contract.
* **deploy-time substitution** — `sed` over the tree once per deploy. No JS, but
  it touches every file, so it is not free and the pages are only rev-free *at
  rest*.

## The cost this design has, stated before measuring

* **The HTML stops being self-contained.** Serving the tree without either
  injection leaves 4,992 dead links. That is the trade §8 names.
* **`jump-src.js` reads the href at runtime** (`.gh_link a` → `getAttribute("href")`
  → `location.replace`). With a placeholder in the attribute it would navigate to
  the placeholder unless the injection runs first. Ordering is a real constraint,
  not a detail.
* **`coverage.ts:512-513` normalises the revision** when comparing against
  doc-gen4 (`/blob/[0-9a-f]{40}/` → `/blob/REV/`). With a placeholder the gh_link
  difference is no longer "the revision only", so the acceptance oracle that
  stages 4c/5b used has to be extended or run after injection.

## Criteria — declared before measuring

| # | proposition | prediction |
|---|---|---|
| **V0** | With the current pipeline, an **empty render set renders every page**: `incremental.sh` builds `--only` arguments from the render set, and no `--only` means "render everything" (`incremental.sh:248-252`). So "0 pages" cannot be reached by changing the renderer alone. | **true** — read off the code; measured here because a latent bug that defeats the whole point of this stage must be a measurement and not a claim |
| **V1** | After the change, a revision-only commit yields **0 changed modules and 0 pages rendered** (against 432 today). | true, once V0 is fixed |
| **V2** | The 40-hex revision occurs **0 times** in the rendered tree (against 4,992). | true |
| **V3** | The pipeline total for a revision-only commit drops from **1.34 s** to **≲0.6 s**, and the render stage from 0.87 s to ≈0. | true — the 0.87 s is 82% page-proportional (`stage5d/README.md`, corrected), so removing the pages removes most of it |
| **V4** | Substituting the token back reproduces the rev-embedded tree **byte for byte** — no information is lost by the change. | true by construction; measured so that a dropped line range cannot hide |
| **V5** | The rendered tree gets **smaller**: the token replaces a longer prefix at every occurrence. | true, ≈ −290 KB【この見積もりの算術は後で外れる — 下の結果参照】|
| **V6** | The deploy-time substitution over 432 files costs more than the JS injection but far less than a re-render. | true, ≲0.3 s |

**Retreat lines.**

* If **V3 is false** — the total does not drop — then the render stage was not
  what made a revision-only commit cost 1.34 s, and the change buys nothing on the
  clock. It would still buy "0 invalidated pages", which matters for a CDN, but
  the §8 entry would have to say so instead of quoting a saving.
* If **V1 is false after fixing V0** — pages still re-render — then something else
  in the render key moves with a commit, and that has to be found before the
  output contract is changed.
* **Not a retreat line but a decision the measurement cannot make:** whether
  losing self-containment is acceptable is a product question. The measurement
  bounds what is bought; §8 keeps the choice.

## Files

| | |
|---|---|
| `run.sh` | V0–V6 |

**No `render.ts` copy and no `inject.ts` exist**, which this section originally
planned for. They turned out to be unnecessary: `--source-url` is a plain prefix
string, so the whole change is passing a constant token as that argument, and the
inverse substitution is four lines of the harness. Recorded here rather than
quietly deleted, because "the design needed no code" is part of the result.

---

## Results (2026-08-10)

**All numbers are 実測.** Source: `benchmarks/results/stage6b-revless.{txt,jsonl}`
(12 pipeline runs). Darwin 25.6.0 arm64 / Apple M1 / 16 GB, `lean4:v4.31.0`, the
APFS clone in its post-move 433-module state, IR reused across both contracts so
**no Lean runs inside the measured loop**. 6 runs per contract, interleaved, run 1
discarded.

| # | prediction | result |
|---|---|---|
| V0 | true | **true** — `render.ts` with no `--only` writes **433 pages** |
| V1 | true | **true** — B renders **0** pages, A renders **433** |
| V2 | true | **true** — **0** occurrences of the revision (A has **4,993**) |
| V3 | true, ≲0.6 s | **true** — total **1.6227 → 0.6441 s (−60.3%)**, render stage **0.9017 → 0.0256 s** |
| V4 | true | **true** — 439 files byte-identical after substituting the token back |
| V5 | true, ≈−290 KB | **true in direction, and the estimate was wrong: −399,440 B (−1.27%)** |
| V6 | true, ≲0.3 s | **true — 0.1893 s** for 439 files |

### The change needs no renderer change

`--source-url` is concatenated with an IR-derived path (`render.ts:1250`), so
`--source-url '{{SOURCE_URL}}'` puts the token in the bytes and takes the revision
out of `renderKey` at the same time — the key stores the string it was handed
(`ledger.ts:203-208`) and that string is now constant across commits. The
**output contract** changes; the code does not.

### V0 — the bug that would have silently defeated this

`render.ts` treats "no `--only`" as "every module" (`render.ts:1381`), and
`incremental.sh` built its `--only` list from the render set — so an **empty**
render set meant **render all 433 pages**. Nothing reached that path while the
revision was in the bytes, because every commit forced `all` anyway. Contract B
lands there on every run. Fixed by skipping the renderer when the set is empty;
without the fix, V1 would have read "433 pages" and the whole stage would have
looked like it bought nothing.

### V5 — the arithmetic, because the estimate missed by 38%

Predicted ≈−290 KB, measured **−399,440 B**. The estimate used the wrong two
lengths. The real ones:

| | bytes |
|---|---:|
| `https://github.com/FujiHaruka/information-theory/blob/` + 40 hex | **94** |
| `{{SOURCE_URL}}` | **14** |
| difference per occurrence | **80** |
| occurrences | **4,993** |
| 4,993 × 80 | **399,440** — exactly the measured delta |

**4,993, not 4,992**: the earlier count was taken on the 432-module tree, and this
one is post-move with 433 modules, so there is one more per-page nav link. The two
numbers agree; the module count differs.

### V3 — where the 0.98 s goes, and what is left

Per-stage medians from `stage6b-revless.jsonl` (runs 2–6):

| stage | A (revision in bytes) | B (token) | share of B's total |
|---|---:|---:|---:|
| detect | 0.1105 | 0.1083 | 16.8% |
| rounds (extract / ownership / merge) | 0.0382 | 0.0365 | 5.7% |
| prune | 0.0278 | 0.0276 | 4.3% |
| **global (L3-3)** | 0.3136 | **0.2903** | **45.1%** |
| impact | 0.1630 | 0.1566 | 24.3% |
| **render** | **0.9017** | **0.0256** (skipped) | 4.0% |
| **total** | **1.6227** | **0.6441** | |

**The single largest remaining item is `global.ts` at 45.1%**, with `impact.ts`
second at 24.3% — together 69%, and `detect` accounts for a further 17%. (An
earlier draft of this section said B's remainder was "almost entirely the
whole-package artefacts"; that overstates it and is corrected here.)

Note that **the pipeline's L3-3 cost is 0.29 s, not the 0.139 s increment 7
measured**, because `incremental.sh` runs `global.ts` twice — `build` and then
`delta` for L3-2's input. Increment 7 declined to incrementalise this on the
grounds that only 0.119 s of IR reading could be removed from a 1.5–2.6% slice of
the run. **Under contract B the same work is 45% of the run**, so that judgment
was made against a different denominator and should be re-made rather than
inherited.

### The cost, measured rather than asserted

* **The pages are no longer self-contained.** 4,993 dead links if neither
  injection runs. This is the trade §8 names and the measurement does not soften
  it.
* **Deploy-time substitution costs 0.1893 s** for the whole tree — 4.8× cheaper
  than the 0.9017 s re-render it replaces, but **not free**, so "the 0.87 s
  disappears" is only true for the JS-injection variant. For the substitution
  variant the honest figure is 0.9017 → 0.1893 s.
* **`jump-src.js` reads `.gh_link a`'s `href` at runtime**, so with a placeholder
  in the attribute the injection must run first. Untested here: this stage
  measured bytes and timings, not browser behaviour.
* **`coverage.ts:512-513` normalises the revision** when comparing against
  doc-gen4. Under contract B the gh_link difference is no longer "the revision
  only", so that oracle must be run **after** injection. Also untested here.
