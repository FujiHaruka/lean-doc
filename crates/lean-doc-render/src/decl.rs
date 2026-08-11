//! One declaration's block on a module page.
//!
//! Ported from `experiments/stage7d/render.ts` (frozen): `declHeader` 726-777,
//! `equationsHtml` 1677-1702, `instancesForHtml` 1705-1709,
//! `classInstancesHtml` 1712-1716, `declNameToLink` 1725-1736,
//! `containedNames` 1750-1761, `structureHtml` 1774-1831, `declHtml`
//! 1834-1897. Upstream: `docInfoHeader` / `structureInfoHeader` /
//! `docInfoToHtml` (`Output/Module.lean`), `argToHtml` (`Output/Arg.lean`),
//! `equationsToHtml` (`Output/Definition.lean`), `structureToHtml` /
//! `fieldToHtml` (`Output/Structure.lean`), `instancesForToHtml`
//! (`Output/Inductive.lean`), `classInstancesToHtml` (`Output/Class.lean`).
//!
//! # Five things that are easy to get subtly wrong
//!
//! 1. **An inherited field is `isDirect === false`, not `!isDirect`.** The
//!    prototype's test is an identity comparison, so a *missing* key is a
//!    direct field. [`lean_doc_ir::Member::is_direct`] is therefore an
//!    `Option<bool>` and [`lean_doc_ir::Member::is_inherited`] is the only
//!    reader — plan §5. No byte comparison on this package can catch the other
//!    reading: all 156 field members carry the key 【実測】.
//! 2. **The equation limit counts code points**, not bytes and not UTF-16
//!    units (`RenderedCode.textLength` is over Lean `Char`s).
//! 3. **The `div.attributes` element ends in a newline.** It is the one
//!    non-flattened element at this level, so `Html.toStringAux` prints
//!    `<div …>…</div>\n`, and the newline belongs to the element rather than to
//!    the join around it.
//! 4. **[`decl_name_to_link`] fails rather than guessing.** doc-gen4 indexes
//!    `name2ModIdx` with `!` and panics; emitting a plausible `href` instead
//!    would be a wrong byte that costs a debugging round to locate.
//! 5. **Attribute order is byte identity.** [`DeclRenderer::structure_html`]'s
//!    two `<li>` shapes write `id` before `class` and `class` alone; the
//!    inherited branch's optional `id` is not the direct branch's `id`, and
//!    the two are written out separately for that reason.

use std::collections::HashSet;
use std::fmt;

use lean_doc_ir::{Decl, Member, ModuleFile, Span, Utf16Text};
use lean_doc_md::Renderer as DocRenderer;

use crate::autolink::{NameIndex, module_link, page_root};
use crate::code::{CodeRenderer, Refs, decl_refs};
use crate::escape::escape_html_into;
use crate::{break_within, css_kind, kind_description};

/// `Process/Base.lean:119` — an equation whose printed text reaches this many
/// **code points** is stored as NULL by the DB and replaced by a notice.
pub const EQUATION_LIMIT: usize = 200;

/// A name that has to be linked and cannot be placed in a module.
///
/// doc-gen4 reaches this state with `name2ModIdx[name]!`, i.e. it panics. This
/// crate returns instead of panicking, but it does **not** invent a link: a
/// wrong `href` is a wrong byte either way, and a silent one costs a debugging
/// round to find (`render.ts:1732-1734`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnplaceableName {
    pub name: String,
}

impl fmt::Display for UnplaceableName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "declNameToLink: no defining module for {} (doc-gen4 would panic here)",
            self.name
        )
    }
}

impl std::error::Error for UnplaceableName {}

/// `declNameToLink` (`Base.lean:231-234`): the module a rendered name lives in,
/// the declaration's own references first.
///
/// The lookup is [`NameIndex::known`] and deliberately **not**
/// [`NameIndex::module_of`] — the dependency closure's `.lidx` belongs to the
/// docstring path, as it does in [`CodeRenderer::const_link`].
pub fn decl_name_to_link(
    name: &str,
    root: &str,
    refs: &Refs<'_>,
    names: &NameIndex,
) -> Result<String, UnplaceableName> {
    let module = refs
        .get(name)
        .copied()
        .or_else(|| names.known(name))
        .ok_or_else(|| UnplaceableName {
            name: name.to_owned(),
        })?;
    Ok(format!("{}#{name}", module_link(root, module)))
}

/// `instancesForToHtml` (`Inductive.lean:12-17`): an empty list the browser
/// fills in from `declaration-data.bmp`.
#[must_use]
pub fn instances_for_html(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 160);
    out.push_str("<details id=\"");
    escape_html_into(&mut out, &format!("instances-for-list-{name}"));
    out.push_str(
        "\" class=\"instances-for-list\"><summary>Instances For</summary>\
         <ul class=\"instances-for-enum\"></ul></details>",
    );
    out
}

/// `classInstancesToHtml` (`Class.lean:11-16`): also a stub, and **not** the
/// same shape as [`instances_for_html`] — the `id` moves from the `<details>`
/// to the `<ul>`.
#[must_use]
pub fn class_instances_html(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 140);
    out.push_str("<details class=\"instances\"><summary>Instances</summary><ul id=\"");
    escape_html_into(&mut out, &format!("instances-list-{name}"));
    out.push_str("\" class=\"instances-list\"></ul></details>");
    out
}

/// The `containedNames` query (`DB/Read.lean:177-185`): which names of the same
/// module have their declaration range **inside** `parent`'s.
///
/// The population is every declaration the IR carries for the module, including
/// the ones that never get a page entry, which is the same population as the
/// DB's `name_info` rows. Both comparisons are non-strict on the inner
/// coordinate (`col >= parent.col`, `end_col <= parent.end_col`) — a field
/// declared at exactly the structure's own start counts.
#[must_use]
pub fn contained_names<'m>(module: &'m ModuleFile, parent: &Decl) -> HashSet<&'m str> {
    let mut out = HashSet::new();
    for decl in &module.declarations {
        if decl.name == parent.name {
            continue;
        }
        let starts_inside =
            decl.line > parent.line || (decl.line == parent.line && decl.col >= parent.col);
        let ends_inside = decl.end_line < parent.end_line
            || (decl.end_line == parent.end_line && decl.end_col <= parent.end_col);
        if starts_inside && ends_inside {
            out.insert(decl.name.as_str());
        }
    }
    out
}

/// `equationsToHtml` (`Definition.lean`) plus the DB's `equationLimit` filter.
///
/// Returns the empty string when there is nothing to show — an equation list
/// that is empty because every equation was dropped still renders, with the
/// notice and no items.
#[must_use]
pub fn equations_html(decl: &Decl, root: &str, refs: &Refs<'_>, code: &CodeRenderer<'_>) -> String {
    let mut keep: Vec<usize> = Vec::with_capacity(decl.equations.len());
    let mut omitted = false;
    for (i, equation) in decl.equations.iter().enumerate() {
        // Code points. `chars().count()` and not `len()`, and not
        // `len_utf16()`: this package has equations that differ under all three.
        if equation.as_str().chars().count() < EQUATION_LIMIT {
            keep.push(i);
        } else {
            omitted = true;
        }
    }
    if keep.is_empty() && !omitted {
        return String::new();
    }
    let mut out = String::with_capacity(256);
    out.push_str("<details><summary>Equations</summary><ul class=\"equations\">");
    if omitted {
        out.push_str(
            "<li class=\"equation\">One or more equations did not get rendered \
             due to their size.</li>",
        );
    }
    let empty: Vec<Span> = Vec::new();
    for i in keep {
        out.push_str("<li class=\"equation\">");
        let body = code.fragment(
            &decl.equations[i],
            decl.equation_code.get(i).unwrap_or(&empty),
            root,
            refs,
        );
        out.push_str(&body.html);
        out.push_str("</li>");
    }
    out.push_str("</ul></details>");
    out
}

/// `argToHtml` (`Arg.lean`), which is byte-identical in the two places that
/// call it — the declaration header and a structure field's own signature.
///
/// `Html.element "span" false` is the non-flattened form, hence the newline
/// after the open tag and after the close tag.
fn push_arg(out: &mut String, body: &str, implicit: bool) {
    if implicit {
        out.push_str("<span class=\"impl_arg\">");
    }
    out.push_str("<span class=\"decl_args\">\n<span class=\"fn\">");
    out.push_str(body);
    out.push_str("</span></span>\n");
    if implicit {
        out.push_str("</span>");
    }
}

/// Every binder of a declaration or of a structure field, in order.
///
/// `implicits` may be shorter than `binders` — the prototype indexes it and
/// gets `undefined`, which is falsy — so a missing entry is explicit.
fn push_args(
    out: &mut String,
    binders: &[Utf16Text],
    binder_code: &[Vec<Span>],
    implicits: &[bool],
    root: &str,
    refs: &Refs<'_>,
    code: &CodeRenderer<'_>,
) {
    let empty: Vec<Span> = Vec::new();
    for (i, binder) in binders.iter().enumerate() {
        let body = code.fragment(binder, binder_code.get(i).unwrap_or(&empty), root, refs);
        push_arg(out, &body.html, implicits.get(i).copied().unwrap_or(false));
    }
}

/// `docInfoHeader` + `structureInfoHeader` (`Module.lean`) — `div.decl_header`.
///
/// Takes the module **name** rather than the module file: this is called for
/// every declaration in the IR, including the ones that get no page entry,
/// before anything about the page is known (`render.ts:2104-2109`).
#[must_use]
pub fn decl_header(decl: &Decl, module: &str, code: &CodeRenderer<'_>) -> String {
    let root = page_root(module);
    let refs = decl_refs(decl);
    let mut out = String::with_capacity(512);

    out.push_str("<div class=\"decl_header\">");
    // `Html.element "span" false #[text kind]`: a lone text child means no
    // newline after the open tag, but there is still one after the close.
    out.push_str("<span class=\"decl_kind\">");
    escape_html_into(&mut out, &kind_description(&decl.kind, &decl.modifiers));
    out.push_str("</span>\n");

    out.push_str("<span class=\"decl_name\"><a class=\"break_within\" href=\"");
    let mut self_link = module_link(&root, module);
    self_link.push('#');
    self_link.push_str(&decl.name);
    escape_html_into(&mut out, &self_link);
    out.push_str("\">");
    out.push_str(&break_within(&decl.name));
    out.push_str("</a></span>");

    push_args(
        &mut out,
        &decl.binders,
        &decl.binder_code,
        &decl.implicits,
        &root,
        &refs,
        code,
    );

    // `structureInfoHeader`, structures and classes only. A `class_inductive`
    // has no parents section even when it has parent members.
    if decl.kind == "structure" || decl.kind == "class" {
        let parents: Vec<&Member> = decl
            .members
            .iter()
            .filter(|m| m.label == "parent")
            .collect();
        if !parents.is_empty() {
            out.push_str("<span class=\"decl_extends\">extends</span> ");
            for (i, parent) in parents.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str("<span id=\"");
                escape_html_into(&mut out, &parent.name);
                out.push_str("\">");
                let body = code.fragment(&parent.text, &parent.code, &root, &refs);
                out.push_str(&body.html);
                out.push_str("</span>");
            }
        }
    }

    // `Html.element "span" true #[text " :"]` — inline, so no newlines.
    out.push_str("<span class=\"decl_args\"> :</span>");
    out.push_str("<div class=\"decl_type\">");
    let ty = code.fragment(&decl.ty, &decl.type_code, &root, &refs);
    out.push_str(&ty.html);
    out.push_str("</div></div>");
    out
}

/// Everything one page's declarations render against.
///
/// Per page rather than per run because three of the five members are: the
/// root, the source URL and the docstring renderer (whose resolver scans *this*
/// module's declarations) all change from page to page.
pub struct DeclRenderer<'a> {
    module: &'a ModuleFile,
    root: &'a str,
    source_url: &'a str,
    code: CodeRenderer<'a>,
    docs: &'a DocRenderer<'a>,
}

impl<'a> DeclRenderer<'a> {
    /// `root` is [`page_root`] of `module`, `source_url` is
    /// [`crate::module_source_url`] of it, and `docs` is the docstring renderer
    /// built from this page's [`crate::PageLinks`].
    ///
    /// The two renderers are separate arguments because they resolve names
    /// against different maps on purpose: the code renderer reads the IR's own
    /// map, the docstring renderer also reads the dependency closure's `.lidx`
    /// (`render.ts:2059-2064`).
    #[must_use]
    pub const fn new(
        module: &'a ModuleFile,
        root: &'a str,
        source_url: &'a str,
        code: CodeRenderer<'a>,
        docs: &'a DocRenderer<'a>,
    ) -> Self {
        Self {
            module,
            root,
            source_url,
            code,
            docs,
        }
    }

    /// [`decl_header`] for a declaration of this page.
    #[must_use]
    pub fn header(&self, decl: &Decl) -> String {
        decl_header(decl, &self.module.module, &self.code)
    }

    /// `structureToHtml` + `fieldToHtml` (`Structure.lean`).
    ///
    /// The constructor decides the outer shape: a constructor whose last
    /// component is `mk` is the anonymous one and the fields are a plain list;
    /// anything else is printed as `Name :: ( … )`. A structure with no `ctor`
    /// member at all is treated as having `<name>.mk`, i.e. the first shape.
    pub fn structure_html(&self, decl: &Decl) -> Result<String, UnplaceableName> {
        let refs = decl_refs(decl);
        let mut contained: Option<HashSet<&str>> = None;
        let mut lis = String::with_capacity(512);
        for field in decl.members.iter().filter(|m| m.label == "field") {
            self.field_html(&mut lis, decl, field, &refs, &mut contained)?;
        }

        let ctor_name = match decl.members.iter().find(|m| m.label == "ctor") {
            Some(ctor) => ctor.name.clone(),
            None => format!("{}.mk", decl.name),
        };
        let short = last_component(&ctor_name);
        let mut out = String::with_capacity(lis.len() + 128);
        if short == "mk" {
            out.push_str("<ul class=\"structure_fields\" id=\"");
            escape_html_into(&mut out, &ctor_name);
            out.push_str("\">");
            out.push_str(&lis);
            out.push_str("</ul>");
            return Ok(out);
        }
        out.push_str("<ul class=\"structure_ext\"><li id=\"");
        escape_html_into(&mut out, &ctor_name);
        out.push_str("\" class=\"structure_ext_ctor\">");
        // The space is inside the escape, as it is in the prototype.
        escape_html_into(&mut out, &format!("{short} "));
        out.push_str(" :: (</li><ul class=\"structure_ext_fields\">");
        out.push_str(&lis);
        out.push_str("</ul><li class=\"structure_ext_ctor\">)</li></ul>");
        Ok(out)
    }

    /// `fieldToHtml`, whose two branches differ in more than a CSS class.
    ///
    /// `contained` is the lazily built [`contained_names`] of the structure: the
    /// prototype computes it on the first inherited field and not at all
    /// otherwise, which matters because it is a scan of the whole module.
    fn field_html(
        &self,
        out: &mut String,
        decl: &Decl,
        field: &Member,
        refs: &Refs<'_>,
        contained: &mut Option<HashSet<&'a str>>,
    ) -> Result<(), UnplaceableName> {
        let short = last_component(&field.name);
        let mut args = String::new();
        push_args(
            &mut args,
            &field.binders,
            &field.binder_code,
            &field.implicits,
            self.root,
            refs,
            &self.code,
        );
        let body = self
            .code
            .fragment(&field.text, &field.code, self.root, refs);

        if field.is_inherited() {
            let link = decl_name_to_link(&field.name, self.root, refs, self.code.names())?;
            let contained = contained.get_or_insert_with(|| contained_names(self.module, decl));
            let proj_name = format!("{}.{short}", decl.name);
            if contained.contains(proj_name.as_str()) {
                out.push_str("<li id=\"");
                escape_html_into(out, &proj_name);
                out.push_str("\" class=\"structure_field inherited_field\">");
            } else {
                out.push_str("<li class=\"structure_field inherited_field\">");
            }
            out.push_str("<div class=\"structure_field_info\"><a href=\"");
            escape_html_into(out, &link);
            out.push_str("\">");
            escape_html_into(out, short);
            out.push_str("</a>");
            out.push_str(&args);
            out.push_str(" : ");
            out.push_str(&body.html);
            out.push_str("</div></li>");
            return Ok(());
        }

        out.push_str("<li id=\"");
        escape_html_into(out, &field.name);
        out.push_str("\" class=\"structure_field\"><div class=\"structure_field_info\">");
        escape_html_into(out, short);
        out.push_str(&args);
        out.push_str(" : ");
        out.push_str(&body.html);
        out.push_str("</div>");
        if let Some(doc) = nonempty(field.doc.as_deref()) {
            out.push_str("<div class=\"structure_field_doc\">");
            out.push_str(&self.docs.docstring(doc));
            out.push_str("</div>");
        }
        out.push_str("</li>");
        Ok(())
    }

    /// `docInfoToHtml` (`Module.lean:67-112`) — the whole `div.decl`.
    ///
    /// `header` is [`DeclRenderer::header`] of the same declaration, passed in
    /// because the run computes every header before it lays out any page.
    pub fn decl_html(&self, decl: &Decl, header: &str) -> Result<String, UnplaceableName> {
        let refs = decl_refs(decl);

        let mut gh = String::with_capacity(self.source_url.len() + 64);
        gh.push_str("<div class=\"gh_link\"><a href=\"");
        escape_html_into(
            &mut gh,
            &format!("{}#L{}-L{}", self.source_url, decl.line, decl.end_line),
        );
        gh.push_str("\">source</a></div>");

        // `Html.element "div" false … #[text s]` is the one non-flattened
        // element at this level, so the trailing newline belongs to it.
        let mut attrs = String::new();
        if !decl.attrs.is_empty() {
            attrs.push_str("<div class=\"attributes\">");
            escape_html_into(&mut attrs, &format!("@[{}]", decl.attrs.join(", ")));
            attrs.push_str("</div>\n");
        }

        let doc = match nonempty(decl.doc.as_deref()) {
            Some(doc) => self.docs.docstring(doc),
            None => String::new(),
        };

        let mut body = String::new();
        let mut extra = String::new();
        match decl.kind.as_str() {
            kind @ ("structure" | "class") => {
                body = self.structure_html(decl)?;
                extra = if kind == "class" {
                    class_instances_html(&decl.name)
                } else {
                    instances_for_html(&decl.name)
                };
            }
            "definition" => {
                extra = equations_html(decl, self.root, &refs, &self.code);
                extra.push_str(&instances_for_html(&decl.name));
            }
            "instance" => extra = equations_html(decl, self.root, &refs, &self.code),
            "inductive" => extra = instances_for_html(&decl.name),
            "class_inductive" => extra = class_instances_html(&decl.name),
            // theorem / axiom / opaque / constructor
            _ => {}
        }

        let mut out = String::with_capacity(
            gh.len() + attrs.len() + header.len() + doc.len() + body.len() + extra.len() + 64,
        );
        out.push_str("<div class=\"decl\" id=\"");
        escape_html_into(&mut out, &decl.name);
        out.push_str("\"><div class=\"");
        escape_html_into(&mut out, css_kind(&decl.kind));
        out.push_str("\">");
        out.push_str(&gh);
        out.push_str(&attrs);
        out.push_str(header);
        out.push_str(&doc);
        out.push_str(&body);
        out.push_str(&extra);
        out.push_str("</div></div>");
        Ok(out)
    }
}

/// `name.split(".").pop()!` — the last dot-separated component, the whole name
/// when there is no dot. Never `None`: `split` yields at least one piece.
fn last_component(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or(name)
}

/// JavaScript truthiness for the two optional docstrings: `""` is falsy, so an
/// empty docstring renders **nothing**, not an empty `<div>`.
fn nonempty(s: Option<&str>) -> Option<&str> {
    s.filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::autolink::{PageLinks, module_decl_names};
    use crate::link_index::LinkIndex;
    use lean_doc_ir::SpanKind;

    fn index(entries: &[(&str, &str)]) -> NameIndex {
        let mut builder = NameIndex::builder();
        for (name, module) in entries {
            builder.declaration(name, module);
        }
        builder.build(LinkIndex::default())
    }

    /// A schema-4 declaration with everything empty, which the tests fill in.
    fn decl(name: &str, kind: &str) -> Decl {
        serde_json::from_str(&format!(
            r#"{{"name": {name:?}, "kind": {kind:?}, "modifiers": [], "binders": [],
                "implicits": [], "binderCode": [], "type": "", "typeCode": [],
                "line": 1, "col": 0, "endLine": 1, "endCol": 1, "index": 0,
                "members": [], "doc": null, "equations": [], "equationCode": [],
                "refs": []}}"#
        ))
        .expect("the literal is schema 4")
    }

    fn module_with(decls: Vec<Decl>) -> ModuleFile {
        let mut module: ModuleFile = serde_json::from_str(
            r#"{"schemaVersion": 4, "module": "Pkg.M", "imports": [],
                "moduleDocs": [], "tactics": [], "declarations": []}"#,
        )
        .expect("the literal is schema 4");
        module.declarations = decls;
        module
    }

    fn member(json: &str) -> Member {
        serde_json::from_str(json).expect("the literal is a schema-4 member")
    }

    #[test]
    fn a_header_is_kind_name_binders_and_type() {
        let names = index(&[("Nat", "Init.Prelude")]);
        let mut d = decl("Pkg.M.f", "definition");
        d.modifiers = vec!["abbrev".to_owned()];
        d.binders = vec![Utf16Text::from("(n : Nat)"), Utf16Text::from("{m : Nat}")];
        d.implicits = vec![false, true];
        d.binder_code = vec![
            vec![Span {
                start: 5,
                stop: 8,
                kind: SpanKind::Const,
                name: Some("Nat".to_owned()),
                front: 0,
                back: 0,
            }],
            vec![],
        ];
        d.ty = Utf16Text::from("Nat");
        d.type_code = vec![Span {
            start: 0,
            stop: 3,
            kind: SpanKind::Const,
            name: Some("Nat".to_owned()),
            front: 0,
            back: 0,
        }];
        assert_eq!(
            decl_header(&d, "Pkg.M", &CodeRenderer::new(&names)),
            "<div class=\"decl_header\"><span class=\"decl_kind\">abbrev</span>\n\
             <span class=\"decl_name\"><a class=\"break_within\" href=\".././Pkg/M.html#Pkg.M.f\">\
             <span class=\"name\">Pkg</span>.<span class=\"name\">M</span>.\
             <span class=\"name\">f</span></a></span>\
             <span class=\"decl_args\">\n<span class=\"fn\">\
             (n : <a href=\".././Init/Prelude.html#Nat\">Nat</a>)</span></span>\n\
             <span class=\"impl_arg\"><span class=\"decl_args\">\n\
             <span class=\"fn\">{m : Nat}</span></span>\n</span>\
             <span class=\"decl_args\"> :</span>\
             <div class=\"decl_type\"><a href=\".././Init/Prelude.html#Nat\">Nat</a></div></div>"
        );
    }

    /// `extends` is emitted for structures and classes only, and the parents
    /// are joined with `", "`.
    #[test]
    fn extends_is_rendered_for_structures_and_classes_only() {
        let names = index(&[]);
        let code = CodeRenderer::new(&names);
        let parents = vec![
            member(r#"{"label": "parent", "name": "P.to<A", "text": "A", "code": []}"#),
            member(r#"{"label": "parent", "name": "P.toB", "text": "B", "code": []}"#),
        ];
        for kind in ["structure", "class"] {
            let mut d = decl("P", kind);
            d.members.clone_from(&parents);
            let html = decl_header(&d, "Pkg", &code);
            assert!(
                html.contains(
                    "<span class=\"decl_extends\">extends</span> \
                     <span id=\"P.to&lt;A\">A</span>, <span id=\"P.toB\">B</span>\
                     <span class=\"decl_args\"> :</span>"
                ),
                "{kind}: {html}"
            );
        }
        // Same members under a kind that has no parents section.
        let mut d = decl("P", "class_inductive");
        d.members = parents;
        assert!(!decl_header(&d, "Pkg", &code).contains("decl_extends"));
    }

    #[test]
    fn the_equation_limit_is_in_code_points() {
        let names = index(&[]);
        let code = CodeRenderer::new(&names);
        let mut d = decl("f", "definition");
        // 199 code points, but 597 bytes and 398 UTF-16 units: a byte- or
        // unit-based limit drops this one.
        d.equations = vec![Utf16Text::from("𝒜".repeat(199).as_str())];
        d.equation_code = vec![vec![]];
        let html = equations_html(&d, "./", &Refs::default(), &code);
        assert!(html.contains(&"𝒜".repeat(199)), "the equation was dropped");
        assert!(!html.contains("did not get rendered"));

        d.equations = vec![Utf16Text::from("𝒜".repeat(200).as_str())];
        let html = equations_html(&d, "./", &Refs::default(), &code);
        assert_eq!(
            html,
            "<details><summary>Equations</summary><ul class=\"equations\">\
             <li class=\"equation\">One or more equations did not get rendered \
             due to their size.</li></ul></details>",
            "the notice appears and the equation does not"
        );

        // No equations at all: nothing, not an empty `<details>`.
        d.equations = vec![];
        d.equation_code = vec![];
        assert_eq!(equations_html(&d, "./", &Refs::default(), &code), "");
    }

    #[test]
    fn the_two_instance_stubs_put_the_id_in_different_places() {
        assert_eq!(
            instances_for_html("A<B"),
            "<details id=\"instances-for-list-A&lt;B\" class=\"instances-for-list\">\
             <summary>Instances For</summary><ul class=\"instances-for-enum\"></ul></details>"
        );
        assert_eq!(
            class_instances_html("A<B"),
            "<details class=\"instances\"><summary>Instances</summary>\
             <ul id=\"instances-list-A&lt;B\" class=\"instances-list\"></ul></details>"
        );
    }

    #[test]
    fn an_unplaceable_name_is_an_error_and_not_a_guess() {
        let names = index(&[("known", "Pkg.M")]);
        assert_eq!(
            decl_name_to_link("known", "./", &Refs::default(), &names).as_deref(),
            Ok("./Pkg/M.html#known")
        );
        assert_eq!(
            decl_name_to_link("nowhere", "./", &Refs::default(), &names),
            Err(UnplaceableName {
                name: "nowhere".to_owned()
            })
        );
    }

    /// The range test is non-strict at both ends and skips the parent itself.
    #[test]
    fn contained_names_uses_closed_ranges() {
        let mut parent = decl("S", "structure");
        parent.line = 10;
        parent.col = 2;
        parent.end_line = 20;
        parent.end_col = 8;
        let at = |name: &str, l: u32, c: u32, el: u32, ec: u32| {
            let mut d = decl(name, "definition");
            d.line = l;
            d.col = c;
            d.end_line = el;
            d.end_col = ec;
            d
        };
        let module = module_with(vec![
            parent.clone(),
            at("exact", 10, 2, 20, 8),
            at("inside", 11, 0, 19, 99),
            at("startsBefore", 10, 1, 20, 8),
            at("endsAfter", 10, 2, 20, 9),
            at("linesBefore", 9, 99, 20, 8),
            at("linesAfter", 10, 2, 21, 0),
        ]);
        let mut got: Vec<&str> = contained_names(&module, &parent).into_iter().collect();
        got.sort_unstable();
        assert_eq!(got, ["exact", "inside"]);
    }

    struct Page {
        names: NameIndex,
        module: ModuleFile,
    }

    impl Page {
        fn new(names: NameIndex, module: ModuleFile) -> Self {
            Self { names, module }
        }

        /// Renders one declaration of the page, wiring the two renderers the
        /// way a run does.
        fn render(&self, at: usize) -> Result<String, UnplaceableName> {
            let root = page_root(&self.module.module);
            let decl_names = module_decl_names(&self.module);
            let links = PageLinks::new(&self.names, &root, &decl_names);
            let docs = links.renderer();
            let code = CodeRenderer::new(&self.names);
            let renderer = DeclRenderer::new(&self.module, &root, "https://x/M.lean", code, &docs);
            let decl = &self.module.declarations[at];
            let header = renderer.header(decl);
            renderer.decl_html(decl, &header)
        }
    }

    /// The whole `div.decl`, with the attribute block's trailing newline and
    /// the docstring in their places.
    #[test]
    fn a_declaration_is_link_attributes_header_doc_and_extra() {
        let mut d = decl("Pkg.M.f", "definition");
        d.line = 7;
        d.end_line = 9;
        d.attrs = vec!["simp".to_owned(), "reducible".to_owned()];
        d.doc = Some("hello".to_owned());
        let page = Page::new(index(&[]), module_with(vec![d]));
        let html = page.render(0).expect("nothing to place");
        assert!(html.starts_with(
            "<div class=\"decl\" id=\"Pkg.M.f\"><div class=\"def\">\
             <div class=\"gh_link\"><a href=\"https://x/M.lean#L7-L9\">source</a></div>\
             <div class=\"attributes\">@[simp, reducible]</div>\n\
             <div class=\"decl_header\">"
        ));
        assert!(html.contains("</div><p>hello</p><details id=\"instances-for-list-Pkg.M.f\""));
        assert!(html.ends_with("</ul></details></div></div>"));
    }

    /// An empty docstring is falsy in JavaScript, so it renders nothing — not
    /// the two newlines `docStringToHtml` would append to it.
    #[test]
    fn an_empty_docstring_renders_nothing() {
        let mut d = decl("f", "theorem");
        d.doc = Some(String::new());
        let page = Page::new(index(&[]), module_with(vec![d]));
        let html = page.render(0).expect("nothing to place");
        assert!(html.contains("</div></div></div>"), "{html}");
        assert!(!html.contains("<p>"), "{html}");
    }

    #[test]
    fn each_kind_gets_its_own_extra() {
        for (kind, css, extra) in [
            ("definition", "def", "instances-for-list-X"),
            ("inductive", "inductive", "instances-for-list-X"),
            ("class_inductive", "class", "instances-list-X"),
            ("instance", "instance", ""),
            ("theorem", "theorem", ""),
            ("constructor", "ctor", ""),
        ] {
            let page = Page::new(index(&[]), module_with(vec![decl("X", kind)]));
            let html = page.render(0).expect("nothing to place");
            assert!(
                html.starts_with(&format!(
                    "<div class=\"decl\" id=\"X\"><div class=\"{css}\">"
                )),
                "{kind}: {html}"
            );
            if extra.is_empty() {
                assert!(!html.contains("<details"), "{kind}: {html}");
            } else {
                assert!(html.contains(extra), "{kind}: {html}");
            }
        }
    }

    /// The two `<ul>` shapes, and the fact that a missing `ctor` takes the
    /// first one.
    #[test]
    fn the_constructor_name_decides_the_structure_shape() {
        let field = r#"{"label": "field", "name": "S.x", "text": "Nat", "code": [],
                        "binders": [], "implicits": [], "binderCode": [],
                        "doc": null, "isDirect": true}"#;
        let mut d = decl("S", "structure");
        d.members = vec![member(field)];
        let page = Page::new(index(&[]), module_with(vec![d.clone()]));
        let html = page.render(0).expect("nothing to place");
        assert!(
            html.contains(
                "<ul class=\"structure_fields\" id=\"S.mk\">\
                 <li id=\"S.x\" class=\"structure_field\">\
                 <div class=\"structure_field_info\">x : Nat</div></li></ul>"
            ),
            "{html}"
        );

        // An explicit `mk` constructor is the same shape.
        d.members.push(member(
            r#"{"label": "ctor", "name": "S.mk", "text": "", "code": []}"#,
        ));
        let page = Page::new(index(&[]), module_with(vec![d.clone()]));
        assert!(
            page.render(0)
                .expect("nothing to place")
                .contains("<ul class=\"structure_fields\" id=\"S.mk\">")
        );

        // Any other constructor name is `structure_ext`.
        d.members.pop();
        d.members.push(member(
            r#"{"label": "ctor", "name": "S.make", "text": "", "code": []}"#,
        ));
        let page = Page::new(index(&[]), module_with(vec![d]));
        let html = page.render(0).expect("nothing to place");
        assert!(
            html.contains(
                "<ul class=\"structure_ext\"><li id=\"S.make\" class=\"structure_ext_ctor\">\
                 make  :: (</li><ul class=\"structure_ext_fields\">"
            ),
            "{html}"
        );
        assert!(
            html.contains("</ul><li class=\"structure_ext_ctor\">)</li></ul>"),
            "{html}"
        );
    }

    /// The inherited branch: a different `<li>`, a link instead of plain text,
    /// no docstring, and an `id` only when the projection is declared inside
    /// the structure's own range.
    #[test]
    fn an_inherited_field_is_a_link_and_an_absent_key_is_not_inherited() {
        let direct = r#"{"label": "field", "name": "S.x", "text": "Nat", "code": [],
                         "doc": "a field", "isDirect": true}"#;
        let inherited = r#"{"label": "field", "name": "P.y", "text": "Nat", "code": [],
                            "doc": "ignored", "isDirect": false}"#;
        let absent = r#"{"label": "field", "name": "S.z", "text": "Nat", "code": []}"#;
        let mut s = decl("S", "structure");
        s.line = 1;
        s.end_line = 5;
        s.end_col = 0;
        s.members = vec![member(direct), member(inherited), member(absent)];
        let names = index(&[("P.y", "Pkg.Parent")]);
        let page = Page::new(names, module_with(vec![s]));
        let html = page.render(0).expect("P.y is in the index");

        assert!(
            html.contains(
                "<li id=\"S.x\" class=\"structure_field\">\
                 <div class=\"structure_field_info\">x : Nat</div>\
                 <div class=\"structure_field_doc\"><p>a field</p></div></li>"
            ),
            "{html}"
        );
        assert!(
            html.contains(
                "<li class=\"structure_field inherited_field\">\
                 <div class=\"structure_field_info\">\
                 <a href=\".././Pkg/Parent.html#P.y\">y</a> : Nat</div></li>"
            ),
            "the inherited field carries no id and no docstring: {html}"
        );
        // The key is missing on `S.z`, so it is direct — the whole point of
        // `Option<bool>`.
        assert!(
            html.contains("<li id=\"S.z\" class=\"structure_field\">"),
            "a member without `isDirect` must not be inherited: {html}"
        );
    }

    /// The `id` on an inherited field comes from `containedNames`, so a
    /// declaration `S.y` inside `S`'s range turns it on.
    #[test]
    fn an_inherited_field_gets_an_id_when_the_projection_is_contained() {
        let inherited = r#"{"label": "field", "name": "P.y", "text": "", "code": [],
                            "isDirect": false}"#;
        let mut s = decl("S", "structure");
        s.line = 1;
        s.end_line = 5;
        s.end_col = 0;
        s.members = vec![member(inherited)];
        let mut proj = decl("S.y", "definition");
        proj.line = 2;
        proj.end_line = 2;
        let names = index(&[("P.y", "Pkg.Parent")]);
        let page = Page::new(names, module_with(vec![s, proj]));
        let html = page.render(0).expect("P.y is in the index");
        assert!(
            html.contains(
                "<li id=\"S.y\" class=\"structure_field inherited_field\">\
                 <div class=\"structure_field_info\">"
            ),
            "{html}"
        );
    }

    /// A field that cannot be placed stops the page rather than producing a
    /// link that points somewhere plausible.
    #[test]
    fn an_inherited_field_with_no_module_fails_the_page() {
        let inherited = r#"{"label": "field", "name": "P.y", "text": "", "code": [],
                            "isDirect": false}"#;
        let mut s = decl("S", "structure");
        s.members = vec![member(inherited)];
        let page = Page::new(index(&[]), module_with(vec![s]));
        assert_eq!(
            page.render(0),
            Err(UnplaceableName {
                name: "P.y".to_owned()
            })
        );
    }
}
