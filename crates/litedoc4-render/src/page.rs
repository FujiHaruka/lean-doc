//! One module page, from the IR to the bytes on disk.
//!
//! Ported from `experiments/stage7d/render.ts` (frozen): `pageHtml` 1900-1973,
//! which is `moduleToHtml` (`Output/Module.lean:179-206`) wrapped in
//! `baseHtmlGenerator` (`Output/Template.lean`). The two things it decides that
//! nothing below it can are **which declarations get an entry** and **in what
//! order they and the module docstrings appear**.
//!
//! # The suppressed set is not a property of a module
//!
//! `DocInfo.ofConstant` sets `render := false` for projection functions and for
//! constructors (`Process/DocInfo.lean:176/186/207`), i.e. exactly the names
//! that appear as some declaration's `members`. The IR-side rule is therefore
//! "is this name a member of *anything*", and `render.ts:2043-2048` collects it
//! **across every module** for that reason: a structure declared in `A` can
//! have its projections attributed to `B`, and building the set per module
//! leaves those on `B`'s page. [`Suppressed::of_site`] takes the whole site for
//! this reason, and 190 of the target package's declarations are in it
//! 【実測: 4,750 declarations / 432 modules at the revision M5 measured, and
//! **still 190** at 4,584 / 422 on 2026-08-21 — the count is the same, the
//! denominator is not, so the denominator carries its revision】.
//!
//! # The order is a stable sort on three keys, and the third one is not `index`
//!
//! `Process.Module.members` is the module docstrings in file order followed by
//! every `DocInfo`, then `qsort ModuleMember.order` on the declaration range.
//! qsort is not stable; a stable sort on `(line, col)` was measured to
//! reproduce doc-gen4's page order on 348/348 pages (increment 1), so that is
//! what is used — with the docstrings kept ahead of the declarations at an
//! equal position, because that is the insertion order qsort was given.
//!
//! That last clause is why the tie-breaker is a **running sequence number** and
//! not `Decl::index`: the docstrings take `0..k` and a declaration takes
//! `k + index`. Using `index` directly puts declaration 0 ahead of the second
//! module docstring whenever the two share a position.

use std::collections::HashSet;

use litedoc4_ir::{Decl, ModuleDoc, ModuleFile};

use crate::autolink::{NameIndex, PageLinks, module_decl_names, page_root};
use crate::code::CodeRenderer;
use crate::decl::{DeclRenderer, UnplaceableName};
use crate::escape::escape_html_into;
use crate::frame::{
    SiteMeta, head_html, module_head_html, module_meta_html, module_source_url, sidebar_html,
    topbar_html,
};

/// The names that get no page entry: every name that is some declaration's
/// member, over the **whole site**.
///
/// There is no per-module constructor on purpose. See the module comment.
#[derive(Clone, Debug, Default)]
pub struct Suppressed(HashSet<String>);

impl Suppressed {
    /// Collects the set from every module of the site.
    ///
    /// The argument is the *site*, not a module: a name declared in one module
    /// can be a member of a declaration in another, and a set built per module
    /// would leave it on the page.
    #[must_use]
    pub fn of_site<'a>(modules: impl IntoIterator<Item = &'a ModuleFile>) -> Self {
        let mut names = HashSet::new();
        for module in modules {
            for decl in &module.declarations {
                for member in &decl.members {
                    if !names.contains(member.name.as_str()) {
                        names.insert(member.name.clone());
                    }
                }
            }
        }
        Self(names)
    }

    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.0.contains(name)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// One thing on a page: a module docstring or a declaration.
#[derive(Clone, Copy, Debug)]
pub enum PageItem<'a> {
    ModuleDoc(&'a ModuleDoc),
    Decl(&'a Decl),
}

/// What the page contains and in what order (`render.ts:1926-1949`).
///
/// Exposed because the order is the part of the page a byte comparison
/// localises worst: a page whose items are shuffled differs at its first
/// declaration and says nothing about why.
#[must_use]
pub fn page_items<'a>(module: &'a ModuleFile, suppressed: &Suppressed) -> Vec<PageItem<'a>> {
    // `(line, col, seq)`, where `seq` numbers the module docstrings `0..k` and
    // then offsets each declaration's in-module index by `k`. See the module
    // comment for why it is not `Decl::index`.
    let mut items: Vec<(u32, u32, usize, PageItem<'a>)> =
        Vec::with_capacity(module.module_docs.len() + module.declarations.len());
    let mut seq = 0usize;
    for doc in &module.module_docs {
        items.push((doc.line, doc.col, seq, PageItem::ModuleDoc(doc)));
        seq += 1;
    }
    for decl in &module.declarations {
        if suppressed.contains(&decl.name) {
            continue;
        }
        items.push((
            decl.line,
            decl.col,
            seq + decl.index as usize,
            PageItem::Decl(decl),
        ));
    }
    items.sort_by_key(|(line, col, seq, _)| (*line, *col, *seq));
    items.into_iter().map(|(_, _, _, item)| item).collect()
}

/// The whole page for one module.
///
/// `source_url` is the repository/revision prefix ([`module_source_url`] turns
/// it into this module's link), `index` is the run's name index, `suppressed` is
/// [`Suppressed::of_site`] over **every** module of the IR — not just this one —
/// and `site` is what the page says about the package it belongs to.
///
/// # The order the body is assembled in is not the order it is written in
///
/// The sidebar's table of contents is the page's declarations *in page order*,
/// which is only known once they have been laid out. So `main` is built first
/// and the frame around it second, even though the frame comes first in the
/// output.
pub fn page_html(
    module: &ModuleFile,
    index: &NameIndex,
    source_url: &str,
    suppressed: &Suppressed,
    site: &SiteMeta<'_>,
) -> Result<String, UnplaceableName> {
    let root = page_root(&module.module);
    let module_url = module_source_url(source_url, &module.module);
    // `nameToLink?`'s last resort walks this list, which is *not* the IR's
    // order; see [`module_decl_names`].
    let names = module_decl_names(module);
    let links = PageLinks::new(index, &root, &names);
    let docs = links.renderer();
    let renderer = DeclRenderer::new(module, &root, &module_url, CodeRenderer::new(index), &docs);

    let items = page_items(module, suppressed);
    let mut main = String::with_capacity(4096);
    let mut member_names: Vec<&str> = Vec::with_capacity(items.len());
    for item in items {
        match item {
            PageItem::ModuleDoc(doc) => {
                main.push_str("<div class=\"moddoc\">");
                main.push_str(&docs.docstring(&doc.text));
                main.push_str("</div>");
            }
            PageItem::Decl(decl) => {
                member_names.push(&decl.name);
                main.push_str(&renderer.decl_html(decl)?);
            }
        }
    }

    let mut out = String::with_capacity(main.len() + 4096);
    // The doctype is not decoration: without it the browser is in quirks mode,
    // where `box-sizing` and the grid the page is laid out on behave
    // differently. doc-gen4 omitted it and got away with it because its
    // stylesheet was written under quirks mode too.
    out.push_str("<!DOCTYPE html><html lang=\"en\">");
    out.push_str(&head_html(&module.module, &root, site));
    out.push_str("<body data-root=\"");
    escape_html_into(&mut out, &root);
    out.push_str("\" data-module=\"");
    escape_html_into(&mut out, &module.module);
    out.push_str("\"><a class=\"skip\" href=\"#content\">Skip to content</a>");
    out.push_str(&topbar_html(&root, site, true));
    out.push_str("<div class=\"shell\">");
    out.push_str(&sidebar_html(&root, &member_names));
    out.push_str("<main class=\"content\" id=\"content\">");
    out.push_str(&module_head_html(&module.module, &module_url));
    out.push_str(&module_meta_html(&root, &module.imports, index));
    out.push_str(&main);
    out.push_str("</main></div></body></html>");
    Ok(out)
}

/// Where a module's page goes under the site root: its components as
/// directories (`render.ts:2122`).
///
/// The components are the **unescaped** ones (M5-b): doc-gen4 builds this path
/// out of `Name.toString (escape := false)` (`Output/Base.lean:188`), so a
/// module Lean spells `Alpha.«Odd-Name»` has its page at `Alpha/Odd-Name.html`.
/// For every module name that is a plain identifier — all of the measurement
/// target's, 432 of them at the revision this was measured at and 422 on
/// 2026-08-21 — this is `module.split('.')` unchanged.
#[must_use]
pub fn page_path(module: &str) -> std::path::PathBuf {
    let mut path = std::path::PathBuf::new();
    let mut parts = litedoc4_ir::module_components(module)
        .into_iter()
        .peekable();
    while let Some(part) = parts.next() {
        if parts.peek().is_some() {
            path.push(part);
        } else {
            path.push(format!("{part}.html"));
        }
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A module with two docstrings and three declarations, one of which is
    /// another declaration's member.
    const MODULE_JSON: &str = r#"{
        "schemaVersion": 4,
        "module": "Pkg.Two",
        "imports": [],
        "moduleDocs": [
            {"line": 1, "col": 0, "text": "first"},
            {"line": 7, "col": 0, "text": "second"}
        ],
        "tactics": [],
        "declarations": [
            {"name": "Pkg.Two.b", "kind": "theorem", "modifiers": [], "binders": [],
             "implicits": [], "binderCode": [], "type": "T", "typeCode": [],
             "line": 7, "col": 0, "endLine": 7, "endCol": 1, "index": 0,
             "members": [], "doc": null, "equations": [], "equationCode": [],
             "refs": []},
            {"name": "Pkg.Two.a", "kind": "structure", "modifiers": [], "binders": [],
             "implicits": [], "binderCode": [], "type": "T", "typeCode": [],
             "line": 5, "col": 0, "endLine": 5, "endCol": 1, "index": 1,
             "members": [{"label": "ctor", "name": "Pkg.Two.a.mk", "text": "",
                          "code": [], "binders": [], "implicits": [],
                          "binderCode": [], "doc": null, "isDirect": true}],
             "doc": null, "equations": [], "equationCode": [], "refs": []},
            {"name": "Pkg.Two.a.mk", "kind": "constructor", "modifiers": [],
             "binders": [], "implicits": [], "binderCode": [], "type": "T",
             "typeCode": [], "line": 5, "col": 0, "endLine": 5, "endCol": 1,
             "index": 2, "members": [], "doc": null, "equations": [],
             "equationCode": [], "refs": []}
        ]
    }"#;

    fn module() -> ModuleFile {
        serde_json::from_str(MODULE_JSON).expect("the literal is schema 4")
    }

    fn shown<'a>(items: &[PageItem<'a>]) -> Vec<&'a str> {
        items
            .iter()
            .map(|item| match item {
                PageItem::ModuleDoc(doc) => doc.text.as_str(),
                PageItem::Decl(decl) => decl.name.as_str(),
            })
            .collect()
    }

    #[test]
    fn the_suppressed_set_spans_the_site() {
        let module = module();
        // Same declaration, filed under a module that does not own the
        // structure it is a member of. Collecting per module leaves it on the
        // page; collecting over the site does not.
        let other: ModuleFile = serde_json::from_str(
            r#"{"schemaVersion": 4, "module": "Pkg.One", "imports": [],
                "moduleDocs": [], "tactics": [],
                "declarations": [
                  {"name": "Pkg.One.s", "kind": "structure", "modifiers": [],
                   "binders": [], "implicits": [], "binderCode": [], "type": "T",
                   "typeCode": [], "line": 1, "col": 0, "endLine": 1, "endCol": 1,
                   "index": 0,
                   "members": [{"label": "ctor", "name": "Pkg.Two.b", "text": "",
                                "code": [], "binders": [], "implicits": [],
                                "binderCode": [], "doc": null, "isDirect": true}],
                   "doc": null, "equations": [], "equationCode": [], "refs": []}
                ]}"#,
        )
        .expect("the literal is schema 4");

        let site = Suppressed::of_site([&module, &other]);
        assert!(
            site.contains("Pkg.Two.a.mk"),
            "this module's own constructor"
        );
        assert!(
            site.contains("Pkg.Two.b"),
            "a member declared in another module: the reason the set is not per module"
        );
        assert_eq!(site.len(), 2);

        let per_module = Suppressed::of_site([&module]);
        assert!(
            !per_module.contains("Pkg.Two.b"),
            "if this ever becomes true the test above stops proving anything"
        );
        assert_eq!(
            shown(&page_items(&module, &per_module)),
            ["first", "Pkg.Two.a", "second", "Pkg.Two.b"],
            "the per-module set leaves the other module's member on the page"
        );
        assert_eq!(
            shown(&page_items(&module, &site)),
            ["first", "Pkg.Two.a", "second"]
        );
    }

    /// The tie-breaker, on its own. `Pkg.Two.b` has in-module index 0 and sits
    /// at the same `(line, col)` as the second module docstring, so `index` and
    /// `k + index` disagree about which comes first — and only here.
    #[test]
    fn a_module_docstring_precedes_a_declaration_at_the_same_position() {
        let module = module();
        let none = Suppressed::default();
        assert_eq!(
            shown(&page_items(&module, &none)),
            ["first", "Pkg.Two.a", "Pkg.Two.a.mk", "second", "Pkg.Two.b"],
            "the docstring at 7:0 comes before the declaration at 7:0"
        );
        // And the declarations keep their own relative order, which is what the
        // offset preserves: `a` (index 1) before `a.mk` (index 2) at 5:0.
        let at_five: Vec<&str> = shown(&page_items(&module, &none))
            .into_iter()
            .filter(|s| s.starts_with("Pkg.Two.a"))
            .collect();
        assert_eq!(at_five, ["Pkg.Two.a", "Pkg.Two.a.mk"]);
    }

    #[test]
    fn pages_live_at_their_module_path() {
        assert_eq!(page_path("Foo"), std::path::Path::new("Foo.html"));
        assert_eq!(
            page_path("Foo.Bar.Baz"),
            std::path::Path::new("Foo/Bar/Baz.html")
        );
    }

    /// The frame around `main`, which is the only part of the page that is not
    /// assembled from a piece checked against the prototype.
    #[test]
    fn the_page_wraps_main_in_the_frame() {
        let module = module();
        let mut builder = NameIndex::builder();
        builder.module(&module);
        let index = builder.build(crate::LinkIndex::default(), crate::ExternalLinks::default());
        let site = SiteMeta::of_modules([module.module.as_str()]);
        let html = page_html(
            &module,
            &index,
            "https://h/o/r/blob/dead",
            &Suppressed::default(),
            &site,
        )
        .expect("every name in the fixture is placeable");
        assert!(
            html.starts_with("<!DOCTYPE html><html lang=\"en\"><head>"),
            "{html}"
        );
        assert!(
            html.contains("</head><body data-root=\".././\" data-module=\"Pkg.Two\">"),
            "the page tells `app.js` where it is: {html}"
        );
        assert!(
            html.contains("<main class=\"content\" id=\"content\"><div class=\"modhead\">"),
            "{html}"
        );
        // The module's own heading and its imports come before the first thing
        // the module itself wrote.
        let modmeta = html
            .find("<div class=\"modmeta\">")
            .expect("the import block");
        let first_doc = html
            .find("<div class=\"moddoc\"><p>first</p></div>")
            .expect("the first module docstring");
        assert!(modmeta < first_doc, "{html}");
        assert!(html.ends_with("</main></div></body></html>"), "{html}");

        // The sidebar's table of contents lists the page's entries in page
        // order, so the docstring ordering reaches two places in the output,
        // not one.
        let toc = &html[html.find("class=\"toc\"").unwrap()..html.find("<main").unwrap()];
        let first = toc.find("#Pkg.Two.a\"").unwrap();
        let second = toc.find("#Pkg.Two.b\"").unwrap();
        assert!(first < second, "{toc}");
    }
}
