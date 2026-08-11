//! CommonMark for docstrings: md4c through FFI, plus doc-gen4's own rendering.
//!
//! Filled in by milestone **M1** — see `docs/implementation-plan.md`.
//!
//! doc-gen4 does not use md4c's HTML renderer. It parses to an AST and builds
//! the HTML itself (`DocGen4/Output/DocString.lean`), with flags
//! `MD_DIALECT_GITHUB | MD_FLAG_LATEXMATHSPANS | MD_FLAG_NOHTML`. This crate
//! does the same, so only `md4c.c` and `md4c.h` need to be vendored —
//! `entity.c` belongs to the HTML renderer, and entities are passed through
//! verbatim anyway.
//!
//! Linking md4c removes the 594-line hand-written CommonMark subset that the
//! TypeScript prototype needed. It does **not** remove the 153 lines of
//! autolink resolution around it (`nameToLink`, `isNameLit`, `autoLinkInline`,
//! ...) — that part is doc-gen4's, not md4c's, and has to be ported.
