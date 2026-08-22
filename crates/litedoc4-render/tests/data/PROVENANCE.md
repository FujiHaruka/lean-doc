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
`autolink-expected.json`, `fragment-expected.json`, `pages-expected.json`) —
**except `page-parts-expected.json`, which changed sides on 2026-08-22**; see
below.

Those are acceptance oracles: the point is that the Rust renderer produces the
same bytes, so they are regenerated from the pinned upstream rather than edited.
See this repository's NOTICE and `docs/provenance.md`.

## `page-parts-expected.json` changed sides on 2026-08-22

It held the **prototype's** declaration blocks, and the test compared them
through `Content` — a reduction to ids, hrefs, stubs and words — because the two
markups were never the same bytes: the prototype wrote `div.decl` with a
`div.gh_link` above the block, and this crate writes `section.decl[data-kind]`
with the source link in the header (M8-b).

Feature-sweep C-2 (`docs/plans/feature-sweep.md`) gives every declaration a
`Used by` block, so **all 187 cases gained a stub the prototype could not have
had**. The role was switched once, deliberately (§3【決定 1】/ C-4), which is
what the last paragraph of this file already prescribed: *"replace the fixture
with a regression test against our own output and say in the test name that the
oracle was lost."* The test is now
`carries_the_same_content_as_the_frozen_output`.

**Two things got stronger rather than weaker in the switch.**

- The declaration blocks are compared **byte for byte** now. `Content` existed to
  bridge two different markups; between a thing and itself it only loses
  information. It is still computed, but only to count the sample — a fixture
  regenerated from a renderer that had stopped emitting things must not pass by
  being uniformly empty.
- **`header` was not blessed.** It is still the prototype's record of the head
  and signature, still compared through `Content::matches_head` with M8-b's one
  declared difference. Nothing in bundle C touches a head or a signature, and the
  regenerator asserts that: it refuses to write if the head moved on any case.

`the_sample_reaches_every_shape` was translated with it — the probes named the
prototype's classes (`ul.structure_fields`, `span.impl_arg`, …) and now name this
crate's (`ul.fields`, `span.binder.implicit`, …). **The shapes are the same
ones.** One probe is gone rather than reworded: "an attribute block ending in a
newline" stood for the prototype's single unflattened element, and this renderer
flattens everything.

**Regenerate rather than edit**, with the printout as the review:

    LITEDOC4_BLESS=1 cargo test -p litedoc4-render --test page_parts

It refuses to run if re-serialising the file does not reproduce it byte for byte,
so a regeneration cannot become a whole-file rewrite nobody can read.

## The prototype's regenerators are gone — `page-parts` has one of its own

`experiments/` was removed from HEAD on 2026-08-16, and the `gen-*-expected.ts`
scripts that drove the prototype went with it. **The four files that are still
the prototype's output are frozen values**, and only `page-parts-expected.json`
can be regenerated, because it is no longer the prototype's (see above). Both the
prototype and its generators are at tag `experiments-frozen`:

    git show experiments-frozen:experiments/stage7d/render.ts
    git show experiments-frozen:crates/lean-doc-render/tests/gen-ts-expected.ts

`docgen4-linked-expected.json` is the one to watch: its expected values come from
doc-gen4, but *which cases it contains* was decided by the prototype, so its
generator went too. The doc-gen4 half — `tests/oracle/dump-html-linked.lean` —
is still here and still runs on its own.

If a change to the IR schema or to the renderer's contract makes one of these
fixtures wrong, the fix is not to edit the JSON. Restore the generator from the
tag, or replace the fixture with a regression test against our own output and
say in the test name that the oracle was lost. **`page-parts-expected.json` is
what taking the second option looks like** — read the section above before taking
it again for another file, because what it costs is a real oracle and the cost
has to be written down where the next reader will find it.
