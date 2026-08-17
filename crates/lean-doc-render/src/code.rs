//! Derived from doc-gen4 (Copyright (c) 2021 Henrik Böving, Apache-2.0) and
//! changed; see this repository's NOTICE and `docs/provenance.md`.
//!
//! Turning one printed code fragment into HTML.
//!
//! A *fragment* is one of the five text/span pairs the IR carries — a
//! declaration's result type, one of its binders, one of its equations, a
//! structure member's text, or one of a member's binders. Each is plain printed
//! Lean plus a flat pre-order list of tag positions over it, and this module is
//! doc-gen4's `renderedCodeToHtmlAux` (`Base.lean:337-381`) over that pair.
//!
//! Ported from `experiments/stage7d/render.ts` (frozen): `buildTree` 419-443,
//! `Renderer` 597-702, `privateToUserName` 705-708, `moduleFromPrivatePrefix`
//! 711-714, `findLinkableParent` 402-413, `kindDescription` 349-380, `cssKind`
//! 832-843, `breakWithin` 719-724.
//!
//! # The three things that are easy to get subtly wrong
//!
//! 1. **Offsets are UTF-16 code units.** Everything here indexes
//!    [`Utf16Text`], never bytes; that is why the fragment text keeps its type
//!    all the way through the whitespace rewrite (plan §7, U2).
//! 2. **An anchor inside an anchor is suppressed, and the suppression is what
//!    the return value carries.** `hasAnchor` propagates *up* out of a subtree
//!    ([`Rendered::has_anchor`]), and both the sort branch and the constant
//!    branch drop their own `<a>` when the subtree already has one. Dropping
//!    the flag produces nested anchors, which is valid-looking HTML and wrong
//!    bytes.
//! 3. **The signature path resolves against the IR's own map only.** The link
//!    lookup goes to [`NameIndex::known`] and deliberately *not* to
//!    [`NameIndex::module_of`]: the dependency closure's `.lidx` is fifty times
//!    larger and belongs to the docstring path (`render.ts:2059-2064`). Letting
//!    it under this path would move links that are byte-exact today.
//!
//! # The counters are not here
//!
//! The prototype threads a `FragCounters` sink through this walk (`stats` /
//! `pageStats`, `render.ts:445-533`) and bumps it in eight places. **None of
//! them reaches a byte**: they exist for the prototype's `--report` JSON, and
//! the acceptance oracle compares pages. They are dropped rather than
//! transcribed, because a counter that no test reads is a second definition of
//! the branch structure that can drift out of step with the first. The one
//! counter that had a caller kept its caller — [`WsRewrite::changed_units`] —
//! and the branch coverage this walk needs is asserted on the *output* instead
//! (`tests/fragment.rs`), which is the thing the gate is about.
//!
//! [`WsRewrite::changed_units`]: crate::whitespace::WsRewrite::changed_units

use std::collections::HashMap;

use lean_doc_ir::{Decl, Span, SpanKind, Utf16Text};
use lean_doc_md::escape_html_into;

use crate::autolink::{NameIndex, PRIVATE_PREFIX};
use crate::whitespace::apply_ws_widths;

/// A declaration's resolved references, inverted to name -> defining module.
///
/// The extractor resolved every constant it tagged with `env.getModuleIdxFor?`,
/// which *is* the `const2ModIdx` doc-gen4 indexes, so this is the IR's stand-in
/// for the map doc-gen4 has because it holds the environment. It is consulted
/// before the global map and is the reason a constant links to the module that
/// defined it rather than to whichever module happened to be read last.
pub type Refs<'a> = HashMap<&'a str, &'a str>;

/// `render.ts:728-729` — `for (const [m, n] of d.refs) refs.set(n, m)`.
///
/// Note the inversion: on the wire a reference is `[module, name]`, and the map
/// is keyed by the name. A later entry wins, as `Map.set` does.
#[must_use]
pub fn decl_refs(decl: &Decl) -> Refs<'_> {
    let mut refs = HashMap::with_capacity(decl.refs.len());
    for r in &decl.refs {
        refs.insert(r.name.as_str(), r.module.as_str());
    }
    refs
}

/// One rendered fragment, or one rendered sub-range of one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rendered {
    pub html: String,
    /// Whether this HTML already contains an `<a>` produced by this walk.
    ///
    /// Not a statistic: an enclosing tag consults it and renders *itself*
    /// without an anchor when it is set (`Base.lean:374-381`).
    pub has_anchor: bool,
}

/// The code-fragment renderer — `render.ts`'s `Renderer` class.
///
/// Named `CodeRenderer` because [`lean_doc_md::Renderer`] is the *docstring*
/// renderer and the page builder holds both. The two never share a code path:
/// this one walks tag spans over printed Lean, that one walks a markdown AST.
#[derive(Clone, Copy, Debug)]
pub struct CodeRenderer<'a> {
    names: &'a NameIndex,
}

impl<'a> CodeRenderer<'a> {
    /// Borrows the run's name index. One renderer serves every page.
    #[must_use]
    pub const fn new(names: &'a NameIndex) -> Self {
        Self { names }
    }

    /// The name index this renderer resolves against.
    #[must_use]
    pub const fn names(&self) -> &'a NameIndex {
        self.names
    }

    /// `renderedCodeToHtmlAux` over one fragment (`render.ts:606-610`).
    ///
    /// `root` is [`crate::page_root`] of the page being written — it prefixes
    /// every link, so it is part of the bytes — and `refs` is [`decl_refs`] of
    /// the declaration the fragment belongs to. Members and equations use the
    /// *declaration's* references, not their own; they have none.
    #[must_use]
    pub fn fragment(
        &self,
        text: &Utf16Text,
        spans: &[Span],
        root: &str,
        refs: &Refs<'_>,
    ) -> Rendered {
        let tree = SpanTree::build(spans);
        // Length-preserving, so the spans still address the result. This is why
        // the rewrite hands back a `Utf16Text` rather than a `String`.
        let ws = apply_ws_widths(text, spans);
        let text = ws.as_text();
        let mut html = String::with_capacity(text.as_str().len());
        let has_anchor = self.range(
            &mut html,
            text,
            0,
            text.len_utf16(),
            &tree.roots,
            &tree,
            root,
            refs,
        );
        Rendered { html, has_anchor }
    }

    /// `Renderer.range` — the untagged text between children, escaped, with each
    /// child rendered in place.
    #[allow(clippy::too_many_arguments)]
    fn range(
        &self,
        out: &mut String,
        text: &Utf16Text,
        lo: u32,
        hi: u32,
        children: &[usize],
        tree: &SpanTree<'_>,
        root: &str,
        refs: &Refs<'_>,
    ) -> bool {
        let mut has_anchor = false;
        let mut pos = lo;
        for &child in children {
            let node = &tree.nodes[child];
            if node.start > pos {
                escape_html_into(out, text.slice(pos..node.start));
            }
            has_anchor |= self.node(out, text, node, tree, root, refs);
            pos = node.stop;
        }
        if hi > pos {
            escape_html_into(out, text.slice(pos..hi));
        }
        has_anchor
    }

    /// `Renderer.node` — one tag, after its subtree.
    ///
    /// The subtree is written into `out` first and the wrapper is inserted in
    /// front of it afterwards, because which wrapper it is depends on whether
    /// the subtree produced an anchor.
    fn node(
        &self,
        out: &mut String,
        text: &Utf16Text,
        node: &TreeNode<'_>,
        tree: &SpanTree<'_>,
        root: &str,
        refs: &Refs<'_>,
    ) -> bool {
        let at = out.len();
        let inner = self.range(
            out,
            text,
            node.start,
            node.stop,
            &node.children,
            tree,
            root,
            refs,
        );
        match node.kind {
            SpanKind::Fn => {
                wrap(out, at, "<span class=\"fn\">", "</span>");
                inner
            }
            SpanKind::Sort => {
                // `Base.lean:374-381`: no `fn` wrapper, and no anchor of its own
                // when the subtree already has one.
                if inner {
                    return true;
                }
                let mut href = String::from(root);
                href.push_str("foundational_types.html");
                anchor(out, at, &href);
                true
            }
            // `.const name` (`Base.lean:337-373`), and anything the extractor
            // starts emitting that this crate does not know about. The prototype
            // reaches this branch the same way — its `node` tests kind 0 and
            // kind 2 and falls through — so an unknown kind renders as an
            // unnamed constant rather than disappearing.
            SpanKind::Const | SpanKind::Other(_) => {
                let link = self.const_link(node.name, root, refs);
                let Some(link) = link else {
                    wrap(out, at, "<span class=\"fn\">", "</span>");
                    return inner;
                };
                if inner {
                    return true;
                }
                anchor(out, at, &link);
                true
            }
        }
    }

    /// `renderedCodeToHtmlAux`'s `.const` resolution (`render.ts:674-701`), in
    /// order:
    ///
    /// 1. a direct hit, unless the name is private — the declaration's own
    ///    references first, the IR's global map second;
    /// 2. [`find_linkable_parent`] after auxiliary-name removal;
    /// 3. for a private name, the module its prefix names;
    /// 4. otherwise no link, and the caller renders a `span.fn`.
    ///
    /// All three linking branches go through [`NameIndex::link_to`] (M7-c), so
    /// a constant defined in a dependency links at that dependency's pinned
    /// source rather than at a page this site never wrote. **Which name the
    /// anchor is taken from is not always `name`**: branch 2 links the *parent*,
    /// and it is the parent's source range that belongs on that URL.
    ///
    /// A branch that resolves the name to an **unpinnable** dependency returns
    /// that branch's `None` rather than trying the next one (2026-08-17): the
    /// caller's answer to `None` is branch 4's, a `span.fn` with the name in it,
    /// which is exactly right here — the constant is named, and nothing claims
    /// to know where to read it.
    #[must_use]
    pub fn const_link(&self, name: &str, root: &str, refs: &Refs<'_>) -> Option<String> {
        let is_private = name.starts_with(PRIVATE_PREFIX);
        if !is_private
            && let Some(module) = refs.get(name).copied().or_else(|| self.names.known(name))
        {
            return self.names.link_to(root, module, Some(name));
        }
        // Step 1: auxiliary name removal.
        let search = if is_private {
            private_to_user_name(name)
        } else {
            name
        };
        if let Some(parent) = find_linkable_parent(self.names, search) {
            let module = self
                .names
                .known(parent)
                .expect("find_linkable_parent only returns names the index knows");
            return self.names.link_to(root, module, Some(parent));
        }
        // Step 2: the module link a private name still has.
        if is_private && let Some(module) = module_from_private_prefix(name) {
            return self.names.link_to(root, module, None);
        }
        None
    }
}

/// Puts `open`/`close` around what was written into `out` from `at` onwards.
fn wrap(out: &mut String, at: usize, open: &str, close: &str) {
    out.insert_str(at, open);
    out.push_str(close);
}

/// [`wrap`] with an `<a href>`, the target escaped as `Html.escape` escapes it.
fn anchor(out: &mut String, at: usize, href: &str) {
    let mut open = String::with_capacity(href.len() + 12);
    open.push_str("<a href=\"");
    escape_html_into(&mut open, href);
    open.push_str("\">");
    wrap(out, at, &open, "</a>");
}

// ------------------------------------------------------------------ span tree

/// The flat pre-order span list rebuilt as a tree.
///
/// Nodes live in one arena and children are indices into it: the shape is a
/// tree, but the lifetime is the fragment's, and an arena says that without a
/// borrow checker argument per node.
#[derive(Debug)]
struct SpanTree<'s> {
    nodes: Vec<TreeNode<'s>>,
    roots: Vec<usize>,
}

#[derive(Debug)]
struct TreeNode<'s> {
    start: u32,
    stop: u32,
    kind: SpanKind,
    /// The constant's name, or `""` for a span form that carries none — which
    /// is what the prototype's `Node` stores (`render.ts:434`).
    name: &'s str,
    children: Vec<usize>,
}

impl<'s> SpanTree<'s> {
    /// `buildTree` (`render.ts:426-443`): pop while the new span starts at or
    /// after the top of the stack ends, then attach to whatever is left.
    ///
    /// **`>=`, not `>`.** Two spans that merely touch — one ending exactly where
    /// the next begins — are siblings. With `>` the second becomes a child of
    /// the first, and the walk then slices text outside its parent's range.
    fn build(spans: &'s [Span]) -> Self {
        let mut nodes: Vec<TreeNode<'s>> = Vec::with_capacity(spans.len());
        let mut roots = Vec::new();
        let mut stack: Vec<usize> = Vec::new();
        for span in spans {
            let me = nodes.len();
            nodes.push(TreeNode {
                start: span.start,
                stop: span.stop,
                kind: span.kind,
                name: span.name.as_deref().unwrap_or(""),
                children: Vec::new(),
            });
            while let Some(&top) = stack.last() {
                if nodes[me].start >= nodes[top].stop {
                    stack.pop();
                } else {
                    break;
                }
            }
            match stack.last() {
                Some(&parent) => nodes[parent].children.push(me),
                None => roots.push(me),
            }
            stack.push(me);
        }
        Self { nodes, roots }
    }
}

// -------------------------------------------------------------- name handling

/// `findLinkableParent` (`Base.lean`, `render.ts:402-413`): strip trailing
/// components that are numeric or start with `_`, and return the first prefix
/// the index knows.
///
/// The IR has no `Name` structure, only the printed string, so a `.num`
/// component is recognised by being all ASCII digits — which is how
/// `Name.toString` prints one. The test is on the *last component*, but the
/// lookup is on the *whole prefix*, and the two are easy to swap.
#[must_use]
pub fn find_linkable_parent<'n>(names: &NameIndex, name: &'n str) -> Option<&'n str> {
    let mut cur = name;
    loop {
        let dot = cur.rfind('.')?;
        let last = &cur[dot + 1..];
        let is_num = !last.is_empty() && last.bytes().all(|b| b.is_ascii_digit());
        if !is_num && !last.starts_with('_') && names.known(cur).is_some() {
            return Some(cur);
        }
        cur = &cur[..dot];
        if cur.is_empty() {
            return None;
        }
    }
}

/// True for the four scalars JavaScript's `.` does not match: it is what makes
/// the two regexes below stop at a line break.
///
/// Lean names are not expected to contain any of these — a `«…»` component
/// could in principle — but the prototype's behaviour on one is *decided* by
/// the regex flags rather than by anything anyone chose, and reproducing it
/// costs four characters.
const fn is_js_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// Splits `_private.<Module>.<n>.<rest>` into `(<Module>, <rest>)`.
///
/// This is the common part of the two regexes at `render.ts:705-714`. The
/// module part is **lazy**: the first `.<digits>.` after `_private.` ends it, so
/// `_private.A.B.0.f` gives `A.B` and not `A`. `\d` is ASCII digits only, and
/// the greedy digit run needs no backtracking (a shorter run is followed by a
/// digit, never by the `.` the pattern wants next).
fn split_private(name: &str) -> Option<(&str, &str)> {
    let rest = name.strip_prefix(PRIVATE_PREFIX)?;
    for (at, c) in rest.char_indices() {
        if c == '.' {
            let after = &rest[at + 1..];
            let digits = after.len() - after.trim_start_matches(|c: char| c.is_ascii_digit()).len();
            if digits > 0
                && let Some(tail) = after[digits..].strip_prefix('.')
            {
                return Some((&rest[..at], tail));
            }
        } else if is_js_line_terminator(c) {
            // `(.*?)` cannot cross one, so no match starts before it either.
            return None;
        }
    }
    None
}

/// `Lean.privateToUserName?`: `_private.<Module>.<n>.<rest>` -> `<rest>`, and
/// the name itself when it is not of that shape (`render.ts:705-708`).
///
/// The prototype's regex ends in `$`, which in JavaScript is end of input, so a
/// `<rest>` containing a line break makes the whole match fail — unlike
/// [`module_from_private_prefix`], whose regex has no anchor at the end.
#[must_use]
pub fn private_to_user_name(name: &str) -> &str {
    match split_private(name) {
        Some((_, rest)) if !rest.chars().any(is_js_line_terminator) => rest,
        _ => name,
    }
}

/// `moduleFromPrivatePrefix`: `_private.Init.Prelude.0.Foo` -> `Init.Prelude`
/// (`render.ts:711-714`).
#[must_use]
pub fn module_from_private_prefix(name: &str) -> Option<&str> {
    split_private(name).map(|(module, _)| module)
}

// ------------------------------------------------------------------- the kind

/// `getKindDescription` (`Process/DocInfo.lean:211-247`) recomposed from the
/// IR's `kind` plus `modifiers` — the text of `span.decl_kind`
/// (`render.ts:349-380`).
///
/// Not the same mapping as [`css_kind`], and the two are next to each other in
/// the page: this one is the words a reader sees, that one is a CSS class.
#[must_use]
pub fn kind_description(kind: &str, modifiers: &[String]) -> String {
    let has = |m: &str| modifiers.iter().any(|x| x == m);
    match kind {
        "definition" | "instance" => {
            let mut parts: Vec<&str> = Vec::with_capacity(3);
            if has("unsafe") {
                parts.push("unsafe");
            }
            if has("noncomputable") {
                parts.push("noncomputable");
            }
            parts.push(if kind == "instance" {
                "instance"
            } else if has("abbrev") {
                "abbrev"
            } else {
                "def"
            });
            parts.join(" ")
        }
        "axiom" if has("unsafe") => "unsafe axiom".to_owned(),
        "opaque" if has("partial") => "partial def".to_owned(),
        "opaque" if has("unsafe") => "unsafe opaque".to_owned(),
        "inductive" if has("unsafe") => "unsafe inductive".to_owned(),
        "class_inductive" => "class inductive".to_owned(),
        // structure / class / theorem / constructor, and the unmodified cases
        // of the four above.
        other => other.to_owned(),
    }
}

/// `DocInfo.getKind` (`Process/DocInfo.lean:48-58`) — the CSS class of the
/// declaration's `div`, which is **not** the text of `span.decl_kind`
/// (`render.ts:832-843`).
#[must_use]
pub fn css_kind(kind: &str) -> &str {
    match kind {
        "definition" => "def",
        "class_inductive" => "class",
        "constructor" => "ctor",
        // axiom / theorem / opaque / instance / inductive / structure / class
        other => other,
    }
}

/// `breakWithin` (`Base.lean`): each dot-separated component in its own
/// `span.name`, with the dots left between them (`render.ts:719-724`).
#[must_use]
pub fn break_within(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 32);
    for (i, part) in name.split('.').enumerate() {
        if i > 0 {
            out.push('.');
        }
        out.push_str("<span class=\"name\">");
        escape_html_into(&mut out, part);
        out.push_str("</span>");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::external::ExternalLinks;
    use crate::link_index::LinkIndex;

    /// The world these cases resolve against: these declarations, and **a page
    /// for every module they name** — which is what a run has for its own
    /// package's modules, and what [`NameIndex::link_to`]'s last branch checks
    /// (2026-08-17).
    fn index(entries: &[(&str, &str)]) -> NameIndex {
        let mut builder = NameIndex::builder();
        for (name, module) in entries {
            builder.declaration(name, module).module_name(module);
        }
        builder.build(LinkIndex::default(), ExternalLinks::default())
    }

    /// [`index`] plus modules that have a page and declare nothing this case
    /// names: a `refs` target, or the module a private name's prefix spells.
    fn index_with_pages(entries: &[(&str, &str)], pages: &[&str]) -> NameIndex {
        let mut builder = NameIndex::builder();
        for (name, module) in entries {
            builder.declaration(name, module).module_name(module);
        }
        for module in pages {
            builder.module_name(module);
        }
        builder.build(LinkIndex::default(), ExternalLinks::default())
    }

    /// The same index with a dependency map and a `.lidx` that carries ranges —
    /// the M7-c side of every case below that has one.
    fn index_with_dependency(entries: &[(&str, &str)], lidx: &str) -> NameIndex {
        let mut builder = NameIndex::builder();
        for (name, module) in entries {
            builder.declaration(name, module).module_name(module);
        }
        builder.build(
            LinkIndex::parse(lidx),
            ExternalLinks::new([("Init", "https://github.com/leanprover/lean4/blob/dead/src")]),
        )
    }

    /// `[start, stop, 1, name]`.
    fn konst(start: u32, stop: u32, name: &str) -> Span {
        Span {
            start,
            stop,
            kind: SpanKind::Const,
            name: Some(name.to_owned()),
            front: 0,
            back: 0,
        }
    }

    fn plain(start: u32, stop: u32, kind: SpanKind) -> Span {
        Span {
            start,
            stop,
            kind,
            name: None,
            front: 0,
            back: 0,
        }
    }

    fn render(text: &str, spans: &[Span], root: &str, names: &NameIndex) -> Rendered {
        CodeRenderer::new(names).fragment(&Utf16Text::from(text), spans, root, &Refs::default())
    }

    /// Touching spans are siblings; the tree only nests on strict containment.
    #[test]
    fn build_tree_nests_on_containment_not_on_touching() {
        let spans = [
            plain(0, 6, SpanKind::Fn),
            plain(0, 3, SpanKind::Fn),
            plain(3, 6, SpanKind::Fn),
            plain(6, 9, SpanKind::Fn),
        ];
        let tree = SpanTree::build(&spans);
        assert_eq!(tree.roots, [0, 3], "the two outermost spans are siblings");
        assert_eq!(tree.nodes[0].children, [1, 2], "and 1 and 2 are inside 0");
        assert!(tree.nodes[1].children.is_empty());
    }

    #[test]
    fn untagged_text_is_escaped_and_tags_wrap() {
        let names = index(&[]);
        let out = render("a<b & c", &[plain(0, 3, SpanKind::Fn)], "./", &names);
        assert_eq!(out.html, "<span class=\"fn\">a&lt;b</span> &amp; c");
        assert!(!out.has_anchor);
    }

    /// The apostrophe is what a general HTML escaper would rewrite and
    /// `Html.escape` does not.
    #[test]
    fn the_apostrophe_is_not_escaped() {
        let names = index(&[]);
        assert_eq!(render("f'", &[], "./", &names).html, "f'");
    }

    #[test]
    fn a_sort_links_to_the_foundational_types_page() {
        let names = index(&[]);
        let out = render("Type", &[plain(0, 4, SpanKind::Sort)], "../.././", &names);
        assert_eq!(
            out.html,
            "<a href=\"../.././foundational_types.html\">Type</a>"
        );
        assert!(out.has_anchor);
    }

    /// The suppression, in both directions: a sort around a linked constant
    /// renders no anchor of its own, and the flag still comes back set.
    #[test]
    fn an_anchor_inside_an_anchor_is_suppressed() {
        let names = index(&[("Nat", "Init.Prelude")]);
        let spans = [plain(0, 3, SpanKind::Sort), konst(0, 3, "Nat")];
        let out = render("Nat", &spans, "./", &names);
        assert_eq!(
            out.html, "<a href=\"./Init/Prelude.html#Nat\">Nat</a>",
            "the outer sort must not wrap this in a second anchor"
        );
        assert!(out.has_anchor);

        // And the other way round: an unlinkable constant around a sort.
        let spans = [konst(0, 4, "Nowhere"), plain(0, 4, SpanKind::Sort)];
        let out = render("Type", &spans, "./", &names);
        assert_eq!(
            out.html,
            "<span class=\"fn\"><a href=\"./foundational_types.html\">Type</a></span>"
        );
    }

    #[test]
    fn a_constant_with_no_module_anywhere_is_a_plain_span() {
        let names = index(&[]);
        let out = render("Nowhere", &[konst(0, 7, "Nowhere")], "./", &names);
        assert_eq!(out.html, "<span class=\"fn\">Nowhere</span>");
        assert!(!out.has_anchor);
    }

    /// The declaration's own references outrank the global map.
    #[test]
    fn references_are_consulted_before_the_global_map() {
        let names = index_with_pages(&[("Nat.succ", "Stale.Module")], &["Init.Prelude"]);
        let refs = Refs::from([("Nat.succ", "Init.Prelude")]);
        let out = CodeRenderer::new(&names).fragment(
            &Utf16Text::from("Nat.succ"),
            &[konst(0, 8, "Nat.succ")],
            "./",
            &refs,
        );
        assert_eq!(
            out.html,
            "<a href=\"./Init/Prelude.html#Nat.succ\">Nat.succ</a>"
        );
    }

    /// The link target goes through `Html.escape` — a module name with a `&` in
    /// it is not reachable in practice, but the escape is in the byte path.
    #[test]
    fn the_link_target_is_escaped() {
        let names = index(&[("f", "A&B")]);
        let out = render("f", &[konst(0, 1, "f")], "./", &names);
        assert_eq!(out.html, "<a href=\"./A&amp;B.html#f\">f</a>");
    }

    /// **M7-c**, on all three of [`CodeRenderer::const_link`]'s linking
    /// branches. The empty-map form of each is the case above or below it.
    #[test]
    fn a_constant_from_a_dependency_links_at_its_pinned_source() {
        // Branch 1: a direct hit.
        let names = index_with_dependency(
            &[
                ("Nat", "Init.Prelude"),
                ("Nat.rec", "Init.Prelude"),
                ("Pkg.f", "Pkg.A"),
            ],
            "Init.Prelude\n\tNat\t26\t27\n\tNat.rec\t44\t50\n",
        );
        let out = render("Nat", &[konst(0, 3, "Nat")], "./", &names);
        assert_eq!(
            out.html,
            "<a href=\"https://github.com/leanprover/lean4/blob/dead/src/Init/Prelude.lean\
             #L26-L27\">Nat</a>"
        );
        // The package being documented is not in the map, so its constants are
        // untouched.
        let out = render("f", &[konst(0, 1, "Pkg.f")], "./", &names);
        assert_eq!(out.html, "<a href=\"./Pkg/A.html#Pkg.f\">f</a>");

        // Branch 2: the *parent* is what is linked, so the parent's range is
        // what the anchor has to come from.
        let out = render("h", &[konst(0, 1, "Nat.rec._eq_2")], "./", &names);
        assert_eq!(
            out.html,
            "<a href=\"https://github.com/leanprover/lean4/blob/dead/src/Init/Prelude.lean\
             #L44-L50\">h</a>",
            "the anchor is Nat.rec's range, not Nat's and not none"
        );

        // Branch 3: a private name falls back to its module — a file, so no
        // anchor at all.
        let out = render(
            "h",
            &[konst(0, 1, "_private.Init.Prelude.0.Foo")],
            "./",
            &names,
        );
        assert_eq!(
            out.html,
            "<a href=\"https://github.com/leanprover/lean4/blob/dead/src/Init/Prelude.lean\">h</a>"
        );
    }

    /// **2026-08-17**, the signature half of the fix: a constant defined in a
    /// dependency with no version-pinned URL renders as branch 4's `span.fn` —
    /// the name is on the page and nothing links out of it.
    ///
    /// The three branches are asserted separately because each of them resolved
    /// the name a different way, and each used to hand back a relative link to a
    /// page this site never wrote.
    #[test]
    fn a_constant_from_a_dependency_that_cannot_be_pinned_is_not_a_link() {
        let mut builder = NameIndex::builder();
        for (name, module) in [
            ("Dep.f", "Dep.Aux"),
            ("Dep.rec", "Dep.Aux"),
            ("Pkg.f", "Pkg.A"),
        ] {
            builder.declaration(name, module).module_name(module);
        }
        let names = builder.build(
            LinkIndex::parse("Dep.Aux\n\tDep.f\t26\t27\n"),
            ExternalLinks::new([("Dep", "")]),
        );

        // Branch 1: a direct hit.
        let out = render("f", &[konst(0, 1, "Dep.f")], "./", &names);
        assert_eq!(out.html, "<span class=\"fn\">f</span>");
        assert!(!out.has_anchor, "nothing on this page anchors anywhere");
        // Branch 2: through the parent.
        let out = render("h", &[konst(0, 1, "Dep.rec._eq_2")], "./", &names);
        assert_eq!(out.html, "<span class=\"fn\">h</span>");
        // Branch 3: a private name's module.
        let out = render("h", &[konst(0, 1, "_private.Dep.Aux.0.Foo")], "./", &names);
        assert_eq!(out.html, "<span class=\"fn\">h</span>");
        // …and the package being documented is untouched, as always.
        let out = render("f", &[konst(0, 1, "Pkg.f")], "./", &names);
        assert_eq!(out.html, "<a href=\"./Pkg/A.html#Pkg.f\">f</a>");
    }

    #[test]
    fn linkable_parents_skip_numeric_and_underscored_components() {
        let names = index(&[("Foo.bar", "Pkg.A"), ("Foo", "Pkg.A")]);
        assert_eq!(
            find_linkable_parent(&names, "Foo.bar._eq_1"),
            Some("Foo.bar")
        );
        assert_eq!(find_linkable_parent(&names, "Foo.bar.42"), Some("Foo.bar"));
        // The last component is tested, the whole prefix is looked up: `Foo.bar`
        // answers for `Foo.bar.x` even though `bar` is not itself in the map.
        assert_eq!(find_linkable_parent(&names, "Foo.bar.x"), Some("Foo.bar"));
        // A prefix with no dot left is never the answer, even when the map has
        // it: the loop bails on `lastIndexOf(".") < 0` before looking.
        assert_eq!(find_linkable_parent(&names, "Foo.gone.x"), None);
        assert_eq!(find_linkable_parent(&names, "Foo"), None);
        assert_eq!(find_linkable_parent(&names, "Nowhere.x"), None);
        // `_`-prefixed and numeric components are never themselves the answer.
        let names = index(&[("Foo._aux", "Pkg.A"), ("Foo.1", "Pkg.A")]);
        assert_eq!(find_linkable_parent(&names, "Foo._aux.x"), None);
        assert_eq!(find_linkable_parent(&names, "Foo.1.x"), None);
    }

    #[test]
    fn a_constant_can_resolve_through_its_parent() {
        let names = index(&[("Nat.rec", "Init.Prelude")]);
        let out = render("h", &[konst(0, 1, "Nat.rec._eq_2")], "./", &names);
        assert_eq!(out.html, "<a href=\"./Init/Prelude.html#Nat.rec\">h</a>");
    }

    #[test]
    fn a_private_name_falls_back_to_its_module() {
        let names = index_with_pages(&[], &["Init.Prelude"]);
        let out = render(
            "h",
            &[konst(0, 1, "_private.Init.Prelude.0.Foo")],
            ".././",
            &names,
        );
        assert_eq!(out.html, "<a href=\".././Init/Prelude.html\">h</a>");
    }

    /// A private name is never looked up directly, even when the map has it —
    /// but its user name can still find a parent, which beats the module link.
    #[test]
    fn a_private_name_is_not_looked_up_directly() {
        let names = index_with_pages(&[("_private.Pkg.A.0.f", "Pkg.Wrong")], &["Pkg.A"]);
        let out = render("h", &[konst(0, 1, "_private.Pkg.A.0.f")], "./", &names);
        assert_eq!(out.html, "<a href=\"./Pkg/A.html\">h</a>");

        let names = index(&[
            ("_private.Pkg.A.0.f.g.h", "Pkg.Wrong"),
            ("f.g", "Pkg.Owner"),
        ]);
        let out = render("h", &[konst(0, 1, "_private.Pkg.A.0.f.g.h")], "./", &names);
        assert_eq!(out.html, "<a href=\"./Pkg/Owner.html#f.g\">h</a>");
    }

    #[test]
    fn private_names_split_lazily_at_the_first_numeric_component() {
        assert_eq!(module_from_private_prefix("_private.A.B.0.f"), Some("A.B"));
        assert_eq!(private_to_user_name("_private.A.B.0.f"), "f");
        // Lazy: a second `.<digits>.` does not end it earlier.
        assert_eq!(private_to_user_name("_private.A.0.g.1.h"), "g.1.h");
        // Not of the shape at all.
        assert_eq!(module_from_private_prefix("_private.A.B"), None);
        assert_eq!(private_to_user_name("_private.A.B"), "_private.A.B");
        assert_eq!(module_from_private_prefix("Pkg.A.f"), None);
        assert_eq!(private_to_user_name(""), "");
        // The `$` in `privateToUserName`'s regex is end of input in JavaScript,
        // so a line break in the tail makes that match fail — while
        // `moduleFromPrivatePrefix`, which has no `$`, still matches.
        assert_eq!(module_from_private_prefix("_private.A.0.f\ng"), Some("A"));
        assert_eq!(
            private_to_user_name("_private.A.0.f\ng"),
            "_private.A.0.f\ng"
        );
        // …and neither matches when the break is in the module part.
        assert_eq!(module_from_private_prefix("_private.A\nB.0.f"), None);
    }

    #[test]
    fn offsets_are_utf16_code_units() {
        let names = index(&[("X", "Pkg.A")]);
        // 𝓧 is two UTF-16 units, so the constant tag on `y` starts at 3.
        let out = render("𝓧 y", &[konst(3, 4, "X")], "./", &names);
        assert_eq!(out.html, "𝓧 <a href=\"./Pkg/A.html#X\">y</a>");
    }

    /// The whitespace widths are replayed *before* the walk, and the walk still
    /// uses the original offsets.
    #[test]
    fn the_whitespace_rewrite_runs_first() {
        let names = index(&[("HAdd.hAdd", "Init.Prelude")]);
        let spans = [Span {
            start: 2,
            stop: 3,
            kind: SpanKind::Const,
            name: Some("HAdd.hAdd".to_owned()),
            front: 1,
            back: 1,
        }];
        let out = render("a\n+\tb", &spans, "./", &names);
        assert_eq!(
            out.html,
            "a <a href=\"./Init/Prelude.html#HAdd.hAdd\">+</a> b"
        );
    }

    #[test]
    fn kind_descriptions_recompose_the_modifiers() {
        let m = |ms: &[&str]| ms.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(kind_description("definition", &m(&[])), "def");
        assert_eq!(kind_description("definition", &m(&["abbrev"])), "abbrev");
        assert_eq!(
            kind_description("definition", &m(&["noncomputable", "unsafe", "abbrev"])),
            "unsafe noncomputable abbrev"
        );
        assert_eq!(
            kind_description("instance", &m(&["noncomputable"])),
            "noncomputable instance"
        );
        // `abbrev` does not reach the instance branch.
        assert_eq!(kind_description("instance", &m(&["abbrev"])), "instance");
        assert_eq!(kind_description("axiom", &m(&["unsafe"])), "unsafe axiom");
        assert_eq!(kind_description("axiom", &m(&["partial"])), "axiom");
        // `partial` beats `unsafe`, and it renames the kind entirely.
        assert_eq!(
            kind_description("opaque", &m(&["partial", "unsafe"])),
            "partial def"
        );
        assert_eq!(kind_description("opaque", &m(&["unsafe"])), "unsafe opaque");
        assert_eq!(kind_description("opaque", &m(&[])), "opaque");
        assert_eq!(
            kind_description("inductive", &m(&["unsafe"])),
            "unsafe inductive"
        );
        assert_eq!(
            kind_description("class_inductive", &m(&["unsafe"])),
            "class inductive"
        );
        assert_eq!(kind_description("theorem", &m(&["unsafe"])), "theorem");
    }

    #[test]
    fn css_kinds_are_a_different_mapping() {
        assert_eq!(css_kind("definition"), "def");
        assert_eq!(css_kind("class_inductive"), "class");
        assert_eq!(css_kind("constructor"), "ctor");
        assert_eq!(css_kind("theorem"), "theorem");
        assert_eq!(css_kind("structure"), "structure");
    }

    #[test]
    fn break_within_wraps_each_component() {
        assert_eq!(
            break_within("Nat.succ"),
            "<span class=\"name\">Nat</span>.<span class=\"name\">succ</span>"
        );
        assert_eq!(break_within("Nat"), "<span class=\"name\">Nat</span>");
        assert_eq!(
            break_within("a<b"),
            "<span class=\"name\">a&lt;b</span>",
            "the component is escaped, the separator is not"
        );
    }
}
