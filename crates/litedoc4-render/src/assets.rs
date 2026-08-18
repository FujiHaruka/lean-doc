//! The static assets a page needs to look like a page, carried in the binary.
//!
//! Milestone **M8-a**, and the whole of `docs/plans/ui-redesign.md` 決定 6. Up
//! to here the renderer *referenced* assets it never produced: [`crate::frame`]
//! writes `<link rel="stylesheet" href="…style.css">` into all 432 pages and
//! nothing under the site root has ever answered it. A tree that has to be
//! completed by hand before it can be opened is not a tree `litedoc4 build`
//! finished.
//!
//! # Why the bytes are in the binary rather than next to it
//!
//! `include_str!`, so installation stays `cargo install` and — the part that is
//! about correctness rather than convenience — **the assets cannot be a version
//! behind the renderer**. The CSS is coupled to class names [`crate::decl`] and
//! [`crate::frame`] emit; a stylesheet shipped separately can be the one from
//! before those names moved, and the failure is a page that renders with half
//! its rules silently not matching. Two things in one binary cannot drift.
//!
//! # These are **not** in `renderKey`, and that is a decision
//!
//! The incremental ledger's render key exists to answer "would this page's
//! bytes come out differently now" (`docs/implementation-plan.md`,
//! `litedoc4_incr::render_key`). **A page's bytes do not depend on these
//! files** — it names them in an `href` and the href does not carry their
//! content — so putting them in the key would re-render 432 pages every time a
//! CSS rule moved and buy nothing at all. The other half of the decision is
//! what pays for it: [`write_assets`] runs on **every** build, full or
//! incremental, so the tree is never carrying yesterday's stylesheet either.
//!
//! # What is here, and what is still a placeholder
//!
//! | | referenced by | written by |
//! |---|---|---|
//! | `style.css` | [`crate::head_html`] | M8-b |
//! | `app.js` | [`crate::head_html`] | M8-c |
//! | `favicon.svg` | [`crate::head_html`] (`icon`) | M8-a |
//!
//! # One test here is about the stylesheet rather than the plumbing
//!
//! This module's `every_class_the_renderer_emits_is_styled` test — not a link,
//! because `#[cfg(test)]` items are invisible to rustdoc — compares the class
//! names in the renderer's string literals against the selectors in `style.css`.
//! It lives here because this is where the two sides meet, and it exists because
//! the failure it catches is **silent**: a class renamed in `decl.rs` still
//! renders, still validates, and simply has no styling. Nothing else in the suite
//! opens a browser, so nothing else would ever notice.

use std::fs;
use std::path::Path;

use crate::site::Error;

/// Each asset's path **under the site root**, paired with its bytes, in the
/// order [`write_assets`] writes them.
///
/// The paths are flat and relative because that is what the pages ask for:
/// [`crate::head_html`] builds every asset href as [`crate::page_root`] of the
/// page plus this name, so `Pkg/A/B.html` reaches the same `style.css` as
/// `Pkg.html` does. There is no separator in any of the three; one added later
/// stays a `/` — a URL path on every platform — exactly as
/// `litedoc4_global::ARTIFACT_PATHS` spells `declarations/name-map.json`.
///
/// Shaped after `litedoc4_global::Artifacts::files`: one array of
/// (path, content) pairs is the whole interface, so a caller that wants to
/// write them, hash them or compare them against a tree does not need three
/// different accessors.
pub const ASSETS: [(&str, &str); 3] = [
    ("style.css", include_str!("../assets/style.css")),
    ("app.js", include_str!("../assets/app.js")),
    ("favicon.svg", include_str!("../assets/favicon.svg")),
];

/// Writes all three under `site`, creating it if it is not there.
///
/// **Unconditional and idempotent**: it overwrites whatever is at each path
/// rather than asking whether it differs. The reason is the failure it removes —
/// a build that skips an asset because "it was already there" is one that leaves
/// an *edited* or truncated file in place, and nothing downstream would ever
/// notice. Three files of a few kilobytes is not a cost worth a comparison, let
/// alone a cache.
///
/// It is a whole-tree operation, so it is **not** part of
/// [`crate::render_site`]: that one is called per incremental round, with a
/// subset of the modules and sometimes with none at all, and the assets have to
/// land on the runs where nothing is re-rendered too.
pub fn write_assets(site: &Path) -> Result<(), Error> {
    fs::create_dir_all(site).map_err(|source| Error::Io {
        path: site.to_owned(),
        source,
    })?;
    for (name, body) in ASSETS {
        let path = site.join(name);
        fs::write(&path, body).map_err(|source| Error::Io { path, source })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unique directory under the system temporary one, removed with its
    /// contents when the test ends.
    struct TempDir {
        path: std::path::PathBuf,
    }

    impl TempDir {
        fn new(what: &str) -> Self {
            use std::sync::atomic::{AtomicU32, Ordering};
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let path = std::env::temp_dir().join(format!(
                "litedoc4-assets-{}-{}-{what}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed),
            ));
            let _ = fs::remove_dir_all(&path);
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    /// The class names in one Rust string literal — `class=\"a b\"` gives `a`
    /// and `b`.
    ///
    /// Matching on the **escaped** quote is what keeps prose out of the result:
    /// a doc comment writes `class="x"` with plain quotes, only a string literal
    /// writes `class=\"x\"`, and this module's own headings talk about classes
    /// at length.
    fn emitted_classes(source: &str) -> Vec<&str> {
        const OPEN: &str = "class=\\\"";
        let mut out = Vec::new();
        let mut rest = source;
        while let Some(at) = rest.find(OPEN) {
            rest = &rest[at + OPEN.len()..];
            let Some(end) = rest.find("\\\"") else { break };
            out.extend(rest[..end].split_whitespace());
            rest = &rest[end..];
        }
        out
    }

    /// Every `.name` in the stylesheet, from anywhere in it.
    ///
    /// Deliberately crude — it takes selectors out of comments too, and it does
    /// not care whether `.a` appeared as `.a` or as `.b .a`. The direction that
    /// matters is "emitted but not styled", and over-collecting here can only
    /// make the test *milder*, never wrong. A `.5rem` is not picked up because
    /// the character after the dot has to be a letter.
    fn styled_classes(css: &str) -> std::collections::HashSet<&str> {
        let bytes = css.as_bytes();
        let mut out = std::collections::HashSet::new();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] != b'.' || !bytes.get(i + 1).is_some_and(u8::is_ascii_alphabetic) {
                i += 1;
                continue;
            }
            let start = i + 1;
            let mut end = start;
            while bytes
                .get(end)
                .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'-' || *b == b'_')
            {
                end += 1;
            }
            out.insert(&css[start..end]);
            i = end;
        }
        out
    }

    /// The stylesheet has a rule for every class the renderer writes.
    ///
    /// The four files are the ones that emit markup. `litedoc4-md` also emits
    /// classes (`markdown-heading`, `hover-link`, `language-…`) and is not
    /// checked here: those come out of a docstring, not out of a literal, and
    /// this test cannot see them.
    #[test]
    fn every_class_the_renderer_emits_is_styled() {
        let css = ASSETS
            .iter()
            .find(|(path, _)| *path == "style.css")
            .expect("the stylesheet is an asset")
            .1;
        let styled = styled_classes(css);

        let sources = [
            ("frame.rs", include_str!("frame.rs")),
            ("page.rs", include_str!("page.rs")),
            ("decl.rs", include_str!("decl.rs")),
            ("code.rs", include_str!("code.rs")),
        ];
        let mut seen = 0;
        let mut missing: Vec<String> = Vec::new();
        for (file, source) in sources {
            for class in emitted_classes(source) {
                seen += 1;
                if !styled.contains(class) {
                    missing.push(format!("{file}: .{class}"));
                }
            }
        }
        missing.sort_unstable();
        missing.dedup();
        assert!(
            missing.is_empty(),
            "the renderer writes classes the stylesheet says nothing about: {missing:#?}"
        );
        // A scan that finds nothing would pass silently, and this test would
        // then be checking that the empty set is a subset of anything.
        assert!(
            seen > 20,
            "only {seen} class names found — did the scan break?"
        );
    }

    /// Nothing here may be empty: an `include_str!` of a file that was moved or
    /// emptied still compiles, and a zero-byte `style.css` is a site that loads
    /// and has no styling — exactly the state M8-a exists to leave behind.
    #[test]
    fn every_asset_has_a_distinct_path_and_a_body() {
        let mut paths: Vec<&str> = ASSETS.iter().map(|(path, _)| *path).collect();
        paths.sort_unstable();
        paths.dedup();
        assert_eq!(paths.len(), ASSETS.len(), "two assets share a path");
        for (path, body) in ASSETS {
            assert!(!body.is_empty(), "{path} is empty");
            assert!(!path.contains(".."), "{path} could leave the site root");
        }
    }

    /// **A size budget, and what it is for.**
    ///
    /// The assets ship inside the binary and land on every generated site, so
    /// they are the one part of the output whose size is a property of this
    /// project rather than of the package being documented. `ui-redesign.md`
    /// 決定 1 refuses a CSS framework and names the failure mode that would
    /// justify revisiting it — "自前 CSS が doc-gen4 の 836 行より大きくなる …
    /// 大きくなったら削る。フレームワークには戻らない" — but nothing was
    /// measuring it.
    ///
    /// The limits are **round numbers above the current size, not the current
    /// size**: a budget pinned to today's bytes is a test that fails on every
    /// edit, which teaches people to raise it without looking. This one only
    /// speaks when something grows by a lot 【実測 2026-08-16: style.css
    /// 21,516 B / app.js 18,424 B / favicon.svg 360 B】.
    ///
    /// Raising a limit is allowed. Raising it *without reading what grew* is
    /// the thing this is here to make awkward.
    #[test]
    fn the_assets_stay_within_their_budget() {
        const BUDGET: [(&str, usize); 3] = [
            ("style.css", 32 * 1024),
            ("app.js", 32 * 1024),
            ("favicon.svg", 4 * 1024),
        ];
        for (path, limit) in BUDGET {
            let body = ASSETS
                .iter()
                .find(|(name, _)| *name == path)
                .unwrap_or_else(|| panic!("{path} is no longer an asset"))
                .1;
            assert!(
                body.len() <= limit,
                "{path} is {} B, over its {limit} B budget. Either shrink it or \
                 raise the budget deliberately — but read what grew first: the \
                 assets are downloaded by every reader of every generated site, \
                 and 決定 1 says the answer to a large stylesheet is to delete \
                 from it, not to adopt a framework.",
                body.len(),
            );
        }
    }

    /// Every asset the `<head>` names is an asset this module writes. Stated in
    /// that direction on purpose: what must never happen is a reference to a
    /// file nothing puts in the tree — which is the bug M8-a is fixing. M8-b
    /// added `app.js` to the `<head>`, so all three are named today.
    #[test]
    fn the_frame_only_names_assets_that_are_written() {
        let head = crate::head_html("Pkg.A", "../", &crate::SiteMeta::default());
        for name in ["style.css", "favicon.svg", "app.js"] {
            assert!(head.contains(name), "the <head> stopped naming {name}");
            assert!(
                ASSETS.iter().any(|(path, _)| *path == name),
                "the <head> names {name} and nothing writes it",
            );
        }
    }

    /// Twice into the same directory is the same three files: a second build
    /// over an unchanged package has to leave the tree alone.
    #[test]
    fn writing_twice_leaves_the_same_bytes() {
        let dir = TempDir::new("idempotent");
        write_assets(&dir.path).expect("the first write creates the directory");
        // An edit between the two runs, as a hand-patched deployment would
        // leave: the second write is what puts the shipped bytes back.
        fs::write(dir.path.join("style.css"), "/* hand-edited */").expect("the file is writable");
        write_assets(&dir.path).expect("the second write overwrites");

        for (name, body) in ASSETS {
            let on_disk = fs::read_to_string(dir.path.join(name)).expect("the asset is there");
            assert_eq!(on_disk, body, "{name} is not what the binary carries");
        }
        let written = fs::read_dir(&dir.path)
            .expect("the directory is readable")
            .count();
        assert_eq!(
            written,
            ASSETS.len(),
            "the write left something else behind"
        );
    }
}
