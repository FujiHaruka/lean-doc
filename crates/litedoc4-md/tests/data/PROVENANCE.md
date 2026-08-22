# Where these fixtures came from

JSON cannot carry a comment, so the attribution for the generated fixtures in
this directory lives here.

| file | produced by | whose output it is |
|---|---|---|
| `docgen4-expected.json` | `LITEDOC4_BLESS=1 cargo test -p litedoc4-md --test docgen4` | **ours**, since 2026-08-22 — it was doc-gen4's until then |
| `md4lean-expected.json` | `tests/oracle/gen-md4lean-expected.ts` → `tests/oracle/dump-ast.lean` | **MD4Lean's** `MD4Lean.parse` |
| `ts-docstring-expected.json` | `tests/oracle/gen-ts-docstring-expected.ts` (**removed** — see below) | `experiments/stage7d/render.ts` (this repository, frozen) |
| `fuzz/*.md` | written by hand for `tests/fuzz_corpus.rs` | **ours** — no third party, no oracle |

`fuzz/` is not a fixture in the sense of the rows above: nothing in it is
compared against an expected output. Each file is an *input* chosen because it
is known or suspected to be dangerous — the NUL-in-a-fenced-block and the
body-less GFM table that kill MD4Lean (`docs/implementation-plan.md` §7), plus
deep nesting, unterminated constructs, astral characters, a 200 KB line, entity
edge cases, CR without LF, and the empty string. Adding a file adds a case.

## `docgen4-expected.json` changed sides on 2026-08-22

It held **doc-gen4's** `docStringToHtml` output, produced by
`tests/oracle/gen-docgen4-expected.ts` → `tests/oracle/dump-html.lean`, and 327
of its cases were compared against it byte for byte. Feature-sweep C-1
(`docs/plans/feature-sweep.md`) converts `$…$` to MathML while the page is
written, which doc-gen4 does not do, so **five of the 327 could never agree with
it again**. The role of the file was switched once, deliberately
(`docs/plans/feature-sweep.md` §3【決定 1】/ C-4): it is now **this crate's own
output**, and the test that reads it is named `every_case_matches_the_frozen_output`
rather than `…_doc_gen4`.

**What that costs is stated rather than hidden.** `cargo test` no longer says
"the CommonMark dialect did not move" — it says "the output did not move".
The doc-gen4 oracle is still here and still runs:

    deno run --allow-read --allow-write --allow-run --allow-env \
      crates/litedoc4-md/tests/oracle/gen-docgen4-expected.ts

so the dialect claim can be re-checked deliberately. It is simply not what a
push is judged by. The five cases that diverge are the math ones: inline math,
display math, math with markdown inside, escapes in math, and a table whose cell
holds math.

**Regenerate rather than edit.** `LITEDOC4_BLESS=1` rewrites the file from this
crate's output and prints every case it changed; it refuses to run if
re-serialising the file does not reproduce it byte for byte, so a regeneration
can never turn into a whole-file rewrite nobody can review.

The bytes it *used to* hold are the output of a program licensed under the Apache
License, Version 2.0:

    Copyright (c) 2021 Henrik Böving. All rights reserved.
    Released under Apache 2.0 license as described in the file LICENSE.
    Authors: Henrik Böving

`md4lean-expected.json` is the output of MD4Lean, MIT, Copyright (c) 2024 Jz Pan,
and **is still an acceptance oracle**: the point of it is that this crate's
parser produces the same tree, so it is regenerated from the pinned upstream
rather than edited. See this repository's NOTICE and `docs/provenance.md`.

`md4lean-expected.json` can still be regenerated from its oracle, MD4Lean, which
this tree can still reach; it is a **parse** oracle and nothing in feature-sweep
C moved the parser, so it did not change. `docgen4-expected.json`'s oracle is
reachable too — see the section above for why it is no longer what the file
holds.

**`ts-docstring-expected.json` cannot.** `experiments/` was removed from HEAD on
2026-08-16 and `gen-ts-docstring-expected.ts` went with it, so that file is now a
frozen value. It records *which* docstrings the prototype and doc-gen4 disagreed
on and what the prototype answered; **it did not change on 2026-08-22** — the one
case of it that math touches is a disagreement either way. What did change is
what it is compared against: `the_prototype_is_the_one_that_differs` now checks
this crate's output against the frozen output above rather than against
doc-gen4's, and still fails if the output ever becomes the prototype's.
Both the prototype and the generator are at tag `experiments-frozen`:

    git show experiments-frozen:experiments/stage7d/render.ts
    git show experiments-frozen:crates/lean-doc-md/tests/oracle/gen-ts-docstring-expected.ts
