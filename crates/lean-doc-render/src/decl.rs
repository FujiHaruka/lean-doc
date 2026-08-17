//! One declaration's block on a module page.
//!
//! **What is emitted here is lean-doc's own markup as of M8-b**; up to M7 it was
//! a transcription of doc-gen4's, byte for byte, because the acceptance oracle
//! compared the two. The *decisions* are still doc-gen4's, and they are the part
//! worth keeping: which kinds get equations, which get an instances block, when
//! an inherited field may claim an anchor, and what counts as an equation too
//! long to print. Those came out of `Output/{Module,Arg,Definition,Structure,
//! Inductive,Class}.lean` by way of `experiments/stage7d/render.ts`, and every
//! one of them is a fact about Lean rather than about a stylesheet.
//!
//! The shape they are poured into is new, and its reference is the hand-written
//! `design/preview/module.html`. Keep the two in step: the stylesheet is written
//! against that file, so a class renamed here loses its styling silently rather
//! than failing.
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
///
/// **Which module the field is in is one question; where that module's source
/// lives is another** (M7-c). The lookup above is unchanged and still refuses to
/// guess; what changed is that the href it hands back is
/// [`NameIndex::link_to`]'s, so an inherited field of a structure declared in a
/// dependency links at that dependency's pinned source. Inherited fields are the
/// case where that matters most: the parent structure is very often mathlib's.
///
/// # The two failures are different and the return type says so
///
/// `Err` is doc-gen4's panic: **no module at all** knows the name, so the IR
/// this run was handed disagrees with itself and the caller stops. `Ok(None)` is
/// the 2026-08-17 case: the module is known and belongs to a dependency with no
/// version-pinned URL, so there is a name to render and no page to point it at.
/// Collapsing the second into the first would refuse to render a page over a
/// missing link; collapsing it into `Ok(href)` is the dead link this changed.
pub fn decl_name_to_link(
    name: &str,
    root: &str,
    refs: &Refs<'_>,
    names: &NameIndex,
) -> Result<Option<String>, UnplaceableName> {
    let module = refs
        .get(name)
        .copied()
        .or_else(|| names.known(name))
        .ok_or_else(|| UnplaceableName {
            name: name.to_owned(),
        })?;
    Ok(names.link_to(root, module, Some(name)))
}

/// The instances of a type: empty markup `app.js` fills in on first open.
///
/// **Which instances exist is a fact about the whole site**, not about the
/// module being rendered — an instance of a type declared here can live in any
/// module — so it cannot be written statically by a renderer that is handed one
/// module at a time. doc-gen4 had the same problem and solved it the same way.
///
/// M8-c changed the contract rather than the shape: doc-gen4 keyed off the
/// element `id` and read `declaration-data.bmp`; this keys off `data-name` and
/// reads `search-index.json`, so the name no longer has to survive a round trip
/// through an HTML identifier. The two blocks below differ only in `data-fill`.
#[must_use]
pub fn instances_for_html(name: &str) -> String {
    fill_block(name, "instances-for", "Instances For")
}

/// The instances of a class. See [`instances_for_html`] — same shape, other map.
#[must_use]
pub fn class_instances_html(name: &str) -> String {
    fill_block(name, "instances", "Instances")
}

fn fill_block(name: &str, fill: &str, summary: &str) -> String {
    let mut out = String::with_capacity(name.len() + 120);
    out.push_str("<details class=\"extra\" data-fill=\"");
    out.push_str(fill);
    out.push_str("\" data-name=\"");
    escape_html_into(&mut out, name);
    out.push_str("\"><summary>");
    out.push_str(summary);
    out.push_str("</summary><ul></ul></details>");
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
    out.push_str("<details class=\"extra\"><summary>Equations</summary><ul class=\"equations\">");
    if omitted {
        out.push_str("<li>One or more equations did not get rendered due to their size.</li>");
    }
    let empty: Vec<Span> = Vec::new();
    for i in keep {
        out.push_str("<li>");
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

/// One binder, in the two places binders appear — a declaration's signature and
/// a structure field's own.
///
/// **The trailing newline is layout, not formatting.** A binder is an
/// `inline-block`, so the whitespace between two of them is what lets a line
/// break there; with the binders run together, a signature with eight implicit
/// arguments has no break point before its first space and overflows the column
/// on a phone. `.sig` deliberately does *not* set `pre-wrap` for this reason —
/// see the note in `assets/style.css`.
fn push_arg(out: &mut String, body: &str, implicit: bool) {
    out.push_str(if implicit {
        "<span class=\"binder implicit\">"
    } else {
        "<span class=\"binder\">"
    });
    out.push_str("<span class=\"fn\">");
    out.push_str(body);
    out.push_str("</span></span>\n");
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

/// The line a reader scans for: what kind of thing this is, what it is called,
/// and where its source is.
///
/// `<h2>` because it *is* the heading of the section below it, and because the
/// sidebar's table of contents is a list of these — a page whose declarations
/// are `<div>`s has no outline for a screen reader to walk.
///
/// Takes the module **name** rather than the module file: it is called for
/// declarations that get no page entry too, before anything about the page is
/// known.
#[must_use]
pub fn decl_head_html(decl: &Decl, module: &str, source_url: &str) -> String {
    let root = page_root(module);
    let mut out = String::with_capacity(384);

    out.push_str("<header class=\"decl-head\"><span class=\"kind\">");
    escape_html_into(&mut out, &kind_description(&decl.kind, &decl.modifiers));
    out.push_str("</span><h2 class=\"decl-name\"><a class=\"break_within\" href=\"");
    let mut self_link = module_link(&root, module);
    self_link.push('#');
    self_link.push_str(&decl.name);
    escape_html_into(&mut out, &self_link);
    out.push_str("\">");
    out.push_str(&break_within(&decl.name));
    out.push_str("</a></h2><a class=\"src\" href=\"");
    escape_html_into(
        &mut out,
        &format!("{source_url}#L{}-L{}", decl.line, decl.end_line),
    );
    out.push_str("\">source</a></header>");
    out
}

/// `<div class="sig">` — the binders, the `extends` clause, and the type.
///
/// Split from [`decl_head_html`] because they wrap differently: the head is a
/// flex row that reflows, the signature is code whose whitespace the IR already
/// decided (see [`push_arg`]).
#[must_use]
pub fn decl_signature(decl: &Decl, module: &str, code: &CodeRenderer<'_>) -> String {
    let root = page_root(module);
    let refs = decl_refs(decl);
    let mut out = String::with_capacity(512);

    out.push_str("<div class=\"sig\">");
    push_args(
        &mut out,
        &decl.binders,
        &decl.binder_code,
        &decl.implicits,
        &root,
        &refs,
        code,
    );

    // Structures and classes only. A `class_inductive` has no parents section
    // even when it has parent members.
    if decl.kind == "structure" || decl.kind == "class" {
        let parents: Vec<&Member> = decl
            .members
            .iter()
            .filter(|m| m.label == "parent")
            .collect();
        if !parents.is_empty() {
            out.push_str("<span class=\"extends\">extends</span> ");
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

    out.push_str("<span class=\"colon\"> :</span><div class=\"sig-type\">");
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
        // A constructor called `mk` is the anonymous one and saying so is
        // noise; anything else is a name the reader has to write, so it gets a
        // line of its own. doc-gen4 spelled the second case as nested lists
        // reading `Name :: ( … )`, which put the fields two levels deep for the
        // sake of a syntax nobody types at that position.
        if short != "mk" {
            out.push_str("<p class=\"ctor-note\">constructor <code>");
            escape_html_into(&mut out, short);
            out.push_str("</code></p>");
        }
        out.push_str("<ul class=\"fields\" id=\"");
        escape_html_into(&mut out, &ctor_name);
        out.push_str("\">");
        out.push_str(&lis);
        out.push_str("</ul>");
        Ok(out)
    }

    /// The constructors of an `inductive` or a `class inductive`
    /// (`inductiveToHtml` / `ctorToHtml`, `Output/Inductive.lean`).
    ///
    /// **This is the branch the measurement target cannot reach.** That package
    /// holds no `inductive` and no `class_inductive` declaration at all
    /// (`tests/page_parts.rs` names it as one of the nine such branches), so
    /// until `e2e/micro` existed nothing rendered a constructor through the real
    /// pipeline — and nothing did: the body came out empty, so the constructors
    /// were absent from the page while staying in the search index, which sends
    /// a reader to `#Micro.Colour.red` and lands them at the top of the page.
    ///
    /// Byte reproduction could not have caught it either: the oracle only ever
    /// saw pages of a package with no inductives on it.
    pub fn constructors_html(&self, decl: &Decl) -> String {
        let refs = decl_refs(decl);
        let mut lis = String::with_capacity(256);
        for ctor in decl.members.iter().filter(|m| m.label == "ctor") {
            self.ctor_html(&mut lis, ctor, &refs);
        }
        if lis.is_empty() {
            return String::new();
        }
        let mut out = String::with_capacity(lis.len() + 32);
        out.push_str("<ul class=\"ctors\">");
        out.push_str(&lis);
        out.push_str("</ul>");
        out
    }

    /// One constructor. Deliberately the same shape as a direct field
    /// ([`Self::field_html`]'s second branch): both are a name, its arguments,
    /// its type and an optional docstring, and a reader gains nothing from
    /// their being laid out differently. There is no inherited case — a
    /// constructor belongs to exactly one inductive.
    fn ctor_html(&self, out: &mut String, ctor: &Member, refs: &Refs<'_>) {
        let short = last_component(&ctor.name);
        let mut args = String::new();
        push_args(
            &mut args,
            &ctor.binders,
            &ctor.binder_code,
            &ctor.implicits,
            self.root,
            refs,
            &self.code,
        );
        let body = self.code.fragment(&ctor.text, &ctor.code, self.root, refs);
        out.push_str("<li id=\"");
        escape_html_into(out, &ctor.name);
        out.push_str("\" class=\"ctor\"><div class=\"field-sig\"><span class=\"field-name\">");
        escape_html_into(out, short);
        out.push_str("</span>");
        out.push_str(&args);
        out.push_str("<span class=\"colon\"> : </span>");
        out.push_str(&body.html);
        out.push_str("</div>");
        if let Some(doc) = nonempty(ctor.doc.as_deref()) {
            out.push_str("<div class=\"field-doc\">");
            out.push_str(&self.docs.docstring(doc));
            out.push_str("</div>");
        }
        out.push_str("</li>");
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
            // The `id` only exists when this structure really does declare the
            // projection: an anchor for a field it merely inherits would take
            // over a fragment that belongs to the parent's page.
            if contained.contains(proj_name.as_str()) {
                out.push_str("<li id=\"");
                escape_html_into(out, &proj_name);
                out.push_str("\" class=\"field inherited\">");
            } else {
                out.push_str("<li class=\"field inherited\">");
            }
            // No href: the field keeps its name and its `field-name` class and
            // loses only the anchor — the same element the branch below writes
            // for a field this structure declares itself. An `<a>` with no
            // target, or a name dropped for want of one, would both be worse
            // than the dead link this replaced.
            match &link {
                Some(link) => {
                    out.push_str("<div class=\"field-sig\"><a class=\"field-name\" href=\"");
                    escape_html_into(out, link);
                    out.push_str("\">");
                    escape_html_into(out, short);
                    out.push_str("</a>");
                }
                None => {
                    out.push_str("<div class=\"field-sig\"><span class=\"field-name\">");
                    escape_html_into(out, short);
                    out.push_str("</span>");
                }
            }
            out.push_str(&args);
            out.push_str("<span class=\"colon\"> : </span>");
            out.push_str(&body.html);
            out.push_str("</div></li>");
            return Ok(());
        }

        out.push_str("<li id=\"");
        escape_html_into(out, &field.name);
        out.push_str("\" class=\"field\"><div class=\"field-sig\"><span class=\"field-name\">");
        escape_html_into(out, short);
        out.push_str("</span>");
        out.push_str(&args);
        out.push_str("<span class=\"colon\"> : </span>");
        out.push_str(&body.html);
        out.push_str("</div>");
        if let Some(doc) = nonempty(field.doc.as_deref()) {
            out.push_str("<div class=\"field-doc\">");
            out.push_str(&self.docs.docstring(doc));
            out.push_str("</div>");
        }
        out.push_str("</li>");
        Ok(())
    }

    /// The whole `<section class="decl">`: head, attributes, signature,
    /// docstring, fields, and whatever the kind adds after them.
    pub fn decl_html(&self, decl: &Decl) -> Result<String, UnplaceableName> {
        let refs = decl_refs(decl);
        let head = decl_head_html(decl, &self.module.module, self.source_url);
        let signature = decl_signature(decl, &self.module.module, &self.code);

        let mut attrs = String::new();
        if !decl.attrs.is_empty() {
            attrs.push_str("<div class=\"attrs\">");
            escape_html_into(&mut attrs, &format!("@[{}]", decl.attrs.join(", ")));
            attrs.push_str("</div>");
        }

        // Wrapped, unlike doc-gen4, which let the docstring's own `<p>` land
        // directly in the declaration block. Everything the stylesheet says
        // about prose — measure, spacing, code, tables — hangs off `.doc`, and
        // an unwrapped docstring loses all of it without failing.
        let doc = match nonempty(decl.doc.as_deref()) {
            Some(doc) => {
                let mut wrapped = String::with_capacity(doc.len() + 64);
                wrapped.push_str("<div class=\"doc\">");
                wrapped.push_str(&self.docs.docstring(doc));
                wrapped.push_str("</div>");
                wrapped
            }
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
            "inductive" => {
                body = self.constructors_html(decl);
                extra = instances_for_html(&decl.name);
            }
            "class_inductive" => {
                body = self.constructors_html(decl);
                extra = class_instances_html(&decl.name);
            }
            // theorem / axiom / opaque / constructor
            _ => {}
        }

        let mut out = String::with_capacity(
            head.len() + attrs.len() + signature.len() + doc.len() + body.len() + extra.len() + 64,
        );
        out.push_str("<section class=\"decl\" id=\"");
        escape_html_into(&mut out, &decl.name);
        // The kind is an attribute rather than a class because it selects a
        // colour and nothing else; `.decl[data-kind="theorem"]` reads as the
        // one-way mapping it is, and it cannot collide with a layout class.
        out.push_str("\" data-kind=\"");
        escape_html_into(&mut out, css_kind(&decl.kind));
        out.push_str("\">");
        out.push_str(&head);
        out.push_str(&attrs);
        out.push_str(&signature);
        out.push_str(&doc);
        out.push_str(&body);
        out.push_str(&extra);
        out.push_str("</section>");
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
    use crate::external::ExternalLinks;
    use crate::link_index::LinkIndex;
    use lean_doc_ir::SpanKind;

    fn index(entries: &[(&str, &str)]) -> NameIndex {
        let mut builder = NameIndex::builder();
        for (name, module) in entries {
            builder.declaration(name, module);
        }
        builder.build(LinkIndex::default(), ExternalLinks::default())
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

    /// **The one mutant that survived.** `cargo mutants` over this file reports
    /// 74 mutants, 73 of them caught; the survivor replaced this type's
    /// `Display` with the empty string 【実測 2026-08-16】.
    ///
    /// That is a real hole rather than noise. This error exists *only* to be
    /// read: doc-gen4 panics at the same point (`name2ModIdx[name]!`) and this
    /// crate returns instead, on the grounds — stated at the top of this file —
    /// that a wrong `href` is a wrong byte either way and a silent one costs a
    /// debugging round to locate. An error that does not name the declaration it
    /// could not place spends that round anyway.
    #[test]
    fn an_unplaceable_name_says_which_name() {
        let text = UnplaceableName {
            name: "Pkg.M.f".to_owned(),
        }
        .to_string();
        assert!(text.contains("Pkg.M.f"), "the name is missing: {text:?}");
        assert!(
            text.contains("declNameToLink"),
            "the operation is missing: {text:?}"
        );
    }

    /// The head is what a reader scans for; the signature is what they read.
    /// They are two functions since M8-b because they wrap differently, so both
    /// halves are pinned here — the kind, the self link, the source link, and
    /// then the binders with the implicit ones marked and the type in its own
    /// block.
    #[test]
    fn a_head_is_kind_name_and_source_and_a_signature_is_binders_and_type() {
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
            decl_head_html(&d, "Pkg.M", "https://x/Pkg/M.lean"),
            "<header class=\"decl-head\"><span class=\"kind\">abbrev</span>\
             <h2 class=\"decl-name\"><a class=\"break_within\" href=\".././Pkg/M.html#Pkg.M.f\">\
             <span class=\"name\">Pkg</span>.<span class=\"name\">M</span>.\
             <span class=\"name\">f</span></a></h2>\
             <a class=\"src\" href=\"https://x/Pkg/M.lean#L1-L1\">source</a></header>"
        );
        assert_eq!(
            decl_signature(&d, "Pkg.M", &CodeRenderer::new(&names)),
            "<div class=\"sig\">\
             <span class=\"binder\"><span class=\"fn\">\
             (n : <a href=\".././Init/Prelude.html#Nat\">Nat</a>)</span></span>\n\
             <span class=\"binder implicit\"><span class=\"fn\">{m : Nat}</span></span>\n\
             <span class=\"colon\"> :</span>\
             <div class=\"sig-type\"><a href=\".././Init/Prelude.html#Nat\">Nat</a></div></div>"
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
            let html = decl_signature(&d, "Pkg", &code);
            assert!(
                html.contains(
                    "<span class=\"extends\">extends</span> \
                     <span id=\"P.to&lt;A\">A</span>, <span id=\"P.toB\">B</span>\
                     <span class=\"colon\"> :</span>"
                ),
                "{kind}: {html}"
            );
        }
        // Same members under a kind that has no parents section.
        let mut d = decl("P", "class_inductive");
        d.members = parents;
        assert!(!decl_signature(&d, "Pkg", &code).contains("class=\"extends\""));
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
            "<details class=\"extra\"><summary>Equations</summary><ul class=\"equations\">\
             <li>One or more equations did not get rendered \
             due to their size.</li></ul></details>",
            "the notice appears and the equation does not"
        );

        // No equations at all: nothing, not an empty `<details>`.
        d.equations = vec![];
        d.equation_code = vec![];
        assert_eq!(equations_html(&d, "./", &Refs::default(), &code), "");
    }

    /// The two stubs are the same shape and differ only in the map `app.js`
    /// fills them from — which is the M8-c contract. Both carry the name in
    /// `data-name`, escaped: doc-gen4 spelled it into an element `id` and so had
    /// to survive a round trip through an HTML identifier.
    #[test]
    fn the_two_instance_stubs_differ_only_in_the_map_that_fills_them() {
        assert_eq!(
            instances_for_html("A<B"),
            "<details class=\"extra\" data-fill=\"instances-for\" data-name=\"A&lt;B\">\
             <summary>Instances For</summary><ul></ul></details>"
        );
        assert_eq!(
            class_instances_html("A<B"),
            "<details class=\"extra\" data-fill=\"instances\" data-name=\"A&lt;B\">\
             <summary>Instances</summary><ul></ul></details>"
        );
    }

    #[test]
    fn an_unplaceable_name_is_an_error_and_not_a_guess() {
        let names = index(&[("known", "Pkg.M")]);
        assert_eq!(
            decl_name_to_link("known", "./", &Refs::default(), &names),
            Ok(Some("./Pkg/M.html#known".to_owned()))
        );
        assert_eq!(
            decl_name_to_link("nowhere", "./", &Refs::default(), &names),
            Err(UnplaceableName {
                name: "nowhere".to_owned()
            })
        );
    }

    /// **M7-c**: the same lookup, with the field's module in the dependency map.
    /// The refusal above is unchanged — a name in no module is still an error,
    /// not a blob URL to nowhere.
    ///
    /// **2026-08-17** added the third answer: `Ok(None)` for a field whose
    /// module belongs to a dependency with no version-pinned URL. The two either
    /// side of it are asserted here as bytes, because they must not move.
    #[test]
    fn an_inherited_field_of_a_dependencys_structure_links_at_its_source() {
        let mut builder = NameIndex::builder();
        builder.declaration("Mathlib.P.y", "Mathlib.Order.Basic");
        builder.declaration("Pkg.M.z", "Pkg.M");
        builder.declaration("Dep.P.w", "Dep.Aux");
        let names = builder.build(
            LinkIndex::parse("Mathlib.Order.Basic\n\tMathlib.P.y\t67\t67\n"),
            ExternalLinks::new([("Mathlib", "https://host/o/mathlib4/blob/abc"), ("Dep", "")]),
        );
        assert_eq!(
            decl_name_to_link("Mathlib.P.y", ".././", &Refs::default(), &names),
            Ok(Some(
                "https://host/o/mathlib4/blob/abc/Mathlib/Order/Basic.lean#L67-L67".to_owned()
            ))
        );
        assert_eq!(
            decl_name_to_link("Pkg.M.z", ".././", &Refs::default(), &names),
            Ok(Some(".././Pkg/M.html#Pkg.M.z".to_owned())),
            "the package being documented is not in the map"
        );
        assert_eq!(
            decl_name_to_link("Dep.P.w", ".././", &Refs::default(), &names),
            Ok(None),
            "the module is known and unlinkable, which is neither an error nor a page link"
        );
        assert!(decl_name_to_link("nowhere", "./", &Refs::default(), &names).is_err());
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
            renderer.decl_html(decl)
        }
    }

    /// The whole `section.decl`, with the head, the attributes and the
    /// docstring in their places.
    ///
    /// The order they are assembled in is the assertion: the kind and the name
    /// lead, the attributes sit between the head and the signature, and the
    /// docstring follows the signature rather than the head — a docstring that
    /// drifted above the type would read as belonging to the declaration before
    /// it.
    #[test]
    fn a_declaration_is_head_attributes_signature_doc_and_extra() {
        let mut d = decl("Pkg.M.f", "definition");
        d.line = 7;
        d.end_line = 9;
        d.attrs = vec!["simp".to_owned(), "reducible".to_owned()];
        d.doc = Some("hello".to_owned());
        let page = Page::new(index(&[]), module_with(vec![d]));
        let html = page.render(0).expect("nothing to place");
        assert!(
            html.starts_with(
                "<section class=\"decl\" id=\"Pkg.M.f\" data-kind=\"def\">\
                 <header class=\"decl-head\"><span class=\"kind\">def</span>"
            ),
            "{html}"
        );
        assert!(
            html.contains(
                "<a class=\"src\" href=\"https://x/M.lean#L7-L9\">source</a></header>\
                 <div class=\"attrs\">@[simp, reducible]</div><div class=\"sig\">"
            ),
            "{html}"
        );
        assert!(
            html.contains(
                "</div></div><div class=\"doc\"><p>hello</p></div>\
                 <details class=\"extra\" data-fill=\"instances-for\" data-name=\"Pkg.M.f\">"
            ),
            "{html}"
        );
        assert!(html.ends_with("</ul></details></section>"), "{html}");
    }

    /// An empty docstring is falsy in JavaScript, so it renders nothing — not
    /// the two newlines `docStringToHtml` would append to it.
    #[test]
    fn an_empty_docstring_renders_nothing() {
        let mut d = decl("f", "theorem");
        d.doc = Some(String::new());
        let page = Page::new(index(&[]), module_with(vec![d]));
        let html = page.render(0).expect("nothing to place");
        // The signature's two closing `</div>`s, and then straight to the end
        // of the block: nothing between them.
        assert!(html.ends_with("</div></div></section>"), "{html}");
        assert!(!html.contains("<p>"), "{html}");
    }

    #[test]
    fn each_kind_gets_its_own_extra() {
        for (kind, css, extra) in [
            ("definition", "def", "data-fill=\"instances-for\""),
            ("inductive", "inductive", "data-fill=\"instances-for\""),
            ("class_inductive", "class", "data-fill=\"instances\""),
            ("instance", "instance", ""),
            ("theorem", "theorem", ""),
            ("constructor", "ctor", ""),
        ] {
            let page = Page::new(index(&[]), module_with(vec![decl("X", kind)]));
            let html = page.render(0).expect("nothing to place");
            assert!(
                html.starts_with(&format!(
                    "<section class=\"decl\" id=\"X\" data-kind=\"{css}\">"
                )),
                "{kind}: {html}"
            );
            if extra.is_empty() {
                assert!(!html.contains("<details"), "{kind}: {html}");
            } else {
                assert!(html.contains(extra), "{kind}: {html}");
                assert!(html.contains("data-name=\"X\""), "{kind}: {html}");
            }
        }
    }

    /// Whether the constructor is named, and the fact that a missing `ctor`
    /// takes the anonymous shape.
    ///
    /// doc-gen4 spelled the named case as nested lists reading `Name :: ( … )`;
    /// M8-b replaced that with a note above the same field list, so what is
    /// asserted is the *distinction* — a `mk` constructor says nothing, any
    /// other name is printed — rather than doc-gen4's two `<ul>` shapes. The
    /// constructor's name still lands on the field list as its `id` either way,
    /// because that is what an inherited field's anchor is resolved against.
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
                "<ul class=\"fields\" id=\"S.mk\">\
                 <li id=\"S.x\" class=\"field\"><div class=\"field-sig\">\
                 <span class=\"field-name\">x</span>\
                 <span class=\"colon\"> : </span>Nat</div></li></ul>"
            ),
            "{html}"
        );
        assert!(
            !html.contains("ctor-note"),
            "`mk` is the anonymous one: {html}"
        );

        // An explicit `mk` constructor is the same shape.
        d.members.push(member(
            r#"{"label": "ctor", "name": "S.mk", "text": "", "code": []}"#,
        ));
        let page = Page::new(index(&[]), module_with(vec![d.clone()]));
        let html = page.render(0).expect("nothing to place");
        assert!(html.contains("<ul class=\"fields\" id=\"S.mk\">"), "{html}");
        assert!(!html.contains("ctor-note"), "{html}");

        // Any other constructor name is printed, and still owns the list's id.
        d.members.pop();
        d.members.push(member(
            r#"{"label": "ctor", "name": "S.make", "text": "", "code": []}"#,
        ));
        let page = Page::new(index(&[]), module_with(vec![d]));
        let html = page.render(0).expect("nothing to place");
        assert!(
            html.contains(
                "<p class=\"ctor-note\">constructor <code>make</code></p>\
                 <ul class=\"fields\" id=\"S.make\">"
            ),
            "{html}"
        );
    }

    /// **The regression the corpus could not have caught.**
    ///
    /// The measurement package holds no `inductive` and no `class_inductive`
    /// declaration at all — `tests/page_parts.rs` counts that among nine
    /// branches real data never reaches — so byte reproduction against doc-gen4
    /// never rendered a constructor, and the curated cases reached the branch
    /// without ever looking at what came out of it. What came out was nothing:
    /// the body was empty, so the constructors were absent from their own page
    /// while the search index went on pointing at `#C.red`.
    ///
    /// Found by `e2e/micro`, the fixture whose purpose is to hold the shapes the
    /// target does not. The lesson is the one plan §7 already states — full
    /// byte equality is not branch coverage — sharpened: it is not even
    /// *reachability*, because a branch the oracle's own input cannot contain is
    /// invisible however many bytes match.
    #[test]
    fn an_inductives_constructors_are_rendered_with_their_own_anchors() {
        let red = r#"{"label": "ctor", "name": "C.red", "text": "C", "code": [],
                      "doc": "The first one."}"#;
        let green = r#"{"label": "ctor", "name": "C.green", "text": "C", "code": []}"#;
        let mut d = decl("C", "inductive");
        d.members = vec![member(red), member(green)];
        let page = Page::new(index(&[]), module_with(vec![d.clone()]));
        let html = page.render(0).expect("nothing to place");

        assert!(html.contains("<ul class=\"ctors\">"), "{html}");
        // An anchor each, because that is what the search index links to.
        assert!(
            html.contains(
                "<li id=\"C.red\" class=\"ctor\"><div class=\"field-sig\">\
                 <span class=\"field-name\">red</span>\
                 <span class=\"colon\"> : </span>C</div>"
            ),
            "{html}"
        );
        assert!(
            html.contains("<li id=\"C.green\" class=\"ctor\">"),
            "{html}"
        );
        // A constructor's docstring is rendered where a field's would be.
        assert!(html.contains("<div class=\"field-doc\">"), "{html}");

        // A class inductive renders the same constructors ...
        d.kind = "class_inductive".to_owned();
        let page = Page::new(index(&[]), module_with(vec![d]));
        let html = page.render(0).expect("nothing to place");
        assert!(html.contains("<li id=\"C.red\" class=\"ctor\">"), "{html}");
        // ... and keeps the block that belongs to a class: its instances, not
        // the instances *for* it.
        assert!(html.contains("data-fill=\"instances\""), "{html}");
        assert!(!html.contains("data-fill=\"instances-for\""), "{html}");
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
                "<li id=\"S.x\" class=\"field\"><div class=\"field-sig\">\
                 <span class=\"field-name\">x</span>\
                 <span class=\"colon\"> : </span>Nat</div>\
                 <div class=\"field-doc\"><p>a field</p></div></li>"
            ),
            "{html}"
        );
        assert!(
            html.contains(
                "<li class=\"field inherited\"><div class=\"field-sig\">\
                 <a class=\"field-name\" href=\".././Pkg/Parent.html#P.y\">y</a>\
                 <span class=\"colon\"> : </span>Nat</div></li>"
            ),
            "the inherited field carries no id and no docstring: {html}"
        );
        // The key is missing on `S.z`, so it is direct — the whole point of
        // `Option<bool>`.
        assert!(
            html.contains("<li id=\"S.z\" class=\"field\">"),
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
            html.contains("<li id=\"S.y\" class=\"field inherited\"><div class=\"field-sig\">"),
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
