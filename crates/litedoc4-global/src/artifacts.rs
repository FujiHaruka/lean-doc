//! The whole-package artifacts, derived from [`ModuleFacts`].
//!
//! ```text
//! declarations/name-map.json  name -> module, flat, declarations and dependencies merged
//! index.html                  the front page: the package, its size, every module
//! 404.html                    what GitHub Pages serves for anything else
//! search.html                 where the top bar's form submits
//! foundational_types.html     what every `Sort` in every signature links to
//! modules.json                every module, its page, and what imports it
//! search-index.bin            every declaration, and the kind vocabulary
//! instances.json              the two instance maps
//! ```
//!
//! # M8-d removed five files and their reader in the same step
//!
//! Up to M7 this module wrote six artifacts, transcribed from
//! `experiments/stage7h/global.ts:277-361` (frozen) because the acceptance
//! oracle was byte equality with doc-gen4's own build. Five of the six existed
//! **only** for doc-gen4's six JavaScript files, and M8-c replaced those with
//! `assets/app.js`, so the files went with their reader
//! (`docs/plans/ui-redesign.md` §8):
//!
//! | | why it is gone |
//! |---|---|
//! | `declarations/declaration-data.bmp` | 1,216,017 B【実測】read by `search.js` / `declaration-data.js` and nothing else — **and already broken**: it never carried `instancesFor` or `modules[].url`, so the deployed site's Instances For (245 pages) and Imported By (432 pages) did nothing【実測 2026-08-16】 |
//! | `navbar.html` | 57,949 B【実測】for an `<iframe>` nav that 決定 4 replaced with `modules.json` |
//! | `tactics.html` | one sentence with two counts in it; this package declares **0** tactics【実測】 |
//! | `references.bib` | always empty — written so that a link to it was not a 404, and the link is gone |
//! | `references.html` | constant HTML, same link |
//!
//! **`declarations/name-map.json` stays**, and it is the one that has nothing to
//! do with the UI: the incremental pipeline reads it back as `--before` to
//! compute the whole-package map delta (M2-b). Deleting it would leave the delta
//! comparing the new map with nothing and silently re-rendering too little.
//!
//! So the byte-reproduction denominator moves with the file list: **432 pages +
//! 6 artifacts = 438** was M6's, M8-d's is **432 pages + 7 artifacts = 439**,
//! and `docs/plans/search-v2.md` P0 makes it **432 + 8 = 440** by splitting
//! `instances.json` out of the search index (plus the 3 static assets, which
//! have never been in it — `tools/build-gate.sh` counts a tree that does have
//! them, which is why its number is 443 and not 440). The old numbers are not
//! rewritten — see `docs/milestone-log.md`.
//!
//! # Every sort here is UTF-16 (plan §7, U1)
//!
//! The prototype sorts with an argument-less `Array.prototype.sort()`, and the
//! bytes of `name-map.json` are that order. `Vec<String>::sort()` is UTF-8 byte
//! order, which agrees with it throughout the BMP and inverts at U+10000: `𝒜`
//! (U+1D49C) sorts *below* `ﬀ` (U+FB00) in UTF-16 and above it by code point. So
//! every sort goes through [`cmp_utf16`], including the ones in the new files —
//! not because anything compares them with the prototype, but because a site
//! with two orders in it is a site whose order is nobody's.
//!
//! # `serde_json`'s `preserve_order` is load-bearing
//!
//! Every JSON object here is built by inserting into a `Map` in explicitly
//! sorted order. A `BTreeMap` would re-sort — by code point, undoing the
//! paragraph above — and `name-map.json` would additionally lose the
//! interleaving of declaration and dependency names. The feature is set on the
//! workspace dependency; this module's `preserve_order_is_enabled` test fails if
//! it is ever dropped as unused. (Not a link: `#[cfg(test)]` items are invisible
//! to rustdoc.)

use std::collections::{BTreeMap, HashMap, HashSet};

use litedoc4_ir::{DepMap, cmp_utf16};
use litedoc4_render::{SiteMeta, css_kind};
use serde_json::{Map, Value};

use crate::entry;
use crate::facts::ModuleFacts;
use crate::search_index;

/// The renderer's path rule: dots become directory separators.
///
/// A URL path, so the separator is `/` on every platform. It has to agree with
/// `litedoc4_render::page_path`, which returns a `PathBuf` for the filesystem
/// side of the same rule; `tests/global.rs` checks that it does.
#[must_use]
pub fn page_path(module: &str) -> String {
    let mut path = litedoc4_ir::module_path(module);
    path.push_str(".html");
    path
}

/// The eight files, as bytes, before anything is written.
///
/// Held in memory rather than streamed: the largest is a few hundred kilobytes
/// on the target package【実測】, and having them as values is what lets the
/// tests compare them without a filesystem.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Artifacts {
    pub name_map_json: String,
    pub index_html: String,
    pub not_found_html: String,
    pub search_html: String,
    pub foundational_types_html: String,
    /// What `app.js` draws the module tree and the "Imported by" block from.
    /// Fetched on **every** page, so it carries names and indices and nothing
    /// else (決定 4).
    pub modules_json: String,
    /// What `app.js` searches. Fetched on the first keystroke, never before
    /// (決定 5). Carries the declarations and the kind vocabulary and nothing
    /// else — module names come from [`Artifacts::modules_json`], which is
    /// already on the page.
    ///
    /// Bytes rather than JSON since `docs/plans/search-v2.md` P1: see
    /// [`crate::search_index`] for the layout and for why.
    pub search_index_bin: Vec<u8>,
    /// The two instance maps, fetched only when a reader opens one of the two
    /// `<details>` blocks. Split from the search index in P0 of
    /// `docs/plans/search-v2.md`.
    pub instances_json: String,
    /// The same `name -> module` map [`Artifacts::name_map_json`] is the
    /// serialisation of, as data.
    ///
    /// This is the delta's `after` side. The prototype's stage 5 read
    /// `name-map.json` back off disk because the delta was a second process, and
    /// says so: doing that here "would be the only way to get a *different*
    /// answer" than the map just written. A `BTreeMap` rather than an ordered
    /// one because nothing reads it in order — [`crate::Delta`] wants membership
    /// and lookup, and does its own UTF-16 sorting on the way out.
    pub name_map: BTreeMap<String, String>,
    pub counts: Counts,
}

/// What the derivation counted on its way through, for [`crate::GlobalSummary`].
///
/// **These used to be read back out of `declaration-data.bmp`** — the summary
/// parsed the file it had just written rather than recounting its inputs, so
/// that a number quoted in a document could not disagree with the file it was
/// about. That file is gone, so the derivation reports its own sizes instead and
/// this module's `the_counts_are_what_the_files_hold` test is what keeps the two
/// honest.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    /// Distinct declaration names in the package.
    pub declarations: usize,
    /// Distinct names the dependency slices contribute.
    pub dependency_names: usize,
    /// Keys of `instances.json`'s `instances`.
    pub instance_classes: usize,
    /// Keys of `instances.json`'s `instancesFor`.
    pub instance_types: usize,
}

/// The order the artifacts are listed and written in, and the paths they take
/// under the site root.
pub const ARTIFACT_PATHS: [&str; 8] = [
    "declarations/name-map.json",
    "index.html",
    "404.html",
    "search.html",
    "foundational_types.html",
    "modules.json",
    "search-index.bin",
    "instances.json",
];

impl Artifacts {
    /// Derives all eight from the facts of every module, in index order, and
    /// the dependency slices, in index order.
    ///
    /// **Index order is behaviour, twice.** Two modules declaring the same name
    /// leave the later one in the map, and a module's importer list is built in
    /// it (before being sorted). Passing the facts in any other order is a
    /// different answer.
    #[must_use]
    pub fn derive(facts: &[ModuleFacts], dep_maps: &[DepMap]) -> Self {
        // name -> (module, kind). Last writer wins, which is why this is fed in
        // index order; `render.ts` resolves the same collision the same way.
        let mut name_map: HashMap<&str, (&str, &str)> = HashMap::new();
        let mut instances: HashMap<&str, Vec<&str>> = HashMap::new();
        let mut instances_for: HashMap<&str, Vec<&str>> = HashMap::new();
        for facts in facts {
            for (name, kind) in &facts.decls {
                name_map.insert(name, (&facts.module, kind));
            }
            for (class, name) in &facts.instances {
                instances.entry(class).or_default().push(name);
            }
            for (ty, name) in &facts.instances_for {
                instances_for.entry(ty).or_default().push(name);
            }
        }

        let own: HashSet<&str> = facts.iter().map(|facts| facts.module.as_str()).collect();
        let mut imported_by: HashMap<&str, Vec<&str>> =
            own.iter().map(|module| (*module, Vec::new())).collect();
        for facts in facts {
            for import in &facts.imports {
                // Imports of packages outside this one are dropped: the artifact
                // is "who in *this* package imports me".
                if let Some(importers) = imported_by.get_mut(import.as_str()) {
                    importers.push(&facts.module);
                }
            }
        }

        // The module array both JSON files index into. One order, computed once:
        // `modules.json`'s `i` and `search-index.json`'s third column are
        // subscripts, and two orders would be two different files agreeing by
        // accident.
        let own_sorted = sorted(own.iter().copied());
        let pages: Vec<(&str, String)> = own_sorted
            .iter()
            .map(|module| (*module, page_path(module)))
            .collect();
        let at: HashMap<&str, usize> = own_sorted
            .iter()
            .enumerate()
            .map(|(i, module)| (*module, i))
            .collect();

        // The dependency half: name -> module for the constants this package
        // refers to from its dependencies. Later slices overwrite earlier ones.
        let mut deps: HashMap<&str, &str> = HashMap::new();
        for map in dep_maps {
            for (name, module) in &map.declarations {
                deps.insert(name, module);
            }
        }

        let sorted_names = sorted(name_map.keys().copied());
        let dep_names = sorted(deps.keys().copied());

        // `[...sortedNames, ...depNames].sort()`: the two lists are concatenated
        // *before* sorting, so a name in both appears twice and the second
        // insertion only overwrites the first's value with the same value. A
        // declaration always wins over a dependency slice.
        let mut merged: Vec<&str> = sorted_names.clone();
        merged.extend(dep_names.iter().copied());
        merged.sort_by(|a, b| cmp_utf16(a, b));
        let mut flat_map: BTreeMap<String, String> = BTreeMap::new();
        let flat = object(merged.into_iter().map(|name| {
            let module = match name_map.get(name) {
                Some((module, _)) => *module,
                None => deps[name],
            };
            flat_map.insert(name.to_owned(), module.to_owned());
            (name.to_owned(), Value::String(module.to_owned()))
        }));

        let modules_json = Value::Object(
            [(
                "modules".to_owned(),
                Value::Array(
                    pages
                        .iter()
                        .map(|(module, page)| {
                            // The importers, as subscripts into this same array.
                            // **The direction is the whole point**: `i` is who
                            // imports *this* module, so a page's "Imported by"
                            // block is a lookup rather than a scan of all 432.
                            let mut importers: Vec<usize> =
                                sorted(imported_by[module].iter().copied())
                                    .into_iter()
                                    .map(|importer| at[importer])
                                    .collect();
                            importers.dedup();
                            Value::Object(
                                [
                                    ("n".to_owned(), Value::String((*module).to_owned())),
                                    ("p".to_owned(), Value::String(page.clone())),
                                    (
                                        "i".to_owned(),
                                        Value::Array(
                                            importers.into_iter().map(index_value).collect(),
                                        ),
                                    ),
                                ]
                                .into_iter()
                                .collect::<Map<String, Value>>(),
                            )
                        })
                        .collect(),
                ),
            )]
            .into_iter()
            .collect::<Map<String, Value>>(),
        );

        // The kind strings, as the declaration headers spell them: `css_kind`
        // maps the IR's `definition` to `def` and `class_inductive` to `class`,
        // and a search result whose badge disagrees with the page it leads to is
        // a badge nobody trusts.
        let mut kinds: Vec<&str> = sorted_names
            .iter()
            .map(|name| css_kind(name_map[name].1))
            .collect();
        kinds.sort_by(|a, b| cmp_utf16(a, b));
        kinds.dedup();
        let kind_at: HashMap<&str, usize> = kinds
            .iter()
            .enumerate()
            .map(|(i, kind)| (*kind, i))
            .collect();

        // Every declared name, including the ones no page has an entry for
        // (constructors, and whatever `Suppressed` drops). That is the same
        // population `name-map.json` has and the same one doc-gen4's own
        // `declarations` had; narrowing it here would make the search index and
        // the map two different answers to "what does this package declare".
        let entries: Vec<search_index::Entry<'_>> = sorted_names
            .iter()
            .map(|name| {
                let (module, kind) = name_map[name];
                search_index::Entry {
                    name,
                    kind: kind_at[css_kind(kind)],
                    module: at[module],
                }
            })
            .collect();

        let instances_out = name_lists(&instances);
        let instances_for_out = name_lists(&instances_for);
        let counts = Counts {
            declarations: sorted_names.len(),
            dependency_names: dep_names.len(),
            instance_classes: instances.len(),
            instance_types: instances_for.len(),
        };

        // **No `modules` array here.** `modules.json` already carries one, in
        // the same order and with the same subscripts — both are built from the
        // one `own_sorted` above, and `app.js` fetches that file on every page
        // for the module tree. A second copy is 12.8% of this file【実測
        // 2026-08-19: 51,975 of 405,402 B】and a second thing to disagree with.
        let search_index_bin = search_index::encode(&entries, &kinds);

        // The instance maps leave with them. A search never reads either, so
        // carrying them here made every first keystroke pay for a block most
        // readers never open.
        let instances_json = object([
            ("instances".to_owned(), instances_out),
            ("instancesFor".to_owned(), instances_for_out),
        ]);

        let site = SiteMeta::of_modules(own_sorted.iter().copied());
        Self {
            name_map_json: to_json(&flat),
            index_html: entry::index_html(&site, &pages, counts.declarations),
            not_found_html: entry::not_found_html(&site),
            search_html: entry::search_html(&site),
            foundational_types_html: entry::foundational_types_html(&site),
            modules_json: to_json(&modules_json),
            search_index_bin,
            instances_json: to_json(&instances_json),
            name_map: flat_map,
            counts,
        }
    }

    /// The eight files paired with the paths they go to, in [`ARTIFACT_PATHS`]
    /// order.
    #[must_use]
    pub fn files(&self) -> [(&'static str, &[u8]); 8] {
        [
            (ARTIFACT_PATHS[0], self.name_map_json.as_bytes()),
            (ARTIFACT_PATHS[1], self.index_html.as_bytes()),
            (ARTIFACT_PATHS[2], self.not_found_html.as_bytes()),
            (ARTIFACT_PATHS[3], self.search_html.as_bytes()),
            (ARTIFACT_PATHS[4], self.foundational_types_html.as_bytes()),
            (ARTIFACT_PATHS[5], self.modules_json.as_bytes()),
            (ARTIFACT_PATHS[6], self.search_index_bin.as_slice()),
            (ARTIFACT_PATHS[7], self.instances_json.as_bytes()),
        ]
    }
}

/// `{ key: [name, …] }` with both levels in UTF-16 order and the names
/// deduplicated.
///
/// **The deduplication is doc-gen4's, not the prototype's.** doc-gen4 collects
/// each list into an `RBTree` (`Output/ToJson.lean:53-55`), so an instance whose
/// class application names the same type twice appears once; the prototype's
/// array `push` would have kept both. No instance of the target package has a
/// repeated type name 【実測 2026-08-16: 91 instances, 91 type names, 0
/// duplicates】, so this changes nothing here and is the rule anyway.
fn name_lists(map: &HashMap<&str, Vec<&str>>) -> Value {
    object(sorted(map.keys().copied()).into_iter().map(|key| {
        let mut names = sorted(map[key].iter().copied());
        names.dedup();
        (key.to_owned(), strings(names))
    }))
}

/// A subscript into one of the two module arrays, as JSON.
///
/// Named rather than inlined because that is what the two files' compactness
/// rests on: a module is written out once and referred to by number everywhere
/// else, which is the difference between `modules.json` being fetched on every
/// page and being too big to.
fn index_value(i: usize) -> Value {
    Value::from(i)
}

/// Sorted in UTF-16 code unit order, as `Array.prototype.sort()` is.
fn sorted<'a>(items: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
    let mut items: Vec<&str> = items.into_iter().collect();
    items.sort_by(|a, b| cmp_utf16(a, b));
    items
}

/// A JSON object whose key order is the order given.
fn object(pairs: impl IntoIterator<Item = (String, Value)>) -> Value {
    Value::Object(pairs.into_iter().collect::<Map<String, Value>>())
}

fn strings<'a>(items: impl IntoIterator<Item = &'a str>) -> Value {
    Value::Array(
        items
            .into_iter()
            .map(|item| Value::String(item.to_owned()))
            .collect(),
    )
}

/// `JSON.stringify` with no spacing. Serialising a `Value` tree of objects,
/// arrays, numbers and strings cannot fail.
fn to_json(value: &Value) -> String {
    serde_json::to_string(value).expect("a tree of objects, arrays and strings serialises")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Without `preserve_order` every object in the JSON artifacts comes back
    /// out in code-point order, which is neither the order this module chose nor
    /// the order the prototype wrote. The feature is a workspace dependency
    /// setting that nothing else in this crate would miss.
    #[test]
    fn preserve_order_is_enabled() {
        let value = object([
            ("z".to_owned(), Value::Null),
            ("a".to_owned(), Value::Null),
            ("\u{1D49C}".to_owned(), Value::Null),
            ("\u{FB00}".to_owned(), Value::Null),
        ]);
        assert_eq!(
            to_json(&value),
            "{\"z\":null,\"a\":null,\"\u{1D49C}\":null,\"\u{FB00}\":null}",
            "serde_json re-sorted the keys: the `preserve_order` feature is off"
        );
    }

    /// Re-inserting a key keeps its first position and takes the new value —
    /// the behaviour `name-map.json` leans on when a name is both declared and
    /// in a dependency slice.
    #[test]
    fn reinserting_a_key_keeps_its_place() {
        let mut map: Map<String, Value> = Map::new();
        map.insert("b".to_owned(), Value::String("1".to_owned()));
        map.insert("a".to_owned(), Value::String("2".to_owned()));
        map.insert("b".to_owned(), Value::String("3".to_owned()));
        assert_eq!(to_json(&Value::Object(map)), "{\"b\":\"3\",\"a\":\"2\"}");
    }

    #[test]
    fn page_paths_are_url_paths() {
        assert_eq!(page_path("Pkg"), "Pkg.html");
        assert_eq!(page_path("Pkg.A.B"), "Pkg/A/B.html");
        assert_eq!(page_path(""), ".html");
    }

    fn facts(module: &str, imports: &[&str]) -> ModuleFacts {
        ModuleFacts {
            module: module.to_owned(),
            content_hash: "0".repeat(16),
            imports: imports.iter().map(|s| (*s).to_owned()).collect(),
            tactics: 0,
            decls: Vec::new(),
            instances: Vec::new(),
            tokens: Vec::new(),
            instances_for: Vec::new(),
        }
    }

    /// A package whose three modules form a chain, so "imports" and "imported
    /// by" cannot be confused for each other by symmetry.
    fn chain() -> Vec<ModuleFacts> {
        let mut root = facts("Pkg", &[]);
        root.decls = vec![("Pkg.a".to_owned(), "definition".to_owned())];
        let mut middle = facts("Pkg.B", &["Pkg"]);
        middle.decls = vec![("Pkg.B.inst".to_owned(), "instance".to_owned())];
        middle.instances = vec![("Cls".to_owned(), "Pkg.B.inst".to_owned())];
        middle.instances_for = vec![
            ("Pkg.a".to_owned(), "Pkg.B.inst".to_owned()),
            ("Pkg.a".to_owned(), "Pkg.B.inst".to_owned()),
        ];
        let mut leaf = facts("Pkg.C", &["Pkg", "Pkg.B"]);
        leaf.decls = vec![("Pkg.C.t".to_owned(), "theorem".to_owned())];
        vec![root, middle, leaf]
    }

    fn parsed(body: &str) -> Value {
        serde_json::from_str(body).expect("the artifact is JSON")
    }

    /// **The direction of `modules[].i`.** Getting it backwards renders an
    /// "Imported by" block that lists the module's imports — markup that is
    /// well formed, styled, populated and wrong.
    #[test]
    fn the_module_index_lists_importers_not_imports() {
        let artifacts = Artifacts::derive(&chain(), &[]);
        let json = parsed(&artifacts.modules_json);
        let modules = json["modules"].as_array().expect("an array of modules");
        let names: Vec<&str> = modules
            .iter()
            .map(|m| m["n"].as_str().expect("a name"))
            .collect();
        assert_eq!(names, ["Pkg", "Pkg.B", "Pkg.C"]);
        assert_eq!(modules[0]["p"], "Pkg.html");
        assert_eq!(modules[1]["p"], "Pkg/B.html");

        let importers = |i: usize| -> Vec<&str> {
            modules[i]["i"]
                .as_array()
                .expect("an array of subscripts")
                .iter()
                .map(|at| names[usize::try_from(at.as_u64().expect("a subscript")).expect("fits")])
                .collect()
        };
        // `Pkg` is imported by both of the others; `Pkg.C` imports both and is
        // imported by nobody. The two are not each other's mirror image, which
        // is what makes this test able to fail.
        assert_eq!(importers(0), ["Pkg.B", "Pkg.C"]);
        assert_eq!(importers(1), ["Pkg.C"]);
        assert_eq!(importers(2), Vec::<&str>::new());
    }

    /// The shape `assets/app.js` reads, field by field.
    #[test]
    fn the_search_index_is_the_shape_the_script_reads() {
        let artifacts = Artifacts::derive(&chain(), &[]);
        let index = search_index::decode(&artifacts.search_index_bin)
            .expect("a file this crate just wrote");
        // The module array is `modules.json`'s, and only its. The subscripts
        // below index into it from the other file.
        let modules_json = parsed(&artifacts.modules_json);
        let modules = modules_json["modules"].as_array().expect("modules");
        assert_eq!(modules.len(), 3);

        assert_eq!(
            index.labels,
            ["def", "instance", "theorem"],
            "the IR's `definition` reaches the index as the badge the page shows"
        );
        assert_eq!(index.names.len(), index.kind_of.len());
        assert_eq!(index.names.len(), index.modules.len());
        for (kind, module) in index.kind_of.iter().zip(&index.modules) {
            assert!(*kind < index.labels.len());
            assert!(*module < modules.len());
        }
        // `Pkg.B.inst` lives in `Pkg.B`, which is module 1.
        let at = index
            .names
            .iter()
            .position(|name| name == "Pkg.B.inst")
            .expect("the instance is indexed");
        assert_eq!(index.modules[at], 1);
        assert_eq!(index.labels[index.kind_of[at]], "instance");

        let instances = parsed(&artifacts.instances_json);
        assert_eq!(
            instances["instances"]["Cls"],
            serde_json::json!(["Pkg.B.inst"])
        );
        assert_eq!(
            instances["instancesFor"]["Pkg.a"],
            serde_json::json!(["Pkg.B.inst"]),
            "a type named twice by one instance lists it once (doc-gen4's RBTree)"
        );
    }

    /// The summary's numbers against the files they are about — the invariant
    /// the old "read it back off the artifact" trick used to give for free.
    #[test]
    fn the_counts_are_what_the_files_hold() {
        let artifacts = Artifacts::derive(&chain(), &[]);
        let instances = parsed(&artifacts.instances_json);
        assert_eq!(
            artifacts.counts.declarations,
            search_index::decode(&artifacts.search_index_bin)
                .expect("a file this crate just wrote")
                .names
                .len()
        );
        assert_eq!(
            artifacts.counts.instance_classes,
            instances["instances"].as_object().expect("instances").len()
        );
        assert_eq!(
            artifacts.counts.instance_types,
            instances["instancesFor"]
                .as_object()
                .expect("instancesFor")
                .len()
        );
        assert_eq!(artifacts.counts.dependency_names, 0);
        assert_eq!(
            artifacts.counts.declarations + artifacts.counts.dependency_names,
            artifacts.name_map.len()
        );
    }

    /// Every path is written, distinct, relative and inside the site root.
    #[test]
    fn the_file_list_and_the_paths_agree() {
        let artifacts = Artifacts::derive(&chain(), &[]);
        let files = artifacts.files();
        assert_eq!(files.len(), ARTIFACT_PATHS.len());
        for (i, (path, body)) in files.iter().enumerate() {
            assert_eq!(*path, ARTIFACT_PATHS[i]);
            assert!(!body.is_empty(), "{path} is empty");
            assert!(!path.starts_with('/') && !path.contains(".."), "{path}");
        }
        let mut paths: Vec<&str> = ARTIFACT_PATHS.to_vec();
        paths.sort_unstable();
        paths.dedup();
        assert_eq!(
            paths.len(),
            ARTIFACT_PATHS.len(),
            "two artifacts share a path"
        );
    }

    /// The five files M8-d stopped writing. Named here so that a revert is a
    /// failure rather than a surprise in a deployment.
    #[test]
    fn the_doc_gen4_only_artifacts_are_gone() {
        let artifacts = Artifacts::derive(&chain(), &[]);
        for dropped in [
            "declarations/declaration-data.bmp",
            "navbar.html",
            "tactics.html",
            "references.bib",
            "references.html",
        ] {
            assert!(
                !ARTIFACT_PATHS.contains(&dropped),
                "{dropped} is being written again"
            );
        }
        assert!(
            ARTIFACT_PATHS.contains(&"declarations/name-map.json"),
            "the map delta's `--before` file is not written any more"
        );
        assert!(!artifacts.name_map_json.is_empty());
    }

    /// U1 where it can still be seen: a name above the BMP sorts before one
    /// inside it in every list this module builds.
    #[test]
    fn the_new_files_sort_in_utf16_order_too() {
        let mut above = facts("Pkg.\u{1D49C}", &[]);
        above.decls = vec![("Pkg.\u{1D49C}.a".to_owned(), "definition".to_owned())];
        let mut inside = facts("Pkg.\u{FB00}", &[]);
        inside.decls = vec![("Pkg.\u{FB00}.a".to_owned(), "definition".to_owned())];
        let artifacts = Artifacts::derive(&[inside, above], &[]);
        // The binary index carries its names as UTF-8, so the same search for
        // the two characters answers the same question about it.
        let index = String::from_utf8_lossy(&artifacts.search_index_bin).into_owned();
        for body in [&artifacts.modules_json, &index] {
            let astral = body.find("1D49C").or_else(|| body.find('\u{1D49C}'));
            let ligature = body.find("FB00").or_else(|| body.find('\u{FB00}'));
            assert!(
                astral < ligature,
                "the astral name did not sort first: {body}"
            );
        }
    }
}
