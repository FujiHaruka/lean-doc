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
//!
//! # What is here (M1-c, first half)
//!
//! The binding and the tree, not the HTML:
//!
//! | | |
//! |---|---|
//! | [`ffi`] | `md4c.h` transcribed: enums, detail structs, `MD_PARSER`, `md_parse` |
//! | [`flags`] | `MD_FLAG_*`, and [`flags::DOCSTRING_FLAGS`], the combination doc-gen4 uses |
//! | [`ast`] | the tree, shaped like MD4Lean's Lean ADT because that is what doc-gen4 matches on |
//! | [`parse`] | the callbacks, transliterated from `MD4Lean/wrapper/wrapper.c` |
//!
//! ```
//! use lean_doc_md::{Block, Text};
//!
//! let doc = lean_doc_md::parse("`Nat.succ` is *fine*").unwrap();
//! let Block::P(texts) = &doc.blocks[0] else { panic!() };
//! assert_eq!(texts[0], Text::Code(vec!["Nat.succ".to_owned()]));
//! ```
//!
//! # How this is checked
//!
//! Two oracles, neither of which is a reading of this code:
//!
//! - `tests/abi.rs` compares every size, alignment, field offset and
//!   enumerator in [`ffi`] against the values the **C compiler** computes from
//!   the vendored header. A struct layout that merely happens to link is the
//!   failure this crate is most exposed to.
//! - `tests/md4lean.rs` compares the tree against **MD4Lean's own**
//!   `MD4Lean.parse`, run under Lean on the target package's docstrings. The
//!   generator is `tests/oracle/`.

pub mod ast;
mod error;
pub mod ffi;
pub mod flags;
mod parse;

pub use ast::{AttrText, Block, Document, Li, Text};
pub use error::{Error, Result};
pub use flags::DOCSTRING_FLAGS;
pub use parse::{parse, parse_with_flags};
