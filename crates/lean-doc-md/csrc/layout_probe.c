/* layout_probe.c -- md4c's ABI as the C compiler sees it.
 *
 * `src/ffi.rs` writes md4c's structs, enums and flags down by hand. Nothing in
 * a build would notice if one of them were wrong: a `#[repr(C)]` struct with a
 * field of the wrong width still links, and the parser then reads details from
 * the wrong offsets and produces a plausible, wrong tree.
 *
 * So the numbers are taken from the header instead of trusted. Every entry
 * below is computed by the C compiler from `vendor/md4c/md4c.h`, and
 * `tests/abi.rs` asserts that the Rust side agrees with all of them.
 *
 * This file is ours, not md4c's; the vendored sources stay untouched.
 */

#include <stddef.h>

#include "md4c.h"

typedef struct lean_doc_md4c_probe {
    /* NULL marks the end of the table. */
    const char* name;
    size_t value;
} lean_doc_md4c_probe;

#define SIZE_OF(type)          { #type "/size", sizeof(type) },
#define ALIGN_OF(type)         { #type "/align", _Alignof(type) },
#define OFFSET_OF(type, field) { #type "." #field, offsetof(type, field) },
#define VALUE_OF(name)         { #name, (size_t)(name) },

static const lean_doc_md4c_probe probes[] = {
    /* Scalars. */
    SIZE_OF(MD_CHAR)
    SIZE_OF(MD_SIZE)
    SIZE_OF(MD_OFFSET)
    SIZE_OF(MD_BLOCKTYPE)
    SIZE_OF(MD_SPANTYPE)
    SIZE_OF(MD_TEXTTYPE)
    SIZE_OF(MD_ALIGN)

    /* MD_ATTRIBUTE. */
    SIZE_OF(MD_ATTRIBUTE)
    ALIGN_OF(MD_ATTRIBUTE)
    OFFSET_OF(MD_ATTRIBUTE, text)
    OFFSET_OF(MD_ATTRIBUTE, size)
    OFFSET_OF(MD_ATTRIBUTE, substr_types)
    OFFSET_OF(MD_ATTRIBUTE, substr_offsets)

    /* Block details. */
    SIZE_OF(MD_BLOCK_UL_DETAIL)
    ALIGN_OF(MD_BLOCK_UL_DETAIL)
    OFFSET_OF(MD_BLOCK_UL_DETAIL, is_tight)
    OFFSET_OF(MD_BLOCK_UL_DETAIL, mark)

    SIZE_OF(MD_BLOCK_OL_DETAIL)
    ALIGN_OF(MD_BLOCK_OL_DETAIL)
    OFFSET_OF(MD_BLOCK_OL_DETAIL, start)
    OFFSET_OF(MD_BLOCK_OL_DETAIL, is_tight)
    OFFSET_OF(MD_BLOCK_OL_DETAIL, mark_delimiter)

    SIZE_OF(MD_BLOCK_LI_DETAIL)
    ALIGN_OF(MD_BLOCK_LI_DETAIL)
    OFFSET_OF(MD_BLOCK_LI_DETAIL, is_task)
    OFFSET_OF(MD_BLOCK_LI_DETAIL, task_mark)
    OFFSET_OF(MD_BLOCK_LI_DETAIL, task_mark_offset)

    SIZE_OF(MD_BLOCK_H_DETAIL)
    ALIGN_OF(MD_BLOCK_H_DETAIL)
    OFFSET_OF(MD_BLOCK_H_DETAIL, level)

    SIZE_OF(MD_BLOCK_CODE_DETAIL)
    ALIGN_OF(MD_BLOCK_CODE_DETAIL)
    OFFSET_OF(MD_BLOCK_CODE_DETAIL, info)
    OFFSET_OF(MD_BLOCK_CODE_DETAIL, lang)
    OFFSET_OF(MD_BLOCK_CODE_DETAIL, fence_char)

    SIZE_OF(MD_BLOCK_TABLE_DETAIL)
    ALIGN_OF(MD_BLOCK_TABLE_DETAIL)
    OFFSET_OF(MD_BLOCK_TABLE_DETAIL, col_count)
    OFFSET_OF(MD_BLOCK_TABLE_DETAIL, head_row_count)
    OFFSET_OF(MD_BLOCK_TABLE_DETAIL, body_row_count)

    SIZE_OF(MD_BLOCK_TD_DETAIL)
    ALIGN_OF(MD_BLOCK_TD_DETAIL)
    OFFSET_OF(MD_BLOCK_TD_DETAIL, align)

    /* Span details. */
    SIZE_OF(MD_SPAN_A_DETAIL)
    ALIGN_OF(MD_SPAN_A_DETAIL)
    OFFSET_OF(MD_SPAN_A_DETAIL, href)
    OFFSET_OF(MD_SPAN_A_DETAIL, title)
    OFFSET_OF(MD_SPAN_A_DETAIL, is_autolink)

    SIZE_OF(MD_SPAN_IMG_DETAIL)
    ALIGN_OF(MD_SPAN_IMG_DETAIL)
    OFFSET_OF(MD_SPAN_IMG_DETAIL, src)
    OFFSET_OF(MD_SPAN_IMG_DETAIL, title)

    SIZE_OF(MD_SPAN_WIKILINK_DETAIL)
    ALIGN_OF(MD_SPAN_WIKILINK_DETAIL)
    OFFSET_OF(MD_SPAN_WIKILINK_DETAIL, target)

    /* The callback table. */
    SIZE_OF(MD_PARSER)
    ALIGN_OF(MD_PARSER)
    OFFSET_OF(MD_PARSER, abi_version)
    OFFSET_OF(MD_PARSER, flags)
    OFFSET_OF(MD_PARSER, enter_block)
    OFFSET_OF(MD_PARSER, leave_block)
    OFFSET_OF(MD_PARSER, enter_span)
    OFFSET_OF(MD_PARSER, leave_span)
    OFFSET_OF(MD_PARSER, text)
    OFFSET_OF(MD_PARSER, debug_log)
    OFFSET_OF(MD_PARSER, syntax)

    /* Enumerators. An enum whose values slid by one would still compile on
     * both sides; only this catches it. */
    VALUE_OF(MD_BLOCK_DOC)
    VALUE_OF(MD_BLOCK_QUOTE)
    VALUE_OF(MD_BLOCK_UL)
    VALUE_OF(MD_BLOCK_OL)
    VALUE_OF(MD_BLOCK_LI)
    VALUE_OF(MD_BLOCK_HR)
    VALUE_OF(MD_BLOCK_H)
    VALUE_OF(MD_BLOCK_CODE)
    VALUE_OF(MD_BLOCK_HTML)
    VALUE_OF(MD_BLOCK_P)
    VALUE_OF(MD_BLOCK_TABLE)
    VALUE_OF(MD_BLOCK_THEAD)
    VALUE_OF(MD_BLOCK_TBODY)
    VALUE_OF(MD_BLOCK_TR)
    VALUE_OF(MD_BLOCK_TH)
    VALUE_OF(MD_BLOCK_TD)

    VALUE_OF(MD_SPAN_EM)
    VALUE_OF(MD_SPAN_STRONG)
    VALUE_OF(MD_SPAN_A)
    VALUE_OF(MD_SPAN_IMG)
    VALUE_OF(MD_SPAN_CODE)
    VALUE_OF(MD_SPAN_DEL)
    VALUE_OF(MD_SPAN_LATEXMATH)
    VALUE_OF(MD_SPAN_LATEXMATH_DISPLAY)
    VALUE_OF(MD_SPAN_WIKILINK)
    VALUE_OF(MD_SPAN_U)

    VALUE_OF(MD_TEXT_NORMAL)
    VALUE_OF(MD_TEXT_NULLCHAR)
    VALUE_OF(MD_TEXT_BR)
    VALUE_OF(MD_TEXT_SOFTBR)
    VALUE_OF(MD_TEXT_ENTITY)
    VALUE_OF(MD_TEXT_CODE)
    VALUE_OF(MD_TEXT_HTML)
    VALUE_OF(MD_TEXT_LATEXMATH)

    VALUE_OF(MD_ALIGN_DEFAULT)
    VALUE_OF(MD_ALIGN_LEFT)
    VALUE_OF(MD_ALIGN_CENTER)
    VALUE_OF(MD_ALIGN_RIGHT)

    /* Flags. The dialect constants are what the acceptance oracle depends on:
     * a different bitmask is a different Markdown. */
    VALUE_OF(MD_FLAG_COLLAPSEWHITESPACE)
    VALUE_OF(MD_FLAG_PERMISSIVEATXHEADERS)
    VALUE_OF(MD_FLAG_PERMISSIVEURLAUTOLINKS)
    VALUE_OF(MD_FLAG_PERMISSIVEEMAILAUTOLINKS)
    VALUE_OF(MD_FLAG_NOINDENTEDCODEBLOCKS)
    VALUE_OF(MD_FLAG_NOHTMLBLOCKS)
    VALUE_OF(MD_FLAG_NOHTMLSPANS)
    VALUE_OF(MD_FLAG_TABLES)
    VALUE_OF(MD_FLAG_STRIKETHROUGH)
    VALUE_OF(MD_FLAG_PERMISSIVEWWWAUTOLINKS)
    VALUE_OF(MD_FLAG_TASKLISTS)
    VALUE_OF(MD_FLAG_LATEXMATHSPANS)
    VALUE_OF(MD_FLAG_WIKILINKS)
    VALUE_OF(MD_FLAG_UNDERLINE)
    VALUE_OF(MD_FLAG_HARD_SOFT_BREAKS)
    VALUE_OF(MD_FLAG_PERMISSIVEAUTOLINKS)
    VALUE_OF(MD_FLAG_NOHTML)
    VALUE_OF(MD_DIALECT_COMMONMARK)
    VALUE_OF(MD_DIALECT_GITHUB)

    { NULL, 0 }
};

const lean_doc_md4c_probe* lean_doc_md4c_probes(void)
{
    return probes;
}
