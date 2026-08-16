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
