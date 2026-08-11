//! The page frame: `<head>`, `<header>` and the left-hand navigation.
//!
//! Ported from `experiments/stage7d/render.ts` (frozen): `headHtml` 846-866,
//! `pageHeaderHtml` 873-880, `internalNavHtml` 883-921, and the one expression
//! `pageHtml` builds a source URL with (1903). Upstream they are
//! `baseHtmlGenerator` (`Output/Template.lean`), `baseHtmlHeadDeclarations`
//! (`Output/Base.lean`) and `internalNav` / `importsHtml` /
//! `declarationToNavLink` (`Output/Module.lean:158-176`).
//!
//! # Three things here are not what a rewrite would produce
//!
//! 1. **The prefetch link has a doubled slash.** `root` already ends in `/`,
//!    and `headHtml` appends `"/declarations/declaration-data.bmp"` — so the
//!    href reads `.././/declarations/…`. doc-gen4 emits it, the reference
//!    pages contain it, and "fixing" it is a byte of mismatch.
//! 2. **The import list is sorted by [`crate::cmp_name`], not by string
//!    order.** `importsHtml` is `(← getImports m).qsort Name.lt`, which
//!    compares parents first: `Init` and `Mathlib` both precede `Init.Core`.
//! 3. **Duplicate imports are dropped, first occurrence kept.** Not because
//!    doc-gen4 de-duplicates but because its DB inserts with
//!    `INSERT OR IGNORE` (`DB.lean:162`); the module system does produce
//!    duplicates.
//!
//! The JSX whitespace of `baseHtmlGenerator` (`<span> {x} </span>`) never
//! reaches the output — Lean's tokenizer eats it — so there is none here
//! either.

use crate::escape::{escape_html_into, lean_quote_into};
use crate::order::cmp_name;
use crate::{break_within, module_link};

/// `getSourceUrl` for a module: the configured repository/revision prefix, the
/// module's components as directories, and `.lean` (`render.ts:1903`).
///
/// The prefix is **configuration, not IR** — doc-gen4 reads it from lake plus
/// git and the extractor never saw it. Plan §4: it has to carry a 40-hex
/// revision, because the acceptance oracle's revision-blind normalisation is
/// `/blob/[0-9a-f]{40}/` and a tag or branch name silently lowers the score.
#[must_use]
pub fn module_source_url(base: &str, module: &str) -> String {
    let mut out = String::with_capacity(base.len() + module.len() + 6);
    out.push_str(base);
    for part in module.split('.') {
        out.push('/');
        out.push_str(part);
    }
    out.push_str(".lean");
    out
}

/// `baseHtmlGenerator`'s `<head>` (`Template.lean` + `baseHtmlHeadDeclarations`).
///
/// `root` is [`crate::page_root`] of this page; it prefixes every asset, so it
/// is part of the bytes, and the two `<script>` constants carry it and the
/// module name through Lean's `String.quote` ([`crate::lean_quote`]) rather
/// than through an HTML escape.
#[must_use]
pub fn head_html(module: &str, root: &str) -> String {
    let mut out = String::with_capacity(1024);
    out.push_str("<head><meta charset=\"UTF-8\"></meta>");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"></meta>");
    asset(
        &mut out,
        "<link rel=\"stylesheet\" href=\"",
        root,
        "style.css",
    );
    out.push_str("></link>");
    asset(&mut out, "<link rel=\"icon\" href=\"", root, "favicon.svg");
    out.push_str("></link>");
    asset(
        &mut out,
        "<link rel=\"mask-icon\" href=\"",
        root,
        "favicon.svg",
    );
    out.push_str(" color=\"#000000\"></link>");
    // The doubled slash is doc-gen4's; see the module comment.
    asset(
        &mut out,
        "<link rel=\"prefetch\" href=\"",
        root,
        "/declarations/declaration-data.bmp",
    );
    out.push_str(" as=\"image\"></link>");
    out.push_str("<title>");
    escape_html_into(&mut out, module);
    out.push_str("</title>");
    asset(
        &mut out,
        "<script defer=\"true\" src=\"",
        root,
        "mathjax-config.js",
    );
    out.push_str("></script>");
    out.push_str(
        "<script defer=\"true\" src=\"https://cdnjs.cloudflare.com/polyfill/v3/polyfill.min.js?features=es6\"></script>",
    );
    out.push_str(
        "<script defer=\"true\" src=\"https://cdn.jsdelivr.net/npm/mathjax@3/es5/tex-mml-chtml.js\"></script>",
    );
    out.push_str("<script>const SITE_ROOT=");
    lean_quote_into(&mut out, root);
    out.push_str(";</script>");
    out.push_str("<script>const MODULE_NAME=");
    lean_quote_into(&mut out, module);
    out.push_str(";</script>");
    for script in [
        "jump-src.js",
        "search.js",
        "expand-nav.js",
        "how-about.js",
        "instances.js",
        "importedBy.js",
    ] {
        asset(&mut out, "<script type=\"module\" src=\"", root, script);
        out.push_str("></script>");
    }
    out.push_str("</head>");
    out
}

/// `<open>"root+name"` — the shape every asset reference in [`head_html`] has.
/// The quote closes here; what follows it differs per attribute.
fn asset(out: &mut String, open: &str, root: &str, name: &str) {
    out.push_str(open);
    // `escapeHtml(root + name)`: the concatenation is escaped, not the parts,
    // which is the same thing for these but not for an arbitrary root.
    let mut href = String::with_capacity(root.len() + name.len());
    href.push_str(root);
    href.push_str(name);
    escape_html_into(out, &href);
    out.push('"');
}

/// `baseHtmlGenerator`'s `<header>`.
#[must_use]
pub fn page_header_html(module: &str, root: &str) -> String {
    let mut out = String::with_capacity(256 + module.len() * 2);
    out.push_str(
        "<header><h1><label for=\"nav_toggle\"></label><span>Documentation</span></h1>\
         <h2 class=\"header_filename break_within\">",
    );
    out.push_str(&break_within(module));
    out.push_str(
        "</h2><form id=\"search_form\"><input type=\"text\" name=\"q\" autocomplete=\"off\">\
         </input>&#32;<button id=\"search_button\" onclick=\"",
    );
    // The whole JavaScript one-liner goes through `Html.escape` — so the `'`
    // around the URL stays a `'` and only the `&`-class characters move.
    let mut onclick = String::with_capacity(root.len() + 40);
    onclick.push_str("javascript: form.action='");
    onclick.push_str(root);
    onclick.push_str("search.html';");
    escape_html_into(&mut out, &onclick);
    out.push_str("\">Search</button></form></header>");
    out
}

/// `internalNav` (`Module.lean:158-176`).
///
/// `member_names` is the page's declarations **in page order** — the order
/// `moduleToHtml` emitted them in, not a sort — because the nav is a table of
/// contents for what is below it.
#[must_use]
pub fn internal_nav_html(
    module: &str,
    root: &str,
    module_source_url: &str,
    imports: &[String],
    member_names: &[&str],
) -> String {
    let mut out = String::with_capacity(512 + imports.len() * 64 + member_names.len() * 64);
    out.push_str("<nav class=\"internal_nav\"><p><a href=\"#top\">return to top</a></p>");
    out.push_str("<p class=\"gh_nav_link\"><a href=\"");
    escape_html_into(&mut out, module_source_url);
    out.push_str("\">source</a></p>");
    out.push_str("<div class=\"imports\"><details><summary>Imports</summary><ul>");
    for import in sorted_imports(imports) {
        out.push_str("<li><a href=\"");
        escape_html_into(&mut out, &module_link(root, import));
        out.push_str("\">");
        escape_html_into(&mut out, import);
        out.push_str("</a></li>");
    }
    out.push_str("</ul></details><details><summary>Imported by</summary><ul id=\"");
    let mut id = String::with_capacity(module.len() + 12);
    id.push_str("imported-by-");
    id.push_str(module);
    escape_html_into(&mut out, &id);
    out.push_str("\" class=\"imported-by-list\"></ul></details></div>");
    for name in member_names {
        out.push_str("<div class=\"nav_link\"><a class=\"break_within\" href=\"");
        let mut href = String::with_capacity(name.len() + 1);
        href.push('#');
        href.push_str(name);
        escape_html_into(&mut out, &href);
        out.push_str("\">");
        out.push_str(&break_within(name));
        out.push_str("</a></div>");
    }
    out.push_str("</nav>");
    out
}

/// The import list as `importsHtml` produces it: duplicates dropped keeping the
/// first occurrence, then a **stable** sort by `Name.lt`.
///
/// The de-duplication is the DB's `INSERT OR IGNORE`, not a rule of the
/// renderer, and it happens before the sort — which is why it keeps the first
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

    /// The doubled slash, on its own, because it is the one thing here a reader
    /// would "fix".
    #[test]
    fn the_prefetch_href_keeps_doc_gen4s_doubled_slash() {
        let head = head_html("Foo", ".././");
        assert!(
            head.contains("href=\".././/declarations/declaration-data.bmp\" as=\"image\""),
            "{head}"
        );
    }

    #[test]
    fn the_head_carries_the_root_and_the_module_through_lean_quote() {
        let head = head_html("Foo.Bar", ".././");
        assert!(head.contains("<script>const SITE_ROOT=\".././\";</script>"));
        assert!(head.contains("<script>const MODULE_NAME=\"Foo.Bar\";</script>"));
        assert!(head.contains("<title>Foo.Bar</title>"));
        assert!(head.starts_with("<head><meta charset=\"UTF-8\"></meta>"));
        assert!(head.ends_with("importedBy.js\"></script></head>"));
    }

    /// `Html.escape` is not `String.quote`: a module name with a `"` in it is
    /// escaped in the title and backslash-escaped in the script.
    #[test]
    fn the_two_escapes_in_the_head_are_different_functions() {
        let head = head_html("A\"B", "./");
        assert!(head.contains("<title>A&quot;B</title>"), "{head}");
        assert!(head.contains("const MODULE_NAME=\"A\\\"B\";"), "{head}");
    }

    #[test]
    fn the_page_header_escapes_the_onclick_but_keeps_its_quotes() {
        let header = page_header_html("Foo.Bar", ".././");
        assert!(header.contains(
            "<h2 class=\"header_filename break_within\">\
             <span class=\"name\">Foo</span>.<span class=\"name\">Bar</span></h2>"
        ));
        assert!(
            header.contains("onclick=\"javascript: form.action='.././search.html';\">Search"),
            "{header}"
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
    fn the_nav_lists_imports_then_members() {
        let imports = vec!["B.C".to_owned(), "A".to_owned()];
        let nav = internal_nav_html("M.N", ".././", "https://x/M/N.lean", &imports, &["M.N.f"]);
        assert_eq!(
            nav,
            "<nav class=\"internal_nav\"><p><a href=\"#top\">return to top</a></p>\
             <p class=\"gh_nav_link\"><a href=\"https://x/M/N.lean\">source</a></p>\
             <div class=\"imports\"><details><summary>Imports</summary>\
             <ul><li><a href=\".././A.html\">A</a></li>\
             <li><a href=\".././B/C.html\">B.C</a></li></ul></details>\
             <details><summary>Imported by</summary>\
             <ul id=\"imported-by-M.N\" class=\"imported-by-list\"></ul></details></div>\
             <div class=\"nav_link\"><a class=\"break_within\" href=\"#M.N.f\">\
             <span class=\"name\">M</span>.<span class=\"name\">N</span>.\
             <span class=\"name\">f</span></a></div></nav>"
        );
    }

    #[test]
    fn a_module_with_no_imports_and_no_members_still_has_both_details() {
        let nav = internal_nav_html("M", "./", "u", &[], &[]);
        assert!(nav.contains("<ul></ul></details>"), "{nav}");
        assert!(nav.ends_with("</details></div></nav>"), "{nav}");
    }
}
