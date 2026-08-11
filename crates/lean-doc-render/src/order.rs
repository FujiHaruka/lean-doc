//! Lean's `String.lt` and `Name.lt`, which the import list is sorted with.
//!
//! Ported from `experiments/stage7d/render.ts` (`stringLt` 800-807, `nameLt`
//! 816-825), which is frozen.
//!
//! # Two different orders live in this port
//!
//! [`string_lt`] compares **code points**, so it coincides with `str`'s own
//! `Ord` (UTF-8 byte order is code point order). The argument-less
//! `Array.prototype.sort` that the global artifacts use does *not*: it compares
//! UTF-16 code units, and the two disagree above U+FFFF — see
//! [`lean_doc_ir::cmp_utf16`]. Picking the wrong one of these two is invisible
//! until a name carries a mathematical alphanumeric, which the target package's
//! names do.
//!
//! Byte order is not used directly here even where it would be equivalent, so
//! that the correspondence to `String.lt` stays readable rather than resting on
//! a coincidence a reader has to re-derive.

use std::cmp::Ordering;

/// `String.lt` in Lean core: `List.lt` over the characters, i.e. code points.
pub fn string_lt(a: &str, b: &str) -> bool {
    cmp_string(a, b) == Ordering::Less
}

/// The total order [`string_lt`] is the strict part of.
pub fn cmp_string(a: &str, b: &str) -> Ordering {
    a.chars().cmp(b.chars())
}

/// `Lean.Name.lt` over a name already split into components.
///
/// It compares the **parents** first and only then the last component, so a
/// name with fewer components sorts before one with more whatever the strings
/// say (`.anonymous` is below everything): `Foo.Bar` sorts after `Zzz` because
/// their parents are `Foo` and `.anonymous`.
///
/// Only the `.str` case is transcribed. Module names — the one thing this is
/// used on — never contain a `.num` component.
pub fn name_lt(a: &[&str], b: &[&str]) -> bool {
    if a.is_empty() {
        return !b.is_empty();
    }
    if b.is_empty() {
        return false;
    }
    let (parent_a, parent_b) = (&a[..a.len() - 1], &b[..b.len() - 1]);
    if name_lt(parent_a, parent_b) {
        return true;
    }
    if parent_a == parent_b {
        return string_lt(a[a.len() - 1], b[b.len() - 1]);
    }
    false
}

/// `Name.lt` as an [`Ordering`], the shape a sort wants.
///
/// `Equal` covers both "the same name" and "neither is below the other", which
/// `Name.lt` does not distinguish. It never happens for distinct names, but
/// this does not assume so: the prototype's comparator (`render.ts:903`) is
/// exactly this expression, and it is fed to a stable sort.
pub fn cmp_name_components(a: &[&str], b: &[&str]) -> Ordering {
    if name_lt(a, b) {
        Ordering::Less
    } else if name_lt(b, a) {
        Ordering::Greater
    } else {
        Ordering::Equal
    }
}

/// [`cmp_name_components`] on dotted names, splitting them the way the
/// prototype does.
///
/// `"".split('.')` is one empty component in both languages, not zero, so an
/// empty name is `.str .anonymous ""` here rather than `.anonymous`. Nothing
/// feeds it an empty module name; the note is so that the difference is a
/// decision rather than a discovery.
pub fn cmp_name(a: &str, b: &str) -> Ordering {
    let a: Vec<&str> = a.split('.').collect();
    let b: Vec<&str> = b.split('.').collect();
    cmp_name_components(&a, &b)
}

/// Sorts dotted names by `Name.lt`, stably, as `importsHtml` does.
pub fn sort_names<T: AsRef<str>>(names: &mut [T]) {
    names.sort_by(|a, b| cmp_name(a.as_ref(), b.as_ref()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_lt_is_code_point_order() {
        assert!(string_lt("a", "b"));
        assert!(string_lt("Nat", "Nat.succ"));
        assert!(!string_lt("Nat", "Nat"));
        assert!(string_lt("A", "ℕ"));
        // Above U+FFFF it stays code point order, which is where it parts
        // company with UTF-16 order.
        assert!(string_lt("\u{FB00}", "𝒜"));
        assert_eq!(lean_doc_ir::cmp_utf16("\u{FB00}", "𝒜"), Ordering::Greater);
    }

    #[test]
    fn shorter_names_sort_first_whatever_the_strings() {
        assert!(name_lt(&["Zzz"], &["Aaa", "Bbb"]));
        assert!(!name_lt(&["Aaa", "Bbb"], &["Zzz"]));
        assert!(name_lt(&[], &["Aaa"]));
        assert!(!name_lt(&[], &[]));
        assert!(!name_lt(&["Aaa"], &[]));
    }

    #[test]
    fn parents_decide_before_the_last_component() {
        assert!(name_lt(&["Mathlib", "Algebra"], &["Mathlib", "Order"]));
        // Same length, different parent: the parent decides even though the
        // last components would say otherwise.
        assert!(name_lt(&["Mathlib", "Zzz"], &["Order", "Aaa"]));
    }

    #[test]
    fn sorting_matches_the_prototype_comparator() {
        let mut names = vec![
            "Mathlib.Order.Basic",
            "Init",
            "Mathlib.Algebra.Group",
            "Mathlib",
            "Init.Core",
        ];
        sort_names(&mut names);
        assert_eq!(
            names,
            [
                "Init",
                "Mathlib",
                "Init.Core",
                "Mathlib.Algebra.Group",
                "Mathlib.Order.Basic",
            ]
        );
    }
}
