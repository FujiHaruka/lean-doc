//! Everything the whole-package artifacts need from **one** module.
//!
//! Ported from `experiments/stage7h/global.ts` (frozen): `headConst` (97-106),
//! `autolinkTokens` (111-125) and `factsOf` (148-170) — the same line ranges
//! `tests/oracle/gen-global-expected.ts` slices to build its oracle.
//!
//! # Why this type exists before the cache does
//!
//! [`ModuleFacts`] is ~2 KB per module against 16 MB of module IR, and plan §3
//! says the `contentHash` cache goes in one place rather than five. M2-a builds
//! every fact from scratch on every run; M2-b keys this struct on
//! [`ModuleFacts::content_hash`] and stops re-reading modules whose hash has not
//! moved. Keeping the seam now costs one struct and means the cache is a change
//! to [`crate::facts_for`], not to the derivation.
//!
//! The prototype states the contract in a comment worth restating: **if a fact
//! the derivation reads is not in here, adding it is a cache-version bump, not
//! an edit.** A cache that keeps entries built by an older rule is fast and
//! wrong.
//!
//! # [`ModuleFacts::tokens`] reaches no artifact
//!
//! The artifacts are derived from the other fields. `tokens` exists for the
//! whole-package map delta (M2-b), which asks "does this module's docstrings
//! mention a name that moved". It is built here because it is a per-module fact
//! and the cache boundary is per module — and because it is the one field a byte
//! comparison of the artifacts cannot check at all.
//!
//! # M8-d added a seventh field, and that is a cache-version bump
//!
//! [`ModuleFacts::instances_for`] is the "which instances mention this type"
//! direction of the instance index, which `search-index.json` needs and no
//! artifact used to carry. Per the contract above, adding it bumps
//! [`crate::STATE_DERIVATION`] — a state file written by the v1 rule has no such
//! key, and using it would produce a site whose Instances For blocks are empty
//! on every module the cache hit.

use std::collections::HashSet;

use lean_doc_ir::{Decl, ModuleFile, SpanKind, cmp_utf16};
use serde::{Deserialize, Serialize};

/// One module's contribution to the whole-package artifacts and to the map
/// delta.
///
/// **The field order below is the state file's bytes** — `crate::state` writes
/// this struct straight out, and `JSON.stringify` of the prototype's object
/// literal emits `module` / `contentHash` / `imports` / `tactics` / `decls` /
/// `instances` / `tokens`. Reordering the fields rewrites a file that is
/// compared with the prototype's byte for byte.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleFacts {
    pub module: String,
    /// Lean's `String.hash` of the module JSON, carried from `index.json`.
    ///
    /// Unused by the derivation and unused by M2-a. It is the cache key of plan
    /// §3, and it is stored here rather than alongside so that a cached entry
    /// cannot be separated from the hash it was built for.
    pub content_hash: String,
    pub imports: Vec<String>,
    /// How many tactic docstrings the module declares — the count is all
    /// `tactics.html` needs. **Zero for all 432 modules of the target
    /// package**【実測】.
    pub tactics: usize,
    /// `(name, kind)` per declaration, in the module's own order.
    pub decls: Vec<(String, String)>,
    /// `(class, instance name)` for each instance whose printed type has a head
    /// constant, in the module's own order.
    pub instances: Vec<(String, String)>,
    /// The names this module's docstrings could autolink: deduplicated and
    /// sorted in **UTF-16 code unit order** (plan §7, U1), as `[...set].sort()`
    /// leaves them.
    pub tokens: Vec<String>,
    /// `(type name, instance name)` for each of an instance's `instTypes`, in
    /// the module's own order — the other direction of the instance index,
    /// "which instances mention this type".
    ///
    /// **Added by M8-d, and it is why this struct's field order stops being a
    /// transcription here.** The prototype has no notion of this field, so it
    /// goes *after* the six it does have: the state file's key order is then the
    /// prototype's, plus one key at the end, and the two files can still be
    /// compared entry by entry (`tests/state_and_delta.rs`).
    ///
    /// The rule is not this crate's. It is doc-gen4's `getInstanceTypes`
    /// (`Process/InstanceInfo.lean:16-31`) — the head constant of each
    /// **explicit** argument of the class application — which the extractor
    /// already ports verbatim into [`lean_doc_ir::Decl::inst_types`]
    /// (`extractor/Extract.lean:1468`). Nothing is re-derived here; the IR is
    /// read. UI-V5 measured the two against doc-gen4's own
    /// `declarations/declaration-data.bmp`: **59 of 59** of this package's
    /// instances that the reference tree also has agree exactly 【実測
    /// 2026-08-16】. The plan's guess — every constant in the printed type —
    /// agrees with **0 of 59**.
    pub instances_for: Vec<(String, String)>,
}

impl ModuleFacts {
    /// Derives the facts of one module. `content_hash` comes from the index
    /// entry, not from the file.
    #[must_use]
    pub fn of(module: &ModuleFile, content_hash: &str) -> Self {
        let mut decls = Vec::with_capacity(module.declarations.len());
        let mut instances = Vec::new();
        let mut instances_for = Vec::new();
        let mut tokens: HashSet<String> = HashSet::new();

        // MODULE DOCSTRINGS CONTRIBUTE NOTHING, ON PURPOSE.
        //
        // `global.ts:152` reads `md.doc` for every element of `moduleDocs`. The
        // extractor writes `line` / `col` / `text` and no `doc`
        // (`Extract.lean:2004-2006`), so that loop body has never run for any
        // module of any package this has been pointed at 【実測: 1,515 module
        // docstrings, 0 with the key】.
        //
        // Reading `md.text` here instead would be a *fix*, and it is not this
        // milestone's to make: it changes which modules the map delta calls
        // affected, so it has to be measured against the prototype's numbers
        // rather than smuggled in with a port (plan §3, §5 M2). Left as this
        // comment because there is nothing to transcribe — the loop has no
        // reachable body. `lean_doc_ir::ModuleDoc` is `deny_unknown_fields`, so
        // an IR that did carry `doc` would fail to parse rather than change
        // behaviour here silently.

        for decl in &module.declarations {
            decls.push((decl.name.clone(), decl.kind.clone()));
            if decl.kind == "instance" {
                // `if (cls)` is a truthiness test: a constant named by the empty
                // string drops the instance rather than falling through to the
                // next span.
                if let Some(class) = head_const(decl).filter(|class| !class.is_empty()) {
                    instances.push((class.to_owned(), decl.name.clone()));
                }
                // Straight off the IR, in the extractor's order. An empty name
                // is dropped for the same reason the class above is: it would
                // be a key nothing can ask for. The extractor emits none
                // 【実測: 91 instances, 91 type names, 0 empty】.
                for ty in &decl.inst_types {
                    if !ty.is_empty() {
                        instances_for.push((ty.clone(), decl.name.clone()));
                    }
                }
            }
            // `if (d.doc)`: an empty docstring is skipped, not tokenised.
            if let Some(doc) = decl.doc.as_deref().filter(|doc| !doc.is_empty()) {
                tokens.extend(autolink_tokens(doc));
            }
        }

        let mut tokens: Vec<String> = tokens.into_iter().collect();
        tokens.sort_by(|a, b| cmp_utf16(a, b));

        Self {
            module: module.module.clone(),
            content_hash: content_hash.to_owned(),
            imports: module.imports.clone(),
            tactics: module.tactics.len(),
            decls,
            instances,
            tokens,
            instances_for,
        }
    }
}

/// The head constant of a printed type: the name on the tagged constant span
/// that starts earliest.
///
/// Ties keep the earlier element of `typeCode`, because the prototype compares
/// with a strict `<` against the best so far. The spans are in the extractor's
/// pre-order, which is **not** sorted by `start`.
///
/// `None` when the type has no constant span at all — a sort, a bound variable,
/// or a span list the extractor left empty.
#[must_use]
pub fn head_const(decl: &Decl) -> Option<&str> {
    let mut best: Option<(u32, &str)> = None;
    for span in &decl.type_code {
        if span.kind != SpanKind::Const {
            continue;
        }
        // `span.length >= 4 && typeof span[3] === "string"`: a constant span
        // with no fourth element is not a candidate.
        let Some(name) = span.name.as_deref() else {
            continue;
        };
        if best.is_none_or(|(start, _)| span.start < start) {
            best = Some((span.start, name));
        }
    }
    best.map(|(_, name)| name)
}

/// The names a docstring could autolink, in the order the prototype pushes
/// them: duplicates kept, empty strings possible.
///
/// Transcribed from `global.ts:111-125`, which is stage 5's function unchanged.
/// The unit is the **whitespace-separated part of a code span**, not the code
/// span: `` `Nat.succ n` `` offers `Nat.succ`, `succ` and `n`. Every part that
/// contains a dot also offers its last component, unconditionally — including
/// when that component is empty, which is how `` `a.` `` contributes `""`.
///
/// Markdown link targets go through the same name resolution in the renderer
/// (`extendLink`), so `](Target)` is tokenised too.
///
/// # Deliberately an over-approximation
///
/// This does not parse Markdown. It is a filter in front of the delta: a token
/// that no renderer would ever link costs a module a place in the affected set,
/// and a token that is missing costs a stale page. Widening is safe, narrowing
/// is not.
///
/// # It splits on **two** separator sets at once, on purpose
///
/// See [`is_token_separator`]. There are two answers to "what is whitespace
/// here" — the prototype's and the renderer's — and this takes the union rather
/// than choosing.
#[must_use]
pub fn autolink_tokens(doc: &str) -> Vec<String> {
    let mut out = Vec::new();
    for inner in code_spans(doc) {
        for part in inner.split(is_token_separator) {
            push_token(&mut out, part);
        }
    }
    for target in link_targets(doc) {
        push_token(&mut out, target);
    }
    out
}

/// What [`autolink_tokens`] breaks a code span into parts on: **the union of
/// two separator sets** (plan §8, V6).
///
/// # Do not narrow this to one of them
///
/// Two implementations answer "is this code point whitespace" differently, and
/// neither is wrong:
///
/// - the frozen prototype splits on **V8's** `/[\p{Z}\p{C}]/u`
///   (`experiments/stage7h/global.ts:120`) — that is what the delta this crate
///   has to agree with was computed with;
/// - the renderer's `autoLinkInline` splits on **UnicodeBasic's** `Z | C`, from
///   the build doc-gen4 links ([`lean_doc_md::gc::is_z_c`]) — that is what
///   actually decides which names a page ends up linking.
///
/// **They disagree on 4,803 code points, and every one of them is a separator
/// for V8 and not for UnicodeBasic** — UnicodeBasic's set is a strict subset
/// 【実測 2026-08-12 → `benchmarks/results/m2b-v6-token-separators.json`,
/// asserted by `the_two_separator_sets_disagree_the_way_v6_measured_them`】.
/// The disagreement is a UCD version gap: the code points are assigned in
/// UnicodeBasic's database and unassigned (`Cn`, inside `C`) in V8's.
///
/// The two ways to be wrong are not each other's mirror image. These tokens are
/// **the filter in front of the whole-package map delta** ([`crate::Delta`]): a
/// module is re-rendered iff one of its tokens is a name that moved.
///
/// | too many tokens | too few tokens |
/// |---|---|
/// | a module is re-rendered that did not need to be | a page that should have been re-rendered is **not** |
/// | costs time | keeps a link pointing at the module a name used to live in, and **nothing downstream notices** |
///
/// Concretely, with `c` a code point V8 separates on and UnicodeBasic does not,
/// `` `Nat.succ<c>Foo` `` is `{Nat.succ, succ, Foo}` under the prototype and
/// `{Nat.succ<c>Foo, succ<c>Foo}` under UnicodeBasic alone. The second never
/// offers `Nat.succ`, so a module whose docstring says that is missing from the
/// affected set when `Nat.succ` moves. The union offers both.
///
/// So this is deliberately wider than either implementation, and wider than the
/// data needs: **no code point either table disagrees about occurs in any code
/// span of the target package** — 432 modules / 4,750 declarations / 3,394
/// docstrings / 16,044 code spans / 233,713 code points inside them, 0
/// occurrences 【実測 2026-08-12、同ログ】. So the widening costs this package
/// no extra re-rendering at all — the six artifacts and the state file are byte
/// for byte what they were before it — and it is the only side of the trade
/// whose failure mode is loud.
///
/// # The first disjunct is redundant **today**, and stays
///
/// Because the disagreement is one-sided, `is_z_c || v8` is currently the same
/// function as `v8` alone, and deleting the first call changes no answer — a
/// mutation of it fails no test in this workspace 【実測 2026-08-12】. It is a
/// union anyway because the two tables are pinned to two things that move
/// independently (a `lake-manifest.json` rev and a V8 build), and the day one of
/// them gains a separator the other lacks, the `||` is what keeps this a
/// superset of both instead of a silent narrowing. The measured fact that makes
/// the disjunct redundant is asserted, not assumed, so the assertion is what
/// says the day has come.
#[must_use]
pub fn is_token_separator(c: char) -> bool {
    lean_doc_md::gc::is_z_c(c) || crate::v8_gc::is_z_c(c)
}

fn push_token(out: &mut Vec<String>, part: &str) {
    if part.is_empty() {
        return;
    }
    out.push(part.to_owned());
    // Note the asymmetry with the guard above: the last component is pushed
    // whether or not it is empty, so a part ending in a dot contributes `""`.
    if let Some(dot) = part.rfind('.') {
        out.push(part[dot + 1..].to_owned());
    }
}

/// The inside of every `` `...` `` in the text, as ``/`([^`\n]+)`/g`` finds
/// them: non-overlapping, left to right, no newline inside, never empty.
fn code_spans(doc: &str) -> Vec<&str> {
    let bytes = doc.as_bytes();
    let mut out = Vec::new();
    let mut open = 0;
    while let Some(offset) = bytes[open..].iter().position(|b| *b == b'`') {
        let start = open + offset + 1;
        let mut at = start;
        while at < bytes.len() && bytes[at] != b'`' && bytes[at] != b'\n' {
            at += 1;
        }
        // A run that stopped anywhere but on a closing backtick fails, and the
        // regex engine retries one position later. The next position that could
        // start a match is the next backtick, which is where this loop goes: the
        // scan above never steps over one.
        if at < bytes.len() && bytes[at] == b'`' && at > start {
            out.push(&doc[start..at]);
            open = at + 1;
        } else {
            open = start;
        }
    }
    out
}

/// Every `](target)` in the text, as `/\]\(([^)\s]+)\)/g` finds them.
fn link_targets(doc: &str) -> Vec<&str> {
    let bytes = doc.as_bytes();
    let mut out = Vec::new();
    let mut open = 0;
    while open + 1 < bytes.len() {
        let Some(offset) = bytes[open..].windows(2).position(|pair| pair == b"](") else {
            break;
        };
        let start = open + offset + 2;
        let mut at = start;
        while at < bytes.len() {
            let ch = doc[at..].chars().next().expect("at is a char boundary");
            if ch == ')' || is_js_space(ch) {
                break;
            }
            at += ch.len_utf8();
        }
        if at < bytes.len() && bytes[at] == b')' && at > start {
            out.push(&doc[start..at]);
            open = at + 1;
        } else {
            open = open + offset + 1;
        }
    }
    out
}

/// JavaScript's `\s`: `WhiteSpace` plus `LineTerminator`.
///
/// Not `char::is_whitespace`, which is `White_Space` and excludes U+FEFF while
/// including U+0085.
fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shapes the scanner has to get right that the corpus has few of. The
    /// authority for every expectation here is the fixture, not this file; these
    /// are the cases that make a failure readable.
    #[test]
    fn code_spans_are_found_the_way_the_regex_finds_them() {
        assert_eq!(code_spans("`a`"), ["a"]);
        assert_eq!(code_spans("x `a` y `b` z"), ["a", "b"]);
        // An empty span is not a match, and its closing backtick can open one.
        assert_eq!(code_spans("``a`"), ["a"]);
        assert_eq!(code_spans("``"), Vec::<&str>::new());
        // A newline inside kills the match; the trailing backtick has nothing
        // to close against.
        assert_eq!(code_spans("`a\nb`"), Vec::<&str>::new());
        assert_eq!(code_spans("`a`b`c`"), ["a", "c"]);
        assert_eq!(code_spans("no ticks"), Vec::<&str>::new());
        assert_eq!(code_spans("`α → β`"), ["α → β"]);
    }

    #[test]
    fn link_targets_are_found_the_way_the_regex_finds_them() {
        assert_eq!(link_targets("[t](Foo.Bar)"), ["Foo.Bar"]);
        assert_eq!(link_targets("[a](x) [b](y)"), ["x", "y"]);
        // Whitespace or an empty target is not a match.
        assert_eq!(link_targets("[t](a b)"), Vec::<&str>::new());
        assert_eq!(link_targets("[t]()"), Vec::<&str>::new());
        assert_eq!(link_targets("[t](a\u{a0}b)"), Vec::<&str>::new());
        // The greedy class eats everything that is not `)` or space.
        assert_eq!(link_targets("](](x)"), ["](x"]);
        assert_eq!(link_targets("]("), Vec::<&str>::new());
    }

    #[test]
    fn a_part_ending_in_a_dot_contributes_the_empty_string() {
        assert_eq!(autolink_tokens("`a.`"), ["a.", ""]);
        assert_eq!(autolink_tokens("`Nat.succ`"), ["Nat.succ", "succ"]);
        assert_eq!(autolink_tokens("`n`"), ["n"]);
        // Empty parts between separators are dropped, dotted ones are not.
        assert_eq!(autolink_tokens("`  a  `"), ["a"]);
    }

    /// U+088F ARABIC HALF MADDA OVER MADDA: **a separator for V8 and not for
    /// UnicodeBasic**, the first of the 4,803 and the direction that costs
    /// correctness. See [`is_token_separator`].
    const V8_ONLY: char = '\u{088F}';

    /// U+00A0 NO-BREAK SPACE: a separator for **both** tables, which is what
    /// the other direction looks like — there is no code point UnicodeBasic
    /// separates on and V8 does not 【実測】, so this is as close as the
    /// disagreement gets to being two-sided.
    const BOTH: char = '\u{00A0}';

    /// The union in the only terms that matter: which names a docstring offers.
    ///
    /// Splitting on UnicodeBasic alone loses `Nat.succ` from the first case —
    /// and losing a token is a page that is never re-rendered.
    #[test]
    fn the_split_is_a_superset_of_both_implementations() {
        assert_eq!(
            autolink_tokens(&format!("`Nat.succ{V8_ONLY}Foo`")),
            ["Nat.succ", "succ", "Foo"],
            "a code point V8 separates on and UnicodeBasic does not was kept inside a token"
        );
        assert_eq!(
            autolink_tokens(&format!("`Nat.succ{BOTH}Foo`")),
            ["Nat.succ", "succ", "Foo"],
            "a code point both tables separate on was kept inside a token"
        );
        // The same shape the delta sees: the prefix has to survive on its own.
        assert!(
            autolink_tokens(&format!("`Nat.succ{V8_ONLY}Foo`")).contains(&"Nat.succ".to_owned())
        );
    }

    /// V6, restated where it can fail: how far apart the two tables are, and in
    /// which direction.
    ///
    /// The second number being **zero** is the whole reason
    /// [`is_token_separator`]'s first disjunct is currently dead — if it ever
    /// stops being zero, this is the assertion that says so, and the union
    /// starts doing work in both directions without any other edit.
    #[test]
    fn the_two_separator_sets_disagree_the_way_v6_measured_them() {
        let mut v8_only = 0usize;
        let mut unicode_basic_only = 0usize;
        for cp in 0..=0x10_FFFFu32 {
            let Some(c) = char::from_u32(cp) else {
                continue;
            };
            let v8 = crate::v8_gc::is_z_c(c);
            let unicode_basic = lean_doc_md::gc::is_z_c(c);
            assert_eq!(
                is_token_separator(c),
                v8 || unicode_basic,
                "U+{cp:04X} is not the union of the two tables"
            );
            if v8 && !unicode_basic {
                v8_only += 1;
            }
            if unicode_basic && !v8 {
                unicode_basic_only += 1;
            }
        }
        // 【実測 2026-08-12 → benchmarks/results/m2b-v6-token-separators.json】
        assert_eq!(
            v8_only, 4_803,
            "the code points only the prototype splits on are not the 4,803 V6 measured"
        );
        assert_eq!(
            unicode_basic_only, 0,
            "UnicodeBasic is no longer a subset of V8: the union now widens both ways, and \
             is_token_separator's doc comment says it is one-sided"
        );
        assert!(is_token_separator(V8_ONLY) && !lean_doc_md::gc::is_z_c(V8_ONLY));
        assert!(is_token_separator(BOTH) && lean_doc_md::gc::is_z_c(BOTH));
    }
}
