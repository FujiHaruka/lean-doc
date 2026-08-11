//! Tag spans over a printed fragment.
//!
//! The extractor flattens doc-gen4's `CodeWithInfos` tree into a pre-order list
//! of spans over the fragment's plain text. On the wire each span is a JSON
//! array in one of three shapes:
//!
//! ```text
//! [start, stop, kind]                      kind 0 (fn) or 2 (sort)
//! [start, stop, 1, name]                   kind 1 (const), no whitespace to restore
//! [start, stop, 1, name, front, back]      kind 1 (const), schema 3 widths
//! ```
//!
//! All four numbers are **UTF-16 code unit** offsets into the fragment text —
//! see [`crate::Utf16Text`]. They are left as plain `u32` here on purpose: the
//! text type refuses to be indexed by bytes, so there is no byte offset around
//! for them to be confused with.

use std::ops::Range;

use serde::de::{self, Deserialize, Deserializer, SeqAccess, Visitor};
use std::fmt;

/// What a span tags. Numbered as the extractor writes them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SpanKind {
    /// `0` — a printed sub-expression with no link of its own (`span.fn`).
    Fn,
    /// `1` — a constant reference, carrying the constant's name.
    Const,
    /// `2` — a sort (`Type`, `Prop`, ...), linked to `foundational_types.html`.
    Sort,
    /// Any other code. Never produced by the current extractor; kept so that
    /// reading an IR from a newer writer cannot fail, and so the renderer can
    /// decide what to do rather than the reader deciding for it.
    Other(u8),
}

impl SpanKind {
    pub fn from_code(code: u8) -> Self {
        match code {
            0 => Self::Fn,
            1 => Self::Const,
            2 => Self::Sort,
            other => Self::Other(other),
        }
    }

    pub fn code(self) -> u8 {
        match self {
            Self::Fn => 0,
            Self::Const => 1,
            Self::Sort => 2,
            Self::Other(other) => other,
        }
    }
}

/// One tag position over a printed fragment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    /// First UTF-16 code unit of the tagged text.
    pub start: u32,
    /// One past the last UTF-16 code unit of the tagged text.
    pub stop: u32,
    pub kind: SpanKind,
    /// The constant's name. `Some` exactly when the wire form carried a fourth
    /// element, which the extractor only writes for [`SpanKind::Const`].
    pub name: Option<String>,
    /// Schema 3: UTF-16 code units of whitespace immediately *before* `start`
    /// that doc-gen4 rewrites as plain spaces. Zero when the wire form was the
    /// short one.
    pub front: u32,
    /// Same, immediately *after* `stop`.
    pub back: u32,
}

impl Span {
    pub fn is_const(&self) -> bool {
        self.kind == SpanKind::Const
    }

    /// The tagged text's own range.
    pub fn range(&self) -> Range<u32> {
        self.start..self.stop
    }

    /// The leading whitespace run doc-gen4 turns into spaces, if any.
    pub fn front_range(&self) -> Option<Range<u32>> {
        (self.front > 0).then(|| self.start - self.front..self.start)
    }

    /// The trailing whitespace run doc-gen4 turns into spaces, if any.
    pub fn back_range(&self) -> Option<Range<u32>> {
        (self.back > 0).then(|| self.stop..self.stop + self.back)
    }
}

impl<'de> Deserialize<'de> for Span {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_seq(SpanVisitor)
    }
}

struct SpanVisitor;

impl<'de> Visitor<'de> for SpanVisitor {
    type Value = Span;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a span array of 3, 4 or 6 elements")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Span, A::Error> {
        let number = |index: usize, seq: &mut A| -> Result<u32, A::Error> {
            seq.next_element::<u32>()?
                .ok_or_else(|| de::Error::invalid_length(index, &self))
        };
        let start = number(0, &mut seq)?;
        let stop = number(1, &mut seq)?;
        let code = number(2, &mut seq)?;
        let code = u8::try_from(code)
            .map_err(|_| de::Error::custom(format!("span kind {code} does not fit a byte")))?;

        let name = seq.next_element::<String>()?;
        let front = seq.next_element::<u32>()?;
        let back = seq.next_element::<u32>()?;
        if seq.next_element::<de::IgnoredAny>()?.is_some() {
            return Err(de::Error::custom("span array has more than 6 elements"));
        }
        // The widths only mean something as a pair, so a 5-element array is a
        // malformed span rather than a short one. A name is implied by them
        // positionally: element 3 cannot be skipped.
        let (front, back) = match (front, back) {
            (None, None) => (0, 0),
            (Some(front), Some(back)) => (front, back),
            _ => return Err(de::Error::invalid_length(5, &self)),
        };

        Ok(Span {
            start,
            stop,
            kind: SpanKind::from_code(code),
            name,
            front,
            back,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> Span {
        serde_json::from_str(json).expect("valid span")
    }

    #[test]
    fn three_element_form() {
        let s = parse("[0,7,0]");
        assert_eq!(s.start, 0);
        assert_eq!(s.stop, 7);
        assert_eq!(s.kind, SpanKind::Fn);
        assert_eq!(s.name, None);
        assert_eq!((s.front, s.back), (0, 0));
        assert_eq!(s.front_range(), None);
        assert_eq!(s.back_range(), None);
    }

    #[test]
    fn four_element_form_names_a_constant() {
        let s = parse(r#"[2,5,1,"Nat.succ"]"#);
        assert!(s.is_const());
        assert_eq!(s.name.as_deref(), Some("Nat.succ"));
        assert_eq!(s.range(), 2..5);
    }

    #[test]
    fn six_element_form_carries_widths() {
        let s = parse(r#"[4,6,1,"HAdd.hAdd",1,3]"#);
        assert_eq!((s.front, s.back), (1, 3));
        assert_eq!(s.front_range(), Some(3..4));
        assert_eq!(s.back_range(), Some(6..9));
    }

    #[test]
    fn sort_kind() {
        assert_eq!(parse("[0,4,2]").kind, SpanKind::Sort);
    }

    #[test]
    fn unknown_kind_is_kept() {
        assert_eq!(parse("[0,1,9]").kind, SpanKind::Other(9));
    }

    #[test]
    fn malformed_forms_are_rejected() {
        for bad in [
            "[0,1]",
            r#"[0,1,1,"n",2]"#,
            r#"[0,1,1,"n",2,3,4]"#,
            "[0,1,300]",
        ] {
            assert!(
                serde_json::from_str::<Span>(bad).is_err(),
                "expected {bad} to be rejected"
            );
        }
    }
}
