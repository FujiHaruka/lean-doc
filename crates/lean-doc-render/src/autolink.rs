//! Resolving the names a docstring mentions to pages.
//!
//! Ported from `experiments/stage7d/render.ts` (925-1077), which is frozen —
//! `nameToLink`, `isNameLit`, `isLetterLike`, and the maps they read
//! (`render.ts:2008-2085`). This is the half of `DocGen4/Output/DocString.lean`
//! that md4c does not replace (plan §7): 153 lines that have to be written out
//! by hand, against a 594-line CommonMark subset that FFI deleted.
//!
//! # Why this is here and not in `lean-doc-md`
//!
//! [`lean_doc_md::Renderer`] takes a [`LinkResolver`]; this module is the
//! implementation. It has to live on this side because the answer needs
//! [`LinkIndex`] and the IR, neither of which a markdown crate should know
//! about, and the crate dependency runs `lean-doc-render` → `lean-doc-md`.
//!
//! **The first branch of `nameToLink?` is not here.** A word that ends in
//! `.lean` and contains a `/` is a path to a source file and needs no index at
//! all, so [`lean_doc_md::Renderer::resolve_link`] answers it before the
//! resolver is consulted. Moving it across the injection point costs 131 of the
//! target package's 4,987 docstrings 【実測 M1-c 後半】.
//!
//! # Which name is documented where — three sources, not one
//!
//! doc-gen4 asks `env.name2ModIdx`, which it has because it holds the whole
//! environment. This renderer holds none of it, so the map is assembled
//! (`render.ts:2001-2085`), and *which* of the three sources answers decides the
//! shape of the link:
//!
//! | source | fills | reached by |
//! |---|---|---|
//! | `deps/*.json` + every declaration + every resolved reference of the IR | `known` | branch 2, and `knownModules` through its values |
//! | the `\t` entries of the `.lidx` | the dependency closure's map | branch 2, after `known` |
//! | the `@` entries of the `.lidx` | `knownModules` | branch 3 |
//!
//! `knownModules` is the **union of all three** (plan §5, pitfall 6): the IR's
//! own module names, every module named as a value in `known`, and the `.lidx`'s
//! `@` section. Resolving with the `.lidx` alone drops links, and the way it
//! drops them is one anchor at a time — the page still renders, so a weak test
//! passes. [`NameIndexBuilder::build`] takes the `.lidx` as an argument rather
//! than letting it be forgotten.
//!
//! `known` is consulted **before** the `.lidx`, and the two are kept apart on
//! purpose (`render.ts:2059-2064`): `known` also backs the signature path, which
//! is byte-exact today, and a map fifty times larger underneath it would move
//! results that are already right.

use std::collections::{HashMap, HashSet};

use lean_doc_ir::{Decl, DepMap, ModuleFile};
use lean_doc_md::{LinkResolver, Renderer};

use crate::external::ExternalLinks;
use crate::link_index::LinkIndex;

/// The prefix Lean prints on a private name. `nameToLink` refuses to look one
/// up (`render.ts:940`), so a `_private.…` word in a code span stays text even
/// when the map happens to know it.
pub const PRIVATE_PREFIX: &str = "_private.";

/// `moduleNameToLink`: the site root, the module's components as directories,
/// and `.html` (`render.ts:385-387`).
#[must_use]
pub fn module_link(root: &str, module: &str) -> String {
    let mut out = String::with_capacity(root.len() + module.len() + 5);
    out.push_str(root);
    // Unescaped, as [`crate::page_path`] is and for the same reason (M5-b): an
    // href has to reach the file the renderer wrote.
    for (i, part) in lean_doc_ir::module_components(module)
        .into_iter()
        .enumerate()
    {
        if i > 0 {
            out.push('/');
        }
        out.push_str(part);
    }
    out.push_str(".html");
    out
}

/// `getRoot`: `../` once per component below the top, then `./`
/// (`render.ts:389-393`). `Foo.Bar` sits two directories deep, so its pages link
/// back through `.././`.
#[must_use]
pub fn page_root(module: &str) -> String {
    let depth = lean_doc_ir::module_components(module).len() - 1;
    let mut out = String::with_capacity(depth * 3 + 2);
    for _ in 0..depth {
        out.push_str("../");
    }
    out.push_str("./");
    out
}

// --------------------------------------------------------------- name literals

/// `Lean.isLetterLike` (`Init/Meta/Defs.lean:101-109`), which is what lets `α`,
/// `ℕ` and `𝒜` start an identifier.
///
/// The last range is **above the BMP**. That is not a curiosity: it is the
/// mathematical alphanumerics, the same block that makes UTF-16 order differ
/// from UTF-8 order (plan §7, U1), and a port that quietly dropped it would
/// still resolve every ASCII name — which is nearly all of them.
#[must_use]
pub fn is_letter_like(c: char) -> bool {
    let c = c as u32;
    // Greek lower case without λ, upper case without Π and Σ: those three are
    // Lean syntax.
    ((0x3b1..=0x3c9).contains(&c) && c != 0x3bb)
        || ((0x391..=0x3a9).contains(&c) && c != 0x3a0 && c != 0x3a3)
        || (0x3ca..=0x3fb).contains(&c)
        || (0x1f00..=0x1ffe).contains(&c)
        || (0x2100..=0x214f).contains(&c)
        || (0x1d49c..=0x1d59f).contains(&c)
        || ((0xc0..=0xff).contains(&c) && c != 0xd7 && c != 0xf7)
        || (0x100..=0x17f).contains(&c)
}

/// `Lean.isSubScriptAlnum` (`Init/Meta/Defs.lean:114-118`): the subscript digits
/// and letters that may appear inside an identifier but not start one.
#[must_use]
pub fn is_sub_script_alnum(c: char) -> bool {
    let c = c as u32;
    (0x2080..=0x2089).contains(&c)
        || (0x2090..=0x209c).contains(&c)
        || (0x1d62..=0x1d6a).contains(&c)
        || c == 0x2c7c
}

fn is_id_first(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || is_letter_like(c)
}

fn is_id_rest(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || c == '_'
        // `'`, `!` and `?` are identifier characters in Lean. Rejecting them —
        // which an ASCII-identifier notion of "name" does — costs every `foo'`
        // in the package its link 【実測: 9 anchors, stage 7b】.
        || c == '\''
        || c == '!'
        || c == '?'
        || is_letter_like(c)
        || is_sub_script_alnum(c)
}

/// `Lean.Syntax.decodeNameLit ("`" ++ s)` — whether `s` is a name literal
/// (`render.ts:995-1022`, i.e. `splitNameLitAux`, `Init/Meta/Defs.lean:1180-1203`,
/// plus "not anonymous").
///
/// A component is `«…»`, an identifier ([`is_id_first`] then [`is_id_rest`]\*)
/// or a run of digits, and after each component the rest must be empty or start
/// with `.`.
///
/// The empty string is **not** a name literal — it decodes to the anonymous
/// name. That matters: `autoLinkInline` splits on separators and keeps the
/// empty pieces between two of them, so this function is asked about `""`
/// routinely, and a resolver that answered would put an empty anchor into every
/// double space.
#[must_use]
pub fn is_name_lit(s: &str) -> bool {
    let bytes = s.as_bytes();
    let mut i = 0;
    loop {
        // An empty component: `a..b`, a leading `.`, or the empty string.
        let Some(c) = s[i..].chars().next() else {
            return false;
        };
        if c == '«' {
            match s[i + c.len_utf8()..].find('»') {
                Some(at) => i += c.len_utf8() + at + '»'.len_utf8(),
                None => return false,
            }
        } else if is_id_first(c) {
            i += c.len_utf8();
            while let Some(d) = s[i..].chars().next() {
                if !is_id_rest(d) {
                    break;
                }
                i += d.len_utf8();
            }
        } else if c.is_ascii_digit() {
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
        } else {
            return false;
        }
        if i >= bytes.len() {
            return true;
        }
        if bytes[i] == b'.' {
            i += 1;
            continue;
        }
        return false;
    }
}

// ------------------------------------------------------------------ the index

/// Which name is documented in which module, which names are modules, and —
/// since M7-c — where a **dependency's** source lives.
///
/// Built once per run by [`NameIndexBuilder`] and shared by every page. See the
/// module comment for what goes in.
///
/// # Why the dependency map is in here
///
/// Because every caller that has to answer "what is the href for this name"
/// already holds this index — the docstring resolver below, the signature path
/// ([`crate::CodeRenderer::const_link`]) and the structure-field path
/// ([`crate::decl_name_to_link`]) — and the answer needs *both* the `.lidx`'s
/// source range and the dependency map's prefix. Threading a second value
/// through the same three constructors would let the two get out of step on the
/// one thing [`NameIndex::link_to`] exists to keep together.
#[derive(Debug, Default)]
pub struct NameIndex {
    known: HashMap<String, String>,
    links: LinkIndex,
    known_modules: HashSet<String>,
    external: ExternalLinks,
}

impl NameIndex {
    /// A builder with nothing in it.
    #[must_use]
    pub fn builder() -> NameIndexBuilder {
        NameIndexBuilder::default()
    }

    /// The module a name is declared in **according to the IR alone**
    /// (`known`), which is the map the signature path reads.
    ///
    /// Deliberately not the same lookup as [`NameIndex::module_of`]: the
    /// dependency closure's map is fifty times larger, and letting it under
    /// `findLinkableParent` would move results that are byte-exact today
    /// (`render.ts:2059-2064`).
    #[must_use]
    pub fn known(&self, name: &str) -> Option<&str> {
        self.known.get(name).map(String::as_str)
    }

    /// The module a name is documented in: the IR first, then the dependency
    /// closure's `.lidx`. What `nameToLink`'s second branch asks.
    #[must_use]
    pub fn module_of(&self, name: &str) -> Option<&str> {
        self.known(name).or_else(|| self.links.module_of(name))
    }

    /// Whether a name is a module with a page of its own — the union of all
    /// three sources.
    #[must_use]
    pub fn is_known_module(&self, name: &str) -> bool {
        self.known_modules.contains(name)
    }

    /// **The link every page draws** (M7-c): [`ExternalLinks::href`] with this
    /// run's two maps supplied — the prefix from the dependency map, the line
    /// range from the `.lidx`.
    ///
    /// `module` is where the target is defined and `anchor` is the declaration
    /// being linked to, or `None` when the link is to a module rather than into
    /// one. Every call site in this crate that builds a link to *another*
    /// module goes through here; the one that does not is a declaration's own
    /// self-link, which is on this page by construction.
    #[must_use]
    pub fn link_to(&self, root: &str, module: &str, anchor: Option<&str>) -> String {
        self.external.href(
            root,
            module,
            anchor,
            anchor.and_then(|name| self.links.range_of(name)),
        )
    }

    /// The dependency map this index was built with, for the one caller that
    /// builds a link without holding an index — the page frame's import list
    /// ([`crate::internal_nav_html`]), where there is no declaration and so
    /// nothing to look a range up for.
    #[must_use]
    pub const fn external(&self) -> &ExternalLinks {
        &self.external
    }

    /// Names in `known` (the IR's own map).
    #[must_use]
    pub fn len(&self) -> usize {
        self.known.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.known.is_empty()
    }

    /// Names in the dependency closure's map.
    #[must_use]
    pub fn link_index_len(&self) -> usize {
        self.links.len()
    }

    /// Modules that can be linked to.
    #[must_use]
    pub fn known_module_count(&self) -> usize {
        self.known_modules.len()
    }
}

/// Accumulates [`NameIndex`] in the order `render.ts:2008-2085` does.
///
/// Order is behaviour, not taste. A declaration overwrites whatever was there;
/// a reference only fills a gap; and the modules are read after the dependency
/// slices, so a name this package declares beats the same name in a dependency.
#[derive(Debug, Default)]
pub struct NameIndexBuilder {
    known: HashMap<String, String>,
    modules: HashSet<String>,
}

impl NameIndexBuilder {
    /// A `deps/<Package>.json` slice: constants of another package that this one
    /// refers to. Overwrites.
    pub fn dep_map(&mut self, dep: &DepMap) -> &mut Self {
        for (name, module) in &dep.declarations {
            self.declaration(name, module);
        }
        self
    }

    /// One module of the IR: its name, its declarations, and the references the
    /// extractor resolved for each of them.
    pub fn module(&mut self, module: &ModuleFile) -> &mut Self {
        self.module_name(&module.module);
        for decl in &module.declarations {
            self.declaration(&decl.name, &module.module);
            for r in &decl.refs {
                self.reference(&r.name, &r.module);
            }
        }
        self
    }

    /// `known.set(name, module)` — the later declaration wins.
    pub fn declaration(&mut self, name: &str, module: &str) -> &mut Self {
        match self.known.get_mut(name) {
            Some(slot) => {
                slot.clear();
                slot.push_str(module);
            }
            None => {
                self.known.insert(name.to_owned(), module.to_owned());
            }
        }
        self
    }

    /// `if (!known.has(name)) known.set(name, module)` — a reference never
    /// overwrites a declaration, and the first reference to a name wins.
    pub fn reference(&mut self, name: &str, module: &str) -> &mut Self {
        if !self.known.contains_key(name) {
            self.known.insert(name.to_owned(), module.to_owned());
        }
        self
    }

    /// A module of the package being rendered. Every one of them is a link
    /// target whether or not it declares anything.
    pub fn module_name(&mut self, module: &str) -> &mut Self {
        if !self.modules.contains(module) {
            self.modules.insert(module.to_owned());
        }
        self
    }

    /// Closes the index over the dependency closure's map and the dependency
    /// **link** map.
    ///
    /// This is where the three sources of `knownModules` meet
    /// (`render.ts:2051-2052, 2079`). Taking the `.lidx` by value here is the
    /// reason a caller cannot finish without deciding about it: the product
    /// always has one (plan 決定 4), and passing [`LinkIndex::default`] is a
    /// visible choice rather than an omission.
    ///
    /// **`external` is the second argument for exactly the same reason** (M7-c).
    /// [`ExternalLinks::default`] is the pre-M7 renderer — every link into a
    /// dependency stays a relative page link — and that has to be something a
    /// caller *says*, because the failure it produces is a site full of hrefs
    /// pointing at pages this run never wrote.
    #[must_use]
    pub fn build(self, links: LinkIndex, external: ExternalLinks) -> NameIndex {
        let Self { known, mut modules } = self;
        for module in known.values() {
            if !modules.contains(module) {
                modules.insert(module.clone());
            }
        }
        for module in links.known_modules() {
            if !modules.contains(module) {
                modules.insert(module.to_owned());
            }
        }
        NameIndex {
            known,
            links,
            known_modules: modules,
            external,
        }
    }
}

// ------------------------------------------------------------------- per page

/// The declarations `nameToLink`'s last branch scans, in the order it scans them
/// (`render.ts:1910-1918`).
///
/// `res.moduleInfo[current].members` is every `DocInfo` of the module — including
/// the ones that get no page entry, because doc-gen4 filters that list with
/// `filterDocInfo` and not with `shouldRender` — minus the private ones, in
/// declaration-range order. Passing the IR's own order instead picks the wrong
/// one of two candidates 【実測: 6 anchors, stage 7b】.
#[must_use]
pub fn module_decl_names(module: &ModuleFile) -> Vec<&str> {
    let mut decls: Vec<&Decl> = module
        .declarations
        .iter()
        .filter(|d| !d.name.starts_with(PRIVATE_PREFIX))
        .collect();
    decls.sort_by_key(|d| (d.line, d.col, d.index));
    decls.into_iter().map(|d| d.name.as_str()).collect()
}

/// [`LinkResolver`] for one page: the run's [`NameIndex`], the page's root, and
/// the page's own declarations.
///
/// Two of the four branches depend on the page rather than the run, which is why
/// this is per page and not per run: the root prefixes every link, and the last
/// branch searches the current module.
pub struct PageLinks<'a> {
    index: &'a NameIndex,
    root: &'a str,
    decl_names: &'a [&'a str],
}

impl<'a> PageLinks<'a> {
    /// `root` is [`page_root`] of the module being rendered, `decl_names` is
    /// [`module_decl_names`] of it.
    #[must_use]
    pub const fn new(index: &'a NameIndex, root: &'a str, decl_names: &'a [&'a str]) -> Self {
        Self {
            index,
            root,
            decl_names,
        }
    }

    /// A docstring renderer wired to this page.
    ///
    /// Use this rather than [`Renderer::new`]: the root reaches the output
    /// through two paths — the renderer's own `extendLink` and this resolver's
    /// `moduleNameToLink` — and handing them different values would produce
    /// links that are half right.
    #[must_use]
    pub fn renderer(&'a self) -> Renderer<'a> {
        Renderer::new(self.root, self)
    }
}

impl LinkResolver for PageLinks<'_> {
    /// `nameToLink?` from its second branch on (`render.ts:938-956`); the first
    /// is [`Renderer::resolve_link`]'s.
    ///
    /// In order: a name literal or nothing; the name in `known` then in the
    /// `.lidx`, unless it is private; the name as a module; and finally the
    /// first declaration of *this* module whose trailing components match —
    /// which is what links a bare `succ` inside `Nat`'s page.
    ///
    /// Three of the four branches name a module that may belong to a
    /// dependency, so all three go through [`NameIndex::link_to`] (M7-c). The
    /// second branch is where most of them are: it is the one the `.lidx`
    /// answers, and the `.lidx` *is* the dependency closure.
    fn name_to_link(&self, s: &str) -> Option<String> {
        if !is_name_lit(s) {
            return None;
        }
        if !s.starts_with(PRIVATE_PREFIX)
            && let Some(module) = self.index.module_of(s)
        {
            return Some(self.index.link_to(self.root, module, Some(s)));
        }
        if self.index.is_known_module(s) {
            return Some(self.index.link_to(self.root, s, None));
        }
        // "find a similar name in the same module": compare components from the
        // end, over as many as the shorter of the two has. `succ` matches
        // `Nat.succ`; `Nat.succ` matches `Foo.Nat.succ`.
        let want: Vec<&str> = s.rsplit('.').collect();
        for name in self.decl_names {
            let have: Vec<&str> = name.rsplit('.').collect();
            let k = want.len().min(have.len());
            if want[..k] == have[..k] {
                // Every name in `decl_names` came from a module that was fed to
                // the builder, so `known` has it.
                let module = self
                    .index
                    .known(name)
                    .expect("a declaration of this page is in the name index");
                return Some(self.index.link_to(self.root, module, Some(name)));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_links_are_rooted_paths() {
        assert_eq!(module_link("../", "Foo.Bar"), "../Foo/Bar.html");
        assert_eq!(module_link("./", "Foo"), "./Foo.html");
        assert_eq!(page_root("Foo"), "./");
        assert_eq!(page_root("Foo.Bar"), ".././");
        assert_eq!(page_root("Foo.Bar.Baz"), "../.././");
    }

    #[test]
    fn name_literals_accept_what_lean_accepts() {
        for ok in [
            "Nat",
            "Nat.succ",
            "Nat.succ'",
            "foo!",
            "foo?",
            "_root_.Nat",
            "Nat.1",
            "«a b».c",
            "«».x",
            "α",
            "ℕ.add",
            "𝒜.mem",
            "x₁",
            "Foo.«bar»",
        ] {
            assert!(is_name_lit(ok), "{ok:?} should be a name literal");
        }
        for bad in [
            "",       // anonymous
            ".",      // empty component
            "a.",     // trailing dot
            ".a",     // leading dot
            "a..b",   // empty component
            "a b",    // space
            "a-b",    // hyphen
            "λ",      // Lean syntax, not letter-like
            "Π",      //
            "Σ",      //
            "«a",     // unterminated
            "a«b»",   // the guillemet is not `isIdRest`
            "1a",     // a digit component cannot grow letters
            "-1",     //
            "Nat.$x", //
        ] {
            assert!(!is_name_lit(bad), "{bad:?} should not be a name literal");
        }
    }

    /// The range that lives above the BMP, on its own: dropping it leaves every
    /// ASCII name resolving, so nothing else in a test suite would notice.
    #[test]
    fn letter_like_reaches_the_mathematical_alphanumerics() {
        assert!(is_letter_like('\u{1d49c}'));
        assert!(is_letter_like('𝒜'));
        assert!(is_letter_like('\u{1d59f}'));
        assert!(!is_letter_like('\u{1d49b}'));
        assert!(!is_letter_like('\u{1d5a0}'));
        assert!(is_name_lit("𝒜"));
        assert!(is_name_lit("Foo.𝒜'"));
    }

    const MODULE_JSON: &str = r#"{
        "schemaVersion": 4,
        "module": "Pkg.Two",
        "imports": [],
        "moduleDocs": [],
        "tactics": [],
        "declarations": [
            {"name": "Pkg.Two.b", "kind": "theorem", "modifiers": [], "binders": [],
             "implicits": [], "binderCode": [], "type": "", "typeCode": [],
             "line": 9, "col": 0, "endLine": 9, "endCol": 1, "index": 1,
             "members": [], "doc": null, "equations": [], "equationCode": [],
             "refs": [["Dep.M", "Dep.shared"], ["Pkg.One", "Pkg.One.a"]]},
            {"name": "_private.Pkg.Two.hidden", "kind": "def", "modifiers": [],
             "binders": [], "implicits": [], "binderCode": [], "type": "",
             "typeCode": [], "line": 3, "col": 0, "endLine": 3, "endCol": 1,
             "index": 0, "members": [], "doc": null, "equations": [],
             "equationCode": [], "refs": []},
            {"name": "Pkg.Two.a", "kind": "def", "modifiers": [], "binders": [],
             "implicits": [], "binderCode": [], "type": "", "typeCode": [],
             "line": 5, "col": 0, "endLine": 5, "endCol": 1, "index": 2,
             "members": [], "doc": null, "equations": [], "equationCode": [],
             "refs": []}
        ]
    }"#;

    fn module() -> ModuleFile {
        serde_json::from_str(MODULE_JSON).expect("the literal is schema 4")
    }

    /// The page's scan list is declaration-range order with the private names
    /// removed — not the IR's order, and not a sort by name.
    #[test]
    fn decl_names_are_in_declaration_range_order_without_the_private_ones() {
        assert_eq!(module_decl_names(&module()), ["Pkg.Two.a", "Pkg.Two.b"]);
    }

    #[test]
    fn a_reference_fills_a_gap_and_a_declaration_overwrites() {
        let dep: DepMap = serde_json::from_str(
            r#"{"schemaVersion": 4, "package": "Dep",
                "declarations": {"Dep.shared": "Dep.Other", "Pkg.Two.a": "Dep.Stale"}}"#,
        )
        .expect("the literal is schema 4");
        let mut builder = NameIndex::builder();
        builder.dep_map(&dep).module(&module());
        let index = builder.build(LinkIndex::default(), ExternalLinks::default());

        // The declaration read later overwrote the dependency slice…
        assert_eq!(index.known("Pkg.Two.a"), Some("Pkg.Two"));
        // …while the reference did not overwrite what was already there.
        assert_eq!(index.known("Dep.shared"), Some("Dep.Other"));
        // A reference to a name nobody declared is still an answer.
        assert_eq!(index.known("Pkg.One.a"), Some("Pkg.One"));
        // Private names are in the map; `nameToLink` is what refuses them.
        assert_eq!(index.known("_private.Pkg.Two.hidden"), Some("Pkg.Two"));
    }

    /// The union, source by source. Each of the three contributes a module that
    /// the other two do not have, so dropping any one of them fails here.
    ///
    /// `Pkg.Empty` is the load-bearing one for the first source: a module that
    /// declares nothing is not a value in `known` either, so it is a link
    /// target only because the IR listed it. Modules that do declare something
    /// hide that source behind the second.
    #[test]
    fn known_modules_is_the_union_of_three_sources() {
        let mut builder = NameIndex::builder();
        builder.module(&module()).module_name("Pkg.Empty");
        let index = builder.build(LinkIndex::parse("@Lidx.Only\n"), ExternalLinks::default());

        assert!(index.is_known_module("Pkg.Empty"), "the IR's module names");
        assert!(index.is_known_module("Pkg.One"), "a value in `known`");
        assert!(index.is_known_module("Dep.M"), "a value in `known`");
        assert!(index.is_known_module("Lidx.Only"), "the .lidx's @ section");
        // Its own module is reachable through two of the three, which is why it
        // is not the one this test stands on.
        assert!(index.is_known_module("Pkg.Two"));
        assert!(!index.is_known_module("Nowhere"));
    }

    fn resolve(index: &NameIndex, decl_names: &[&str], s: &str) -> Option<String> {
        PageLinks::new(index, "../", decl_names).name_to_link(s)
    }

    /// The `.lidx` the branch tests share: one dependency declaration, with a
    /// source range, and one dependency module that declares nothing.
    const LIDX: &str = "@Lidx.Only\nDep.M\n\tDep.only_in_lidx\t12\t14\n";

    #[test]
    fn resolution_takes_the_branches_in_order() {
        let mut builder = NameIndex::builder();
        builder.module(&module());
        let index = builder.build(LinkIndex::parse(LIDX), ExternalLinks::default());
        let names = ["Pkg.Two.a", "Pkg.Two.b"];

        // 2: `known`, then the .lidx.
        assert_eq!(
            resolve(&index, &names, "Pkg.Two.a").as_deref(),
            Some("../Pkg/Two.html#Pkg.Two.a")
        );
        assert_eq!(
            resolve(&index, &names, "Dep.only_in_lidx").as_deref(),
            Some("../Dep/M.html#Dep.only_in_lidx")
        );
        // 3: a module, with no fragment.
        assert_eq!(
            resolve(&index, &names, "Lidx.Only").as_deref(),
            Some("../Lidx/Only.html")
        );
        // 4: the trailing components of one of this page's declarations.
        assert_eq!(
            resolve(&index, &names, "a").as_deref(),
            Some("../Pkg/Two.html#Pkg.Two.a")
        );
        // …and the scan takes the first match in page order, not the best one.
        assert_eq!(
            resolve(&index, &["Pkg.Two.b", "Pkg.Two.a"], "Two.a").as_deref(),
            Some("../Pkg/Two.html#Pkg.Two.a")
        );
        // Not a name literal: no lookup at all.
        assert_eq!(resolve(&index, &names, "a b"), None);
        assert_eq!(resolve(&index, &names, ""), None);
        // Private: branch 2 is skipped, and nothing below it answers either.
        assert_eq!(resolve(&index, &names, "_private.Pkg.Two.hidden"), None);
    }

    /// **M7-c**: the same four branches with a dependency map, one root of which
    /// (`Dep`) is a dependency and one of which (`Pkg`) is not.
    ///
    /// The three branches that can name another package's module move to its
    /// pinned source; the two that can only name this package's own do not. The
    /// case above is the same assertions with an empty map, so the two together
    /// are both sides of [`ExternalLinks::href`].
    #[test]
    fn a_docstring_link_into_a_dependency_is_a_blob_url() {
        let mut builder = NameIndex::builder();
        builder.module(&module());
        let index = builder.build(
            LinkIndex::parse(LIDX),
            ExternalLinks::new([
                ("Dep", "https://host/o/dep/blob/abc"),
                ("Lidx", "https://h/o/l/blob/def"),
            ]),
        );
        let names = ["Pkg.Two.a", "Pkg.Two.b"];

        // 2, through the .lidx — which carried a range, so the URL is anchored
        // at the lines rather than at the declaration.
        assert_eq!(
            resolve(&index, &names, "Dep.only_in_lidx").as_deref(),
            Some("https://host/o/dep/blob/abc/Dep/M.lean#L12-L14")
        );
        // 3, a module: no anchor of any kind.
        assert_eq!(
            resolve(&index, &names, "Lidx.Only").as_deref(),
            Some("https://h/o/l/blob/def/Lidx/Only.lean")
        );
        // 2 and 4 into this package: **unchanged**, which is what §M7
        // 「自パッケージのリンクを巻き込まない」 means at a call site.
        assert_eq!(
            resolve(&index, &names, "Pkg.Two.a").as_deref(),
            Some("../Pkg/Two.html#Pkg.Two.a")
        );
        assert_eq!(
            resolve(&index, &names, "a").as_deref(),
            Some("../Pkg/Two.html#Pkg.Two.a")
        );
        // A name the .lidx has no range for keeps the file's URL rather than
        // losing the link (§M7「行範囲が取れない宣言でリンクを消さない」).
        let no_range = NameIndex::builder().build(
            LinkIndex::parse("Dep.M\n\tDep.bare\n"),
            ExternalLinks::new([("Dep", "https://host/o/dep/blob/abc")]),
        );
        assert_eq!(
            resolve(&no_range, &[], "Dep.bare").as_deref(),
            Some("https://host/o/dep/blob/abc/Dep/M.lean")
        );
    }

    /// The empty piece `splitAround` leaves between two separators must not
    /// resolve — an anchor there would land in the middle of every double space.
    #[test]
    fn the_empty_string_never_resolves() {
        let mut builder = NameIndex::builder();
        builder.declaration("", "Pkg.Two").module_name("");
        let index = builder.build(LinkIndex::parse("@\n"), ExternalLinks::default());
        assert_eq!(resolve(&index, &[""], ""), None);
    }
}
