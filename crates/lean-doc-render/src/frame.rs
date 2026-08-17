//! The page frame: `<head>`, the top bar, the sidebar and the module heading.
//!
//! **M8-b replaced this module wholesale.** Up to M7 it was a transcription of
//! doc-gen4's `baseHtmlGenerator` / `internalNav`, kept byte-identical because
//! the acceptance oracle compared bytes with doc-gen4's own output. That oracle
//! is gone (plan §1: gate A is suspended, and M8 stops claiming byte
//! compatibility), and what is here now is lean-doc's own frame:
//!
//! | doc-gen4 had | here |
//! |---|---|
//! | four CDN dependencies (Lato, JuliaMono, polyfill, MathJax) | none — the site renders from a bare directory |
//! | `<nav><iframe src="navbar.html">` | a sidebar the module tree is drawn into from `modules.json` |
//! | `</meta>` / `</link>` closing tags (Lean's `Html` prints them) | HTML5 |
//! | one `<script>` per behaviour, six of them | one `app.js` |
//!
//! The hand-written page this has to agree with is `design/preview/module.html`;
//! **keep the two in step**, because the stylesheet is written against that file
//! and a class renamed here silently loses its styling rather than failing.
//!
//! # Two things here are deliberate and look like mistakes
//!
//! 1. **The theme is set by an inline script in `<head>`.** An external module
//!    runs after first paint, so a reader on the dark theme would see a white
//!    flash on every navigation. The cost is ~120 bytes on every page.
//! 2. **The import list is sorted by [`crate::cmp_name`], not by string order.**
//!    `Name.lt` compares parents first: `Init` and `Mathlib` both precede
//!    `Init.Core`. This is doc-gen4's order and there is no reason to change it
//!    — it groups a package's modules together, which alphabetical order does
//!    not.
//!
//! Duplicate imports are dropped keeping the first occurrence, as they were
//! before: the module system does produce duplicates, and doc-gen4's DB hid
//! them behind an `INSERT OR IGNORE` (`DB.lean:162`).

use crate::break_within;
use crate::escape::escape_html_into;
use crate::external::ExternalLinks;
use crate::order::cmp_name;

/// What every page says about the site it belongs to. Configuration, not IR:
/// `lean-doc build` fills it from the package it was pointed at.
#[derive(Clone, Copy, Debug)]
pub struct SiteMeta<'a> {
    /// Shown in the top bar and after the module name in `<title>`. The
    /// package's root module, or `"Documentation"` when nothing better is
    /// known.
    pub title: &'a str,
}

impl Default for SiteMeta<'_> {
    fn default() -> Self {
        Self {
            title: "Documentation",
        }
    }
}

impl<'a> SiteMeta<'a> {
    /// The name the modules share, when they share one.
    ///
    /// The IR does not carry a package name, and the CLI's `--root` is a
    /// directory rather than a name — but a package whose modules are all
    /// `Foo.*` is called `Foo`, and that is the name a reader types to import
    /// it. When they do not all agree there is no right answer, and the generic
    /// title is the honest one.
    ///
    /// Derived rather than configured on purpose: a `--title` flag would be one
    /// more thing that can be forgotten on one of the three commands that
    /// render, and then two of them would disagree about what the site is
    /// called.
    #[must_use]
    pub fn of_modules(modules: impl IntoIterator<Item = &'a str>) -> Self {
        let mut roots = modules
            .into_iter()
            .map(|module| lean_doc_ir::module_components(module).into_iter().next());
        match roots.next().flatten() {
            Some(head) if roots.all(|root| root == Some(head)) => Self { title: head },
            _ => Self::default(),
        }
    }
}

/// Runs before first paint so a dark-theme reader never sees a white flash.
/// Kept to one line because it is inlined into every page.
const THEME_BOOT: &str = "<script>try{var t=localStorage.getItem(\"lean-doc-theme\");\
     if(t===\"light\"||t===\"dark\")document.documentElement.dataset.theme=t}catch(e){}</script>";

const ICON_MENU: &str =
    "<svg viewBox=\"0 0 20 20\" aria-hidden=\"true\"><path d=\"M3 5h14M3 10h14M3 15h14\"/></svg>";
const ICON_THEME: &str = "<svg viewBox=\"0 0 20 20\" aria-hidden=\"true\"><path d=\"M10 3a7 7 0 1 0 7 7 5.5 5.5 0 0 1-7-7z\"/></svg>";

/// `getSourceUrl` for a module: the configured repository/revision prefix, the
/// module's components as directories, and `.lean`.
///
/// The prefix is **configuration, not IR** — doc-gen4 reads it from lake plus
/// git and the extractor never saw it.
#[must_use]
pub fn module_source_url(base: &str, module: &str) -> String {
    let mut out = String::with_capacity(base.len() + module.len() + 6);
    out.push_str(base);
    // The source file's path, so the components are the unescaped ones (M5-b):
    // `Alpha.«Odd-Name»` lives in `Alpha/Odd-Name.lean`.
    for part in lean_doc_ir::module_components(module) {
        out.push('/');
        out.push_str(part);
    }
    out.push_str(".lean");
    out
}

/// `<head>`: two meta tags, the title, the stylesheet, the icon, the theme boot
/// and `app.js`. Nothing else — in particular nothing from another host.
#[must_use]
pub fn head_html(module: &str, root: &str, site: &SiteMeta<'_>) -> String {
    let mut out = String::with_capacity(640);
    out.push_str(
        "<head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>",
    );
    escape_html_into(&mut out, module);
    if !site.title.is_empty() && site.title != module {
        out.push_str(" · ");
        escape_html_into(&mut out, site.title);
    }
    out.push_str("</title><link rel=\"stylesheet\" href=\"");
    push_asset(&mut out, root, "style.css");
    out.push_str("\"><link rel=\"icon\" href=\"");
    push_asset(&mut out, root, "favicon.svg");
    out.push_str("\">");
    out.push_str(THEME_BOOT);
    out.push_str("<script type=\"module\" src=\"");
    push_asset(&mut out, root, "app.js");
    out.push_str("\"></script></head>");
    out
}

/// `root + name`, escaped as one string — which is the same as escaping the
/// parts for every root this crate produces, but not for an arbitrary one.
fn push_asset(out: &mut String, root: &str, name: &str) {
    let mut href = String::with_capacity(root.len() + name.len());
    href.push_str(root);
    href.push_str(name);
    escape_html_into(out, &href);
}

/// The top bar: the drawer button (only where there is a drawer), the site
/// name, the search box and the theme button.
///
/// `with_nav` is false on the pages that have no sidebar — the index, search
/// and not-found pages are one column, and a button that opens nothing is
/// worse than no button.
#[must_use]
pub fn topbar_html(root: &str, site: &SiteMeta<'_>, with_nav: bool) -> String {
    let mut out = String::with_capacity(768);
    out.push_str("<header class=\"topbar\">");
    if with_nav {
        out.push_str(
            "<button class=\"iconbtn\" id=\"nav-toggle\" aria-label=\"Modules\" \
             aria-expanded=\"false\" aria-controls=\"sidebar\">",
        );
        out.push_str(ICON_MENU);
        out.push_str("</button>");
    }
    out.push_str("<a class=\"home\" href=\"");
    push_asset(&mut out, root, "index.html");
    out.push_str("\">");
    escape_html_into(&mut out, site.title);
    out.push_str("</a><form class=\"search\" role=\"search\" action=\"");
    push_asset(&mut out, root, "search.html");
    out.push_str(
        "\"><input type=\"search\" id=\"search-input\" name=\"q\" autocomplete=\"off\" \
         spellcheck=\"false\" placeholder=\"Search declarations\" \
         aria-label=\"Search declarations\">\
         <ul class=\"search-results\" id=\"search-results\" hidden></ul></form>\
         <button class=\"iconbtn\" id=\"theme-toggle\" aria-label=\"Theme\">",
    );
    out.push_str(ICON_THEME);
    out.push_str("</button></header>");
    out
}

/// The sidebar: this page's declarations, then the module tree `app.js` draws.
///
/// `member_names` is the page's declarations **in page order** — the order
/// `page_html` emitted them in, not a sort — because it is a table of contents
/// for what is below it.
///
/// The tree is empty markup on purpose (plan 決定 4): `navbar.html` is 57,949 B
/// for this package and putting it on all 432 pages would add ~25 MB. The
/// `<noscript>` link is the fallback, and it is the reason `index.html` has to
/// exist.
#[must_use]
pub fn sidebar_html(root: &str, member_names: &[&str]) -> String {
    let mut out = String::with_capacity(512 + member_names.len() * 96);
    out.push_str(
        "<div class=\"scrim\" id=\"scrim\" hidden></div>\
         <nav class=\"sidebar\" id=\"sidebar\" aria-label=\"Navigation\">",
    );
    if !member_names.is_empty() {
        out.push_str(
            "<section class=\"side\"><h2 class=\"side-title\">On this page</h2><ul class=\"toc\">",
        );
        for name in member_names {
            out.push_str("<li><a href=\"#");
            escape_html_into(&mut out, name);
            out.push_str("\">");
            out.push_str(&break_within(name));
            out.push_str("</a></li>");
        }
        out.push_str("</ul></section>");
    }
    out.push_str(
        "<section class=\"side\"><h2 class=\"side-title\">Modules</h2>\
         <div class=\"tree\" id=\"module-tree\"><noscript><a href=\"",
    );
    push_asset(&mut out, root, "index.html");
    out.push_str("\">Module index</a></noscript></div></section></nav>");
    out
}

/// The module's own heading: its name and a link to its source.
///
/// No breadcrumb: the name is spelled out in full here and the tree in the
/// sidebar already shows where it sits, so a second copy of the hierarchy would
/// only compete with it.
#[must_use]
pub fn module_head_html(module: &str, module_source_url: &str) -> String {
    let mut out = String::with_capacity(256 + module.len() * 2);
    out.push_str("<div class=\"modhead\"><h1>");
    out.push_str(&break_within(module));
    out.push_str("</h1><p class=\"modactions\"><a class=\"src\" href=\"");
    escape_html_into(&mut out, module_source_url);
    out.push_str("\">source</a></p></div>");
    out
}

/// The two import blocks under the heading.
///
/// # The import list was the most visibly dead link on the page before M7
///
/// Nearly every import of a package like the measurement target is a
/// *dependency's* module — `Mathlib.Order.Basic`, `Init.Prelude` — and this site
/// has a page for none of them, so the relative link this used to write was a
/// 404 on every page. [`ExternalLinks::href`] turns those into the dependency's
/// pinned source; an import of the package's own modules is not in the map and
/// keeps its page link.
///
/// **An import of a dependency that could not be version-pinned is neither**: it
/// is listed by name, inside its `<li>`, with no `<a>` at all (2026-08-17). The
/// import is a fact about the module and stays on the page; where to read it is
/// a fact this run does not have, and writing the relative link anyway put the
/// pre-M7 404 back one dependency at a time.
///
/// "Imported by" is empty markup and starts `hidden`: it is a fact about the
/// whole site, `app.js` fills it from `modules.json`, and a module nothing
/// imports has the block removed rather than shown empty.
#[must_use]
pub fn module_meta_html(root: &str, imports: &[String], external: &ExternalLinks) -> String {
    let sorted = sorted_imports(imports);
    let mut out = String::with_capacity(256 + sorted.len() * 96);
    out.push_str("<div class=\"modmeta\"><details class=\"imports\"><summary>Imports");
    if !sorted.is_empty() {
        out.push_str(" <span class=\"count\">");
        out.push_str(&sorted.len().to_string());
        out.push_str("</span>");
    }
    out.push_str("</summary><ul>");
    for import in sorted {
        match external.href(root, import, None, None) {
            Some(href) => {
                out.push_str("<li><a href=\"");
                escape_html_into(&mut out, &href);
                out.push_str("\">");
                escape_html_into(&mut out, import);
                out.push_str("</a></li>");
            }
            None => {
                out.push_str("<li>");
                escape_html_into(&mut out, import);
                out.push_str("</li>");
            }
        }
    }
    out.push_str(
        "</ul></details>\
         <details class=\"imports\" data-fill=\"imported-by\" hidden>\
         <summary>Imported by</summary><ul></ul></details></div>",
    );
    out
}

/// The import list: duplicates dropped keeping the first occurrence, then a
/// **stable** sort by `Name.lt`.
///
/// The de-duplication happens before the sort, which is why it keeps the first
/// occurrence rather than any other.
#[must_use]
pub fn sorted_imports(imports: &[String]) -> Vec<&str> {
    let mut seen = std::collections::HashSet::with_capacity(imports.len());
    let mut out: Vec<&str> = Vec::with_capacity(imports.len());
    for import in imports {
        if seen.insert(import.as_str()) {
            out.push(import.as_str());
        }
    }
    out.sort_by(|a, b| cmp_name(a, b));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_source_url_joins_components_with_slashes() {
        assert_eq!(
            module_source_url("https://host/o/r/blob/abc", "Foo.Bar"),
            "https://host/o/r/blob/abc/Foo/Bar.lean"
        );
        assert_eq!(module_source_url("", "Foo"), "/Foo.lean");
    }

    /// The whole point of M8: a page asks its own directory for everything.
    #[test]
    fn the_head_names_no_other_host() {
        let head = head_html("Foo.Bar", ".././", &SiteMeta { title: "Pkg" });
        assert!(!head.contains("//cdn"), "{head}");
        assert!(!head.contains("http"), "{head}");
        assert!(head.contains("href=\".././style.css\""), "{head}");
        assert!(head.contains("src=\".././app.js\""), "{head}");
        assert!(head.contains("<title>Foo.Bar · Pkg</title>"), "{head}");
    }

    /// The theme has to be applied before the first paint, so it cannot live in
    /// `app.js`.
    #[test]
    fn the_theme_is_set_inline_before_the_module_script() {
        let head = head_html("Foo", "./", &SiteMeta::default());
        let boot = head.find("lean-doc-theme").expect("theme boot is inline");
        let app = head.find("app.js").expect("app.js is linked");
        assert!(boot < app, "{head}");
    }

    #[test]
    fn the_title_does_not_repeat_itself_on_the_root_module() {
        let head = head_html("Pkg", "./", &SiteMeta { title: "Pkg" });
        assert!(head.contains("<title>Pkg</title>"), "{head}");
    }

    #[test]
    fn the_head_escapes_the_module_name() {
        let head = head_html("A\"B", "./", &SiteMeta { title: "S" });
        assert!(head.contains("<title>A&quot;B · S</title>"), "{head}");
    }

    #[test]
    fn a_page_without_a_sidebar_gets_no_drawer_button() {
        let with = topbar_html("./", &SiteMeta::default(), true);
        let without = topbar_html("./", &SiteMeta::default(), false);
        assert!(with.contains("id=\"nav-toggle\""));
        assert!(!without.contains("id=\"nav-toggle\""));
        // Both still search and still lead home.
        for bar in [&with, &without] {
            assert!(bar.contains("id=\"search-input\""), "{bar}");
            assert!(bar.contains("href=\"./index.html\""), "{bar}");
        }
    }

    #[test]
    fn the_sidebar_lists_the_page_in_page_order() {
        let side = sidebar_html(".././", &["M.N.b", "M.N.a"]);
        let b = side.find("#M.N.b").expect("first entry");
        let a = side.find("#M.N.a").expect("second entry");
        assert!(b < a, "the toc is the page's order, not a sort: {side}");
        assert!(side.contains("<div class=\"tree\" id=\"module-tree\">"));
        assert!(
            side.contains("<noscript><a href=\".././index.html\">Module index</a></noscript>"),
            "{side}"
        );
    }

    /// A module with no declarations gets no "On this page" heading — an empty
    /// table of contents is a heading with nothing under it.
    #[test]
    fn an_empty_page_has_no_table_of_contents() {
        let side = sidebar_html("./", &[]);
        assert!(!side.contains("On this page"), "{side}");
        assert!(side.contains("Modules"), "{side}");
    }

    #[test]
    fn the_module_heading_is_the_name_and_its_source() {
        let head = module_head_html("M.N", "https://x/M/N.lean");
        assert_eq!(
            head,
            "<div class=\"modhead\"><h1><span class=\"name\">M</span>.\
             <span class=\"name\">N</span></h1><p class=\"modactions\">\
             <a class=\"src\" href=\"https://x/M/N.lean\">source</a></p></div>"
        );
    }

    #[test]
    fn imports_are_deduplicated_before_being_sorted_by_name_lt() {
        let imports: Vec<String> = ["Mathlib.Order", "Init", "Mathlib.Order", "Init.Core", "Zzz"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        assert_eq!(
            sorted_imports(&imports),
            ["Init", "Zzz", "Init.Core", "Mathlib.Order"],
            "`Name.lt` compares parents first, so the one-component names lead"
        );
    }

    #[test]
    fn the_meta_block_counts_imports_and_leaves_imported_by_to_the_script() {
        let imports = vec!["B.C".to_owned(), "A".to_owned()];
        let meta = module_meta_html(".././", &imports, &ExternalLinks::default());
        assert!(
            meta.contains("<summary>Imports <span class=\"count\">2</span></summary>"),
            "{meta}"
        );
        assert!(
            meta.contains(
                "<ul><li><a href=\".././A.html\">A</a></li>\
                 <li><a href=\".././B/C.html\">B.C</a></li></ul>"
            ),
            "{meta}"
        );
        assert!(meta.contains("data-fill=\"imported-by\" hidden"), "{meta}");
    }

    #[test]
    fn a_module_with_no_imports_still_gets_the_block_but_no_count() {
        let meta = module_meta_html("./", &[], &ExternalLinks::default());
        assert!(
            meta.contains("<summary>Imports</summary><ul></ul>"),
            "{meta}"
        );
    }

    /// **M7-c**: an import of a dependency is the dependency's source file; an
    /// import of the package's own modules keeps its page link.
    ///
    /// The sort is unaffected — it is over the module names, not the hrefs — so
    /// `A` still leads even though its href is now the longer one.
    #[test]
    fn an_import_of_a_dependency_points_at_its_pinned_source() {
        let imports = vec!["B.C".to_owned(), "A".to_owned()];
        let meta = module_meta_html(
            ".././",
            &imports,
            &ExternalLinks::new([("A", "https://host/o/dep/blob/abc")]),
        );
        assert!(
            meta.contains(
                "<ul><li><a href=\"https://host/o/dep/blob/abc/A.lean\">A</a></li>\
                 <li><a href=\".././B/C.html\">B.C</a></li></ul>"
            ),
            "{meta}"
        );
    }

    /// **2026-08-17**: an import of a dependency with no version-pinned URL is
    /// the module's name and nothing else — the `<li>` stays, the `<a>` goes.
    ///
    /// The count and the sort are over the imports, not the links, so both are
    /// unchanged; `B.C` beside it still gets its page link, which is what says
    /// the loss is confined to the root that could not be resolved.
    #[test]
    fn an_import_that_cannot_be_version_pinned_is_listed_without_a_link() {
        let imports = vec!["B.C".to_owned(), "A".to_owned()];
        let meta = module_meta_html(".././", &imports, &ExternalLinks::new([("A", "")]));
        assert!(
            meta.contains("<ul><li>A</li><li><a href=\".././B/C.html\">B.C</a></li></ul>"),
            "{meta}"
        );
        assert!(
            meta.contains("<summary>Imports <span class=\"count\">2</span></summary>"),
            "an unlinkable import is still an import: {meta}"
        );
        assert!(!meta.contains("<a href=\".././A.html\">"), "{meta}");
    }
}
