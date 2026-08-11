//! Replaying doc-gen4's `splitWhitespaces` from the schema-3 widths.
//!
//! Ported from `experiments/stage7d/render.ts` (`applyWsWidths` 555-593), which
//! is frozen. Its comment explains the history; what matters to the port is the
//! contract:
//!
//! doc-gen4 rewrites the whitespace that sits immediately outside a tagged
//! sub-expression as plain spaces, so a `\n` or `\t` there comes out as `' '`.
//! Every tag carries the width of that run on each side, in UTF-16 code units,
//! and the rewrite is length-preserving: no offset moves, so the spans stay
//! valid over the result. That is why this returns a [`Utf16Text`] rather than
//! a `String` — the caller keeps slicing it with the same span offsets.
//!
//! The ranges are disjoint by construction. An overlap or an out-of-range width
//! means the IR disagrees with its own text, and this panics rather than
//! quietly producing plausible bytes — the prototype threw, and
//! [`Utf16Text::slice`] takes the same position.

use std::borrow::Cow;
use std::ops::Range;

use lean_doc_ir::{Span, Utf16Text};

/// The rewritten fragment, plus what the prototype's report counts.
pub struct WsRewrite<'a> {
    /// The fragment with the whitespace runs flattened. Borrowed when nothing
    /// needed flattening.
    pub text: Cow<'a, Utf16Text>,
    /// Code units that were not already `' '`, i.e. the prototype's
    /// `wsWidthChars`. Zero exactly when `text` is borrowed.
    pub changed_units: u32,
}

impl WsRewrite<'_> {
    pub fn as_text(&self) -> &Utf16Text {
        &self.text
    }
}

/// Rewrites the whitespace runs the spans point at as plain spaces.
///
/// # Panics
///
/// If the widths overlap, run backwards, or reach past the end of the
/// fragment.
pub fn apply_ws_widths<'a>(text: &'a Utf16Text, spans: &[Span]) -> WsRewrite<'a> {
    let unchanged = || WsRewrite {
        text: Cow::Borrowed(text),
        changed_units: 0,
    };

    let mut ranges: Vec<Range<u32>> = Vec::new();
    for span in spans {
        ranges.extend(span.front_range());
        ranges.extend(span.back_range());
    }
    if ranges.is_empty() {
        return unchanged();
    }
    ranges.sort_by_key(|range| range.start);

    let units = text.len_utf16();
    let mut changed = 0;
    let mut end = 0;
    for range in &ranges {
        assert!(
            range.start >= end && range.end <= units,
            "schema-3 whitespace width out of range: [{},{}) in a {units}-unit fragment \
             (previous run ended at {end})",
            range.start,
            range.end,
        );
        for at in range.clone() {
            if text.unit(at) != Some(u16::from(b' ')) {
                changed += 1;
            }
        }
        end = range.end;
    }
    if changed == 0 {
        return unchanged();
    }

    let mut out = String::with_capacity(text.as_str().len());
    let mut pos = 0;
    for range in &ranges {
        out.push_str(text.slice(pos..range.start));
        // Every scalar being replaced is whitespace, and no whitespace scalar
        // lives above U+FFFF, so one space per code unit keeps the length.
        for _ in range.clone() {
            out.push(' ');
        }
        pos = range.end;
    }
    out.push_str(text.slice(pos..units));

    let out = Utf16Text::new(out);
    debug_assert_eq!(
        out.len_utf16(),
        units,
        "the whitespace rewrite must not move any offset"
    );
    WsRewrite {
        text: Cow::Owned(out),
        changed_units: changed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use lean_doc_ir::SpanKind;

    /// The wire form `[start, stop, 1, name, front, back]`.
    fn span(start: u32, stop: u32, front: u32, back: u32) -> Span {
        Span {
            start,
            stop,
            kind: SpanKind::Const,
            name: Some("n".to_owned()),
            front,
            back,
        }
    }

    /// The wire form `[start, stop, 0]` — no widths at all.
    fn plain(start: u32, stop: u32) -> Span {
        Span {
            start,
            stop,
            kind: SpanKind::Fn,
            name: None,
            front: 0,
            back: 0,
        }
    }

    #[test]
    fn no_widths_borrows() {
        let text = Utf16Text::from("f\n x");
        let out = apply_ws_widths(&text, &[plain(0, 1), span(3, 4, 0, 0)]);
        assert!(matches!(out.text, Cow::Borrowed(_)));
        assert_eq!(out.as_text().as_str(), "f\n x");
    }

    #[test]
    fn newline_and_tab_become_spaces() {
        // "a =\n\tb": the tag `=` has a 1-unit front run and a 2-unit back run.
        let text = Utf16Text::from("a =\n\tb");
        let out = apply_ws_widths(&text, &[span(2, 3, 1, 2)]);
        assert_eq!(out.as_text().as_str(), "a =  b");
        assert_eq!(out.changed_units, 2);
    }

    #[test]
    fn runs_that_are_already_spaces_borrow() {
        let text = Utf16Text::from("a = b");
        let out = apply_ws_widths(&text, &[span(2, 3, 1, 1)]);
        assert!(matches!(out.text, Cow::Borrowed(_)));
        assert_eq!(out.changed_units, 0);
    }

    /// The widths are UTF-16 code units, so a fragment with a surrogate pair in
    /// it is the case a byte-indexed port gets wrong.
    #[test]
    fn offsets_are_utf16_code_units() {
        // "𝓧\n:\tType" — 𝓧 is two code units, so the tag on `:` starts at 3.
        let text = Utf16Text::from("𝓧\n:\tType");
        assert_eq!(text.len_utf16(), 2 + 1 + 1 + 1 + 4);
        let out = apply_ws_widths(&text, &[span(3, 4, 1, 1)]);
        assert_eq!(out.as_text().as_str(), "𝓧 : Type");
        assert_eq!(out.changed_units, 2);
        assert_eq!(out.as_text().len_utf16(), text.len_utf16());
        // And the spans still address the same text afterwards.
        assert_eq!(out.as_text().slice(3..4), ":");
    }

    #[test]
    fn several_runs_are_sorted_before_use() {
        let text = Utf16Text::from("a\tb\tc\td");
        // The `c` tag only claims the run after it: the one before is already
        // claimed as the `b` tag's trailing run, and the two may not overlap.
        let spans = [span(4, 5, 0, 1), span(2, 3, 1, 1)];
        let out = apply_ws_widths(&text, &spans);
        assert_eq!(out.as_text().as_str(), "a b c d");
        assert_eq!(out.changed_units, 3);
    }

    #[test]
    #[should_panic(expected = "out of range")]
    fn overlapping_runs_panic() {
        let text = Utf16Text::from("a\tb\tc");
        apply_ws_widths(&text, &[span(2, 3, 1, 1), span(2, 3, 1, 1)]);
    }

    #[test]
    #[should_panic(expected = "out of range")]
    fn a_run_past_the_end_panics() {
        let text = Utf16Text::from("a\t");
        apply_ws_widths(&text, &[span(0, 1, 0, 3)]);
    }
}
