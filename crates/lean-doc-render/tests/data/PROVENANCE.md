# Where these fixtures came from

JSON cannot carry a comment, so the attribution for the generated fixtures in
this directory lives here.

`docgen4-linked-expected.json` is the output of **doc-gen4's** `docStringToHtml`
with link resolution, produced by `tests/oracle/dump-html-linked.lean`
(`import DocGen4.Output.DocString`). doc-gen4 is licensed under the Apache
License, Version 2.0:

    Copyright (c) 2021 Henrik Böving. All rights reserved.
    Released under Apache 2.0 license as described in the file LICENSE.
    Authors: Henrik Böving

Every other file here is the output of this repository's own code — the frozen
prototype `experiments/stage7d/render.ts` (`ts-expected.json`,
`autolink-expected.json`, `fragment-expected.json`, `page-parts-expected.json`,
`pages-expected.json`).

These are acceptance oracles: the point is that the Rust renderer produces the
same bytes, so they are regenerated from the pinned upstream rather than edited.
See this repository's NOTICE and `docs/provenance.md`.

## There is no regenerator in this tree any more

`experiments/` was removed from HEAD on 2026-08-16, and the `gen-*-expected.ts`
scripts that drove the prototype went with it. **These files are now frozen
values.** Both the prototype and its generators are at tag `experiments-frozen`:

    git show experiments-frozen:experiments/stage7d/render.ts
    git show experiments-frozen:crates/lean-doc-render/tests/gen-ts-expected.ts

`docgen4-linked-expected.json` is the one to watch: its expected values come from
doc-gen4, but *which cases it contains* was decided by the prototype, so its
generator went too. The doc-gen4 half — `tests/oracle/dump-html-linked.lean` —
is still here and still runs on its own.

If a change to the IR schema or to the renderer's contract makes one of these
fixtures wrong, the fix is not to edit the JSON. Restore the generator from the
tag, or replace the fixture with a regression test against our own output and
say in the test name that the oracle was lost.
