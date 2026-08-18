# md4c — where these files came from

Copied on 2026-08-11 from the MD4Lean package as it is pinned in the
measurement target, `lean-projects`:

    .lake/packages/MD4Lean/md4c/{md4c.c,md4c.h,LICENSE.md,CHANGELOG.md}

- MD4Lean rev `6a3fb240133bcb7e1a066fdc784b3fdc304e3fc5` (`lake-manifest.json`)
- md4c version **0.5.2** (`CHANGELOG.md`)
- License: MIT (`LICENSE.md`) — keep it next to the sources when redistributing

## Why vendor instead of using a crate

The acceptance oracle compares bytes against doc-gen4's output, and doc-gen4
parses docstrings with *this* md4c. Taking the same C sources makes the parser
identical by construction rather than by version number, which no crates.io
dependency can promise across upgrades.

## What is deliberately not copied

`entity.c` / `entity.h` / `md4c-html.c` / `md4c-html.h` belong to md4c's HTML
renderer. doc-gen4 does not use it — it takes the AST and builds HTML itself
(`DocGen4/Output/DocString.lean:202-393`), and passes entities through verbatim
(`:211`, `.entity s => Html.raw s`). `md4c.c` includes only `md4c.h` and
standard headers, so the parser builds without them.

## Updating

If the target's MD4Lean pin moves, re-copy from the same path and record the new
revision here. Do not edit these files: any local change is a place where our
parser and doc-gen4's can silently disagree.
