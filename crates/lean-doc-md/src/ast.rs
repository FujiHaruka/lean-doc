//! The document tree, shaped exactly like MD4Lean's Lean ADT.
//!
//! # Why this shape and not a nicer one
//!
//! The next milestone step ports `DocGen4/Output/DocString.lean:202-393`, which
//! is a `match` over `MD4Lean.Block` / `MD4Lean.Text`. Every branch it takes,
//! and every field it reads, has to exist here under the same meaning, or the
//! port becomes a translation rather than a transcription — and a translation
//! is where byte differences come from. So the constructors below are the ones
//! in `MD4Lean.lean`, in that order, carrying the same payloads:
//!
//! | Lean | here |
//! |---|---|
//! | `MD4Lean.AttrText` | [`AttrText`] |
//! | `MD4Lean.Text` | [`Text`] |
//! | `MD4Lean.Li Block` | [`Li`] |
//! | `MD4Lean.Block` | [`Block`] |
//! | `MD4Lean.Document` | [`Document`] |
//!
//! Two consequences of following Lean rather than md4c:
//!
//! - **Verbatim contents are `Vec<String>`, not `Vec<Text>`.** Code blocks,
//!   code spans, math spans and raw HTML blocks receive exactly one text type
//!   from md4c, so MD4Lean drops the wrapper and keeps the strings. doc-gen4
//!   relies on that: it calls `String.join` on them.
//! - **Table cells are `Vec<Text>` with no alignment.** md4c reports
//!   `MD_ALIGN` per cell; MD4Lean discards it, so doc-gen4 never emits an
//!   `align` attribute, so neither do we.
//!
//! Entities are **not** expanded. md4c has no entity table (that lives in the
//! HTML renderer we deliberately do not vendor), and doc-gen4 passes them
//! through with `Html.raw` (`DocString.lean:211`), so [`Text::Entity`] carries
//! the source text including the `&` and `;`.

/// Text inside an attribute — a link destination, an image source, a title, or
/// a code fence's info string.
///
/// `MD4Lean.AttrText`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AttrText {
    /// Ordinary text.
    Normal(String),
    /// An entity, verbatim and unvalidated, e.g. `&quot;`.
    Entity(String),
    /// A NUL character in the source.
    NullChar,
}

/// An inline element.
///
/// `MD4Lean.Text`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Text {
    /// Ordinary text.
    Normal(String),
    /// A NUL character in the source.
    NullChar,
    /// A hard line break. The payload is md4c's source text for it.
    Br(String),
    /// A soft line break. The payload is md4c's source text for it.
    SoftBr(String),
    /// An entity, verbatim and unvalidated, e.g. `&nbsp;`.
    Entity(String),
    /// Emphasis.
    Em(Vec<Self>),
    /// Strong emphasis.
    Strong(Vec<Self>),
    /// Underline. Needs `MD_FLAG_UNDERLINE`, which docstrings do not use.
    U(Vec<Self>),
    /// A link.
    A {
        /// The destination.
        href: Vec<AttrText>,
        /// The title, empty when the source gave none.
        title: Vec<AttrText>,
        /// True for `<...>` and permissive autolinks.
        is_auto: bool,
        /// The link text.
        children: Vec<Self>,
    },
    /// An image.
    Img {
        /// The source.
        src: Vec<AttrText>,
        /// The title, empty when the source gave none.
        title: Vec<AttrText>,
        /// The alt text, which may itself contain spans.
        alt: Vec<Self>,
    },
    /// A code span. The parser may split it into several pieces.
    Code(Vec<String>),
    /// Struck-through text.
    Del(Vec<Self>),
    /// An inline `$...$` math span.
    LatexMath(Vec<String>),
    /// A display `$$...$$` math span.
    LatexMathDisplay(Vec<String>),
    /// A wiki link. Needs `MD_FLAG_WIKILINKS`, which docstrings do not use.
    WikiLink {
        /// The target article.
        target: Vec<AttrText>,
        /// The link text.
        children: Vec<Self>,
    },
}

/// A list item.
///
/// `MD4Lean.Li`. The task fields are populated only under
/// `MD_FLAG_TASKLISTS`, which the docstring dialect does enable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Li {
    /// Whether the item began with a `[ ]` / `[x]` marker.
    pub is_task: bool,
    /// The character between the brackets: `x`, `X` or a space.
    pub task_char: Option<char>,
    /// Byte offset of that character in the input.
    pub task_mark_offset: Option<u32>,
    /// The item's blocks.
    pub contents: Vec<Block>,
}

/// A block-level element.
///
/// `MD4Lean.Block`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    /// A paragraph.
    P(Vec<Text>),
    /// An unordered list.
    Ul {
        /// False when any item is separated by a blank line.
        tight: bool,
        /// The bullet character used in the source.
        mark: char,
        /// The items.
        items: Vec<Li>,
    },
    /// An ordered list.
    Ol {
        /// False when any item is separated by a blank line.
        tight: bool,
        /// The first number. doc-gen4 emits `start=` only when this is not 1.
        start: u32,
        /// The delimiter after the number: `.` or `)`.
        mark: char,
        /// The items.
        items: Vec<Li>,
    },
    /// A thematic break.
    Hr,
    /// A heading, level 1 through 6.
    Header {
        /// The level.
        level: u32,
        /// The heading text.
        texts: Vec<Text>,
    },
    /// A code block.
    Code {
        /// Everything after the opening fence.
        info: Vec<AttrText>,
        /// The first word of `info`; what doc-gen4 turns into `language-…`.
        lang: Vec<AttrText>,
        /// The fence character, or `None` for an indented code block.
        fence_char: Option<char>,
        /// The contents, in the pieces md4c produced (one per line, plus the
        /// newlines).
        content: Vec<String>,
    },
    /// A raw HTML block. Never produced under `MD_FLAG_NOHTML`.
    Html(Vec<String>),
    /// A block quote.
    BlockQuote(Vec<Self>),
    /// A table.
    Table {
        /// The header row's cells. md4c guarantees exactly one header row.
        head: Vec<Vec<Text>>,
        /// The body's rows, each a list of cells.
        body: Vec<Vec<Vec<Text>>>,
    },
}

/// A parsed document.
///
/// `MD4Lean.Document`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Document {
    /// The top-level blocks, in source order.
    pub blocks: Vec<Block>,
}
