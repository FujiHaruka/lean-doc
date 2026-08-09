# experiments/stage4c — `div.decl_header` rebuilt from the IR, without Lean

Verification stage 4 of [`docs/approach.md`](../../docs/approach.md) §7,
increment 3, and the core of **judgement point 2**: *can the signature HTML be
produced from the IR alone, with Lean never started?*

This directory answers that for **`div.decl_header` only** — the kind word, the
declaration name, the binders, `extends`, and the result type. The rest of a
declaration's HTML (docstring, equations, instance lists, `gh_link`, page
chrome) is the next increment's job. **Nothing here is timed**; increment 4
does that.

Everything is Deno/TypeScript because §6 pre-decision #1 says the throwaway
consumer-side tooling is Deno/TS. **This is not the implementation-language
decision for the product** (§5.6 is still open).

## What is here

| | |
|---|---|
| `render.ts` | reads the schema-2 IR, writes one `div.decl_header` per declaration as JSONL. Never reads the ground truth. |
| `compare.ts` | scores the generated markup against `decl-header-truth.jsonl` as **ordered anchor sequences**. Has its own HTML extractor, and a `--self-test` that injects faults to prove it can report a failure. |
| `bytes.ts` | diffs the generated markup **byte for byte** against the `div.decl_header` substrings of doc-gen4's own pages. |

Three programs rather than one because each is a different kind of evidence and
a bug in one must not be able to hide behind another:

* `compare.ts` measures what the task asks for — the `(offset, text, href)`
  sequence — but both it and the ground-truth extractor turn markup into
  sequences, so a *shared* misreading of the markup would cancel out;
* `bytes.ts` does no extraction at all, so it cannot share that bug. It also
  sees everything `compare.ts` is designed to ignore (attribute order, `class`
  values, elements with no text);
* `render.ts` is kept blind to the answer so that "matching" cannot become
  "copying".

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
```

`--modules-from-mine` exists only for the small loop; the full run must not use
it, because the denominator has to stay 3,477.

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

## Known omissions

* Only `div.decl_header`. `gh_link`, docstrings, `ul.equations`, instance
  lists, `nav.internal_nav` and the page frame are the next increment.
* Attributes (`div.attributes`) are not in the IR at all — they sit *outside*
  `decl_header`, so they do not affect these numbers, but a full page needs
  them.
* The **940 constant occurrences with no position** (`../stage4b/README.md`,
  "What is tagged, and what is not") are the binders of structure *members'*
  signatures. `decl_header` never prints those — it prints the member's parent
  type via `label == "parent"`, which is complete. So the gap costs **0 anchors
  here**, and is only a debt for the member tables the next increment renders.
* `href="#top"` appears once per page in doc-gen4's output but **never inside
  `decl_header`** (実測: 348 occurrences, 0 in a header), so the "do not imitate
  this dead link" question does not arise at this scope.

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
