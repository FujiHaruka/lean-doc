//! The six whole-package artifacts, derived from [`ModuleFacts`].
//!
//! Ported from `experiments/stage7h/global.ts:277-361` (frozen), which is stage
//! 5's derivation unchanged — the prototype says so in its own header and keeps
//! it that way on purpose, because the oracle is byte equality with a
//! from-scratch build.
//!
//! ```text
//! declarations/declaration-data.bmp   declarations / dependencies / instances / modules
//! declarations/name-map.json          name -> module, flat, declarations and dependencies merged
//! navbar.html                         one <li> per module of this package
//! tactics.html                        a sentence with two counts in it
//! references.bib                      empty
//! references.html                     constant
//! ```
//!
//! # Every sort here is UTF-16 (plan §7, U1)
//!
//! The prototype sorts with an argument-less `Array.prototype.sort()` in seven
//! places, and the bytes of three of the six artifacts are that order.
//! `Vec<String>::sort()` is UTF-8 byte order, which agrees with it throughout
//! the BMP and inverts at U+10000: `𝒜` (U+1D49C) sorts *below* `ﬀ` (U+FB00) in
//! UTF-16 and above it by code point. So every sort goes through
//! [`cmp_utf16`], and the fixture carries a case whose names are above the BMP —
//! the target package has **no such name**【実測】, so nothing else in the suite
//! can tell the two orders apart.
//!
//! # `serde_json`'s `preserve_order` is load-bearing
//!
//! The prototype builds each JSON object by inserting into a `Record` in
//! explicitly sorted order and calling `JSON.stringify`: the key order is
//! insertion order, and insertion order is a decision made here. A `BTreeMap`
//! would re-sort — by code point, undoing the paragraph above — and
//! `name-map.json` would additionally lose the interleaving of declaration and
//! dependency names. The feature is set on the workspace dependency;
//! [`crate::artifacts::tests::preserve_order_is_enabled`] fails if it is ever
//! dropped as unused.

use std::collections::{BTreeMap, HashMap, HashSet};

use lean_doc_ir::{DepMap, cmp_utf16};
use lean_doc_md::escape_html;
use serde_json::{Map, Value};

use crate::facts::ModuleFacts;

/// The renderer's path rule: dots become directory separators.
///
/// A URL path, so the separator is `/` on every platform. It has to agree with
/// `lean_doc_render::page_path`, which returns a `PathBuf` for the filesystem
/// side of the same rule; `tests/global.rs` checks that it does.
#[must_use]
pub fn page_path(module: &str) -> String {
    let mut path = module.replace('.', "/");
    path.push_str(".html");
    path
}

/// The six files, as bytes, before anything is written.
///
/// Held in memory rather than streamed: the largest is 1.2 MB on the target
/// package【実測】, and having them as values is what lets the tests compare
/// them without a filesystem.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Artifacts {
    pub declaration_data_bmp: String,
    pub name_map_json: String,
    pub navbar_html: String,
    pub tactics_html: String,
    /// Always empty. The prototype writes the file so that a link to it from a
    /// page is not a 404; there is no bibliography to put in it.
    pub references_bib: String,
    pub references_html: String,
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
}

/// The order the artifacts are listed and written in, and the paths they take
/// under the site root.
pub const ARTIFACT_PATHS: [&str; 6] = [
    "declarations/declaration-data.bmp",
    "declarations/name-map.json",
    "navbar.html",
    "tactics.html",
    "references.bib",
    "references.html",
];

impl Artifacts {
    /// Derives all six from the facts of every module, in index order, and the
    /// dependency slices, in index order.
    ///
    /// **Index order is behaviour, twice.** Two modules declaring the same name
    /// leave the later one in the map, and a module's `importedBy` list is built
    /// in it (before being sorted). Passing the facts in any other order is a
    /// different answer.
    #[must_use]
    pub fn derive(facts: &[ModuleFacts], dep_maps: &[DepMap]) -> Self {
        // name -> (module, kind). Last writer wins, which is why this is fed in
        // index order; `render.ts` resolves the same collision the same way.
        let mut name_map: HashMap<&str, (&str, &str)> = HashMap::new();
        let mut instances: HashMap<&str, Vec<&str>> = HashMap::new();
        let mut tactics = 0usize;
        for facts in facts {
            tactics += facts.tactics;
            for (name, kind) in &facts.decls {
                name_map.insert(name, (&facts.module, kind));
            }
            for (class, name) in &facts.instances {
                instances.entry(class).or_default().push(name);
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

        let sorted_names = sorted(name_map.keys().copied());
        let declarations = object(sorted_names.iter().copied().map(|name| {
            let (module, kind) = name_map[name];
            let link = format!("./{}#{name}", page_path(module));
            (
                name.to_owned(),
                object([
                    ("docLink".to_owned(), Value::String(link)),
                    ("kind".to_owned(), Value::String(kind.to_owned())),
                ]),
            )
        }));

        let instances_out = object(sorted(instances.keys().copied()).into_iter().map(|class| {
            let names = sorted(instances[class].iter().copied());
            (class.to_owned(), strings(names))
        }));

        let own_sorted = sorted(own.iter().copied());
        let modules_out = object(own_sorted.iter().copied().map(|module| {
            let importers = sorted(imported_by[module].iter().copied());
            (
                module.to_owned(),
                object([("importedBy".to_owned(), strings(importers))]),
            )
        }));

        // The dependency half: name -> module for the constants this package
        // refers to from its dependencies. Later slices overwrite earlier ones.
        let mut deps: HashMap<&str, &str> = HashMap::new();
        for map in dep_maps {
            for (name, module) in &map.declarations {
                deps.insert(name, module);
            }
        }
        let dep_names = sorted(deps.keys().copied());
        let dependencies = object(
            dep_names
                .iter()
                .copied()
                .map(|name| (name.to_owned(), Value::String(deps[name].to_owned()))),
        );

        let bmp = object([
            ("declarations".to_owned(), declarations),
            ("dependencies".to_owned(), dependencies),
            ("instances".to_owned(), instances_out),
            ("modules".to_owned(), modules_out),
        ]);

        // `[...sortedNames, ...depNames].sort()`: the two lists are concatenated
        // *before* sorting, so a name in both appears twice and the second
        // insertion only overwrites the first's value with the same value. A
        // declaration always wins over a dependency slice.
        let mut merged: Vec<&str> = sorted_names.to_vec();
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

        let mut rows = String::new();
        for module in own_sorted.iter().copied() {
            rows.push_str("<li><a href=\"./");
            rows.push_str(&escape_html(&page_path(module)));
            rows.push_str("\">");
            rows.push_str(&escape_html(module));
            rows.push_str("</a></li>");
        }

        Self {
            declaration_data_bmp: to_json(&bmp),
            name_map_json: to_json(&flat),
            navbar_html: page(
                "Modules",
                &format!("<nav class=\"nav\"><ul>{rows}</ul></nav>"),
            ),
            tactics_html: page(
                "Tactics",
                &format!(
                    "<main><p>This package declares no tactics ({tactics} tactic docstrings \
                     across {} modules).</p></main>",
                    facts.len()
                ),
            ),
            references_bib: String::new(),
            references_html: page("References", "<main><p>No references.</p></main>"),
            name_map: flat_map,
        }
    }

    /// The six files paired with the paths they go to, in [`ARTIFACT_PATHS`]
    /// order.
    #[must_use]
    pub fn files(&self) -> [(&'static str, &str); 6] {
        [
            (ARTIFACT_PATHS[0], self.declaration_data_bmp.as_str()),
            (ARTIFACT_PATHS[1], self.name_map_json.as_str()),
            (ARTIFACT_PATHS[2], self.navbar_html.as_str()),
            (ARTIFACT_PATHS[3], self.tactics_html.as_str()),
            (ARTIFACT_PATHS[4], self.references_bib.as_str()),
            (ARTIFACT_PATHS[5], self.references_html.as_str()),
        ]
    }
}

/// The frame all four HTML artifacts share, character for character —
/// `</meta>` and `</link>` closing tags included. They are not valid HTML5 and
/// they are what doc-gen4 emits, so they are what the pages have to say.
fn page(title: &str, body: &str) -> String {
    format!(
        "<html lang=\"en\"><head><meta charset=\"UTF-8\"></meta>\
         <link rel=\"stylesheet\" href=\"./style.css\"></link><title>{title}</title></head>\
         <body>{body}</body></html>"
    )
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
/// arrays and strings cannot fail.
fn to_json(value: &Value) -> String {
    serde_json::to_string(value).expect("a tree of objects, arrays and strings serialises")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Without `preserve_order` every object in the two JSON artifacts comes
    /// back out in code-point order, which is neither the order this module
    /// chose nor the order the prototype wrote. The feature is a workspace
    /// dependency setting that nothing else in this crate would miss.
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
}
