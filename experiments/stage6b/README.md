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
| **V5** | The rendered tree gets **smaller**: `{{SOURCE_URL}}` is 16 bytes against the 74-byte prefix it replaces, 4,992 times. | true, ≈ −290 KB |
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
| `render.ts` | a copy of `stage4c/render.ts` with `--source-url-token` | 
| `inject.ts` | both injections (JS asset, deploy-time substitution) + the inverse used by V4 |
| `run.sh` | V0–V6 |
