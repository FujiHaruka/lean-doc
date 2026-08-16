//! The one rewrite every oracle in this directory needs, in one place.
//!
//! **M8, gate UI-2** (`docs/plans/ui-redesign.md` §1). doc-gen4 and the frozen
//! prototype both turn a source path in a code span — `` `EPI/Stam/ToBridge.lean` ``
//! — into a link without consulting anything: the path is read as relative to
//! the repository root and the extension is swapped. The measurement target
//! writes those paths relative to the *module* they sit in, so the page named
//! does not exist; 160 of the site's 32,868 internal links were dangling
//! 【実測 2026-08-16, `benchmarks/results/m8-ui2-dead-links.txt`】. This crate
//! asks `NameIndex::module_for_source_path` instead, which is the only branch of
//! `nameToLink?` where its answer is deliberately not its oracle's.
//!
//! So the oracles compare against their recorded bytes *with that one branch
//! re-answered* ([`rewrite_source_path_anchors`]) and pin how many anchors
//! moved ([`Tally`]). Everything else stays byte for byte, and a second
//! divergence still fails.

// Each test binary uses a part of this module; `mod common;` compiles all of it
// into each of them.
#![allow(dead_code)]

/// What [`rewrite_source_path_anchors`] did, so that a comparison cannot pass
/// by rewriting everything or by rewriting nothing.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Tally {
    /// The new answer names the page the oracle named.
    pub unchanged: usize,
    /// The new answer names a different page.
    pub relinked: usize,
    /// There is no new answer: no module matches the path, or several do.
    pub dropped: usize,
}

impl Tally {
    /// Every source-path anchor seen, however it was answered.
    pub fn total(&self) -> usize {
        self.unchanged + self.relinked + self.dropped
    }
}

/// `html` with every anchor whose text is a **source path** re-answered by
/// `answer`, which is given the path without its `.lean`.
///
/// `Some(href)` replaces the anchor's href and `None` replaces the whole anchor
/// with its text — which is exactly what `autoLinkInline` writes for a word that
/// resolves to nothing. Every other anchor, and every byte between anchors, is
/// copied.
pub fn rewrite_source_path_anchors(
    html: &str,
    answer: &dyn Fn(&str) -> Option<String>,
    tally: &mut Tally,
) -> String {
    const OPEN: &str = "<a href=\"";
    let mut out = String::new();
    let mut rest = html;
    while let Some(at) = rest.find(OPEN) {
        let after = &rest[at + OPEN.len()..];
        let (Some(shut), Some(end)) = (after.find("\">"), after.find("</a>")) else {
            break;
        };
        let (href, text) = (&after[..shut], &after[shut + 2..end]);
        let path = unescape(text);
        // A source path is what `nameToLink?`'s first branch takes: `.lean` and
        // a `/`. The anchor text is the word `splitAround` cut out, so it is the
        // path itself.
        let Some(stem) = path.strip_suffix(".lean").filter(|p| p.contains('/')) else {
            out.push_str(&rest[..at + OPEN.len() + end + 4]);
            rest = &after[end + 4..];
            continue;
        };
        out.push_str(&rest[..at]);
        match answer(stem) {
            Some(link) => {
                // Nothing in these corpora needs escaping in an href, and a
                // corpus that did would need this helper to escape it.
                assert!(
                    !link.contains(['&', '<', '>', '"']),
                    "{link} needs escaping"
                );
                if link == unescape(href) {
                    tally.unchanged += 1;
                } else {
                    tally.relinked += 1;
                }
                out.push_str(OPEN);
                out.push_str(&link);
                out.push_str("\">");
                out.push_str(text);
                out.push_str("</a>");
            }
            None => {
                tally.dropped += 1;
                out.push_str(text);
            }
        }
        rest = &after[end + 4..];
    }
    out.push_str(rest);
    out
}

/// The four characters `Html.escape` writes, back.
pub fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
}
