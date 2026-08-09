# experiments/stage4c — doc-gen4's HTML rebuilt from the IR, without Lean

Verification stage 4 of [`docs/approach.md`](../../docs/approach.md) §7, and the
core of **judgement point 2**: *can doc-gen4's HTML be produced from the IR
alone, with Lean never started?*

* **Increment 3** answers it for `div.decl_header` — the kind word, the
  declaration name, the binders, `extends`, and the result type.
* **Increment 4** extends the generator to the **whole module page** and adds
  the **byte accounting**: for each region of doc-gen4's output, how many bytes
  can be reproduced exactly, and for the rest, why not.

**Nothing here is timed.** Timing is the next task, and it needs its own
measurement conditions (warm/cold, five runs or more, `/usr/bin/time -l`).

Everything is Deno/TypeScript because §6 pre-decision #1 says the throwaway
consumer-side tooling is Deno/TS. **This is not the implementation-language
decision for the product** (§5.6 is still open).

## What is here

| | |
|---|---|
| `render.ts` | reads the schema-2 IR and writes either one `div.decl_header` per declaration as JSONL (`--out`, increment 3) or one full HTML page per module (`--pages`, increment 4). Never reads doc-gen4's output. |
| `compare.ts` | scores the generated headers against `decl-header-truth.jsonl` as **ordered anchor sequences**. Has its own HTML extractor, and a `--self-test` that injects faults to prove it can report a failure. |
| `bytes.ts` | diffs the generated headers **byte for byte** against the `div.decl_header` substrings of doc-gen4's own pages. |
| `coverage.ts` | cuts both sides' **whole pages** into regions that tile the file, and counts bytes: reproduced (byte identical) vs not, with every unreproduced byte attributed to a cause by rule. |

Four programs rather than one because each is a different kind of evidence and
a bug in one must not be able to hide behind another:

* `compare.ts` measures what the task asks for — the `(offset, text, href)`
  sequence — but both it and the ground-truth extractor turn markup into
  sequences, so a *shared* misreading of the markup would cancel out;
* `bytes.ts` does no extraction at all, so it cannot share that bug. It also
  sees everything `compare.ts` is designed to ignore (attribute order, `class`
  values, elements with no text);
* `render.ts` is kept blind to the answer so that "matching" cannot become
  "copying";
* `coverage.ts` answers a different question again — not "is the header right"
  but "of everything doc-gen4 writes, what fraction can exist at all", which is
  the denominator the timing task needs.

## The specification, and where each rule comes from

doc-gen4 v4.31.0 at `/Users/haruka/dev/lean-projects/.lake/packages/doc-gen4`,
read only. **That tree carries the instrumentation patch — never write to it.**

| piece | doc-gen4 source | what the IR supplies |
|---|---|---|
| header skeleton | `Output/Module.lean` `docInfoHeader` | — |
| `span.decl_kind` text | `Process/DocInfo.lean:211-247` `getKindDescription` | `kind` + `modifiers`, recomposed by the table in [`../stage4b/README.md`](../stage4b/README.md) "Kind modifiers" |
| `span.decl_name` / `a.break_within` | `Output/Base.lean` `declNameToHtmlBreakWithinLink`, `breakWithin` | `name` |
| one binder | `Output/Arg.lean` `argToHtml` | `binders[i]` + `binderCode[i]` + `implicits[i]` |
| `extends` | `Output/Module.lean` `structureInfoHeader` | `members` with `label == "parent"` (`name` = the `projFn`, `text` + `code` = the parent type) |
| result type | `Output/Module.lean`, `div.decl_type` | `type` + `typeCode` |
| every `<a>` inside a fragment | `Output/Base.lean:327-389` `renderedCodeToHtmlAux` | the span list, replayed as a tree |
| href | `Output/Base.lean` `declNameToLink` / `moduleNameToLink`, `Output.lean:150` | `refs` for the defining module, module name for the depth |
| which declarations reach a page | `Process/DocInfo.lean:176/186/207` (`render := false`) | a name that is some declaration's `members[].name` |
| page order | — (measured in increment 1) | stable sort on `(line, col)`, ties by `index` |

Four things about `Html.toStringAux` (`Output/ToHtmlFormat.lean`) are load
bearing, because they put **newlines into the header's plaintext** and therefore
into every anchor offset after them:

* JSX literals build `Html.element … true` — inline, no newlines;
* `Html.element … false` with a single *text* child prints
  `<tag>text</tag>\n` — that is `span.decl_kind`, so every header's plaintext
  starts `"<kind>\n"`;
* `Html.element … false` with a single *element* child prints
  `<tag>\n{child}</tag>\n` — that is `argToHtml`'s `span.decl_args`, so every
  binder contributes a `\n` before it and a `\n` after it;
* whitespace written next to a tag in the JSX source (`<span> {x} </span>`) is
  eaten by Lean's tokenizer and never reaches the output.

The one rule that is not in doc-gen4's source but in the IR's: **the span list is
pre-order and must be replayed as a tree**. 81,055 of the IR's 142,181 const
spans sit inside another const span, and `renderedCodeToHtmlAux` suppresses the
*outer* anchor in that case (`Base.lean:342-345`). A flat walk emits invalid
nested anchors for more than half of them. On the header path alone this
suppression fires **25,076 times** (実測).

## Reproduction

```sh
IR=<schema-2 IR from experiments/stage4b>      # index.json / modules/ / deps/
S=<scratch dir>                                # outputs are large; keep them out of the repo
T=$S/decl-header-truth.jsonl                   # benchmarks/tools/decl-header-truth.py

# small loop first: 9 declarations, must be exact before going wide
deno run --allow-read --allow-write render.ts --ir "$IR" \
  --out "$S/mine-asymptotic.jsonl" --only InformationTheory.Asymptotic
deno run --allow-read --allow-write compare.ts \
  --mine "$S/mine-asymptotic.jsonl" --truth "$T" --modules-from-mine

# full run
deno run --allow-read --allow-write render.ts --ir "$IR" \
  --out "$S/mine-all.jsonl" --stats "$S/render-stats.txt"
deno run --allow-read --allow-write compare.ts \
  --mine "$S/mine-all.jsonl" --truth "$T" \
  --report "$S/compare-all.txt" --diffs "$S/diff-all.jsonl" --max-diffs 100000 --self-test
deno run --allow-read --allow-write --allow-env bytes.ts \
  --mine "$S/mine-all.jsonl" --report "$S/bytes-all.txt" --diffs "$S/bytes-diffs.txt"

# the splitWhitespaces experiment (see below) — NOT the headline number
deno run --allow-read --allow-write render.ts --ir "$IR" \
  --out "$S/mine-ws.jsonl" --ws-heuristic
deno run --allow-read --allow-write --allow-env bytes.ts --mine "$S/mine-ws.jsonl"

# increment 4: whole pages + the byte accounting
URL=https://github.com/FujiHaruka/information-theory/blob/573793b243fb1343636088eb62d1789ab2b14cec
deno run --allow-read --allow-write render.ts --ir "$IR" \
  --pages "$S/pages" --source-url "$URL" --stats "$S/render-pages-stats.txt"
deno run --allow-read --allow-write --allow-env coverage.ts \
  --pages "$S/pages" --report "$S/coverage.txt" --diffs "$S/coverage-diffs.txt"
```

`--modules-from-mine` exists only for the small loop; the full run must not use
it, because the denominator has to stay 3,477.

`--source-url` is **configuration, not IR** — doc-gen4 gets it from lake and git,
and the extractor never saw it. It is passed on the command line for exactly that
reason, and the accounting keeps the bytes it produces in their own bucket. See
"The two revisions" below for why one value cannot make all 348 pages agree.

## Results

**All 実測.** Conditions: Deno 2.7.14 / V8 14.7.173.20-rusty, macOS 26.6 arm64;
IR = `experiments/stage4b` schema 2, 432 modules, 4,750 declarations; ground
truth = `benchmarks/results/stage4-decl-header-truth.txt` over doc-gen4 v4.31.0's
348 rendered pages. Logs are regenerable by the commands above and are not
committed (23 MB of JSONL).

### Denominators first

doc-gen4's HTML for this target is **348 modules, not 432** — the full build was
stopped at 42%. The IR side of those 348 modules is 3,663 declarations, 186 of
which are projection functions or constructors that doc-gen4 does not put on a
page, leaving **3,477**. Reproduced here from the IR alone (実測): 348 modules,
3,663 declarations, 186 suppressed, 3,477 rendered — the same 3,477 the ground
truth has, name for name (0 missing, 0 extra).

The generator also emits 1,083 declarations for the 84 modules that have no
HTML. **They are not scored and no claim is made about them.**

### The metrics asked for

Every row is a comparison of **ordered sequences**. No set or multiset
comparison appears anywhere in `compare.ts`.

| 指標 | 一致 | 母数 | 率 |
|---|---:|---:|---:|
| 宣言単位の完全一致 — `(位置, テキスト, href)` の列 | 3,477 | 3,477 | **100.000%** |
| アンカー単位の一致 (三つ組) | 81,542 | 81,542 | **100.000%** |
| … href だけ違う | 0 | 81,542 | 0% |
| … 位置だけずれ | 0 | 81,542 | 0% |
| … 位置も href も違う | 0 | 81,542 | 0% |
| … 正解にあって生成側に無い | 0 | 81,542 | 0% |
| … 生成側にしか無い | 0 | 81,542 | 0% |
| 平文の完全一致 | 2,705 | 3,477 | 77.797% |
| `span.decl_kind` の一致 | 3,477 | 3,477 | **100.000%** |
| `span.impl_arg` の個数の一致 | 3,477 | 3,477 | **100.000%** |

Anchors are aligned by an LCS on anchor *text* — the weakest of the three
fields, so that an href error or an offset shift cannot break the alignment and
hide itself as a "missing" entry.

### The byte diff

| | |
|---|---:|
| ページ側の `div.decl_header` | 3,477 |
| **byte 完全一致** | **2,705** |
| 不一致 | 772 |
| … 長さは同じで空白文字だけが違う | 772 |
| … 長さが違う | 0 |
| … 長さは同じだが空白以外も違う | 0 |

So the generated markup is not merely equivalent — for 2,705 of 3,477
declarations it is the **same bytes**, and the remaining 772 differ only in
whitespace characters at unchanged length. That is why the anchor table above
can be 100% while the plaintext table is 77.8%: a same-length whitespace
substitution moves no offset.

### Why the scorer's 100% is not taken on trust

`compare.ts --self-test` injects four faults into the generator's own markup and
requires each one to land in the right bucket. 実測, 300 declarations each:

| 注入した欠陥 | 検出 / 試行 |
|---|---:|
| href を 1 個壊す | 300 / 300 |
| 平文に空白を 1 文字入れて以降の位置をずらす | 300 / 300 |
| アンカーを 1 個消す (中身は残す) | 300 / 300 |
| `decl_kind` を書き換える | 300 / 300 |

Two further cross-checks (実測):

* over all 432 modules the header path consumes **126,717** const spans and
  emits **101,641** anchors; 126,717 − 101,641 = **25,076**, exactly the number
  of nested-anchor suppressions the renderer counted independently;
* it consumes **7,354** sort spans and emits **7,354** sort anchors (no sort
  span was ever nested inside an anchor), plus **6** private-name module-link
  fallbacks. Those totals are over 432 modules; the 348 scored pages contain
  5,638 sort anchors and 6 module anchors, and all of them matched.

## The one real gap: `splitWhitespaces`

`renderTagged` (`RenderedCode.lean:249-256`) hands a `.const` tag's leading and
trailing whitespace to `splitWhitespaces` (`RenderedCode.lean:150-157` — there is
a second, identical copy at `Base.lean:281`, which is not the one on this path),
which pushes it outside the anchor **and rebuilds it as spaces**
(`"".pushn ' ' n`). The IR
stores the pretty printer's original text plus the *narrowed* span, so the tag's
full extent — the only thing that says which whitespace was inside the tag — is
gone.

Measured (実測):

| | |
|---|---:|
| 差のある宣言 | 772 / 3,477 (22.203%) |
| 差のある文字 | 1,765 |
| 差のある連続区間 | 1,765 |
| 正解の平文の総長 (UTF-16 code units) | 1,528,638 |
| 差の内訳 | `"\n"` → `" "` ×1,765、他は 0 |

Every site is a single `\n` that doc-gen4 printed as a space; the indentation
spaces after it already agree. 1,765 / 1,528,638 = **0.115%** of the header
plaintext. No offset moves, so no anchor is affected.

**Is it recoverable from schema 2? Measured: no.** `--ws-heuristic` rewrites
every whitespace run touching a childless const span into spaces. That is the
most that can be guessed without the tag extent, and it is wrong in both
directions:

| | byte 完全一致 / 3,477 |
|---|---:|
| 忠実な生成 (既定) | **2,705** |
| `--ws-heuristic` | 3,012 |

The heuristic **fixes 407 declarations and breaks 100** (実測, by set difference
of the two diff lists). It breaks them because a newline next to a const token
is sometimes *outside* the tag — `<a>Filter.Tendsto</a>\n  (fun …` keeps its
newline in doc-gen4's output — and the IR cannot tell the two cases apart.

Closing this needs the extractor to record the const tag's **untrimmed** extent
as well as the trimmed one (two extra offsets on kind-1 spans whose body was
bare text, or equivalently the `front`/`back` widths `wsTrim` already computes
and throws away). That is a schema-3 change, and it is small: `wsTrim` returns
exactly those two numbers today.

**The default renderer does not apply the heuristic.** The headline numbers
above are the faithful ones.

## What this corpus does *not* exercise

100% on 3,477 declarations is 100% on what this target contains, which is not
all of doc-gen4's header logic. Measured coverage of the scored set (実測):

* `decl_kind`: only 9 distinct strings occur — `theorem` 2,734, `noncomputable
  def` 443, `def` 194, `instance` 55, `structure` 36, `abbrev` 12, `opaque` 1,
  `noncomputable abbrev` 1, `noncomputable instance` 1. **Never exercised**:
  `class`, `class inductive`, `inductive`, `constructor`, `axiom`, `partial
  def`, and every `unsafe *` form. The composition table is transcribed for
  those branches but unverified.
* `extends`: **1 occurrence in 348 pages**. `structureInfoHeader` is verified on
  a single structure with a single parent; the `", "` separator between two
  parents is transcribed, not verified.
* the `.const` fallback chain: the direct hit fires 126,711 times and the
  private-name → module-link fallback 6 times. `findLinkableParent` fires **0
  times** and `<span class="fn">` (nothing linkable at all) fires **0 times** —
  both are transcribed, not verified.
* non-BMP offsets: 4 declarations contain `𝓧`/`𝓨`. Small, but they are the only
  case where UTF-16 units and code points disagree, and they match.
* `div.decl.sorried` / `span.decl_name[title]`: 0 occurrences on this target, so
  the fact that the IR has no `sorried` flag costs nothing *here*. It is still a
  known omission (`../stage4b/README.md`, "out of scope").

---

# Increment 4 — the whole module page, and the byte accounting

`render.ts --pages` writes one HTML file per IR module; `coverage.ts` cuts both
sides into regions and counts bytes. Everything below is **実測** under the same
conditions as increment 3 (Deno 2.7.14 / V8 14.7.173.20-rusty, macOS 26.6 arm64,
schema-2 IR of 432 modules / 4,750 declarations, doc-gen4 v4.31.0's 348 pages).
Logs are regenerable by the commands above and are not committed.

## What a page is made of, and where each rule comes from

| region | doc-gen4 source | what the IR supplies |
|---|---|---|
| `<head>` / `header` / page frame | `Output/Template.lean` `baseHtmlGenerator`, `Output/Base.lean` `baseHtmlHeadDeclarations` | the module name; every path is `getRoot` + a static file name |
| `nav.internal_nav` jump list | `Output/Module.lean` `internalNav`, `declarationToNavLink` | the rendered declarations, in page order |
| the import list | `Output/Module.lean` `importsHtml` | `imports`, **de-duplicated** then sorted by `Name.lt` |
| the imported-by list | `Output/Module.lean` `internalNav` | nothing — it is an **empty stub**, see below |
| `div.gh_link` | `Output/Base.lean` `getSourceUrl`, `Output/SourceLinker.lean` `mkGithubSourceLinker` | `line` / `endLine`; the URL prefix is **configuration** |
| `div.decl_header` | increment 3 | — |
| docstring / `div.mod_doc` | `Output/DocString.lean` `docStringToHtml` | `doc` / `moduleDocs[].text`, raw markdown |
| `ul.equations` | `Output/Definition.lean` `equationsToHtml` + `DB.lean:289` | `equations` + `equationCode`, filtered by the 200-code-point rule |
| `details.instances-for-list` / `details.instances` | `Output/Inductive.lean`, `Output/Class.lean` | the declaration name — they are **empty stubs**, see below |
| structure member table | `Output/Structure.lean` `structureToHtml` / `fieldToHtml` | `members` with `label == "ctor" / "field"`; **binders and field docstrings are missing** |
| `div.attributes` | `Output/Module.lean` `docInfoToHtml` | **nothing — not in the IR** |
| page order of `<main>` | `Process/Analyze.lean:238` `qsort ModuleMember.order` | `line` / `col`, module docstrings ahead of declarations at an equal position |

Three of these are worth stating flatly because they change what "we cannot do
this" means:

* **The instance lists are not in the page.** `instancesForToHtml` and
  `classInstancesToHtml` emit `<details>…<ul …></ul></details>` with an empty
  `<ul>`; `instances.js` fills it in the browser from the search data. So the
  instance *index* is out of scope for page bytes (it belongs to
  `declaration-data-*.bmp`, which this increment does not generate), and the
  instance *regions* reproduce exactly: **129,898 / 129,898 bytes, 686 / 686**.
* **The imported-by list is not in the page either** — same shape, filled by
  `importedBy.js`. Building the reverse import graph would produce nothing that
  goes into a page.
* **The import list is de-duplicated.** doc-gen4 reads it back out of its SQLite
  DB, which is written with `INSERT OR IGNORE` (`DB.lean:162`), so the duplicate
  `Init` that the module system puts in `moduleData.imports` disappears. The IR
  keeps the duplicate, so the consumer has to drop it: **432 duplicates dropped,
  one per module** (実測), after which all 348 import lists match byte for byte.

## The result

Every byte of all 348 pages is in exactly one region — `coverage.ts` refuses to
report unless the segments tile the file — so nothing can be quietly left out of
the denominator.

| 領域 | doc-gen4 のバイト | 比 | 再現できたバイト | 再現率 | 領域数 | 一致 |
|---|---:|---:|---:|---:|---:|---:|
| `decl_header` | 15,661,530 | 71.1% | 8,679,478 | 55.4% | 3,477 | 2,705 |
| `mod_doc` | 1,244,831 | 5.7% | 895,971 | 72.0% | 1,232 | 1,121 |
| `docstring` | 1,080,205 | 4.9% | 906,817 | 83.9% | 2,646 | 2,502 |
| `nav_links` | 976,075 | 4.4% | 976,075 | **100%** | 3,477 | 3,477 |
| `gh_link` | 724,922 | 3.3% | 624,418 | 86.1% | 3,477 | 2,980 |
| `equations` | 528,594 | 2.4% | 258,970 | 49.0% | 653 | 452 |
| `head` | 428,608 | 1.9% | 428,608 | **100%** | 348 | 348 |
| `decl_frame` | 405,937 | 1.8% | 405,937 | **100%** | 6,954 | 6,954 |
| `nav_imports` | 376,102 | 1.7% | 376,102 | **100%** | 348 | 348 |
| `header` | 166,883 | 0.8% | 166,883 | **100%** | 348 | 348 |
| `members` | 138,059 | 0.6% | 2,981 | 2.2% | 36 | 3 |
| `instances` | 129,898 | 0.6% | 129,898 | **100%** | 686 | 686 |
| `frame` | 69,717 | 0.3% | 69,717 | **100%** | 1,392 | 1,392 |
| `nav_gh` | 69,008 | 0.3% | 60,740 | 88.0% | 348 | 305 |
| `nav_top` | 13,572 | 0.1% | 13,572 | **100%** | 348 | 348 |
| `nav_frame` | 11,136 | 0.1% | 11,136 | **100%** | 696 | 696 |
| `attributes` | 3,651 | 0.0% | 0 | 0% | 89 | 0 |
| **合計** | **22,028,728** | 100.0% | **14,007,303** | **63.6%** | | |

A region counts as reproduced **only when it is byte identical**. Emitting
plausible HTML in the right place counts for nothing.

Cross-check against increment 1: `coverage.ts` recomputes
`benchmarks/results/stage4-html-inventory.txt` §C from scratch with a different
implementation, and all seven numbers come out the same — total 22,028,728,
`div.decl_header` 15,661,530, `ul.equations` 497,903, chrome 2,065,396, `head`
428,608, `header` 166,883, `nav.internal_nav` 1,445,893.

### Where the missing 36.4% goes

Every unreproduced byte is attributed to one cause, by a rule applied in a fixed
order — no per-case hand assignment, and the buckets add back up to the
denominator exactly.

| 原因 | バイト | 母数比 | 領域数 |
|---|---:|---:|---:|
| `splitWhitespaces` — IR schema 2 の既知欠落 | 7,251,676 | 32.9% | 973 |
| docstring の autolink 索引が IR に無い | 401,987 | 1.8% | 208 |
| IR に情報が無い (メンバの binder / field docstring) | 135,078 | 0.6% | 33 |
| CommonMark 未実装 | 120,261 | 0.5% | 47 |
| 設定値 (rev) — IR に無い | 108,772 | 0.5% | 540 |
| IR に情報が無い (`div.attributes`) | 3,651 | 0.0% | 89 |
| 未分類 | 0 | 0% | 0 |
| **合計** | **8,021,425** | 36.4% | |

14,007,303 + 8,021,425 = 22,028,728. **Nothing lands in "unclassified"**, which
is the check that the table is an explanation and not a bucket of leftovers.

### The number in that table that is most likely to be misread

`splitWhitespaces` is 32.9% of *all page bytes* and **1,976 characters** of
actual difference. Byte attribution is all-or-nothing: one `\n` where doc-gen4
printed a space invalidates the whole 9 KB `div.decl_header` it sits in. The two
numbers measure different things and neither is wrong:

* as *bytes that are not currently byte-identical*, it is 7,251,676;
* as *work remaining*, it is one schema-3 field (the const tag's untrimmed
  extent — see "The one real gap" above), and 1,976 characters across 973
  regions.

Do not quote the 32.9% as "a third of the page is unreproducible".

### Page-level

| | |
|---|---:|
| doc-gen4 のページ | 348 |
| **byte 完全一致したページ** | **34** |
| 差が docstring / mod_doc にしか無いページ (完全一致を含む) | 52 |
| 差が `gh_link`/`nav_gh` の rev にしか無いページ (完全一致を含む) | 48 |
| 領域に切れなかったページ | 0 |

34 / 348 is low and it is the honest number: a page is byte identical only when
*every* declaration on it, its docstrings, its equations and its source URLs all
are. The last two rows are overlapping supersets of the first and are printed to
show where the remaining pages fail, not as a projection.

The generator also writes pages for the 84 IR modules doc-gen4 never rendered —
one full run is 432 files and 29,671,173 bytes, of which 348 are scored above.
**The other 84 are not scored and no claim is made about them.**

## The two revisions

The on-disk doc tree was written in a three-second window on 2026-08-09, and its
`gh_link`s carry **two different git revisions**: 305 pages say
`573793b243fb1343636088eb62d1789ab2b14cec` and 43 say
`5e38aecd1e086aed4ec3475dcfad3df44184aca7` (実測). Same build, two revisions,
because doc-gen4's per-module source URL comes out of its incremental SQLite DB
and 43 modules' rows had been written by an earlier run against an earlier
working tree.

Consequences:

* no single `--source-url` can make all 348 pages agree. With the majority
  revision, 497 `gh_link` regions and 43 `nav_gh` regions differ, and
  `coverage.ts` verifies that **497 / 497 differ only in the 40-hex revision**
  and 0 differ in anything else;
* the 108,772 bytes are booked under "configuration", not under an IR gap. The
  IR genuinely does not have the URL — doc-gen4 gets it from lake and git — but
  the mismatch here is not the IR's fault either;
* this is a **stale incremental artefact in doc-gen4's own output**, which is
  the exact failure mode stage 5 is about (`docs/approach.md` §7, incremental
  generation and stale detection). Worth carrying forward as evidence that
  content and metadata can go stale independently: increment 3's anchor score is
  100% over all 3,477 declarations of all 348 pages, these 43 included, so their
  *signatures* agree with the current IR and only their source links are old.

## Known omissions

* **CommonMark is not implemented.** `render.ts` has a naive block/inline
  renderer (paragraphs, ATX headings, fenced code, thematic breaks, bullet and
  ordered lists, blockquotes, `` `code` ``, `[t](u)`, `**strong**`, `*em*`,
  `$math$`). doc-gen4 runs md4c, a C CommonMark implementation, through MD4Lean.
  Reproducing it byte for byte in Deno with no external libraries is out of
  scope (plan §6 pre-decision #5: no network, therefore no library). The 47
  regions that survive as genuine CommonMark differences are indented code
  blocks, loose-vs-tight list items (`<li><p>` vs `<li>`), lazy continuation,
  and nested lists by indentation. **No agreement is claimed for the docstring
  renderer**; the 83.9% / 72.0% above is a measurement, not a target, and the
  bytes it misses are counted as missed.
* **The docstring autolink index is not in the IR.** doc-gen4 resolves a name
  inside `<code>` against the whole environment; the IR's name map is
  declarations + `deps/*.json` + every constant a signature referred to.
  Increment 1 bounded the gap at 293 / 5,044 docstring anchors
  (`stage4-html-inventory.txt` §F); here it costs 208 regions / 401,987 bytes.
  Closing it needs a name → module index for the environment, which is a
  different artefact from the per-module IR.
* **`div.attributes` is not in the IR** (`../stage4b/README.md`, "out of
  scope"): 89 regions, 3,651 bytes, emitted as nothing at all.
* **Structure members are incomplete.** The IR stores a member's name, printed
  type and spans, but not the binders of its signature (the **940 constant
  occurrences with no position** in `../stage4b/README.md`) and not the field
  docstring. doc-gen4's 348 pages contain 153 `div.structure_field_info`, of
  which 48 print a `span.decl_args` and 114 carry a `div.structure_field_doc`
  (実測). `f.isDirect` is missing too, so the 4 `inherited_field` items are
  rendered as direct ones. 33 of 36 member tables therefore differ, all of them
  in length.
* **`div.decl.sorried`** — 0 occurrences on this target, still an IR omission.
* `href="#top"` is emitted, dead link and all, because the goal here is byte
  agreement with doc-gen4 (実測: 348 occurrences, 0 targets). A product would not
  copy it.
* Nothing outside the module pages is generated: no `declaration-data-*.bmp`, no
  `backrefs-*.json`, no `search.html`, `index.html`, `navbar.html`,
  `foundational_types.html` or static assets. The instance and search indices
  live in those files, not in the pages.

## Pitfalls

**A generator that is allowed to see the answer is not a generator.**
`render.ts` never opens `decl-header-truth.jsonl`, and `compare.ts` never writes
markup. Keep it that way; the previous increment's 100% precision came from
scoring multisets, and the cheapest way to get a fake 100% here is to let the
two halves share code.

**Do not fix the score by loosening the comparison.** The 772 whitespace
declarations are a real IR gap and are reported as such. `--ws-heuristic` is
kept because it *measures* the gap (407 fixed, 100 broken), not because it
improves the headline.

**Offsets are UTF-16 code units.** JavaScript string indices are the same unit,
which is why `render.ts` can slice the IR's text directly — but `[...text]`
(code points) is not, and using it anywhere in the span path silently shifts 4
declarations by up to 16 units. `applyWsHeuristic` builds a per-code-unit array
for exactly this reason.

**One literal NUL byte in a template string cost a debugging round.** It made
the ground-truth map key differ from the self-test's key, so the self-test
silently attempted 0 mutations and reported FAIL. A self-test that reports "0
attempted" is a failing self-test, not a passing one — `--self-test` treats
`attempted == 0` as FAIL for that reason.

**`--pages` must not be able to move increment 3's numbers.** `render.ts` keeps
two counter blocks and a `sink` pointer: the header path always counts into
`stats`, the page path into `pageStats`. Verified (実測): the `--out` JSONL is
byte identical with and without `--pages`, the `## decl_header path` block of
`--stats` is character identical, and two `--pages` runs into two directories
are identical under `diff -r`. Re-run those three whenever `render.ts` changes.

**Two units of "length" are live at once, and they are not the same.** Span
offsets are UTF-16 code units; `RenderedCode.textLength`, which decides the
200-limit on equations, counts Lean `Char`s, i.e. **code points**
(`RenderedCode.lean:109`). `equationsHtml` uses `[...s].length` for the limit and
plain indices for the spans, deliberately. Swapping them changes which equations
are dropped on any declaration containing `ℝ`, which is most of them.

**JSX whitespace is eaten, `Html.text " "` is not.** `<span> {x} </span>` in
doc-gen4's source produces no spaces (Lean's tokenizer drops them before the
parser sees them), which is why `internalNav`'s `<ul …> </ul>` comes out as
`<ul …></ul>`. But `structureInfoHeader`'s `#[Html.text " "]` is a real text node
and does survive. Reading the source without this rule produces a page that is
wrong by one byte in a dozen places.

**Do not add regions to the accounting without keeping the tiling check.**
`coverage.ts` throws if the segments leave a gap or overlap, and the report
prints the sum next to the true file size. That check is the only thing standing
between this table and a denominator that quietly shrinks to whatever is
convenient. It has already caught one real bug — `div.attributes`'s trailing
newline was being counted twice, in 42 pages.

## For the timing task

**Nothing in this directory has been timed and no timing claim is made.** What
the next task needs to know before it starts:

* **What is being timed is not what doc-gen4 does.** This generator writes 432
  module pages (29,671,173 bytes) and nothing else. doc-gen4's run also writes
  `declaration-data-*.bmp` and `backrefs-*.json` per module, plus the site-wide
  `search.html`, `index.html`, `navbar.html`, `foundational_types.html`,
  `declaration-data.bmp` and the static assets, and it builds and reads a SQLite
  DB. Comparing wall clocks without saying so is the "1,251×" mistake
  `CLAUDE.md` warns about: state the denominator, and prefer "same work" over
  "whole site".
* **This generator is not feature-complete**, and the two heaviest missing
  pieces are not free. CommonMark and the docstring autolink index would both go
  on the hot path. A time measured now is a **lower bound on the consumer side**,
  and must be labelled as one.
* **`render.ts` reads the whole IR into memory before rendering** (432 module
  files plus `deps/`, then builds the global name map). That is one design among
  several, not a measured optimum. Time the read and the render separately —
  `benchmarks/tools/read-ir.ts` already measures the read alone (schema-2 median
  0.100 s, `docs/verification-log.md`), so the two are comparable.
* **Deno start-up is in the wall clock.** For a run this short it is not
  negligible; measure it separately (`deno run` on an empty script) rather than
  subtracting a guess.
* **Page cache.** The IR is ~15.9 MB and the doc tree ~22 MB; both fit in RAM
  easily, so the cold/warm spread here is nothing like the olean mmap case. Still
  measure both, and still take five runs or more — `CLAUDE.md` is not asking for
  a ritual, it is asking because this project has already been bitten once.
* **Do not write into `/Users/haruka/dev/lean-projects`.** `--pages` goes to a
  scratch directory; the doc tree is opened read-only, and
  `.lake/packages/doc-gen4` carries the instrumentation patch.
