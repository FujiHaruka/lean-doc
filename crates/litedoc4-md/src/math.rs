//! `$…$` and `$$…$$` to MathML, at build time.
//!
//! `docs/plans/feature-sweep.md` C-1【決定 4】. doc-gen4 leaves the dollars in
//! the page and lets MathJax find them in the browser; this converts them while
//! the page is written, so a reader downloads no script and runs no layout pass
//! for the mathematics. MathML Core is what every current browser draws
//! natively, which is why the target format is MathML and not an image or a
//! span tree.
//!
//! # Which converter, and how that was decided
//!
//! 決定 4 named `pulldown-latex`, because its whole dependency closure is one
//! crate. Measuring it against Mathlib's docstrings overturned that: it escapes
//! `<` and `&` inside `\text{…}` and nowhere else, so `$a < b$` renders as
//! `<mo><</mo>` — markup an HTML parser has to guess at. **61 of Mathlib's
//! 2,123 math spans (2.9%) come out that way**, and `$a < b$` is not an exotic
//! input. [`math_core`] writes none 【実測 2026-08-22 →
//! `benchmarks/results/mathml-2026-08-22.txt`】. The two convert the same
//! corpus to within one span of each other (2,114 against 2,113), so the choice
//! was between eighteen crates and correct markup.
//!
//! # A failed conversion is not rendered
//!
//! [`math_core`] returns an error rather than writing something into the output
//! — there is no in-band failure to detect — and [`to_mathml`] turns that into
//! `None`. The caller then emits the dollars and the source, which is what the
//! page said before this existed.
//!
//! It is not a rare branch. Ten of Mathlib's spans do not convert: four are
//! LaTeX no parser can read (`\sum (i=0}^{q-1}`, `n$th`, …) and the rest use
//! commands this one does not implement 【実測 2026-08-22】.
//!
//! # No annotation, and no equation numbering
//!
//! `MathCoreConfig::annotation` would copy the LaTeX source into the output as
//! an `<annotation encoding="application/x-tex">`; on the Mathlib corpus that is
//! another 40 KB of markup no browser shows and the search index would have to
//! be taught to skip. `convert_with_local_state` rather than
//! `convert_with_global_state` for a related reason: a global equation counter
//! would make one docstring's rendering depend on how many were rendered before
//! it, and pages are rendered in an order the incremental build decides.

use std::sync::OnceLock;

use math_core::{LatexToMathML, MathCoreConfig, MathDisplay};

/// The converter, built once.
///
/// Building it compiles the macro table, so one per span would be the whole
/// cost of the feature. `convert_with_local_state` takes `&self`, so one shared
/// instance is all a renderer needs.
fn converter() -> &'static LatexToMathML {
    static CONVERTER: OnceLock<LatexToMathML> = OnceLock::new();
    CONVERTER.get_or_init(|| {
        LatexToMathML::new(MathCoreConfig {
            // The default. Named because the alternative — `true` — is the one
            // that would put a red "unknown command" into the page instead of
            // letting `to_mathml` answer `None`, which is the whole contract
            // here.
            ignore_unknown_commands: false,
            ..MathCoreConfig::default()
        })
        .expect("the default config defines no macros, so it cannot fail to parse them")
    })
}

/// The MathML for one span, or `None` when the LaTeX does not parse.
///
/// `display` picks `<math display="block">` (`$$…$$`) over
/// `<math display="inline">` (`$…$`).
///
/// ```
/// let html = litedoc4_md::to_mathml("x^2", false).expect("x^2 parses");
/// assert_eq!(html, "<math><msup><mi>x</mi><mn>2</mn></msup></math>");
/// assert!(litedoc4_md::to_mathml("\\colim_k F", false).is_none());
/// ```
#[must_use]
pub fn to_mathml(latex: &str, display: bool) -> Option<String> {
    let display = if display {
        MathDisplay::Block
    } else {
        MathDisplay::Inline
    };
    converter()
        .convert_with_local_state(latex, display)
        .ok()
        .map(|result| result.mathml)
}

#[cfg(test)]
mod tests {
    use super::to_mathml;

    /// Inline is the MathML default, so it is written by **leaving the
    /// attribute out** — asserting `display="inline"` would be asserting a
    /// spelling nothing requires.
    #[test]
    fn inline_and_display_are_told_apart_in_the_output() {
        let inline = to_mathml("x", false).expect("x parses");
        let block = to_mathml("x", true).expect("x parses");
        assert_eq!(inline, "<math><mi>x</mi></math>");
        assert!(block.contains("display=\"block\""), "{block}");
        assert_eq!(block.replace(" display=\"block\"", ""), inline);
    }

    #[test]
    fn a_sum_becomes_elements_and_not_text() {
        let html = to_mathml("\\sum_{i=0}^{n} x_i", true).expect("a sum parses");
        assert!(html.contains("<munderover>"), "{html}");
        assert!(!html.contains('$'), "{html}");
    }

    /// The reason this crate is `math-core` and not `pulldown-latex`: the
    /// characters HTML reserves have to leave as entities, in *math* content and
    /// not only inside `\text{…}`.
    #[test]
    fn html_metacharacters_come_out_escaped() {
        let lt = to_mathml("a < b", false).expect("a < b parses");
        assert!(lt.contains("&lt;"), "{lt}");
        assert!(!lt.contains("<mo><"), "{lt}");

        let amp = to_mathml("\\text{a \\& b}", false).expect("an escaped ampersand parses");
        assert!(amp.contains("&amp;"), "{amp}");
    }

    /// Nothing this function returns may carry a bare `<` or `&` outside a tag.
    /// Written as a scan rather than as three `contains` because the failure it
    /// guards against is *any* content character reaching the page unescaped,
    /// not the three that were known when it was written.
    #[test]
    fn no_output_carries_markup_outside_a_tag() {
        for latex in [
            "a < b",
            "a > b",
            "a \\le b < c",
            "\\text{if } a < b",
            "\\{x : x < 1\\}",
            "f(x) = \\begin{cases} 1 & x < 0 \\\\ 0 & x \\ge 0 \\end{cases}",
        ] {
            let html = to_mathml(latex, false).unwrap_or_else(|| panic!("{latex} should parse"));
            assert!(strays(&html).is_empty(), "{latex} -> {html}");
        }
    }

    /// The four shapes that fail, all but the last from the Mathlib corpus.
    #[test]
    fn unreadable_latex_answers_none() {
        for latex in [
            "x = x' + \\sum (i=0}^{q-1} y",  // unbalanced group
            "\\lim_{x\\to\\infty^{-1}|x|_m", // never closed
            "\\colim_k F(k)",                // not a command this parser has
            "\\thiscommanddoesnotexist",
        ] {
            assert_eq!(to_mathml(latex, false), None, "{latex} should not render");
        }
    }

    /// A span that is empty or only spaces is legal LaTeX; it must not be
    /// mistaken for a failure.
    #[test]
    fn an_empty_span_is_not_a_failure() {
        assert!(to_mathml("", false).is_some());
        assert!(to_mathml("   ", false).is_some());
    }

    /// Every `<` that does not open a tag, and every `&` that does not open an
    /// entity. Returns the byte offsets so a failure names where.
    fn strays(html: &str) -> Vec<usize> {
        let bytes = html.as_bytes();
        let mut found = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'<' => {
                    let mut j = i + 1;
                    if bytes.get(j) == Some(&b'/') {
                        j += 1;
                    }
                    if bytes.get(j).is_some_and(u8::is_ascii_alphabetic) {
                        while i < bytes.len() && bytes[i] != b'>' {
                            i += 1;
                        }
                    } else {
                        found.push(i);
                    }
                    i += 1;
                }
                b'&' => {
                    let rest = &html[i..];
                    let entity = rest[1..]
                        .find(';')
                        .is_some_and(|end| end > 0 && end <= 8 && !rest[1..=end].contains('<'));
                    if !entity {
                        found.push(i);
                    }
                    i += 1;
                }
                _ => i += 1,
            }
        }
        found
    }
}
