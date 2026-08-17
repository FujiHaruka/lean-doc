//! Rendering module pages from the IR.
//!
//! Filled in by milestone **M1** — see `docs/implementation-plan.md`.
//!
//! The acceptance oracle compares bytes, so a few defaults have to be
//! overridden rather than inherited (plan §7):
//!
//! - HTML escaping covers `& < > "` and nothing else. `'` is left alone.
//! - Sorting follows UTF-16 code unit order, not UTF-8 byte order. The two
//!   disagree above U+FFFF, which is exactly where the mathematical
//!   alphanumerics live.
//! - String literals inside `<script>` follow Lean's `String.quote`.
//!
//! One interface rule: the set of modules to render is `Option<Vec<..>>`, and
//! `Some(vec![])` means *render nothing*. The prototype expressed this as the
//! presence or absence of a flag, which collapsed "empty set" into "every
//! module" and silently re-rendered all 432 pages. See plan §5.
//!
//! # What is here so far (M1-b, M1-c)
//!
//! The pieces the page builder is written on top of, each a transcription of a
//! named function in the frozen prototype (`experiments/stage7d/render.ts`):
//!
//! | | prototype | here |
//! |---|---|---|
//! | HTML escape / Lean string literal | `escapeHtml`, `leanQuote` | [`escape`] |
//! | `String.lt` / `Name.lt` | `stringLt`, `nameLt` | [`order`] |
//! | schema-3 whitespace replay | `applyWsWidths` | [`whitespace`] |
//! | the dependency closure's map | `render.ts:2067-2084` | [`link_index`] |
//! | docstring name resolution | `nameToLink`, `isNameLit`, `isLetterLike` | [`autolink`] |
//!
//! # What is here so far (M1-d1)
//!
//! [`code`] — one printed code fragment (a signature, a binder, an equation, a
//! structure field) to HTML: `buildTree`, `Renderer`, and the name handling
//! around them. It is the leaf the page builder calls, and the only part of the
//! page that resolves constants against the IR's own map rather than against
//! the dependency closure's.
//!
//! The fifth item of plan §7's list — UTF-16 code unit order — is in
//! [`lean_doc_ir::cmp_utf16`] instead: M2's global artifacts and M3's ledger
//! sort with it too, and one comparator with one set of tests is the point.
//!
//! Each of the five is checked against the prototype's own answers rather than
//! against a reading of it; see `tests/differential.rs`.
//!
//! # What is here so far (M1-d2)
//!
//! The parts a declaration's block on a page is made of, and the frame around
//! them. [`decl`] is `div.decl` — header, attributes, docstring, structure
//! members, equations, instance stubs — and [`frame`] is `<head>`, `<header>`
//! and the left-hand navigation. Both are byte-compared against the prototype
//! over the real IR (`tests/page_parts.rs`).
//!
//! # What is here so far (M1-d3)
//!
//! [`page`] assembles one module's page — which declarations get an entry
//! ([`Suppressed`], site-wide) and in what order they and the module
//! docstrings appear — and [`site`] is the run: read the IR, build the maps in
//! the order that decides what links where, write one file per wanted module.
//!
//! [`site::render_site`] is the whole of M1 as one call. Its output is
//! compared to the frozen prototype's, byte for byte, by
//! `tools/render-compare.sh`.
//!
//! # What is here so far (M7-b, M7-c)
//!
//! [`external`] — the map from a **dependency's** module root to the
//! version-pinned GitHub blob prefix its sources live under, and the lookup that
//! turns a module name into a URL. `lean-doc`'s `packages` module resolves it out
//! of the target's `lake-manifest.json` and its toolchain.
//!
//! **M7-c is where the pages use it.** Every link *into another module* — a
//! docstring autolink, a constant in a signature, an inherited structure field,
//! an entry in the import list — is built by [`NameIndex::link_to`], which is
//! the only place the choice is made:
//!
//! | the module is | the href |
//! |---|---|
//! | in the map with a `/blob/<rev>` (a pinned dependency) | `…/blob/<rev>/<path>.lean#L<from>-L<to>` |
//! | in the map with an empty base (an unpinnable dependency) | none: the name is text |
//! | not in the map, and this run wrote its page | the relative page link, unchanged |
//! | not in the map, and it has no page | none: the name is text |
//!
//! The package being documented is never in the map, so its own links do not
//! move — and **an empty map, over a run that renders every module it can name,
//! reproduces the pre-M7 bytes exactly**, which is what keeps the frozen
//! prototype's fixtures meaningful as the fallback branch's oracle
//! ([`NameIndexBuilder::build_with_a_page_for_every_module`] is that world;
//! `docs/implementation-plan.md` §1: gate A is suspended, and byte
//! compatibility with doc-gen4 is no longer claimed for dependency links).
//!
//! The last row is 2026-08-17's: a package whose `lakefile.toml` declares more
//! than one `[[lean_lib]]` has modules that this run does not render, and
//! linking to them relatively wrote an href to a file nobody created.
//!
//! # What is here so far (M8-a)
//!
//! [`assets`] — the `style.css` / `app.js` / `favicon.svg` the pages reference,
//! carried in the binary by `include_str!` and written into the site tree by
//! [`write_assets`]. Until M8-a the `<head>` named files nothing produced, so
//! the tree `lean-doc build` wrote could not be opened without hand-copying
//! doc-gen4's own assets over it (`docs/plans/ui-redesign.md` §2.2 / 決定 6).
//! **They are deliberately outside the incremental render key** — see the
//! module's own heading.

pub mod assets;
pub mod autolink;
pub mod code;
pub mod decl;
pub mod escape;
pub mod external;
pub mod frame;
pub mod link_index;
pub mod order;
pub mod page;
pub mod site;
pub mod whitespace;

pub use assets::{ASSETS, write_assets};
pub use autolink::{
    NameIndex, NameIndexBuilder, PRIVATE_PREFIX, PageLinks, is_letter_like, is_name_lit,
    module_decl_names, module_link, page_root,
};
pub use code::{
    CodeRenderer, Refs, Rendered, break_within, css_kind, decl_refs, find_linkable_parent,
    kind_description, module_from_private_prefix, private_to_user_name,
};
pub use decl::{
    DeclRenderer, EQUATION_LIMIT, UnplaceableName, class_instances_html, contained_names,
    decl_head_html, decl_name_to_link, decl_signature, equations_html, instances_for_html,
};
pub use escape::{escape_html, escape_html_into, lean_quote, lean_quote_into};
pub use external::{DIGEST_MARKER, ExternalLinks};
pub use frame::{
    SiteMeta, head_html, module_head_html, module_meta_html, module_source_url, sidebar_html,
    sorted_imports, topbar_html,
};
pub use link_index::LinkIndex;
pub use order::{cmp_name, cmp_name_components, cmp_string, name_lt, sort_names, string_lt};
pub use page::{PageItem, Suppressed, page_html, page_items, page_path};
pub use site::{ModuleSet, RenderOptions, RenderSummary, render_site};
pub use whitespace::{WsRewrite, apply_ws_widths};
