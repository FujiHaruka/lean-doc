//! Where a **dependency's** source lives: a version-pinned GitHub blob URL.
//!
//! Milestone **M7-b** — the value type; the resolver that fills it is
//! `lean-doc`'s `packages` module. **M7-c** added [`ExternalLinks::href`], which
//! is the one place in this crate that decides between a blob URL and a relative
//! page link, and wired the map into every call site that builds a link to
//! another module (`docs/implementation-plan.md` §M7).
//!
//! # The rule is doc-gen4's, not a new one
//!
//! Every page of the reference tree already carries the URL this builds
//! (`docs/implementation-plan.md` §M7)【実測】:
//!
//! ```text
//! Mathlib/Order/Basic.html : …/mathlib4/blob/fabf563a7c95…/Mathlib/Order/Basic.lean#L67-L67
//! Init/Prelude.html        : …/lean4/blob/68218e876d2a…/src/Init/Prelude.lean#L26-L27
//! ```
//!
//! So the only thing a lookup needs is **which prefix a module's first component
//! belongs to**. `Mathlib.Order.Basic` and `Archive.Wiedijk100Theorems.Konigsberg`
//! are both mathlib's, and their prefix is the manifest's `url` plus its 40-hex
//! `rev`; `Init.Prelude` is core's, and core's prefix carries a `/src` on the end
//! because that is where the lean4 checkout keeps its libraries. Nothing here
//! decides any of that — it is a map somebody else resolved, and the
//! **path within it is [`crate::module_source_url`]**, the same function the
//! page's own `gh_nav_link` is built with.
//!
//! # Why the map has an identity
//!
//! Its contents reach every rendered page, so it is an input to the render key
//! exactly as the dependency map's bytes are (`lean_doc_incr::render_key`). A
//! bumped dependency moves a `rev`, which moves the href of every link into that
//! dependency, and a run whose only changed input was this map has to re-render
//! rather than report success — the same failure M5-b closed for the `.lidx`.
//! [`ExternalLinks::digest`] is what the ledger records.
//!
//! # What is deliberately *not* here
//!
//! The target package's own root. M7 changes dependency links only
//! (`docs/implementation-plan.md` §M7「自パッケージのリンクを巻き込まない」), and a
//! map that does not hold the root cannot resolve it — the omission is
//! structural rather than a rule at the call site.
//!
//! # A dependency with no `/blob/<rev>` is a **third** state, not the first one
//!
//! Absence used to carry both "this package's own module" and "a dependency
//! nothing could be resolved for", and the second one is a bug: a `path`
//! dependency, or one pinned at a tag, has no version-pinned URL, so the
//! resolver dropped it, so the map did not hold its root, so every link into it
//! became a **relative link to a page this site never writes**. That is a dead
//! link, and 実測 2026-08-17 it was three of them per shape — the import list,
//! a docstring's name reference and a signature's constant — on a fixture whose
//! `lake-manifest.json` declares `{"type": "path", …}`.
//!
//! So such a root is carried **with an empty base**, and [`ExternalLinks::href`]
//! answers `None` for it: no link at all. The rule is
//! [`crate::autolink::NameIndex::module_for_source_path`]'s, applied one level
//! up — *a link to the wrong page is worse than no link*. This is **not** the
//! case §M7「行範囲が取れない宣言でリンクを消さない」 refuses, which is about a
//! declaration with no line range inside a package that *is* pinned; there the
//! file's URL is still right.

use sha2::{Digest, Sha256};

use crate::autolink::module_link;
use crate::frame::module_source_url;

/// The first line of the canonical serialization [`ExternalLinks::digest`]
/// hashes.
///
/// Present so that a later change to the *shape* of that serialization moves the
/// digest even for the empty map, which otherwise has no bytes to move.
pub const DIGEST_MARKER: &str = "lean-doc external-links v1\n";

/// Module root component -> the `…/blob/<rev>` prefix its source lives under,
/// **or the empty string** when the root belongs to a dependency this run could
/// not version-pin (see the heading's third state).
///
/// A `Vec` rather than a map: it holds one entry per dependency package plus
/// core — 19 on the measurement target — every lookup is one pass over it, and
/// the insertion order is the caller's to choose and worth keeping for a log
/// line. Duplicate roots are dropped on construction, **first one wins**.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExternalLinks {
    roots: Vec<(String, String)>,
}

impl ExternalLinks {
    /// The map, in the order given, with a repeated root dropped.
    ///
    /// First-wins rather than last-wins because the caller orders the entries by
    /// authority: core's four roots are not a package's to redefine, so the
    /// resolver puts them first.
    ///
    /// **An empty `base` is a value, not a missing entry**: it says "this root
    /// is a dependency's and there is no version-pinned URL for it", which
    /// [`ExternalLinks::href`] turns into no link rather than into a relative
    /// one.
    #[must_use]
    pub fn new<K: Into<String>, V: Into<String>>(
        entries: impl IntoIterator<Item = (K, V)>,
    ) -> Self {
        let mut roots: Vec<(String, String)> = Vec::new();
        for (root, base) in entries {
            let root = root.into();
            if roots.iter().any(|(seen, _)| *seen == root) {
                continue;
            }
            // A trailing slash would produce `…/blob/<rev>//Mathlib/…`, which
            // resolves on GitHub but is not the byte the reference tree has.
            roots.push((root, base.into().trim_end_matches('/').to_owned()));
        }
        Self { roots }
    }

    /// The prefix a module root's sources live under, or `None` when this map
    /// does not know the root.
    ///
    /// `Some("")` is the third state: a root the map knows to be a dependency's
    /// and has no prefix for. Callers that want "can I build a URL" should ask
    /// [`ExternalLinks::url_for`], which folds the two into one `None`; callers
    /// that need to tell a dependency from this package's own module — there is
    /// one, [`ExternalLinks::href`] — have to ask this.
    #[must_use]
    pub fn base_for(&self, root: &str) -> Option<&str> {
        self.roots
            .iter()
            .find(|(name, _)| name == root)
            .map(|(_, base)| base.as_str())
    }

    /// The blob URL for `module`, with a line anchor when there is one.
    ///
    /// `None` for a module whose first component is not in the map — which is
    /// every module of the package being documented — **and** for one whose
    /// root is in the map with an empty base, which is a dependency with no
    /// `/blob/<rev>` to build a URL out of. The two are the same answer here
    /// (there is no URL) and different answers to [`ExternalLinks::href`] (one
    /// gets a page link, the other gets nothing).
    ///
    /// **The line anchor is optional and its absence is not a failure**
    /// (`docs/implementation-plan.md` §M7「行範囲が取れない宣言でリンクを消さない」):
    /// a declaration with no source range gets the file's URL, which is the
    /// shape doc-gen4's own `gh_nav_link` already has.
    #[must_use]
    pub fn url_for(&self, module: &str, lines: Option<(u32, u32)>) -> Option<String> {
        // The *unescaped* first component, because that is what the directory on
        // disk is called: `«Odd-Name».Inner` lives under `Odd-Name/`.
        let root = *lean_doc_ir::module_components(module).first()?;
        let base = self.base_for(root)?;
        // A root with no prefix: the resolver knew the package and could not
        // pin it. `module_source_url("", …)` would produce `/Dep/M.lean`, an
        // absolute path on whatever host the site is served from.
        if base.is_empty() {
            return None;
        }
        let mut url = module_source_url(base, module);
        if let Some((from, to)) = lines {
            url.push_str("#L");
            url.push_str(&from.to_string());
            url.push_str("-L");
            url.push_str(&to.to_string());
        }
        Some(url)
    }

    /// **The M7-c decision, and the only copy of it**: the `href` a page uses to
    /// reach `module`, anchored at the declaration `anchor` when there is one —
    /// or `None` when the honest answer is no link at all.
    ///
    /// Three states, decided by what the map holds for the module's first
    /// component:
    ///
    /// | the map | the answer | which module |
    /// |---|---|---|
    /// | no entry | `Some(<root><module path>.html#<anchor>)` | the package being documented |
    /// | a non-empty base | `Some(<base>/<module path>.lean#L…-L…)` | a version-pinned dependency |
    /// | an **empty** base | **`None`** | a dependency with no `/blob/<rev>` |
    ///
    /// The third row is the 2026-08-17 fix (see the heading). It is `None`
    /// rather than the first row's page link because that page does not exist:
    /// this site writes pages for the target package only, so a relative link
    /// into a dependency is a 404 the reader cannot tell from a real page until
    /// they follow it.
    ///
    /// **The empty map therefore reproduces the pre-M7 bytes exactly**, and so
    /// does any map with no empty-base entry. That is load-bearing rather than
    /// incidental: it is what confines M7 to dependency links
    /// (`docs/implementation-plan.md` §M7「自パッケージのリンクを巻き込まない」),
    /// and it is what keeps the frozen prototype's and doc-gen4's fixtures valid
    /// as the oracle of the *fallback* branch.
    ///
    /// The two anchors are different kinds of thing and never both appear:
    /// `#Nat.succ` names a declaration on a page this site wrote, `#L26-L27`
    /// names lines of a file on GitHub. A blob URL carrying a declaration
    /// fragment would point at nothing.
    #[must_use]
    pub fn href(
        &self,
        root: &str,
        module: &str,
        anchor: Option<&str>,
        lines: Option<(u32, u32)>,
    ) -> Option<String> {
        // Membership first, and the base's contents second: `url_for` alone
        // cannot tell the first row from the third, and they differ.
        if let Some(component) = lean_doc_ir::module_components(module).first()
            && self.base_for(component).is_some()
        {
            return self.url_for(module, lines);
        }
        let mut out = module_link(root, module);
        if let Some(anchor) = anchor {
            out.push('#');
            out.push_str(anchor);
        }
        Some(out)
    }

    /// The roots, in the order they were given.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.roots
            .iter()
            .map(|(root, base)| (root.as_str(), base.as_str()))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.roots.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }

    /// SHA-256 of [`ExternalLinks::canonical`], lower-case hex — the identity
    /// the incremental render key records.
    #[must_use]
    pub fn digest(&self) -> String {
        let digest = Sha256::digest(self.canonical().as_bytes());
        let mut hex = String::with_capacity(digest.len() * 2);
        for byte in digest {
            hex.push_str(&format!("{byte:02x}"));
        }
        hex
    }

    /// The bytes the digest is taken over: [`DIGEST_MARKER`], then one
    /// `<root>\t<base>\n` line per entry, **sorted by root**.
    ///
    /// Sorted rather than in map order because the roots are unique: two maps
    /// that resolve every module alike have to hash alike, whatever order they
    /// were built in. Byte order is enough here — unlike the sorts that reach a
    /// generated file (plan §7, U1), this string is only ever compared with
    /// another one of its own.
    ///
    /// **An empty-base root is a line like any other** — `<root>\t\n` — which
    /// is what makes it an input to the render key: a dependency that becomes
    /// pinnable, or stops being, moves the digest and re-renders the pages whose
    /// links change. It also means a map with no empty-base root hashes to
    /// exactly the bytes it hashed to before that state existed, so the ledgers
    /// written for the measurement target stay valid.
    #[must_use]
    pub fn canonical(&self) -> String {
        let mut lines: Vec<&(String, String)> = self.roots.iter().collect();
        lines.sort_by(|(a, _), (b, _)| a.cmp(b));
        let mut out = String::with_capacity(DIGEST_MARKER.len() + lines.len() * 96);
        out.push_str(DIGEST_MARKER);
        for (root, base) in lines {
            out.push_str(root);
            out.push('\t');
            out.push_str(base);
            out.push('\n');
        }
        out
    }
}

impl<K: Into<String>, V: Into<String>> FromIterator<(K, V)> for ExternalLinks {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(entries: I) -> Self {
        Self::new(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MATHLIB: &str = "https://github.com/leanprover-community/mathlib4/blob/fabf563a7c95a166b8d7b6efca11c8b4dc9d911f";
    const CORE: &str =
        "https://github.com/leanprover/lean4/blob/68218e876d2a38b1985b8590fff244a83c321783/src";

    fn links() -> ExternalLinks {
        ExternalLinks::new([("Mathlib", MATHLIB), ("Init", CORE)])
    }

    /// The two URLs `docs/implementation-plan.md` §M7 quotes off the reference
    /// tree, built from the map rather than read out of a page.
    #[test]
    fn the_two_urls_the_plan_quotes_come_out_of_the_map() {
        assert_eq!(
            links()
                .url_for("Mathlib.Order.Basic", Some((67, 67)))
                .unwrap(),
            format!("{MATHLIB}/Mathlib/Order/Basic.lean#L67-L67")
        );
        assert_eq!(
            links().url_for("Init.Prelude", None).unwrap(),
            format!("{CORE}/Init/Prelude.lean")
        );
    }

    #[test]
    fn a_root_the_map_does_not_hold_resolves_to_nothing() {
        // The package being documented is exactly this case (see the heading).
        assert_eq!(links().url_for("InformationTheory.Shannon", None), None);
        assert_eq!(links().url_for("", None), None);
        assert_eq!(
            ExternalLinks::default().url_for("Mathlib.Order", None),
            None
        );
        assert!(ExternalLinks::default().is_empty());
    }

    /// A one-component module is a page too — `Mathlib.html` is the root file.
    #[test]
    fn a_root_module_is_the_file_next_to_its_directory() {
        assert_eq!(
            links().url_for("Mathlib", None).unwrap(),
            format!("{MATHLIB}/Mathlib.lean")
        );
    }

    /// The path is the *source* path, so a quoted component loses its guillemets
    /// — the same rule [`module_source_url`] follows (M5-b).
    #[test]
    fn a_quoted_component_is_unescaped_in_the_path_and_in_the_lookup() {
        let odd = ExternalLinks::new([("Odd-Name", "https://host/o/r/blob/abc")]);
        assert_eq!(
            odd.url_for("«Odd-Name».Inner", None).unwrap(),
            "https://host/o/r/blob/abc/Odd-Name/Inner.lean"
        );
    }

    /// [`ExternalLinks::href`]'s two *linking* branches, side by side, with the
    /// same arguments: into a dependency it is the blob URL, and into the
    /// package being documented it is the relative page link.
    #[test]
    fn a_link_into_a_dependency_is_a_blob_url_and_one_into_this_site_is_not() {
        let links = links();
        assert_eq!(
            links
                .href(
                    ".././",
                    "Mathlib.Order.Basic",
                    Some("LE.ext"),
                    Some((67, 67))
                )
                .unwrap(),
            format!("{MATHLIB}/Mathlib/Order/Basic.lean#L67-L67"),
            "the declaration fragment is replaced by the line anchor, not appended"
        );
        // No range: the file's URL, which is the shape `gh_nav_link` already has
        // (§M7「行範囲が取れない宣言でリンクを消さない」).
        assert_eq!(
            links
                .href(".././", "Mathlib.Order.Basic", Some("LE.ext"), None)
                .unwrap(),
            format!("{MATHLIB}/Mathlib/Order/Basic.lean")
        );
        // A module link — the import list, and `nameToLink`'s third branch.
        assert_eq!(
            links.href(".././", "Init.Prelude", None, None).unwrap(),
            format!("{CORE}/Init/Prelude.lean")
        );
        // The package being documented: unchanged, anchor and all.
        assert_eq!(
            links
                .href(".././", "Pkg.Two", Some("Pkg.Two.a"), Some((5, 5)))
                .unwrap(),
            ".././Pkg/Two.html#Pkg.Two.a",
            "a range must not turn an own-package link into anything else"
        );
    }

    /// **The third state** 【実測 2026-08-17】: a root the map holds with an
    /// empty base is a dependency nothing could pin, and every shape of link
    /// into it is `None` — not the relative page link, which is the 404 this
    /// state exists to stop.
    ///
    /// The two roots either side of it are in the same map, so a change that
    /// answered `None` too widely fails here rather than somewhere downstream.
    #[test]
    fn a_dependency_with_no_pinned_base_gets_no_link_at_all() {
        let mixed = ExternalLinks::new([("Mathlib", MATHLIB), ("Dep", "")]);
        // The three call shapes: a declaration with a range, a declaration
        // without one, and a module.
        assert_eq!(
            mixed.href("../", "Dep.Aux", Some("Dep.f"), Some((3, 4))),
            None
        );
        assert_eq!(mixed.href("../", "Dep.Aux", Some("Dep.f"), None), None);
        assert_eq!(mixed.href("../", "Dep", None, None), None);
        // …and no URL either, which is what the `.lidx` oracle asks.
        assert_eq!(mixed.url_for("Dep.Aux", Some((3, 4))), None);
        // The root is in the map — `None` here would mean the resolver had
        // dropped it, which is the bug rather than the fix.
        assert_eq!(mixed.base_for("Dep"), Some(""));
        assert_eq!(mixed.len(), 2);
        // Its neighbours are untouched, in both directions.
        assert_eq!(
            mixed.href("../", "Mathlib.Order", None, None).unwrap(),
            format!("{MATHLIB}/Mathlib/Order.lean")
        );
        assert_eq!(
            mixed
                .href("../", "Pkg.Two", Some("Pkg.Two.a"), None)
                .unwrap(),
            "../Pkg/Two.html#Pkg.Two.a"
        );
    }

    /// **The property M7-c is confined by**: with no map, every one of those
    /// answers is the byte the renderer wrote before M7.
    #[test]
    fn the_empty_map_reproduces_the_relative_link_for_everything() {
        let none = ExternalLinks::default();
        assert_eq!(
            none.href(
                ".././",
                "Mathlib.Order.Basic",
                Some("LE.ext"),
                Some((67, 67))
            )
            .unwrap(),
            ".././Mathlib/Order/Basic.html#LE.ext"
        );
        assert_eq!(
            none.href("./", "Init.Prelude", None, None).unwrap(),
            "./Init/Prelude.html"
        );
        assert_eq!(
            none.href("", "Foo", Some("Foo.f"), None).unwrap(),
            "Foo.html#Foo.f"
        );
    }

    #[test]
    fn a_trailing_slash_on_a_base_is_dropped() {
        let one = ExternalLinks::new([("Mathlib", format!("{MATHLIB}/"))]);
        assert_eq!(one.base_for("Mathlib"), Some(MATHLIB));
    }

    #[test]
    fn a_repeated_root_keeps_the_first() {
        let two = ExternalLinks::new([("Init", CORE), ("Init", MATHLIB)]);
        assert_eq!(two.len(), 1);
        assert_eq!(two.base_for("Init"), Some(CORE));
    }

    /// The digest is a function of what the map *resolves*, not of how it was
    /// built — otherwise a resolver that reorders its scan would re-render every
    /// page for nothing.
    #[test]
    fn the_digest_ignores_insertion_order_and_moves_with_a_revision() {
        let forward = ExternalLinks::new([("Mathlib", MATHLIB), ("Init", CORE)]);
        let backward = ExternalLinks::new([("Init", CORE), ("Mathlib", MATHLIB)]);
        assert_eq!(forward.digest(), backward.digest());
        assert_ne!(forward.iter().next(), backward.iter().next());

        let bumped = ExternalLinks::new([
            ("Mathlib", MATHLIB.replace("fabf563", "0000000")),
            ("Init", CORE.to_owned()),
        ]);
        assert_ne!(forward.digest(), bumped.digest());
        assert_ne!(forward.digest(), ExternalLinks::default().digest());
    }

    /// **The digest did not move on 2026-08-17**, when the empty-base state was
    /// added: a map with no empty-base root hashes to the byte it hashed to
    /// before, so every ledger written for the measurement target still matches
    /// and nothing re-renders for a change that cannot reach its pages.
    ///
    /// The two values are **not** this implementation's output copied down —
    /// they are `shasum -a 256` of the canonical bytes, which is an oracle
    /// outside this crate. Copying the code's own answer would make the test
    /// pass for any change to the canonical form, which is the one thing it is
    /// here to catch.
    #[test]
    fn a_map_with_no_empty_base_hashes_to_the_byte_it_did_before_the_third_state() {
        assert_eq!(
            ExternalLinks::default().digest(),
            "7f9885484c3f5583dd5c84faf08abc263a2c40ca79d3888c2be92231dcceddac"
        );
        assert_eq!(
            links().digest(),
            "65c6448264fa67f93920f7ad8d5691c4a38e1e2c61fc376a47ec4f0e1ce8c5ab"
        );
        // …and an empty-base root does move it, because it changes which links
        // the pages get.
        let with_unpinned = ExternalLinks::new([("Mathlib", MATHLIB), ("Init", CORE), ("Dep", "")]);
        assert_ne!(with_unpinned.digest(), links().digest());
        assert_eq!(
            with_unpinned.canonical(),
            format!("{DIGEST_MARKER}Dep\t\nInit\t{CORE}\nMathlib\t{MATHLIB}\n"),
            "an unpinnable root is a line with an empty second field, not an absent line"
        );
    }

    #[test]
    fn the_canonical_form_is_the_marker_and_one_sorted_line_per_root() {
        assert_eq!(
            links().canonical(),
            format!("{DIGEST_MARKER}Init\t{CORE}\nMathlib\t{MATHLIB}\n")
        );
        assert_eq!(ExternalLinks::default().canonical(), DIGEST_MARKER);
        assert_eq!(ExternalLinks::default().digest().len(), 64);
    }

    #[test]
    fn the_map_collects_from_an_iterator() {
        let collected: ExternalLinks = [("Mathlib", MATHLIB)].into_iter().collect();
        assert_eq!(collected.iter().collect::<Vec<_>>(), [("Mathlib", MATHLIB)]);
    }
}
