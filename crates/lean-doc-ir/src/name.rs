//! Lean module names: the two spellings, and where each one is right.
//!
//! Milestone **M5-b**, and it exists because the second target broke on it.
//! Plan §7 (M5-a, trap 4) recorded the hazard as a note — "the `.lidx` writer
//! spells declaration names with `Name.toString` and module names without
//! escaping; the two coincide on this target and a target with `«…»` in a module
//! name would break quietly". A module named `Alpha.«Odd-Name»` was put into
//! target 2 to find out, and **it does not break quietly**: the extractor exits
//! 1 with `import failed, trying to import module with anonymous name` before
//! it imports anything 【実測 2026-08-15】.
//!
//! # The mechanism, read out of the toolchain rather than guessed
//!
//! `lean-doc modules` derives the list from a **glob over the sources**, so a
//! name is built out of path components: `Alpha/Odd-Name.lean` becomes
//! `Alpha.Odd-Name`. The extractor turns each line back into a `Name` with
//! `String.toName` (`Extract.lean:readNameList`), and that function is a
//! *parser*: `Substring.Raw.toName` splits on `.` and accepts a component only
//! if it is an identifier, an escaped `«…»`, or a numeral
//! (`Init/Meta/Defs.lean:1218-1233`). `Odd-Name` is none of the three, so the
//! whole list entry parses as `Name.anonymous` — and importing the anonymous
//! module is the error above.
//!
//! So the two spellings are not interchangeable, and which one is canonical is
//! not a choice:
//!
//! | | spelling | who fixes it |
//! |---|---|---|
//! | **the name** | `Alpha.«Odd-Name»` | the extractor writes it into `index.json` and into `modules/<name>.json` (`Extract.lean:2168`, `ToString Name` = escaping on), and `merge --modules` refuses a list that disagrees with the tree (M3-d2b, exit 3) |
//! | **the path** | `Alpha/Odd-Name` | the olean is at the source's path, and doc-gen4 builds a page path out of `Name.toString (escape := false)` components (`Output/Base.lean:188`) |
//!
//! Hence [`escape_module`], which the glob applies once, and
//! [`module_components`] / [`fn@module_path`], which every path derivation goes
//! through.
//!
//! # This is inert on the measurement target 【実測】
//!
//! All 432 of its module names are plain identifiers, so escaping and
//! unescaping are both the identity there and not one recorded byte moves.
//! `tests/name.rs` asserts that over the real module list rather than assuming
//! it.
//!
//! # What is transcribed, and from where
//!
//! [`is_id_first`] / [`is_id_rest`] / [`is_letter_like`] / [`is_subscript_alnum`]
//! are `Init/Meta/Defs.lean:98-137` of the toolchain target 2 and the
//! measurement target both pin (`leanprover/lean4:v4.31.0`). Note that Lean's
//! `Char.isAlpha` and `Char.isAlphanum` are **ASCII only** — every non-ASCII
//! identifier character comes from the two range tables, which is why they are
//! written out here rather than reached for in a Unicode crate. A crate would
//! bring a different UCD and the same class of divergence plan §5 already
//! registers for the general-category tables.

use std::borrow::Cow;

/// `Lean.isLetterLike` — `Init/Meta/Defs.lean:101-112`.
#[must_use]
pub fn is_letter_like(c: char) -> bool {
    let v = c as u32;
    (0x3b1..=0x3c9).contains(&v) && v != 0x3bb          // lower Greek, not lambda
        || (0x391..=0x3a9).contains(&v) && v != 0x3a0 && v != 0x3a3 // upper Greek, not Pi/Sigma
        || (0x3ca..=0x3fb).contains(&v)                 // Coptic
        || (0x1f00..=0x1ffe).contains(&v)               // polytonic Greek
        || (0x2100..=0x214f).contains(&v)               // letterlike symbols
        || (0x1d49c..=0x1d59f).contains(&v)             // script / double-struck / fraktur
        || (0x00c0..=0x00ff).contains(&v) && v != 0x00d7 && v != 0x00f7
        || (0x0100..=0x017f).contains(&v)
}

/// `Lean.isSubScriptAlnum` — `Init/Meta/Defs.lean:114-118`.
#[must_use]
pub fn is_subscript_alnum(c: char) -> bool {
    let v = c as u32;
    (0x2080..=0x2089).contains(&v)
        || (0x2090..=0x209c).contains(&v)
        || (0x1d62..=0x1d6a).contains(&v)
        || v == 0x2c7c
}

/// `Lean.isIdFirst`. `Char.isAlpha` is ASCII in Lean.
#[must_use]
pub fn is_id_first(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || is_letter_like(c)
}

/// `Lean.isIdRest`. `Char.isAlphanum` is ASCII in Lean.
#[must_use]
pub fn is_id_rest(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || c == '_'
        || c == '\''
        || c == '!'
        || c == '?'
        || is_letter_like(c)
        || is_subscript_alnum(c)
}

/// Whether a name component is spelled as itself by `Name.toString`.
#[must_use]
pub fn needs_no_escape(component: &str) -> bool {
    let mut chars = component.chars();
    match chars.next() {
        None => false,
        Some(first) => is_id_first(first) && chars.all(is_id_rest),
    }
}

/// One component, as `Name.toString` writes it — `Name.escapePart` with
/// `force := false` (`Init/Meta/Defs.lean:198-207`).
///
/// A component containing `»` is **not escaped**: `escapePart` returns `none`
/// there and `maybeEscape` falls back to the raw string, because wrapping it
/// would produce something that does not parse back. Transcribed, not improved
/// — the spelling this has to agree with is the extractor's, and the extractor
/// is Lean.
#[must_use]
pub fn escape_component(component: &str) -> Cow<'_, str> {
    if needs_no_escape(component) || component.contains('»') {
        Cow::Borrowed(component)
    } else {
        Cow::Owned(format!("«{component}»"))
    }
}

/// A module name built from path components, as `Name.toString` spells it.
///
/// The input is what a glob produces (`Alpha/Odd-Name.lean` → `Alpha`,
/// `Odd-Name`); the output is what the extractor, the IR's `index.json` and the
/// IR's file names all use.
pub fn escape_module<'a>(components: impl IntoIterator<Item = &'a str>) -> String {
    components
        .into_iter()
        .map(|component| escape_component(component).into_owned())
        .collect::<Vec<String>>()
        .join(".")
}

/// The components of a module name, **unescaped**: what its olean, its source
/// file and its page path are built from.
///
/// The split is on `.` outside `«…»`, because an escaped component may contain
/// one — `«a.b»` is one component and not two. A glob can never produce that
/// (a path separator is not a dot), but an IR written by the extractor can, and
/// this function is applied to both.
#[must_use]
pub fn module_components(module: &str) -> Vec<&str> {
    // **`char_indices`, not a byte walk** — the first version of this advanced
    // one byte at a time and sliced, which panics the moment a name holds a
    // multi-byte character: `𝒜` is four bytes and `ﬀ` is three, and both are in
    // this project's curated test cases already. They caught it 【実測】.
    let mut out = Vec::new();
    let mut start = 0;
    let mut depth = 0usize;
    for (at, c) in module.char_indices() {
        match c {
            '«' => depth += 1,
            '»' => depth = depth.saturating_sub(1),
            '.' if depth == 0 => {
                out.push(unescape_component(&module[start..at]));
                start = at + 1;
            }
            _ => {}
        }
    }
    out.push(unescape_component(&module[start..]));
    out
}

/// `«x»` → `x`, anything else unchanged.
#[must_use]
pub fn unescape_component(component: &str) -> &str {
    component
        .strip_prefix('«')
        .and_then(|rest| rest.strip_suffix('»'))
        .unwrap_or(component)
}

/// The module's path stem: `Alpha.«Odd-Name»` → `Alpha/Odd-Name`.
///
/// Every place that turns a module name into a file path goes through this —
/// the olean lookup, the page path, the `href` a page uses to reach another
/// page. `module.replace('.', "/")` is the same string for every module name
/// that needs no escaping, which is all 432 of the measurement target's.
#[must_use]
pub fn module_path(module: &str) -> String {
    module_components(module).join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_names_are_their_own_spelling() {
        for module in [
            "InformationTheory.Shannon.BroadcastChannel.Basic",
            "Alpha",
            "Mathlib.Data.Nat.Basic",
            "Alpha.AstralNames",
        ] {
            let parts: Vec<&str> = module.split('.').collect();
            assert_eq!(escape_module(parts.clone()), module);
            assert_eq!(module_components(module), parts);
            assert_eq!(module_path(module), module.replace('.', "/"));
        }
    }

    #[test]
    fn a_component_that_is_not_an_identifier_is_escaped() {
        assert_eq!(escape_module(["Alpha", "Odd-Name"]), "Alpha.«Odd-Name»");
        assert_eq!(module_components("Alpha.«Odd-Name»"), ["Alpha", "Odd-Name"]);
        assert_eq!(module_path("Alpha.«Odd-Name»"), "Alpha/Odd-Name");
    }

    /// The letter-like table is what makes an astral identifier plain: `𝒜` is
    /// inside `0x1d49c..=0x1d59f`, so a module or declaration named with it is
    /// **not** escaped — which is why plan §7's U1 test names are spelled
    /// `«𝒜-z»` (the `-` is what forces the escape) rather than `𝒜`.
    #[test]
    fn astral_letter_like_characters_are_identifiers() {
        assert!(is_id_first('𝒜'));
        assert_eq!(escape_component("𝒜"), "𝒜");
        assert_eq!(escape_component("𝒜-z"), "«𝒜-z»");
        assert_eq!(escape_component("ﬀ-z"), "«ﬀ-z»");
        // U+FB00 is not letter-like in Lean's table, so even alone it escapes.
        assert_eq!(escape_component("ﬀ"), "«ﬀ»");
    }

    /// `escapePart` gives up on a component containing `»` rather than
    /// producing something that will not parse back (`Init/Meta/Defs.lean:202`).
    #[test]
    fn a_component_holding_the_closing_bracket_is_left_alone() {
        assert_eq!(escape_component("a»b"), "a»b");
    }

    /// An escaped component may hold a dot; splitting on every dot would make
    /// two components out of one and then look for a directory that is not
    /// there.
    #[test]
    fn the_split_does_not_cut_inside_an_escape() {
        assert_eq!(module_components("Alpha.«a.b».C"), ["Alpha", "a.b", "C"]);
        assert_eq!(module_path("Alpha.«a.b».C"), "Alpha/a.b/C");
    }

    /// The regression the curated `𝒜` / `ﬀ` cases found: a byte walk slices
    /// inside a multi-byte character.
    #[test]
    fn a_multi_byte_character_does_not_split_a_component() {
        assert_eq!(module_components("Pkg.𝒜.ﬀ"), ["Pkg", "𝒜", "ﬀ"]);
        assert_eq!(module_path("Pkg.«𝒜-z»"), "Pkg/𝒜-z");
    }

    #[test]
    fn an_empty_component_escapes_rather_than_vanishing() {
        assert_eq!(escape_component(""), "«»");
        assert!(!needs_no_escape(""));
    }
}
