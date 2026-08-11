//! Every number in this file comes from the **C compiler**, not from a reading
//! of `md4c.h`.
//!
//! `src/ffi.rs` is a hand transcription of the vendored header. The failure
//! mode that transcription has is silent: a field of the wrong width, a
//! reordered pair, an enumerator off by one — all of them still compile, still
//! link, and then make the parser read details from the wrong place. So
//! `csrc/layout_probe.c` computes `sizeof`, `_Alignof`, `offsetof` and every
//! enumerator and flag from the same header, and the tests below assert that
//! the Rust side agrees with all of them, name by name.
//!
//! A name that exists on one side and not the other is a failure too: this is
//! also what notices that `vendor/md4c` moved to a version with a new field.

use std::collections::BTreeMap;
use std::ffi::{CStr, c_char, c_int, c_uint};

use lean_doc_md::ffi::{
    MdAlign, MdAttribute, MdBlockCodeDetail, MdBlockHDetail, MdBlockLiDetail, MdBlockOlDetail,
    MdBlockTableDetail, MdBlockTdDetail, MdBlockType, MdBlockUlDetail, MdChar, MdOffset, MdParser,
    MdSize, MdSpanADetail, MdSpanImgDetail, MdSpanType, MdSpanWikilinkDetail, MdTextType,
};
use lean_doc_md::flags;

#[repr(C)]
struct Probe {
    name: *const c_char,
    value: usize,
}

unsafe extern "C" {
    fn lean_doc_md4c_probes() -> *const Probe;
}

/// What `csrc/layout_probe.c` says, keyed by the name it used.
fn from_c() -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    // SAFETY: the table is a `static` array terminated by a null name, and the
    // strings in it are string literals with static storage duration.
    unsafe {
        let mut probe = lean_doc_md4c_probes();
        assert!(!probe.is_null(), "the layout probe returned nothing");
        while !(*probe).name.is_null() {
            let name = CStr::from_ptr((*probe).name)
                .to_str()
                .expect("probe names are ASCII")
                .to_owned();
            let previous = out.insert(name.clone(), (*probe).value);
            assert!(previous.is_none(), "the probe table lists {name} twice");
            probe = probe.add(1);
        }
    }
    out
}

/// What `src/ffi.rs` and `src/flags.rs` say, under the same names.
#[expect(
    clippy::too_many_lines,
    reason = "one line per field is the point; a shorter form would be a second \
              transcription to get wrong"
)]
fn from_rust() -> BTreeMap<String, usize> {
    let mut out: BTreeMap<String, usize> = BTreeMap::new();
    let mut put = |name: &str, value: usize| {
        assert!(
            out.insert(name.to_owned(), value).is_none(),
            "{name} listed twice"
        );
    };

    macro_rules! size_of {
        ($c_name:literal, $t:ty) => {
            put(concat!($c_name, "/size"), size_of::<$t>());
        };
    }
    macro_rules! align_of {
        ($c_name:literal, $t:ty) => {
            put(concat!($c_name, "/align"), align_of::<$t>());
        };
    }
    macro_rules! offset_of {
        ($c_name:literal, $t:ty, $c_field:literal, $field:ident) => {
            put(
                concat!($c_name, ".", $c_field),
                std::mem::offset_of!($t, $field),
            );
        };
    }
    macro_rules! value_of {
        ($c_name:literal, $value:expr) => {
            put(
                $c_name,
                usize::try_from($value).expect("a non-negative value"),
            );
        };
    }

    size_of!("MD_CHAR", MdChar);
    size_of!("MD_SIZE", MdSize);
    size_of!("MD_OFFSET", MdOffset);
    // The Rust enums are `#[repr(i32)]`; these four assert that C agrees.
    size_of!("MD_BLOCKTYPE", MdBlockType);
    size_of!("MD_SPANTYPE", MdSpanType);
    size_of!("MD_TEXTTYPE", MdTextType);
    size_of!("MD_ALIGN", MdAlign);

    size_of!("MD_ATTRIBUTE", MdAttribute);
    align_of!("MD_ATTRIBUTE", MdAttribute);
    offset_of!("MD_ATTRIBUTE", MdAttribute, "text", text);
    offset_of!("MD_ATTRIBUTE", MdAttribute, "size", size);
    offset_of!("MD_ATTRIBUTE", MdAttribute, "substr_types", substr_types);
    offset_of!(
        "MD_ATTRIBUTE",
        MdAttribute,
        "substr_offsets",
        substr_offsets
    );

    size_of!("MD_BLOCK_UL_DETAIL", MdBlockUlDetail);
    align_of!("MD_BLOCK_UL_DETAIL", MdBlockUlDetail);
    offset_of!("MD_BLOCK_UL_DETAIL", MdBlockUlDetail, "is_tight", is_tight);
    offset_of!("MD_BLOCK_UL_DETAIL", MdBlockUlDetail, "mark", mark);

    size_of!("MD_BLOCK_OL_DETAIL", MdBlockOlDetail);
    align_of!("MD_BLOCK_OL_DETAIL", MdBlockOlDetail);
    offset_of!("MD_BLOCK_OL_DETAIL", MdBlockOlDetail, "start", start);
    offset_of!("MD_BLOCK_OL_DETAIL", MdBlockOlDetail, "is_tight", is_tight);
    offset_of!(
        "MD_BLOCK_OL_DETAIL",
        MdBlockOlDetail,
        "mark_delimiter",
        mark_delimiter
    );

    size_of!("MD_BLOCK_LI_DETAIL", MdBlockLiDetail);
    align_of!("MD_BLOCK_LI_DETAIL", MdBlockLiDetail);
    offset_of!("MD_BLOCK_LI_DETAIL", MdBlockLiDetail, "is_task", is_task);
    offset_of!(
        "MD_BLOCK_LI_DETAIL",
        MdBlockLiDetail,
        "task_mark",
        task_mark
    );
    offset_of!(
        "MD_BLOCK_LI_DETAIL",
        MdBlockLiDetail,
        "task_mark_offset",
        task_mark_offset
    );

    size_of!("MD_BLOCK_H_DETAIL", MdBlockHDetail);
    align_of!("MD_BLOCK_H_DETAIL", MdBlockHDetail);
    offset_of!("MD_BLOCK_H_DETAIL", MdBlockHDetail, "level", level);

    size_of!("MD_BLOCK_CODE_DETAIL", MdBlockCodeDetail);
    align_of!("MD_BLOCK_CODE_DETAIL", MdBlockCodeDetail);
    offset_of!("MD_BLOCK_CODE_DETAIL", MdBlockCodeDetail, "info", info);
    offset_of!("MD_BLOCK_CODE_DETAIL", MdBlockCodeDetail, "lang", lang);
    offset_of!(
        "MD_BLOCK_CODE_DETAIL",
        MdBlockCodeDetail,
        "fence_char",
        fence_char
    );

    size_of!("MD_BLOCK_TABLE_DETAIL", MdBlockTableDetail);
    align_of!("MD_BLOCK_TABLE_DETAIL", MdBlockTableDetail);
    offset_of!(
        "MD_BLOCK_TABLE_DETAIL",
        MdBlockTableDetail,
        "col_count",
        col_count
    );
    offset_of!(
        "MD_BLOCK_TABLE_DETAIL",
        MdBlockTableDetail,
        "head_row_count",
        head_row_count
    );
    offset_of!(
        "MD_BLOCK_TABLE_DETAIL",
        MdBlockTableDetail,
        "body_row_count",
        body_row_count
    );

    size_of!("MD_BLOCK_TD_DETAIL", MdBlockTdDetail);
    align_of!("MD_BLOCK_TD_DETAIL", MdBlockTdDetail);
    offset_of!("MD_BLOCK_TD_DETAIL", MdBlockTdDetail, "align", align);

    size_of!("MD_SPAN_A_DETAIL", MdSpanADetail);
    align_of!("MD_SPAN_A_DETAIL", MdSpanADetail);
    offset_of!("MD_SPAN_A_DETAIL", MdSpanADetail, "href", href);
    offset_of!("MD_SPAN_A_DETAIL", MdSpanADetail, "title", title);
    offset_of!(
        "MD_SPAN_A_DETAIL",
        MdSpanADetail,
        "is_autolink",
        is_autolink
    );

    size_of!("MD_SPAN_IMG_DETAIL", MdSpanImgDetail);
    align_of!("MD_SPAN_IMG_DETAIL", MdSpanImgDetail);
    offset_of!("MD_SPAN_IMG_DETAIL", MdSpanImgDetail, "src", src);
    offset_of!("MD_SPAN_IMG_DETAIL", MdSpanImgDetail, "title", title);

    size_of!("MD_SPAN_WIKILINK_DETAIL", MdSpanWikilinkDetail);
    align_of!("MD_SPAN_WIKILINK_DETAIL", MdSpanWikilinkDetail);
    offset_of!(
        "MD_SPAN_WIKILINK_DETAIL",
        MdSpanWikilinkDetail,
        "target",
        target
    );

    size_of!("MD_PARSER", MdParser);
    align_of!("MD_PARSER", MdParser);
    offset_of!("MD_PARSER", MdParser, "abi_version", abi_version);
    offset_of!("MD_PARSER", MdParser, "flags", flags);
    offset_of!("MD_PARSER", MdParser, "enter_block", enter_block);
    offset_of!("MD_PARSER", MdParser, "leave_block", leave_block);
    offset_of!("MD_PARSER", MdParser, "enter_span", enter_span);
    offset_of!("MD_PARSER", MdParser, "leave_span", leave_span);
    offset_of!("MD_PARSER", MdParser, "text", text);
    offset_of!("MD_PARSER", MdParser, "debug_log", debug_log);
    offset_of!("MD_PARSER", MdParser, "syntax", syntax);

    value_of!("MD_BLOCK_DOC", MdBlockType::Doc as i32);
    value_of!("MD_BLOCK_QUOTE", MdBlockType::Quote as i32);
    value_of!("MD_BLOCK_UL", MdBlockType::Ul as i32);
    value_of!("MD_BLOCK_OL", MdBlockType::Ol as i32);
    value_of!("MD_BLOCK_LI", MdBlockType::Li as i32);
    value_of!("MD_BLOCK_HR", MdBlockType::Hr as i32);
    value_of!("MD_BLOCK_H", MdBlockType::H as i32);
    value_of!("MD_BLOCK_CODE", MdBlockType::Code as i32);
    value_of!("MD_BLOCK_HTML", MdBlockType::Html as i32);
    value_of!("MD_BLOCK_P", MdBlockType::P as i32);
    value_of!("MD_BLOCK_TABLE", MdBlockType::Table as i32);
    value_of!("MD_BLOCK_THEAD", MdBlockType::Thead as i32);
    value_of!("MD_BLOCK_TBODY", MdBlockType::Tbody as i32);
    value_of!("MD_BLOCK_TR", MdBlockType::Tr as i32);
    value_of!("MD_BLOCK_TH", MdBlockType::Th as i32);
    value_of!("MD_BLOCK_TD", MdBlockType::Td as i32);

    value_of!("MD_SPAN_EM", MdSpanType::Em as i32);
    value_of!("MD_SPAN_STRONG", MdSpanType::Strong as i32);
    value_of!("MD_SPAN_A", MdSpanType::A as i32);
    value_of!("MD_SPAN_IMG", MdSpanType::Img as i32);
    value_of!("MD_SPAN_CODE", MdSpanType::Code as i32);
    value_of!("MD_SPAN_DEL", MdSpanType::Del as i32);
    value_of!("MD_SPAN_LATEXMATH", MdSpanType::LatexMath as i32);
    value_of!(
        "MD_SPAN_LATEXMATH_DISPLAY",
        MdSpanType::LatexMathDisplay as i32
    );
    value_of!("MD_SPAN_WIKILINK", MdSpanType::Wikilink as i32);
    value_of!("MD_SPAN_U", MdSpanType::U as i32);

    value_of!("MD_TEXT_NORMAL", MdTextType::Normal as i32);
    value_of!("MD_TEXT_NULLCHAR", MdTextType::NullChar as i32);
    value_of!("MD_TEXT_BR", MdTextType::Br as i32);
    value_of!("MD_TEXT_SOFTBR", MdTextType::SoftBr as i32);
    value_of!("MD_TEXT_ENTITY", MdTextType::Entity as i32);
    value_of!("MD_TEXT_CODE", MdTextType::Code as i32);
    value_of!("MD_TEXT_HTML", MdTextType::Html as i32);
    value_of!("MD_TEXT_LATEXMATH", MdTextType::LatexMath as i32);

    value_of!("MD_ALIGN_DEFAULT", MdAlign::Default as i32);
    value_of!("MD_ALIGN_LEFT", MdAlign::Left as i32);
    value_of!("MD_ALIGN_CENTER", MdAlign::Center as i32);
    value_of!("MD_ALIGN_RIGHT", MdAlign::Right as i32);

    value_of!(
        "MD_FLAG_COLLAPSEWHITESPACE",
        flags::MD_FLAG_COLLAPSEWHITESPACE
    );
    value_of!(
        "MD_FLAG_PERMISSIVEATXHEADERS",
        flags::MD_FLAG_PERMISSIVEATXHEADERS
    );
    value_of!(
        "MD_FLAG_PERMISSIVEURLAUTOLINKS",
        flags::MD_FLAG_PERMISSIVEURLAUTOLINKS
    );
    value_of!(
        "MD_FLAG_PERMISSIVEEMAILAUTOLINKS",
        flags::MD_FLAG_PERMISSIVEEMAILAUTOLINKS
    );
    value_of!(
        "MD_FLAG_NOINDENTEDCODEBLOCKS",
        flags::MD_FLAG_NOINDENTEDCODEBLOCKS
    );
    value_of!("MD_FLAG_NOHTMLBLOCKS", flags::MD_FLAG_NOHTMLBLOCKS);
    value_of!("MD_FLAG_NOHTMLSPANS", flags::MD_FLAG_NOHTMLSPANS);
    value_of!("MD_FLAG_TABLES", flags::MD_FLAG_TABLES);
    value_of!("MD_FLAG_STRIKETHROUGH", flags::MD_FLAG_STRIKETHROUGH);
    value_of!(
        "MD_FLAG_PERMISSIVEWWWAUTOLINKS",
        flags::MD_FLAG_PERMISSIVEWWWAUTOLINKS
    );
    value_of!("MD_FLAG_TASKLISTS", flags::MD_FLAG_TASKLISTS);
    value_of!("MD_FLAG_LATEXMATHSPANS", flags::MD_FLAG_LATEXMATHSPANS);
    value_of!("MD_FLAG_WIKILINKS", flags::MD_FLAG_WIKILINKS);
    value_of!("MD_FLAG_UNDERLINE", flags::MD_FLAG_UNDERLINE);
    value_of!("MD_FLAG_HARD_SOFT_BREAKS", flags::MD_FLAG_HARD_SOFT_BREAKS);
    value_of!(
        "MD_FLAG_PERMISSIVEAUTOLINKS",
        flags::MD_FLAG_PERMISSIVEAUTOLINKS
    );
    value_of!("MD_FLAG_NOHTML", flags::MD_FLAG_NOHTML);
    value_of!("MD_DIALECT_COMMONMARK", flags::MD_DIALECT_COMMONMARK);
    value_of!("MD_DIALECT_GITHUB", flags::MD_DIALECT_GITHUB);

    out
}

#[test]
fn the_bindings_agree_with_the_c_compiler() {
    let c = from_c();
    let rust = from_rust();

    let mut wrong: Vec<String> = Vec::new();
    for (name, &want) in &c {
        match rust.get(name) {
            Some(&got) if got == want => {}
            Some(&got) => wrong.push(format!("{name}: C says {want}, src/ffi.rs says {got}")),
            None => wrong.push(format!("{name}: C says {want}, src/ffi.rs does not say")),
        }
    }
    for name in rust.keys() {
        if !c.contains_key(name) {
            wrong.push(format!("{name}: src/ffi.rs says so, the header does not"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));

    // A probe table that shrank checks less than it claims to.
    assert!(c.len() >= 120, "only {} probes", c.len());
}

/// The `#[repr(i32)]` on the enums is only sound because C's enums are `int`
/// here, and the `usize`-sized pointers in `MD_ATTRIBUTE` only line up because
/// `MD_SIZE` is 32 bits. Both are asserted above by name; this states the two
/// that would be easiest to lose in the noise.
#[test]
fn the_two_assumptions_the_transcription_rests_on() {
    let c = from_c();
    assert_eq!(c["MD_BLOCKTYPE/size"], size_of::<c_int>());
    assert_eq!(c["MD_SIZE/size"], size_of::<c_uint>());
    assert_eq!(c["MD_CHAR/size"], 1);
}

/// `Option<fn>` has to be the null pointer, or `MD_PARSER`'s two reserved
/// members would not be reserved at all.
#[test]
fn a_none_callback_is_a_null_pointer() {
    assert_eq!(
        size_of::<Option<unsafe extern "C" fn()>>(),
        size_of::<*const ()>()
    );
    let parser = MdParser {
        abi_version: 0,
        flags: 0,
        enter_block: None,
        leave_block: None,
        enter_span: None,
        leave_span: None,
        text: None,
        debug_log: None,
        syntax: None,
    };
    // SAFETY: reading `size_of::<MD_PARSER>()` bytes of an initialised
    // `MdParser` as bytes; every field is an integer or a pointer, so there is
    // no padding byte that is not also zero here.
    let bytes = unsafe {
        std::slice::from_raw_parts((&raw const parser).cast::<u8>(), size_of::<MdParser>())
    };
    assert!(bytes.iter().all(|&b| b == 0), "a None callback is not null");
}
