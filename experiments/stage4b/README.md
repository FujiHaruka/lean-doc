# experiments/stage4b — positional tagged code in the IR

Verification stage 4 of [`docs/approach.md`](../../docs/approach.md) §7,
increment 2: *the IR has to say **where** in a printed signature each tag sits,
not just which constants the signature mentions.*

This directory is a copy of [`../stage4`](../stage4) plus `--tagged-code`.
`../stage1` … `../stage4` are frozen so their numbers stay reproducible — in
particular stage 4's `writeIR` 0.198 s and 8.34 MiB IR must still come out of
that tree. Everything not described below behaves exactly as in stage 4: same
blacklist, same signature path, same `Core.Context` options, same JSONL record
shape, same IR layout. Read [`../stage4/README.md`](../stage4/README.md) and the
stage-3 and stage-2 READMEs behind it for those.

## Why this exists

Stage 4 increment 1 (see `docs/verification-log.md` and
`../../benchmarks/results/stage4-html-inventory.txt`) measured that stage 4's IR
cannot rebuild doc-gen4's signature HTML:

* **44.0%** of doc-gen4's 72,421 signature anchors have no textual relation
  between the printed token and the constant behind it — `ℕ`→`Nat`, `≤`→`LE.le`,
  `{`/`}`→`Singleton.singleton`;
* rebuilding the links from plain text plus a *set* of `(module, name)` pairs
  tops out at **56% recall** (51.7% measured; 4.0% of declarations exactly right);
* **71.1%** of the rendered bytes of a doc-gen4 page are the signature.

`../stage4/README.md` wrote the branch down before it was taken ("Positional
rendered code … the set is not enough — measured"), so this is that branch, not a
regression.

## What is new

| flag | default | what it does |
|---|---|---|
| `--tagged-code` | off | record tag **positions** as well as names, and add the declaration-range end / kind modifiers / in-module index to the IR |

One flag, four additions, because all four are "what increment 1 found the IR
missing" and the three small ones are free (they read fields the extractor
already had in hand). With the flag off this binary is stage 4, byte for byte —
see "Baseline identity".

`--tagged-code` bumps the IR **schema version to 2**. Schema 1 is stage 4's and
is what the flag-off run still writes; since `schemaVersion` sits inside every
module file it is inside every content hash, so a consumer cannot mistake one for
the other.

## The span list

For every printed fragment — one binder, the result type, one equation, one
structure member — the IR carries a **flat pre-order list of spans** over *that
fragment's* plain text:

```json
"type": "DotEq a a",
"typeCode": [[0,9,0], [0,5,1,"InformationTheory.Asymptotic.DotEq"], [6,7,0], [8,9,0]]
```

* an entry is `[start, stop, kind]`, or `[start, stop, 1, name]` when `kind` is 1;
* `start`/`stop` are half-open offsets into the fragment's plain text
  (**UTF-16 code units** — see "Offsets");
* the plain text (`binders` / `type` / `equations` / `members[].text`) **stays**.
  It is redundant with the spans, and that is the point: a consumer can check one
  against the other, and a consumer that does not want links can ignore the spans
  entirely.

Flat rather than a tree because (a) it is smaller, (b) the consumer language is
undecided (§5.6) and every language can walk an array, (c) the tree is
recoverable — see "Nesting".

### Kinds

`kind` is exactly the three `RenderedCode.Tag`s that become an HTML element.
doc-gen4 builds `RenderedCode` in `renderTagged`
(`DocGen4/RenderedCode.lean:240-274`) and turns it into HTML in
`renderedCodeToHtmlAux` (`DocGen4/Output/Base.lean:327-389`):

| `kind` | `RenderedCode.Tag` | built at | rendered at | element |
|---:|---|---|---|---|
| 0 | `.otherExpr` | `RenderedCode.lean:272-273` | `Base.lean:387` | `<span class="fn">` |
| 1 | `.const name` | `RenderedCode.lean:249-256` | `Base.lean:337-373` | `<a href="…#name">`, falling back to `<span class="fn">` when the name is not linkable |
| 2 | `.sort _` | `RenderedCode.lean:257-271` | `Base.lean:374-381` | `<a href="…foundational_types.html">` |

Two tags are deliberately **absent**: `.keyword` and `.string`
(`RenderedCode.lean:207-234`, produced by `tokenize` over untagged text). They
render as bare content — `Base.lean:385-386` returns the inner HTML unchanged —
so they produce no element and no bytes. If doc-gen4 ever styles them, they have
to be added here.

The kind of a tag is decided the way `renderTagged` decides it:
`Elab.Info.ofTermInfo` whose expression is `.const c _` → 1, `.sort _` → 2,
anything else → 0; any other `Elab.Info` → 0. This reproduces doc-gen4 including
its blind spot: `.ofFieldInfo` and `.ofDelabTermInfo` carry constants but
`renderTagged` does not match them, so they are not links in its output and are
not kind 1 here.

A tag position that carries no `Elab.Info` produces **no span**: Lean's
`Widget.tagCodeInfos` (`Lean/Widget/InteractiveCode.lean`, the `none` branch)
drops the tag before `renderTagged` ever sees it. Its subtree is still walked.

Two spans are **narrower than the tag** because doc-gen4 narrows them:

* a `.const` tag over bare text has its surrounding whitespace pushed outside the
  anchor (`splitWhitespaces`, `RenderedCode.lean:150-157`) — this is what makes
  the span for ` ↔ ` be `↔` and not ` ↔ `;
* a `.sort` tag over bare text covers only the word before the first space
  (`RenderedCode.lean:258-269`), so `Type u_1` links `Type` and not `u_1`.

### Nesting

**Pre-order: parent before child, outer before inner at equal offsets.** That
ordering *is* the nesting rule; nothing else in the format encodes structure. A
consumer replays the list on a stack — pop while `start ≥ top.stop`, then the new
span is a child of whatever is left on top — and gets the tree back. Measured on
the full target: 408,804 spans, every one properly nested, maximum depth 40
(`check-spans.ts`).

Nesting is not decoration. 81,055 of the 142,181 `kind = 1` spans sit inside
another `kind = 1` span, and `renderedCodeToHtmlAux` suppresses the **outer**
anchor in that case (`Base.lean:342-345`: when the subtree already produced an
`<a>`, the enclosing tag returns it unwrapped). A consumer that ignores the
nesting emits invalid nested anchors for more than half of them.

### Offsets

Offsets are **UTF-16 code units**, not characters and not UTF-8 bytes.

The reason is the consumer, not Lean: the reader is a `String`-slicing runtime
(the stage-4 read side is `../../benchmarks/tools/read-ir.ts`), and UTF-16 units
are what `String.prototype.slice` indexes. Any other unit would put a conversion
pass in front of every fragment on the hot path. The extractor pays nothing for
the choice — it counts whatever unit it is asked to count in the same walk. Lean's
own `String.length` is *code points*, so nothing in `Extract.lean` may use it;
`utf16Len` is the only length function in the tagged path.

**This is a choice, and it is the consumer's to revisit.** If §5.6 lands on a
language where UTF-8 byte offsets are natural, changing the unit is a one-line
change in `charUtf16` plus a schema bump.

Two things keep the offsets honest:

* **In the extractor**: `RefSink.collect` throws unless the width the span walk
  accumulated equals the UTF-16 width of the printed text it will store. This ran
  over every fragment of all 432 modules (55,465 stored plus the member binders
  that are walked and dropped) without firing. It is also what keeps the walk
  inside its timer — see "Pitfalls".
* **In the consumer**: [`check-spans.ts`](check-spans.ts) re-slices every
  fragment with every span and checks bounds, nesting and whitespace trimming.

The round trip is tested on **real non-BMP data**, which is the only case where
UTF-16 units and code points disagree. The target uses `𝓧` (31), `𝓨` (22) and
`𝕜` (6) — 41 fragments where the two units differ, 105 spans whose slice crosses
one. Worked example, from
`InformationTheory.Shannon.CondKLIntegral#InformationTheory.klDiv_compProd_const_toReal_integral`:

```
text     "{𝓧 : Type u_1}"        UTF-16 width 15, 14 code points
[1,3,0]  UTF-16 -> "𝓧"          code points -> "𝓧 "     ← wrong
[6,10,2] UTF-16 -> "Type"        code points -> "ype "    ← wrong
```

Non-ASCII is not a corner case here: 81.97% of fragments contain it, and 34.06%
of the const spans slice out a non-ASCII token.

## The other three additions

All three are behind the same flag, and all three come from increment 1's list of
things doc-gen4's HTML needs and stage 4's IR did not have.

### Declaration range end

`endLine` / `endCol`, from `DeclarationRanges.range.endPos` — the same range
doc-gen4 stores (`DocGen4/Process/NameInfo.lean:124`) and feeds to
`mkGithubSourceLinker` (`DocGen4/Output/SourceLinker.lean:12-14`) as
`#L{pos.line}-L{endPos.line}`. Stage 4 kept only the start, so the IR could not
produce a `gh_link` at all; increment 1 measured the start line already agreeing
3,477/3,477. 4,750 declarations now carry both ends (206 of them are one-liners
where `endLine == line`).

### Kind modifiers

`"modifiers"` is the list of words doc-gen4's `getKindDescription`
(`DocGen4/Process/DocInfo.lean:211-247`) puts in front of the kind word. The IR's
`kind` alone is not `span.decl_kind`: on the full target **611 of 842
definitions (72.6%)** and **1 of 91 instances** carry at least one modifier.

Composition rule, which the consumer has to reapply:

| `kind` | `span.decl_kind` |
|---|---|
| `definition` | `unsafe`? `noncomputable`? then `abbrev` if present else `def` |
| `instance` | `unsafe`? `noncomputable`? then `instance` |
| `axiom` | `unsafe`? then `axiom` |
| `opaque` | `partial def` if `partial`; else `unsafe opaque` if `unsafe`; else `opaque` |
| `inductive` | `unsafe`? then `inductive` |
| anything else | the kind word alone |

Where each flag comes from in doc-gen4, and the three places it is deliberately
*not* emitted:

* `unsafe` / `noncomputable` / `abbrev` — `DefinitionInfo.ofDefinitionVal`
  (`Process/DefinitionInfo.lean:41-60`): `v.safety`, `isNoncomputable env v.name`,
  `v.hints.isAbbrev`;
* `partial` — `OpaqueInfo.ofOpaqueVal` (`Process/OpaqueInfo.lean:15-29`): the
  existence of `Compiler.mkUnsafeRecName v.name`, and it **wins over** `unsafe`;
* **nothing for a theorem**, even one that is an instance:
  `InstanceInfo.ofTheoremVal` (`Process/InstanceInfo.lean:66-85`) hard-codes
  `isUnsafe := false` and `isNonComputable := false`;
* **no `abbrev` for an instance**: `getKindDescription`'s `instanceInfo` branch
  never looks at `hints`;
* **no `unsafe` for `structure` / `class` / `class inductive`**: those branches
  ignore `isUnsafe`, and `.quotInfo` becomes an `opaque` with
  `definitionSafety := .safe`.

Observed on the target: `noncomputable` 549, `abbrev` 59, both 4.

### In-module index

`"index"` is the declaration's position in the order the extractor enumerated its
module (`moduleData.constNames`, blacklisted names dropped) — i.e. its position
in the `declarations` array, made explicit so a consumer that reorders can get
back. Increment 1 found 2 modules / 4 declarations whose `(line, col)` are equal,
so page order is not recoverable from the range alone. Reproduced here: 2 modules
(`InformationTheory.Polymatroid.Basic`,
`InformationTheory.Shannon.EntropyPower.Ext`), 4 declarations.

## What is tagged, and what is not

| fragment | tagged | field |
|---|---|---|
| each binder of a declaration | yes | `binderCode[i]` |
| the result type | yes | `typeCode` |
| each equation | **yes** | `equationCode[i]` |
| each structure member (ctor / parent / field) | **yes** | `members[i].code` |
| the binders of a member's signature | **no — not stored at all** | — |

Equations and members were the open question in the task; both work, because both
already go through a path that has the `Format` and the `infos` side by side
(`ppTermTagged` / `ppSignature`). 863 equation fragments carry 30,600 spans; 194
member fragments carry 2,235.

The one gap is inherited from stage 4's record shape, not from the tagging.
`structureMembers` / `inductiveMembers` pretty-print a *full* signature for each
ctor and field and then keep only `sig.type` as `Member.text`; the binders in
front of the `:` are discarded, so there is no text for their spans to index.
**Measured, not estimated**: with `--refs --tagged-code` the walk sees 143,121
constant occurrences and the IR stores 142,181 const spans, so **940 (0.66%)
constant occurrences have no position**. Re-running with `--only` restricted to
the 37 structures reproduces the gap exactly (1,884 occurrences, 944 stored
spans, difference 940), which pins all of it on the member binders. Increment 1
counted 153 `structure_field` anchors in doc-gen4's HTML, so the visible part of
this is small — but closing it means storing the member binders too, which is a
record-shape change, not a tagging change.

`equations` themselves are complete: increment 1 counted 3,278 equation anchors
across 348 pages and every equation fragment here carries its spans.

Still deliberately **out of scope** (unchanged from stage 4, fixed by increment
1): attributes, instance `className`/`typeNames`, `sorried`, structure-field
binders / docstrings / `isDirect`, and the docstring autolink index.

## Build and run

```sh
./build.sh
./run.sh stage4b-tag -- --equations --refs --write-ir --tagged-code --ir-dir "$IR_DIR"
deno run --allow-read ./check-spans.ts --ir "$IR_DIR"
```

Same constraints as stages 2-4 (no toolchain and no lakefile here, `lake env`
borrows the measurement target's, `--root` because the source is outside that
repository, `leanc -rdynamic` for the interpreted module initializers). Never
measure with `lean --run`. `--ir-dir` must stay outside
`/Users/haruka/dev/lean-projects`.

For smoke runs, set `MODULES` to a short list and `RESULTS_DIR` to a scratch
directory so nothing lands next to the committed measurements.

The JSONL phase names are `stage4b.*`, not `stage4.*`, so a log from this binary
can never be pooled with a stage-4 log by accident.

## What one full run produces

**No timing conclusions here — that is the next increment.** These are counts
from a **single** run of all 432 target modules with
`--equations --refs --write-ir --tagged-code`; the wall times are quoted only as
one-off reference values and no median, spread or comparison is claimed.

Conditions: Apple M1 / 8 cores / 16 GB, Lean / Mathlib / doc-gen4 all v4.31.0,
`LEAN_NUM_THREADS` unset, oleans built, one process, warm. Log:
`<scratch>/results/d1-tag.jsonl` + `-summary.txt` (not committed; regenerable).

| | schema 1 (`--tagged-code` off) | schema 2 (on) | ratio |
|---|---:|---:|---:|
| module bytes | 8,627,735 | 15,751,310 | ×1.826 |
| dependency map bytes | 27,208 | 27,208 | ×1 |
| index bytes | 88,453 | 88,541 | ×1.001 |
| **total** | **8,743,396** | **15,867,059** | **×1.815** |
| files | 436 | 436 | |
| declarations | 4,750 | 4,750 | |

Against stage 4's own IR (8,743,395 bytes, `../stage4/README.md`) the tagged IR
is **×1.815**. The one-byte difference between stage 4 and this binary's schema-1
output is the `generator` string (`…/stage4` → `…/stage4b`) and nothing else.

Spans written:

| | |
|---|---:|
| fragments carrying a span list | 55,465 |
| **spans** | **408,804** |
| of which kind 1 (const) | 142,181 |
| of which kind 2 (sort) | 7,550 |
| of which kind 0 (other) | 259,073 |
| const spans nested inside another const span | 81,055 |
| maximum nesting depth | 40 |

Sanity check against increment 1's HTML inventory. **The denominators differ** —
that counted elements on 348 rendered pages, this counts spans over 432 modules
including equations and members, before doc-gen4's nested-anchor suppression — so
these are ratios to eyeball, not an equality to check:

| | increment 1, 348 pages | stage 4b, 432 modules | ratio |
|---|---:|---:|---:|
| sort links | 5,638 | 7,550 | 1.34 |
| `span.fn` | 216,838 | 259,073 | 1.19 |
| signature anchors | 72,421 | 142,181 const spans | 1.96 |

The first two land near where a 432/348 = 1.24 scaling puts them. The third does
not, and the nesting is why: 81,055 const spans sit inside another one and
doc-gen4 emits no anchor for those. Turning span counts into element counts is
the next increment's job, not a claim made here.

`refOccurrences` is **143,121**, identical to stage 4's, because with
`--tagged-code` on the names are read off the spans instead of being collected by
a second walk — so this is a cross-check that the two walks see the same tags,
not a coincidence. 143,121 − 142,181 = the 940 member-binder occurrences above.

One-off wall times from that single run, for scale only: `total` 13.69 s,
`analyze` 10.47 s, `writeIR` 0.547 s (0.488 serialize + 0.003 hash + 0.052
write). The flag-off run in the same session: `total` 14.01 s, `analyze` 10.00 s,
`writeIR` 0.207 s. **Do not read a cost out of these** — n=1, and stage 4's
README already documents `total` moving by up to 1.0 s between orderings for
reasons that have nothing to do with the writer.

## Baseline identity

With `--tagged-code` off this binary must still be stage 4, and with it on it must
only *add*. Re-run all four whenever `Extract.lean` changes — that is what keeps
these numbers comparable with stages 2, 3 and 4:

```sh
./baseline-identity.sh "$WORK" [<an existing stage-4 IR to compare against>]
```

| | result |
|---|---|
| **1. `--dump` / `--dump-modules` / `--dump-refs` == stage 4**, 10-module smoke list, both with `--refs` and without | **PASS** (6 comparisons) |
| **2. IR == stage 4's IR**, 432 modules, `--equations --refs --write-ir` | **PASS** — `modules/` and `deps/` byte-identical under `diff -r`; `index.json` differs in exactly one field, `generator`, and is byte-identical once that string is renamed |
| **3. `--tagged-code` is purely additive** — with it on, `--dump` / `--dump-modules` / `--dump-refs` are byte-identical to the run with it off, both with `--refs` and without | **PASS** (6 comparisons) |
| **4. the tagged IR is deterministic** — two runs into two directories | **PASS** under `diff -r` |

Check 3 is stronger than it looks: `--dump` contains the `refs` array, so it is
what proves that reading the names off the spans yields exactly the sequence
`collectConsts` produced — same names, same order, same duplicates.

The 10-module smoke list is stage 4's, unchanged.

## Pitfalls

**Everything stage 4 lists still applies** — read `../stage4/README.md`
"Pitfalls" first, in particular the three separate times a phase timer read low
because a pure `let` had no consumer before the next clock read, **all three in
the flattering direction**.

That trap is why `RefSink.collect` checks the accumulated width against the
printed width instead of just building the array: the check is a branch the
compiler cannot drop, so the walk cannot be sunk past `t1`. The check is
therefore *inside* the walk's timer, and it is one extra linear scan of the
fragment per fragment. When the next increment times the tagged walk against the
plain one, that scan is part of what it is timing; do not subtract it silently,
and do not remove it to make the number nicer.

**`refUs` measures a different walk depending on the flag.** With
`--tagged-code` off it is stage 4's name-only walk; with it on it is the span
walk (whose result the names are read off). Same timer, same position, different
work — which is exactly the comparison the next increment wants, but it means the
two numbers must never be pooled. The `stage4b.analyze` record carries
`taggedCode` so a log always says which one it is.

**`--tagged-code` changes which pretty printer runs, even without `--refs`.**
`ppTermTagged` needs `ppExprWithInfos`, so the sink is created when *either* flag
is set, and `Meta.ppExpr` is only used when both are off. The printed text is the
same either way (`Meta.ppExpr` is `ppExprWithInfos` with the map dropped) and
check 3 above is what verifies it.

**Whitespace inside a trimmed const span is not byte-faithful.**
`splitWhitespaces` rebuilds the whitespace it trimmed as *spaces*
(`"".pushn ' ' n`), so a `.const` tag whose bare text was broken across a line
renders `"\n   "` as `"    "` in doc-gen4's HTML while the IR keeps the newline.
The character count is the same, so no offset moves; only the character differs.
Increment 1's 99.992% href agreement bounds how often this can matter, but a
byte-for-byte HTML diff in a later increment will see it.
