//! `MD_FLAG_*` and `MD_DIALECT_*`, transcribed from `vendor/md4c/md4c.h`.
//!
//! The values are checked against the C compiler's own view of the same header
//! by `tests/abi.rs`; a flag that quietly drifted would change what the parser
//! accepts without changing anything that fails to build.

/// Collapse non-trivial whitespace in `MD_TEXT_NORMAL` into a single space.
pub const MD_FLAG_COLLAPSEWHITESPACE: u32 = 0x0001;
/// Do not require a space in ATX headers (`###header`).
pub const MD_FLAG_PERMISSIVEATXHEADERS: u32 = 0x0002;
/// Recognise bare URLs as autolinks.
pub const MD_FLAG_PERMISSIVEURLAUTOLINKS: u32 = 0x0004;
/// Recognise bare e-mail addresses as autolinks.
pub const MD_FLAG_PERMISSIVEEMAILAUTOLINKS: u32 = 0x0008;
/// Disable indented code blocks; only fenced code is recognised.
pub const MD_FLAG_NOINDENTEDCODEBLOCKS: u32 = 0x0010;
/// Disable raw HTML blocks.
pub const MD_FLAG_NOHTMLBLOCKS: u32 = 0x0020;
/// Disable inline raw HTML.
pub const MD_FLAG_NOHTMLSPANS: u32 = 0x0040;
/// Enable the tables extension.
pub const MD_FLAG_TABLES: u32 = 0x0100;
/// Enable the strikethrough extension.
pub const MD_FLAG_STRIKETHROUGH: u32 = 0x0200;
/// Recognise `www.` prefixed autolinks with no scheme.
pub const MD_FLAG_PERMISSIVEWWWAUTOLINKS: u32 = 0x0400;
/// Enable the task list extension.
pub const MD_FLAG_TASKLISTS: u32 = 0x0800;
/// Enable `$` and `$$` LaTeX math spans.
pub const MD_FLAG_LATEXMATHSPANS: u32 = 0x1000;
/// Enable the wiki links extension.
pub const MD_FLAG_WIKILINKS: u32 = 0x2000;
/// Enable underline, which also stops `_` from marking emphasis.
pub const MD_FLAG_UNDERLINE: u32 = 0x4000;
/// Treat every soft break as a hard break.
pub const MD_FLAG_HARD_SOFT_BREAKS: u32 = 0x8000;

/// All three permissive autolink flags.
pub const MD_FLAG_PERMISSIVEAUTOLINKS: u32 = MD_FLAG_PERMISSIVEEMAILAUTOLINKS
    | MD_FLAG_PERMISSIVEURLAUTOLINKS
    | MD_FLAG_PERMISSIVEWWWAUTOLINKS;
/// Raw HTML off, blocks and spans both.
pub const MD_FLAG_NOHTML: u32 = MD_FLAG_NOHTMLBLOCKS | MD_FLAG_NOHTMLSPANS;

/// Plain CommonMark: no extensions.
pub const MD_DIALECT_COMMONMARK: u32 = 0;
/// GitHub Flavored Markdown, as far as md4c implements it.
pub const MD_DIALECT_GITHUB: u32 =
    MD_FLAG_PERMISSIVEAUTOLINKS | MD_FLAG_TABLES | MD_FLAG_STRIKETHROUGH | MD_FLAG_TASKLISTS;

/// The flags doc-gen4 parses docstrings with.
///
/// 実測 — `DocGen4/Output/DocString.lean:393`:
///
/// ```text
/// let flags := MD4Lean.MD_DIALECT_GITHUB ||| MD4Lean.MD_FLAG_LATEXMATHSPANS ||| MD4Lean.MD_FLAG_NOHTML
/// ```
///
/// This is the whole reason the parser is vendored rather than reimplemented:
/// the acceptance oracle compares bytes against doc-gen4's pages, so the
/// dialect has to be the same one, not a close relative of it.
pub const DOCSTRING_FLAGS: u32 = MD_DIALECT_GITHUB | MD_FLAG_LATEXMATHSPANS | MD_FLAG_NOHTML;
