# Where these fixtures came from

JSON cannot carry a comment, so the attribution for the generated fixtures in
this directory lives here.

| file | produced by | whose output it is |
|---|---|---|
| `docgen4-expected.json` | `tests/oracle/gen-docgen4-expected.ts` → `tests/oracle/dump-html.lean` | **doc-gen4's** `docStringToHtml` |
| `md4lean-expected.json` | `tests/oracle/gen-md4lean-expected.ts` → `tests/oracle/dump-ast.lean` | **MD4Lean's** `MD4Lean.parse` |
| `ts-docstring-expected.json` | `tests/oracle/gen-ts-docstring-expected.ts` (**removed** — see below) | `experiments/stage7d/render.ts` (this repository, frozen) |
| `fuzz/*.md` | written by hand for `tests/fuzz_corpus.rs` | **ours** — no third party, no oracle |

`fuzz/` is not a fixture in the sense of the rows above: nothing in it is
compared against an expected output. Each file is an *input* chosen because it
is known or suspected to be dangerous — the NUL-in-a-fenced-block and the
body-less GFM table that kill MD4Lean (`docs/implementation-plan.md` §7), plus
deep nesting, unterminated constructs, astral characters, a 200 KB line, entity
edge cases, CR without LF, and the empty string. Adding a file adds a case.

`docgen4-expected.json` is the output of a program licensed under the Apache
License, Version 2.0:

    Copyright (c) 2021 Henrik Böving. All rights reserved.
    Released under Apache 2.0 license as described in the file LICENSE.
    Authors: Henrik Böving

`md4lean-expected.json` is the output of MD4Lean, MIT, Copyright (c) 2024 Jz Pan.

Both are acceptance oracles: the point is that our renderer produces the same
bytes, so these are regenerated from the pinned upstream rather than edited.
See this repository's NOTICE and `docs/provenance.md`.

`docgen4-expected.json` and `md4lean-expected.json` can still be regenerated —
their oracles are doc-gen4 and MD4Lean, both of which this tree can still reach.

**`ts-docstring-expected.json` cannot.** `experiments/` was removed from HEAD on
2026-08-16 and `gen-ts-docstring-expected.ts` went with it, so that file is now a
frozen value. Both the prototype and the generator are at tag
`experiments-frozen`:

    git show experiments-frozen:experiments/stage7d/render.ts
    git show experiments-frozen:crates/lean-doc-md/tests/oracle/gen-ts-docstring-expected.ts
