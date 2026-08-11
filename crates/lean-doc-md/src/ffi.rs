//! The md4c C ABI, transcribed from `vendor/md4c/md4c.h` (md4c 0.5.2).
//!
//! Everything here is a one-to-one transcription of the vendored header. It is
//! written by hand rather than generated because the header is pinned by
//! `vendor/md4c/PROVENANCE.md` and will not move on its own; what makes that
//! safe is that `tests/abi.rs` compares every size, alignment, field offset and
//! enumerator below against the values the C compiler computes from the same
//! header (`csrc/layout_probe.c`). A layout that merely happens to link is the
//! failure mode this crate is most exposed to, so it is checked rather than
//! reasoned about.
//!
//! Two deliberate deviations from a literal transcription, both for safety:
//!
//! - The callbacks take the enum arguments as [`c_int`] rather than as the
//!   Rust enums. C may hand us any `int`; materialising a Rust enum from an
//!   out-of-range value is undefined behaviour, so conversion goes through
//!   [`MdBlockType::from_raw`] and friends, which return `None` instead.
//! - [`MdAttribute::substr_types`] is `*const c_int`, not `*const MdTextType`,
//!   for the same reason: the values are read and converted, never transmuted.

#![allow(non_camel_case_types)]

use std::ffi::{c_char, c_int, c_uint, c_void};

/// `MD_CHAR` — the header's character type when `MD4C_USE_UTF16` is not
/// defined, which is the only configuration we build.
pub type MdChar = c_char;
/// `MD_SIZE`.
pub type MdSize = c_uint;
/// `MD_OFFSET`.
pub type MdOffset = c_uint;

/// The C enums are `int`-sized in every ABI we target. `tests/abi.rs` checks
/// this against the C compiler as well; the assertion here is what makes the
/// `#[repr(i32)]` below not merely a hope.
const _: () = assert!(size_of::<c_int>() == 4);

macro_rules! c_enum {
    (
        $(#[$meta:meta])*
        $name:ident { $( $(#[$vmeta:meta])* $variant:ident = $value:expr, )* }
    ) => {
        $(#[$meta])*
        #[repr(i32)]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum $name {
            $( $(#[$vmeta])* $variant = $value, )*
        }

        impl $name {
            /// The value C passed us, or `None` if it names no enumerator of
            /// the vendored header.
            #[must_use]
            pub fn from_raw(raw: c_int) -> Option<Self> {
                match raw {
                    $( $value => Some(Self::$variant), )*
                    _ => None,
                }
            }
        }
    };
}

c_enum! {
    /// `MD_BLOCKTYPE`.
    MdBlockType {
        /// `<body>...</body>`.
        Doc = 0,
        /// `<blockquote>...</blockquote>`.
        Quote = 1,
        /// `<ul>`. Detail: [`MdBlockUlDetail`].
        Ul = 2,
        /// `<ol>`. Detail: [`MdBlockOlDetail`].
        Ol = 3,
        /// `<li>`. Detail: [`MdBlockLiDetail`].
        Li = 4,
        /// `<hr>`.
        Hr = 5,
        /// `<h1>`..`<h6>`. Detail: [`MdBlockHDetail`].
        H = 6,
        /// `<pre><code>`. Detail: [`MdBlockCodeDetail`].
        Code = 7,
        /// A raw HTML block. Never produced under [`crate::flags::MD_FLAG_NOHTML`].
        Html = 8,
        /// `<p>...</p>`.
        P = 9,
        /// `<table>`. Detail: [`MdBlockTableDetail`].
        Table = 10,
        /// `<thead>`.
        Thead = 11,
        /// `<tbody>`.
        Tbody = 12,
        /// `<tr>`.
        Tr = 13,
        /// `<th>`. Detail: [`MdBlockTdDetail`].
        Th = 14,
        /// `<td>`. Detail: [`MdBlockTdDetail`].
        Td = 15,
    }
}

c_enum! {
    /// `MD_SPANTYPE`.
    MdSpanType {
        /// `<em>`.
        Em = 0,
        /// `<strong>`.
        Strong = 1,
        /// `<a>`. Detail: [`MdSpanADetail`].
        A = 2,
        /// `<img>`. Detail: [`MdSpanImgDetail`].
        Img = 3,
        /// `<code>`.
        Code = 4,
        /// `<del>`. Needs `MD_FLAG_STRIKETHROUGH`.
        Del = 5,
        /// `$...$`. Needs `MD_FLAG_LATEXMATHSPANS`.
        LatexMath = 6,
        /// `$$...$$`. Needs `MD_FLAG_LATEXMATHSPANS`.
        LatexMathDisplay = 7,
        /// `[[...]]`. Needs `MD_FLAG_WIKILINKS`. Detail: [`MdSpanWikilinkDetail`].
        Wikilink = 8,
        /// `<u>`. Needs `MD_FLAG_UNDERLINE`.
        U = 9,
    }
}

c_enum! {
    /// `MD_TEXTTYPE`.
    MdTextType {
        /// Ordinary text.
        Normal = 0,
        /// A NUL in the input. md4c passes an empty string with size 1.
        NullChar = 1,
        /// A hard line break.
        Br = 2,
        /// A soft line break.
        SoftBr = 3,
        /// An entity, verbatim. md4c keeps no table of entity names.
        Entity = 4,
        /// Text inside a code block or code span.
        Code = 5,
        /// Raw HTML.
        Html = 6,
        /// Text inside a LaTeX math span.
        LatexMath = 7,
    }
}

c_enum! {
    /// `MD_ALIGN`. Only reachable through table cells, which this crate's AST
    /// does not carry (neither does MD4Lean's, so neither does doc-gen4's).
    MdAlign {
        /// Unspecified.
        Default = 0,
        /// `:---`.
        Left = 1,
        /// `:---:`.
        Center = 2,
        /// `---:`.
        Right = 3,
    }
}

/// `MD_ATTRIBUTE` — a string that may be cut into runs of different text
/// types, used for link destinations, titles and code-fence info strings.
///
/// The invariants the header states, which the reader in `crate::parse` relies
/// on: `substr_offsets[0] == 0`, the offsets array has one more entry than the
/// types array, and its last entry equals `size`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MdAttribute {
    /// Not NUL-terminated; `size` is authoritative.
    pub text: *const MdChar,
    /// Length of `text` in bytes.
    pub size: MdSize,
    /// One `MD_TEXTTYPE` per run. Read as `int`, converted, never transmuted.
    pub substr_types: *const c_int,
    /// One offset per run, plus a terminating `size`.
    pub substr_offsets: *const MdOffset,
}

/// `MD_BLOCK_UL_DETAIL`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MdBlockUlDetail {
    /// Non-zero for a tight list.
    pub is_tight: c_int,
    /// The bullet character in the source: `-`, `+` or `*`.
    pub mark: MdChar,
}

/// `MD_BLOCK_OL_DETAIL`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MdBlockOlDetail {
    /// The list's first number.
    pub start: c_uint,
    /// Non-zero for a tight list.
    pub is_tight: c_int,
    /// `.` or `)`.
    pub mark_delimiter: MdChar,
}

/// `MD_BLOCK_LI_DETAIL`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MdBlockLiDetail {
    /// Non-zero only under `MD_FLAG_TASKLISTS`.
    pub is_task: c_int,
    /// `x`, `X` or a space when `is_task`; undefined otherwise.
    pub task_mark: MdChar,
    /// Offset of the character between `[` and `]` when `is_task`.
    pub task_mark_offset: MdOffset,
}

/// `MD_BLOCK_H_DETAIL`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MdBlockHDetail {
    /// 1 through 6.
    pub level: c_uint,
}

/// `MD_BLOCK_CODE_DETAIL`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MdBlockCodeDetail {
    /// Everything after the opening fence.
    pub info: MdAttribute,
    /// The first word of `info`.
    pub lang: MdAttribute,
    /// The fence character, or zero for an indented code block.
    pub fence_char: MdChar,
}

/// `MD_BLOCK_TABLE_DETAIL`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MdBlockTableDetail {
    /// Columns in the table.
    pub col_count: c_uint,
    /// Rows in the header; currently always 1.
    pub head_row_count: c_uint,
    /// Rows in the body.
    pub body_row_count: c_uint,
}

/// `MD_BLOCK_TD_DETAIL`, shared by `MD_BLOCK_TH` and `MD_BLOCK_TD`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MdBlockTdDetail {
    /// An `MD_ALIGN`. Read as `int`, converted, never transmuted.
    pub align: c_int,
}

/// `MD_SPAN_A_DETAIL`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MdSpanADetail {
    /// The link destination.
    pub href: MdAttribute,
    /// The link title, empty when absent.
    pub title: MdAttribute,
    /// Non-zero for `<...>` and permissive autolinks.
    pub is_autolink: c_int,
}

/// `MD_SPAN_IMG_DETAIL`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MdSpanImgDetail {
    /// The image source.
    pub src: MdAttribute,
    /// The image title, empty when absent.
    pub title: MdAttribute,
}

/// `MD_SPAN_WIKILINK_DETAIL`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MdSpanWikilinkDetail {
    /// The wiki link target.
    pub target: MdAttribute,
}

/// `int (*)(MD_BLOCKTYPE, void*, void*)`.
pub type MdBlockCallback =
    unsafe extern "C" fn(ty: c_int, detail: *mut c_void, userdata: *mut c_void) -> c_int;
/// `int (*)(MD_SPANTYPE, void*, void*)`.
pub type MdSpanCallback =
    unsafe extern "C" fn(ty: c_int, detail: *mut c_void, userdata: *mut c_void) -> c_int;
/// `int (*)(MD_TEXTTYPE, const MD_CHAR*, MD_SIZE, void*)`.
pub type MdTextCallback = unsafe extern "C" fn(
    ty: c_int,
    text: *const MdChar,
    size: MdSize,
    userdata: *mut c_void,
) -> c_int;
/// `void (*)(const char*, void*)`.
pub type MdDebugLogCallback = unsafe extern "C" fn(msg: *const c_char, userdata: *mut c_void);

/// `MD_PARSER` — the callback table md4c drives the caller through.
///
/// The nullable members are `Option<fn>`, which has the same layout as the
/// function pointer itself with `None` as the null pointer, so this stays a
/// literal transcription.
#[repr(C)]
pub struct MdParser {
    /// Reserved; must be zero.
    pub abi_version: c_uint,
    /// A bitmask of `MD_FLAG_*`.
    pub flags: c_uint,
    /// Entering a block.
    pub enter_block: Option<MdBlockCallback>,
    /// Leaving a block.
    pub leave_block: Option<MdBlockCallback>,
    /// Entering a span.
    pub enter_span: Option<MdSpanCallback>,
    /// Leaving a span.
    pub leave_span: Option<MdSpanCallback>,
    /// Text inside the innermost open block or span.
    pub text: Option<MdTextCallback>,
    /// Optional diagnostics sink.
    pub debug_log: Option<MdDebugLogCallback>,
    /// Reserved; must be `None`.
    pub syntax: Option<unsafe extern "C" fn()>,
}

unsafe extern "C" {
    /// `md_parse` — parse `text` and drive `parser`'s callbacks.
    ///
    /// Returns zero on success, `-1` on a runtime error, or whatever value a
    /// callback returned to abort.
    pub fn md_parse(
        text: *const MdChar,
        size: MdSize,
        parser: *const MdParser,
        userdata: *mut c_void,
    ) -> c_int;
}
