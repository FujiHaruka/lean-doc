//! What can go wrong between md4c and the AST.

use std::fmt;

/// A parse that did not produce a document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// The input is longer than `MD_SIZE` can address. md4c's offsets are
    /// 32-bit, so this is a hard limit of the parser, not of this crate.
    InputTooLarge {
        /// The input's length in bytes.
        bytes: usize,
    },
    /// `md_parse` reported a runtime failure (it returns `-1` for, e.g., a
    /// failed allocation).
    Md4c {
        /// `md_parse`'s return value.
        code: i32,
    },
    /// md4c handed back a byte range that is not valid UTF-8. Its callbacks
    /// hand out slices of the input, which was a `&str`, so this cannot happen
    /// unless the parser or this binding is wrong.
    NotUtf8,
    /// md4c produced something MD4Lean's ADT has no constructor for, so this
    /// crate's AST has none either.
    ///
    /// Only reachable by parsing with flags doc-gen4 does not use: inline raw
    /// HTML puts text directly in a paragraph where `MD4Lean.Text` allows only
    /// its own constructors. MD4Lean does not report this — it puts a bare
    /// `String` there and dies later — so refusing is the difference, not a
    /// shared behaviour.
    Unrepresentable(&'static str),
    /// The callback sequence did not fit the shape the builder relies on.
    /// Every case is a bug here or a change in md4c, never bad input.
    Malformed(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputTooLarge { bytes } => {
                write!(f, "{bytes} bytes is past md4c's 32-bit MD_SIZE limit")
            }
            Self::Md4c { code } => write!(f, "md_parse failed with {code}"),
            Self::NotUtf8 => f.write_str("md4c produced a fragment that is not UTF-8"),
            Self::Unrepresentable(what) => write!(f, "no MD4Lean constructor for {what}"),
            Self::Malformed(what) => write!(f, "unexpected callback sequence: {what}"),
        }
    }
}

impl std::error::Error for Error {}

/// The result of a parse.
pub type Result<T> = std::result::Result<T, Error>;
