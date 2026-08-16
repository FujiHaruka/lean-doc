//! Derived from doc-gen4 (Copyright (c) 2021 Henrik Böving, Apache-2.0) and
//! changed; see this repository's NOTICE and `docs/provenance.md`.
//!
//! `Html.escape` — the only escape doc-gen4 applies to HTML text and attributes.
//!
//! It covers `& < > "` and **nothing else**; in particular `'` is left alone
//! (`DocGen4/Output/ToHtmlFormat.lean:35-55`, transcribed in the frozen
//! prototype as `experiments/stage7d/render.ts:330-341`). Every general-purpose
//! HTML-escaping crate also rewrites `'`, and one such character in a page is
//! one byte of mismatch against the acceptance oracle — plan §7.
//!
//! # Why this lives in the markdown crate
//!
//! It is not markdown-specific; it is the escape *every* HTML assembly in this
//! workspace uses. It sits here because this is the lowest crate that assembles
//! HTML, and `lean-doc-render` depends on `lean-doc-md` rather than the other
//! way round. `lean_doc_render::escape_html` re-exports this function — one
//! implementation, so the differential test M1-b built against the prototype
//! keeps covering the copy this crate's renderer calls.

use std::borrow::Cow;

/// True for the four characters `Html.escape` rewrites. `'` is deliberately
/// absent.
const fn is_escapable(byte: u8) -> bool {
    matches!(byte, b'&' | b'<' | b'>' | b'"')
}

/// `Html.escape`: `&` `<` `>` `"` and nothing else.
///
/// Borrows when there is nothing to do, which is the common case for the
/// identifier and type fragments this runs over.
pub fn escape_html(s: &str) -> Cow<'_, str> {
    if !s.bytes().any(is_escapable) {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len() + 8);
    escape_html_into(&mut out, s);
    Cow::Owned(out)
}

/// [`escape_html`] appending to an existing buffer, for the page builder that
/// is concatenating anyway.
pub fn escape_html_into(out: &mut String, s: &str) {
    // The four characters are ASCII, and UTF-8 never encodes a non-ASCII scalar
    // with an ASCII byte, so scanning bytes cannot split a multi-byte scalar.
    let mut rest = s;
    while let Some(at) = rest.bytes().position(is_escapable) {
        out.push_str(&rest[..at]);
        out.push_str(match rest.as_bytes()[at] {
            b'&' => "&amp;",
            b'<' => "&lt;",
            b'>' => "&gt;",
            _ => "&quot;",
        });
        rest = &rest[at + 1..];
    }
    out.push_str(rest);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_four_characters_and_no_others() {
        assert_eq!(
            escape_html("a & b < c > d \" e"),
            "a &amp; b &lt; c &gt; d &quot; e"
        );
        // The apostrophe is what a general-purpose escaper would also rewrite.
        assert_eq!(escape_html("Nat.succ'"), "Nat.succ'");
        assert_eq!(escape_html("a/b"), "a/b");
    }

    #[test]
    fn escape_borrows_when_it_can() {
        assert!(matches!(escape_html("∀ x, p x"), Cow::Borrowed(_)));
        assert!(matches!(escape_html("a<b"), Cow::Owned(_)));
    }

    #[test]
    fn escape_leaves_non_ascii_alone() {
        assert_eq!(escape_html("ℕ ∑ ≐ μ 𝒜 日本語"), "ℕ ∑ ≐ μ 𝒜 日本語");
    }
}
