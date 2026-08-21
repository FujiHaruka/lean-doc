//! Derived from doc-gen4 (Copyright (c) 2021 Henrik Böving, Apache-2.0) and
//! changed; see this repository's NOTICE and `docs/provenance.md`.
//!
//! Resolving the names a docstring mentions to pages.
//!
//! Ported from `experiments/stage7d/render.ts` (925-1077), which is frozen —
//! `nameToLink`, `isNameLit`, `isLetterLike`, and the maps they read
//! (`render.ts:2008-2085`). This is the half of `DocGen4/Output/DocString.lean`
//! that md4c does not replace (plan §7): 153 lines that have to be written out
//! by hand, against a 594-line CommonMark subset that FFI deleted.
//!
//! # Why this is here and not in `litedoc4-md`
//!
//! [`litedoc4_md::Renderer`] takes a [`LinkResolver`]; this module is the
//! implementation. It has to live on this side because the answer needs
//! [`LinkIndex`] and the IR, neither of which a markdown crate should know
//! about, and the crate dependency runs `litedoc4-render` → `litedoc4-md`.
//!
//! **The first branch of `nameToLink?` is here too, and that is an M8 change.**
//! A word that ends in `.lean` and contains a `/` is a path to a source file,
//! and doc-gen4 turns it into a page with no index at all: the path is read as
//! relative to the repository root and the extension is swapped. That is right
//! for `Mathlib/Order/Basic.lean` and wrong for the target package, whose
//! docstrings write the path relative to their own module — the produced page
//! was never written, and 160 of the site's 32,868 internal links were dangling
//! because of it 【実測 2026-08-16,
//! `benchmarks/results/m8-ui2-dead-links.txt`】.
//! [`NameIndex::module_for_source_path`] is the answer that consults
//! `knownModules`, and gate UI-2 is what it is for
//! (`docs/plans/ui-redesign.md` §1).
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
//!
//! # "A name that is a module" and "a module this run wrote a page for" differ
//!
//! `knownModules` answers the first question and [`NameIndex::has_page`] the
//! second, and the gap between them is **modules of the package being
//! documented that this run does not render**. A `lakefile.toml` may declare
//! more than one `[[lean_lib]]` — `batteries` declares three — and
//! `litedoc4 build --lib Batteries` extracts one of them while the `.lidx` holds
//! the whole environment, so `BatteriesRecycling.*` is a known module, is in no
//! dependency's map, and has no page 【実測 2026-08-17】. Reading absence from
//! [`ExternalLinks`] as "this package's module, so link to its page" wrote a
//! relative href to a file that was never created, which is the same dead link
//! the empty-base state closed one level up (see [`crate::external`]).
//!
//! So the page set is carried separately: [`NameIndexBuilder::build`] freezes it
//! **before** the union that widens `knownModules`, and
//! [`NameIndex::link_to`]'s last branch consults it.

use std::collections::{HashMap, HashSet};

use litedoc4_ir::{Decl, DepMap, ModuleFile};
use litedoc4_md::{LinkResolver, Renderer};

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
    for (i, part) in litedoc4_ir::module_components(module)
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
    let depth = litedoc4_ir::module_components(module).len() - 1;
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
    page_modules: HashSet<String>,
    /// Set by [`NameIndexBuilder::build_with_a_page_for_every_module`] only.
    /// `false` — the derived default — is the product's answer and the safe one:
    /// a page has to be in `page_modules` to be linked to.
    every_module_has_a_page: bool,
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

    /// Whether a name **is a module** — the union of all three sources.
    ///
    /// Not the same question as [`NameIndex::has_page`]: this one decides which
    /// branch of `nameToLink?` answers, and that one decides what the answer
    /// looks like. A module of a dependency is `true` here.
    #[must_use]
    pub fn is_known_module(&self, name: &str) -> bool {
        self.known_modules.contains(name)
    }

    /// Whether **this run wrote a page for** `module` — the modules the builder
    /// was fed, before the union that widens `known_modules`.
    ///
    /// This is the set the site actually has files for, and the reason it is
    /// kept apart from `known_modules` is in the module comment.
    #[must_use]
    pub fn has_page(&self, module: &str) -> bool {
        self.every_module_has_a_page || self.page_modules.contains(module)
    }

    /// The module a **source path** written in a docstring names, or `None`
    /// when no known module matches it or more than one does.
    ///
    /// `path` is the word `nameToLink?`'s first branch took, without its
    /// `.lean`: `EPI/Stam/ToBridge`. Its components are a module name —
    /// `EPI.Stam.ToBridge` — and the question is which module of this run's
    /// world that is:
    ///
    /// 1. that name itself, when it is a known module. This is the
    ///    repository-root-relative path doc-gen4 assumes, and it is what
    ///    `Mathlib/Order/Basic.lean` takes.
    /// 2. otherwise the one known module that has it as a **proper suffix on a
    ///    component boundary**: `InformationTheory.Shannon.EPI.Stam.ToBridge`
    ///    is a match for `EPI.Stam.ToBridge` and `XEPI.Stam.ToBridge` is not.
    ///    This is the path a docstring writes relative to its own module, which
    ///    is what the target package does.
    ///
    /// **Two matches is `None`, not the first one.** A link to the wrong page
    /// is worse than no link: the reader who follows a 404 knows something is
    /// missing, and the reader who lands on a plausible wrong page does not.
    #[must_use]
    pub fn module_for_source_path(&self, path: &str) -> Option<&str> {
        // `escape_module` rather than `replace('/', ".")` because a directory
        // whose name is not an identifier is a quoted component in the module
        // name it belongs to: `Odd-Name/Inner.lean` is `«Odd-Name».Inner`.
        let candidate = litedoc4_ir::escape_module(path.split('/'));
        if let Some(exact) = self.known_modules.get(&candidate) {
            return Some(exact.as_str());
        }
        // A `.` inside a component can only be inside `«…»`, and a name that
        // ends in a quoted component ends in `»` — so a module that ends with
        // these bytes and has a `.` in front of them ends with these
        // *components*, and no component was cut in half.
        let mut found = None;
        for module in &self.known_modules {
            if module.len() <= candidate.len()
                || !module.ends_with(&candidate)
                || module.as_bytes()[module.len() - candidate.len() - 1] != b'.'
            {
                continue;
            }
            if found.is_some() {
                return None;
            }
            found = Some(module.as_str());
        }
        found
    }

    /// **The link every page draws** (M7-c), and the only copy of the decision:
    /// where to point at `module`, anchored at the declaration `anchor` when
    /// there is one — or `None` when the honest answer is no link at all.
    ///
    /// `module` is where the target is defined and `anchor` is the declaration
    /// being linked to, or `None` when the link is to a module rather than into
    /// one ([`NameIndex::link_to_module`] is that call spelled out). Every call
    /// site in this crate that builds a link to *another* module goes through
    /// here; the one that does not is a declaration's own self-link, which is on
    /// this page by construction.
    ///
    /// # Four questions, because absence means four different things
    ///
    /// | asked | answer | which module |
    /// |---|---|---|
    /// | 0. [`ExternalLinks::docs_url_for`] resolves | the dependency's own `…/Mathlib/Order/Basic.html#…` | a dependency whose documentation site documents *this name* |
    /// | 1. [`ExternalLinks::url_for`] resolves | the `…/blob/<rev>/….lean#L…` | a version-pinned dependency |
    /// | 2. [`ExternalLinks::base_for`] holds the root | **`None`** | a dependency with no `/blob/<rev>` |
    /// | 3. [`NameIndex::has_page`] | `<root><module path>.html#<anchor>` | a module this run rendered |
    /// | 3. …and otherwise | **`None`** | a module of this package this run does not render |
    ///
    /// One lookup would do if "the map has no entry for this root" meant "this
    /// package's own module" and that in turn meant "a page exists". **Neither
    /// implication holds**, and each was a dead link 【実測 2026-08-17】: a
    /// `path` dependency breaks the first (see [`crate::external`]) and
    /// `batteries` breaks the second (see the module comment). So the questions
    /// are asked separately and in this order — question 2 has to come before 3,
    /// because a dependency's module has no page either and answering it with
    /// question 3 would lose *why* there is no link.
    ///
    /// **Question 0 is not a preference between two links, it is a different
    /// question** (A-1). It asks whether the dependency's own documentation site
    /// was **verified to document this name**, and only a `yes` answers; a `no`
    /// falls to question 1 and gets the version-pinned source, which is where
    /// every dependency link went before the flag existed. There is no third
    /// outcome and in particular no "try the docs site and see": a 404 is not
    /// visible from here, and avoiding one is the whole reason the pin is there.
    ///
    /// **`None` means "render the name, draw no link"**. Every caller turns that
    /// into text, and none of them falls through to a later branch: a resolved
    /// name that happens to be unlinkable must not be re-resolved to some
    /// *other* declaration that happens to have a page.
    #[must_use]
    pub fn link_to(&self, root: &str, module: &str, anchor: Option<&str>) -> Option<String> {
        if let Some(url) = self.external.docs_url_for(module, anchor) {
            return Some(url);
        }
        let lines = anchor.and_then(|name| self.links.range_of(name));
        if let Some(url) = self.external.url_for(module, lines) {
            return Some(url);
        }
        // Membership rather than `url_for`'s answer: that one folds "not a
        // dependency" and "a dependency with nothing to build a URL from" into
        // the same `None`, and those two get different answers here.
        // The *unescaped* first component, as `url_for` looks it up.
        if litedoc4_ir::module_components(module)
            .first()
            .is_some_and(|component| self.external.base_for(component).is_some())
        {
            return None;
        }
        if !self.has_page(module) {
            return None;
        }
        let mut out = module_link(root, module);
        if let Some(anchor) = anchor {
            out.push('#');
            out.push_str(anchor);
        }
        Some(out)
    }

    /// [`NameIndex::link_to`] for a link **to** a module rather than into one:
    /// the page frame's import list, where there is no declaration.
    ///
    /// Its own method rather than `link_to(root, module, None)` at the call site
    /// so that the import list cannot drift onto a second rule — the anchor is
    /// the only thing that differs, and a module has no source range to look up.
    #[must_use]
    pub fn link_to_module(&self, root: &str, module: &str) -> Option<String> {
        self.link_to(root, module, None)
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
    ///
    /// **This is also the page set** ([`NameIndex::has_page`]): the modules fed
    /// here — by [`NameIndexBuilder::module`], once per module of the IR — are
    /// the ones the run writes files for, and a module named only as a value in
    /// `known` or in the `.lidx` is not one of them.
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
    ///
    /// **The page set is taken here, before the union** (2026-08-17): the
    /// modules fed to this builder are the ones the run renders, and the two
    /// sources below only widen "which names are modules". Widening both is the
    /// dead link the module comment describes.
    #[must_use]
    pub fn build(self, links: LinkIndex, external: ExternalLinks) -> NameIndex {
        let pages = self.modules.clone();
        self.close(links, external, pages, false)
    }

    /// [`NameIndexBuilder::build`] for a world where **every module has a
    /// page**, which no product run is and none should become.
    ///
    /// It is what the frozen oracles need. doc-gen4 and the prototype rendered
    /// the whole environment, so their bytes link into `Mathlib.*`, into the
    /// modules the `.lidx` groups its entries under, and into the module a
    /// private name's prefix spells — a list no page set could be enumerated
    /// from, since the last of those is conjured from the name rather than read
    /// out of a map. Resolving a fixture in a *run's* world would drop links the
    /// oracle has and report it as a difference in this crate's output.
    ///
    /// Spelling it as a second constructor rather than an argument keeps the
    /// assumption in one place: the product path cannot reach it by passing a
    /// wrong flag, and a reader of a test can see which world is being claimed.
    #[must_use]
    pub fn build_with_a_page_for_every_module(
        self,
        links: LinkIndex,
        external: ExternalLinks,
    ) -> NameIndex {
        self.close(links, external, HashSet::new(), true)
    }

    /// The union `knownModules` is (`render.ts:2051-2052, 2079`), with the page
    /// set the caller decided on.
    fn close(
        self,
        links: LinkIndex,
        external: ExternalLinks,
        page_modules: HashSet<String>,
        every_module_has_a_page: bool,
    ) -> NameIndex {
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
            page_modules,
            every_module_has_a_page,
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
    /// is [`PageLinks::source_path_to_link`].
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
    ///
    /// **A branch that answers returns its answer, `None` included**: when the
    /// module is an unpinnable dependency's the word stays a code span, and the
    /// scan does *not* continue into the branches below it. Continuing would
    /// let branch 4 link `Nat.succ` to whatever declaration of *this* page ends
    /// in `succ`, which is a wrong link where there was going to be none.
    fn name_to_link(&self, s: &str) -> Option<String> {
        if !is_name_lit(s) {
            return None;
        }
        if !s.starts_with(PRIVATE_PREFIX)
            && let Some(module) = self.index.module_of(s)
        {
            return self.index.link_to(self.root, module, Some(s));
        }
        if self.index.is_known_module(s) {
            return self.index.link_to(self.root, s, None);
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
                return self.index.link_to(self.root, module, Some(name));
            }
        }
        None
    }

    /// `nameToLink?`'s first branch, **through the index** (M8, gate UI-2):
    /// [`NameIndex::module_for_source_path`] decides which module the path
    /// names, and the link is that module's page — or its pinned source, when
    /// the module belongs to a dependency, since this goes through
    /// [`NameIndex::link_to`] like every other module link (M7-c).
    ///
    /// `root` is the renderer's, which is [`PageLinks::root`] by construction
    /// ([`PageLinks::renderer`]); taking it as an argument is what lets the
    /// trait's index-free default answer at all.
    fn source_path_to_link(&self, root: &str, path: &str) -> Option<String> {
        let module = self.index.module_for_source_path(path)?;
        self.index.link_to(root, module, None)
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
        "schemaVersion": 5,
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
        serde_json::from_str(MODULE_JSON).expect("the literal is schema 5")
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
            r#"{"schemaVersion": 5, "package": "Dep",
                "declarations": {"Dep.shared": "Dep.Other", "Pkg.Two.a": "Dep.Stale"}}"#,
        )
        .expect("the literal is schema 5");
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
        // The two modules the `.lidx` contributes are pages of this world too:
        // the branches are what this case is about, and a page set that left
        // them out would answer `None` for a reason the next case covers.
        builder.module_name("Dep.M").module_name("Lidx.Only");
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
    /// are both sides of [`NameIndex::link_to`]'s first two branches.
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

    /// **2026-08-17**, the docstring half of the fix: a name that resolves into
    /// a dependency with no version-pinned URL stays a code span.
    ///
    /// The bytes are asserted through the renderer, not just the resolver,
    /// because the failure mode this replaced is invisible at the resolver: a
    /// `<a href>` that renders perfectly and 404s. `Dep.only_in_lidx` is
    /// load-bearing in the other direction too — it is a name this page's own
    /// declarations *end* with nothing like, so branch 4 cannot quietly answer
    /// for it, and `a` right after it is the one that could.
    #[test]
    fn a_docstring_name_in_a_dependency_that_cannot_be_pinned_stays_text() {
        let mut builder = NameIndex::builder();
        builder.module(&module());
        let index = builder.build(
            LinkIndex::parse(LIDX),
            ExternalLinks::new([("Dep", ""), ("Lidx", "")]),
        );
        let names = ["Pkg.Two.a", "Pkg.Two.b"];

        // 2, through the .lidx; 3, a module. Both known, neither linkable.
        assert_eq!(resolve(&index, &names, "Dep.only_in_lidx"), None);
        assert_eq!(resolve(&index, &names, "Lidx.Only"), None);
        // 2 and 4 into this package: unchanged.
        assert_eq!(
            resolve(&index, &names, "Pkg.Two.a").as_deref(),
            Some("../Pkg/Two.html#Pkg.Two.a")
        );
        assert_eq!(
            resolve(&index, &names, "a").as_deref(),
            Some("../Pkg/Two.html#Pkg.Two.a")
        );

        // The bytes: the name is still there, and there is no anchor around it.
        let render = |md: &str| {
            PageLinks::new(&index, "../", &names)
                .renderer()
                .docstring(md)
        };
        assert_eq!(
            render("`Dep.only_in_lidx`\n"),
            "<p><code>Dep.only_in_lidx</code></p>"
        );
        assert_eq!(render("`Lidx.Only`\n"), "<p><code>Lidx.Only</code></p>");
        assert_eq!(
            render("`Pkg.Two.a`\n"),
            "<p><code><a href=\"../Pkg/Two.html#Pkg.Two.a\">Pkg.Two.a</a></code></p>"
        );
    }

    /// [`NameIndex::link_to`]'s four answers on **one** index, which is the only
    /// way to see that they are one decision and not four call sites.
    ///
    /// M7-c's two rows, the 2026-08-17 empty-base row, and the same day's
    /// no-page row. The map and the page set disagree on purpose: `Mathlib` is
    /// pinned, `Dep` is not, `Pkg.Two` has a page and `Pkg.One` does not.
    #[test]
    fn a_link_takes_one_of_four_branches() {
        const MATHLIB: &str = "https://host/o/mathlib4/blob/fabf563";
        let mut builder = NameIndex::builder();
        builder.module(&module());
        let index = builder.build(
            LinkIndex::parse("Mathlib.Order.Basic\n\tLE.ext\t67\t67\n"),
            ExternalLinks::new([("Mathlib", MATHLIB), ("Dep", "")]),
        );

        // 1: a version-pinned dependency. The declaration fragment is
        // **replaced** by the line anchor rather than appended — the two name
        // different kinds of thing and a blob URL with `#LE.ext` on it points at
        // nothing.
        assert_eq!(
            index
                .link_to(".././", "Mathlib.Order.Basic", Some("LE.ext"))
                .as_deref(),
            Some("https://host/o/mathlib4/blob/fabf563/Mathlib/Order/Basic.lean#L67-L67")
        );
        // …and a declaration the `.lidx` has no range for keeps the file's URL
        // (§M7「行範囲が取れない宣言でリンクを消さない」).
        assert_eq!(
            index
                .link_to(".././", "Mathlib.Order.Basic", Some("no.range"))
                .as_deref(),
            Some("https://host/o/mathlib4/blob/fabf563/Mathlib/Order/Basic.lean")
        );
        // 2: a dependency with no `/blob/<rev>`, in every call shape.
        assert_eq!(index.link_to("../", "Dep.Aux", Some("Dep.f")), None);
        assert_eq!(index.link_to_module("../", "Dep"), None);
        // 3: a module this run rendered — the pre-M7 bytes, anchor and all.
        assert_eq!(
            index
                .link_to(".././", "Pkg.Two", Some("Pkg.Two.a"))
                .as_deref(),
            Some(".././Pkg/Two.html#Pkg.Two.a")
        );
        assert_eq!(
            index.link_to_module("./", "Pkg.Two").as_deref(),
            Some("./Pkg/Two.html")
        );
        // 4: a module of this package with no page.
        assert_eq!(index.link_to("../", "Pkg.One", Some("Pkg.One.a")), None);
        assert_eq!(index.link_to_module("../", "Pkg.One"), None);
    }

    /// **The branch in front of all four** (A-1): a dependency whose own
    /// documentation site was verified to hold the name.
    ///
    /// The map is [`a_link_takes_one_of_four_branches`]'s with one difference —
    /// mathlib now publishes documentation, and its table holds one of the two
    /// names and one of the two modules. So the same index answers both ways,
    /// which is the statement that this is a lookup and not a preference: a name
    /// **on** the site gets the site, a name **off** it gets the version-pinned
    /// source it got before the flag existed, and neither is a retry of the
    /// other.
    #[test]
    fn a_verified_name_takes_the_dependencys_own_documentation() {
        const MATHLIB: &str = "https://host/o/mathlib4/blob/fabf563";
        const DOCS: &str = "https://leanprover-community.github.io/mathlib4_docs";
        let mut builder = NameIndex::builder();
        builder.module(&module());
        let index = builder.build(
            LinkIndex::parse("Mathlib.Order.Basic\n\tLE.ext\t67\t67\n"),
            ExternalLinks::new([("Mathlib", MATHLIB), ("Dep", "")]).with_docs([(
                "Mathlib".to_owned(),
                crate::external::DepDocs::new(
                    DOCS,
                    [("LE.ext", "./Mathlib/Order/Basic.html#LE.ext")],
                    [("Mathlib.Order.Basic", "./Mathlib/Order/Basic.html")],
                ),
            )]),
        );

        // 0: the table documents it. The line range the `.lidx` holds is not
        // consulted — a docs page is anchored by name, not by line.
        assert_eq!(
            index
                .link_to(".././", "Mathlib.Order.Basic", Some("LE.ext"))
                .as_deref(),
            Some(
                "https://leanprover-community.github.io/mathlib4_docs/Mathlib/Order/Basic.html#LE.ext"
            ),
        );
        assert_eq!(
            index.link_to_module("./", "Mathlib.Order.Basic").as_deref(),
            Some("https://leanprover-community.github.io/mathlib4_docs/Mathlib/Order/Basic.html"),
        );
        // 1: the table does not, so the version-pinned source, unchanged.
        assert_eq!(
            index
                .link_to(".././", "Mathlib.Order.Basic", Some("no.range"))
                .as_deref(),
            Some("https://host/o/mathlib4/blob/fabf563/Mathlib/Order/Basic.lean"),
        );
        assert_eq!(
            index.link_to_module("./", "Mathlib.Order.Defs").as_deref(),
            Some("https://host/o/mathlib4/blob/fabf563/Mathlib/Order/Defs.lean"),
        );
        // A root with no documentation site is untouched by any of it.
        assert_eq!(index.link_to("../", "Dep.Aux", Some("Dep.f")), None);
        assert_eq!(
            index
                .link_to(".././", "Pkg.Two", Some("Pkg.Two.a"))
                .as_deref(),
            Some(".././Pkg/Two.html#Pkg.Two.a"),
        );
    }

    /// **2026-08-17, `batteries`**: a module of *this* package that this run
    /// writes no page for is not a link target either.
    ///
    /// `batteries`' `lakefile.toml` declares three `[[lean_lib]]`s, so
    /// `--lib Batteries` extracts one of them while the `.lidx` — which is the
    /// whole environment — holds all three. `Pkg.Recycling` is that shape: a
    /// module no dependency map says anything about, known to the index through
    /// the `.lidx` and through a resolved reference, and with **no page**. The
    /// relative link this used to draw was a 404
    /// 【実測 2026-08-17, `tools/site-gate.sh`: DEAD internal link 1】.
    #[test]
    fn a_module_of_this_package_with_no_page_is_not_a_link() {
        let mut builder = NameIndex::builder();
        builder.module(&module());
        let index = builder.build(
            LinkIndex::parse("@Pkg.Recycling\nPkg.Recycling\n\tPkg.Recycling.helper\t3\t4\n"),
            ExternalLinks::default(),
        );
        let names = ["Pkg.Two.a", "Pkg.Two.b"];

        // The two questions, on the same name: `Pkg.Recycling` **is** a module
        // and this run wrote no page for it.
        assert!(index.is_known_module("Pkg.Recycling"));
        assert!(!index.has_page("Pkg.Recycling"));
        assert!(index.has_page("Pkg.Two"));

        // 2, through the `.lidx`; 3, the module itself; 2, through `known` —
        // `Pkg.One` is a module a reference named and the IR never carried.
        assert_eq!(resolve(&index, &names, "Pkg.Recycling.helper"), None);
        assert_eq!(resolve(&index, &names, "Pkg.Recycling"), None);
        assert_eq!(resolve(&index, &names, "Pkg.One.a"), None);
        // The one page this run wrote is unchanged, anchor and all.
        assert_eq!(
            resolve(&index, &names, "Pkg.Two.a").as_deref(),
            Some("../Pkg/Two.html#Pkg.Two.a")
        );
        assert_eq!(
            index.link_to_module("../", "Pkg.Two").as_deref(),
            Some("../Pkg/Two.html")
        );
        assert_eq!(index.link_to_module("../", "Pkg.Recycling"), None);

        // The bytes: the name stays, the anchor goes.
        let render = |md: &str| {
            PageLinks::new(&index, "../", &names)
                .renderer()
                .docstring(md)
        };
        assert_eq!(
            render("`Pkg.Recycling.helper`\n"),
            "<p><code>Pkg.Recycling.helper</code></p>"
        );
    }

    /// **M8, gate UI-2**: a source path is a question for the index like any
    /// other, and the answers it has no right to give are the interesting ones.
    ///
    /// `PkgSole.Target` is the load-bearing distractor: it ends with the bytes
    /// of `Sole.Target` and is not a match, because the match has to fall on a
    /// component boundary. A port that compared bytes would find two candidates
    /// for `Sole/Target.lean` and answer `None` — so this case fails in the
    /// *safe* direction, which is why it is asserted positively.
    #[test]
    fn a_source_path_is_resolved_through_the_known_modules() {
        let mut builder = NameIndex::builder();
        for module in [
            "Mathlib.Order.Basic",
            "Pkg.Deep.EPI.Stam.ToBridge",
            "Other.EPI.Stam.ToBridge",
            "Pkg.Sole.Target",
            "PkgSole.Target",
            "Alpha.«Odd-Name».Inner",
            "A.B",
            "X.A.B",
        ] {
            builder.module_name(module);
        }
        let index = builder.build(LinkIndex::default(), ExternalLinks::default());

        // 1: the path the repository root would give, which is doc-gen4's rule.
        assert_eq!(
            index.module_for_source_path("Mathlib/Order/Basic"),
            Some("Mathlib.Order.Basic")
        );
        // …and it wins over a suffix match, without asking whether that one is
        // unique.
        assert_eq!(index.module_for_source_path("A/B"), Some("A.B"));
        // 2: the path relative to a module, resolved by its one owner.
        assert_eq!(
            index.module_for_source_path("Sole/Target"),
            Some("Pkg.Sole.Target")
        );
        // A directory whose name is not an identifier is a quoted component.
        assert_eq!(
            index.module_for_source_path("Odd-Name/Inner"),
            Some("Alpha.«Odd-Name».Inner")
        );
        // Two owners: no link at all.
        assert_eq!(index.module_for_source_path("EPI/Stam/ToBridge"), None);
        // No owner.
        assert_eq!(index.module_for_source_path("Nope/Missing"), None);
        // Not a suffix of `PkgSole.Target` either, from the other side.
        assert_eq!(index.module_for_source_path("Sole"), None);
    }

    /// The same three answers as bytes, through the renderer that asks the
    /// question — including the one that must **not** appear.
    #[test]
    fn an_unresolved_source_path_stays_a_code_span() {
        let mut builder = NameIndex::builder();
        for module in [
            "Pkg.Deep.Sub.Thing",
            "Dep.Sub.Thing",
            "Other.X.Amb",
            "P.X.Amb",
        ] {
            builder.module_name(module);
        }
        let index = builder.build(
            LinkIndex::default(),
            ExternalLinks::new([("Dep", "https://host/o/dep/blob/abc")]),
        );
        let render = |md: &str| PageLinks::new(&index, "../", &[]).renderer().docstring(md);

        assert_eq!(
            render("`Deep/Sub/Thing.lean`\n"),
            "<p><code><a href=\"../Pkg/Deep/Sub/Thing.html\">Deep/Sub/Thing.lean</a></code></p>"
        );
        // Into a dependency: the pinned source, like every other module link
        // (M7-c).
        assert_eq!(
            render("`Dep/Sub/Thing.lean`\n"),
            "<p><code><a href=\"https://host/o/dep/blob/abc/Dep/Sub/Thing.lean\">\
             Dep/Sub/Thing.lean</a></code></p>"
        );
        // Ambiguous, and unknown: text, not a guess. `autoLinkInline`'s second
        // lookup asks about `lean` and gets nothing either.
        assert_eq!(render("`X/Amb.lean`\n"), "<p><code>X/Amb.lean</code></p>");
        assert_eq!(
            render("`Nope/Missing.lean`\n"),
            "<p><code>Nope/Missing.lean</code></p>"
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
