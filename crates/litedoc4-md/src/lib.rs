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
//! # What is here (M1-c)
//!
//! | | |
//! |---|---|
//! | [`ffi`] | `md4c.h` transcribed: enums, detail structs, `MD_PARSER`, `md_parse` |
//! | [`flags`] | `MD_FLAG_*`, and [`flags::DOCSTRING_FLAGS`], the combination doc-gen4 uses |
//! | [`ast`] | the tree, shaped like MD4Lean's Lean ADT because that is what doc-gen4 matches on |
//! | [`mod@parse`] | the callbacks, transliterated from `MD4Lean/wrapper/wrapper.c` |
//! | [`html`] | `DocGen4/Output/DocString.lean`: the tree to HTML |
//! | [`escape`] | `Html.escape`, which is four characters and not five |
//! | [`gc`] | the Unicode general category tables the two of those need |
//! | [`math`] | `$…$` to MathML, which doc-gen4 left to MathJax in the browser |
//!
//! ```
//! use litedoc4_md::{NoLinks, Renderer};
//!
//! let html = Renderer::new("../", &NoLinks).docstring("`Nat.succ` is *fine*");
//! assert_eq!(html, "<p><code>Nat.succ</code> is <em>fine</em></p>");
//! ```
//!
//! Name resolution is the caller's: [`Renderer`] takes a [`LinkResolver`], so
//! this crate never learns what a `LinkIndex` is. [`NoLinks`] is the resolver
//! that answers no name, which is what a caller without a dependency map wants
//! and what this milestone step renders with.
//!
//! # How this is checked
//!
//! Three oracles, none of which is a reading of this code:
//!
//! - `tests/abi.rs` compares every size, alignment, field offset and
//!   enumerator in [`ffi`] against the values the **C compiler** computes from
//!   the vendored header. A struct layout that merely happens to link is the
//!   failure this crate is most exposed to.
//! - `tests/md4lean.rs` compares the tree against **MD4Lean's own**
//!   `MD4Lean.parse`, run under Lean on the target package's docstrings.
//! - `tests/docgen4.rs` compares the HTML against **doc-gen4's own**
//!   `docStringToHtml`, run under Lean on the same corpus, byte for byte.
//!
//! The generators for all three are in `tests/oracle/`.

pub mod ast;
mod error;
pub mod escape;
pub mod ffi;
pub mod flags;
pub mod gc;
pub mod html;
pub mod math;
mod parse;

pub use ast::{AttrText, Block, Document, Li, Text};
pub use error::{Error, Result};
pub use escape::{escape_html, escape_html_into};
pub use flags::DOCSTRING_FLAGS;
pub use html::{LinkResolver, NoLinks, Renderer, attr_text_to_string, heading_id};
pub use math::to_mathml;
pub use parse::{parse, parse_with_flags};
