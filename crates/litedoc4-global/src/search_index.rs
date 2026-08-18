//! `search-index.bin` — every declaration the package documents, as bytes.
//!
//! The plan is `docs/plans/search-v2.md`; the measurements it rests on are
//! `benchmarks/results/search-design-2026-08-19.txt`. The short version: the
//! JSON this replaces cost **860 KiB of JS heap for a 405,402 B file, and 1,651
//! KiB at the peak** because `response.json()` has to materialise the body as a
//! string before parsing it【実測】. This file is read with `arrayBuffer()` and
//! searched in place, so what the page holds is the file.
//!
//! **It is not faster.** At 4,584 declarations the scan is 0.65 ms either way
//! and the debounce is 90 ms【実測】. What changes is memory, and the headroom
//! for a package with more declarations than this one.
//!
//! # The layout
//!
//! All integers are little-endian and unaligned-safe: the reader assembles them
//! byte by byte, so no section needs padding and no platform needs to agree
//! about alignment.
//!
//! ```text
//! 0   magic "LD4S"        4 bytes
//! 4   version             u32 = 2
//! 8   count               u32   declarations
//! 12  restart             u32   restart interval of the name section
//! 16  names_off/len       u32 u32
//! 24  restarts_off        u32   ceil(count / restart) entries of u32
//! 28  kind_labels_off/len u32 u32
//! 36  kind_of_off         u32   count bytes, one kind subscript each
//! 40  module_off          u32   count entries of u16
//! 44  fold_off/len        u32 u32
//! ```
//!
//! ## Names are front-coded
//!
//! Each name is `(shared u8, suffix_len u8, suffix bytes)` against the one
//! before it, restarting every [`RESTART`] declarations so that a single
//! declaration can be decoded without reading the file from the start. The
//! order is the JSON's — UTF-16 code unit order over the original names — and
//! **that is load-bearing**: the search sorts equal-scoring hits by their
//! position here, so a different order is a different result list.
//!
//! Sorted Lean names share long prefixes: 285,148 B of names became 93,497
//! 【実測、平均共有 46.7 文字】.
//!
//! A `suffix_len` of 255 means the real length follows as a u16 — the escape
//! exists because a name longer than 254 bytes is possible even though the
//! measured corpus stops at 129, and **an encoder that truncated one would
//! produce a site whose search silently disagrees with its pages**. `shared` is
//! capped at 254 instead, which only costs compression.
//!
//! ## Case folding, and the exceptions to it
//!
//! The search is case-insensitive, and the reader folds ASCII by adding 32 to
//! `A`..=`Z`. For the measured corpus that is exactly `toLowerCase()` — **0 of
//! 4,584 names differ**【実測】 — but a package with `Γ` in a name would, so the
//! encoder compares the two per name and writes the ones that disagree into the
//! fold section. The reader substitutes them. `0 entries` is the common case
//! and costs 4 bytes.

/// The first four bytes of the file.
pub const MAGIC: [u8; 4] = *b"LD4S";
/// Bumped when a reader that does not know the change would be wrong.
pub const VERSION: u32 = 2;
/// Declarations between full names in the name section.
pub const RESTART: usize = 16;
/// Everything before the first section.
pub const HEADER_BYTES: usize = 52;
/// A `suffix_len` of this value means the real length follows as a u16.
pub const LONG_SUFFIX: u8 = 255;

/// One declaration, as the index carries it.
#[derive(Clone, Copy, Debug)]
pub struct Entry<'a> {
    pub name: &'a str,
    /// Subscript into the kind labels.
    pub kind: usize,
    /// Subscript into `modules.json`'s array — **not** into anything here.
    pub module: usize,
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_len(out: &mut Vec<u8>, len: usize) {
    if len >= usize::from(LONG_SUFFIX) {
        out.push(LONG_SUFFIX);
        put_u16(
            out,
            u16::try_from(len).expect("a declaration name under 64 KiB"),
        );
    } else {
        out.push(u8::try_from(len).expect("checked above"));
    }
}

/// ASCII-only lowering — what the reader does, so the encoder has to know it to
/// find the names it is wrong for.
fn ascii_fold(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_uppercase() { c.to_ascii_lowercase() } else { c })
        .collect()
}

/// Serialises the index.
///
/// `entries` must already be in the order the site wants them ranked in;
/// nothing here sorts. `kinds` are the badge labels the subscripts point at.
#[must_use]
pub fn encode(entries: &[Entry<'_>], kinds: &[&str]) -> Vec<u8> {
    assert!(
        kinds.len() <= 255,
        "{} kinds: the index carries a kind as one byte",
        kinds.len()
    );

    let mut names: Vec<u8> = Vec::with_capacity(entries.len() * 24);
    let mut restarts: Vec<u8> = Vec::with_capacity(entries.len() / RESTART * 4 + 4);
    let mut folds: Vec<u8> = Vec::new();
    let mut fold_count = 0u32;
    let mut previous: &[u8] = &[];

    for (i, entry) in entries.iter().enumerate() {
        let bytes = entry.name.as_bytes();
        if i % RESTART == 0 {
            put_u32(
                &mut restarts,
                u32::try_from(names.len()).expect("a name section under 4 GiB"),
            );
            previous = &[];
        }
        let mut shared = 0;
        while shared < previous.len()
            && shared < bytes.len()
            && previous[shared] == bytes[shared]
            && shared < 254
        {
            shared += 1;
        }
        names.push(u8::try_from(shared).expect("capped at 254"));
        put_len(&mut names, bytes.len() - shared);
        names.extend_from_slice(&bytes[shared..]);
        previous = bytes;

        // The name the reader will match against, when folding ASCII is not
        // what `toLowerCase()` does to it.
        let lowered = entry.name.to_lowercase();
        if lowered != ascii_fold(entry.name) {
            fold_count += 1;
            put_u32(
                &mut folds,
                u32::try_from(i).expect("fewer than 4 billion declarations"),
            );
            put_u16(
                &mut folds,
                u16::try_from(lowered.len()).expect("a folded name under 64 KiB"),
            );
            folds.extend_from_slice(lowered.as_bytes());
        }
    }

    let mut labels: Vec<u8> = Vec::new();
    put_u32(&mut labels, u32::try_from(kinds.len()).expect("checked above"));
    for kind in kinds {
        labels.push(u8::try_from(kind.len()).expect("a kind label under 256 bytes"));
        labels.extend_from_slice(kind.as_bytes());
    }

    let kind_of: Vec<u8> = entries
        .iter()
        .map(|entry| u8::try_from(entry.kind).expect("a kind subscript under 256"))
        .collect();
    let mut modules: Vec<u8> = Vec::with_capacity(entries.len() * 2);
    for entry in entries {
        put_u16(
            &mut modules,
            u16::try_from(entry.module).expect("fewer than 65,536 modules"),
        );
    }

    let names_off = HEADER_BYTES;
    let restarts_off = names_off + names.len();
    let labels_off = restarts_off + restarts.len();
    let kind_of_off = labels_off + labels.len();
    let module_off = kind_of_off + kind_of.len();
    let fold_off = module_off + modules.len();

    let mut out: Vec<u8> = Vec::with_capacity(fold_off + folds.len() + 4);
    out.extend_from_slice(&MAGIC);
    put_u32(&mut out, VERSION);
    put_u32(&mut out, u32::try_from(entries.len()).expect("fewer than 4 billion"));
    put_u32(&mut out, u32::try_from(RESTART).expect("small"));
    for value in [
        names_off,
        names.len(),
        restarts_off,
        labels_off,
        labels.len(),
        kind_of_off,
        module_off,
        fold_off,
    ] {
        put_u32(&mut out, u32::try_from(value).expect("an index under 4 GiB"));
    }
    put_u32(&mut out, 4 + folds.len().try_into().unwrap_or(u32::MAX));
    debug_assert_eq!(out.len(), HEADER_BYTES);
    out.extend_from_slice(&names);
    out.extend_from_slice(&restarts);
    out.extend_from_slice(&labels);
    out.extend_from_slice(&kind_of);
    out.extend_from_slice(&modules);
    put_u32(&mut out, fold_count);
    out.extend_from_slice(&folds);
    out
}

/// What [`decode`] hands back: the four parallel arrays the file holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decoded {
    /// Declaration names, in the file's order — which is the site's ranking
    /// order for equal scores.
    pub names: Vec<String>,
    /// The kind vocabulary, indexed by [`Decoded::kind_of`].
    pub labels: Vec<String>,
    /// One kind subscript per declaration.
    pub kind_of: Vec<usize>,
    /// One `modules.json` subscript per declaration.
    pub modules: Vec<usize>,
}

/// Reads a file [`encode`] wrote, or `None` if it is not one.
///
/// **This is not what decides the format.** A reader beside its writer agrees
/// with it by construction; what decides it is `tools/search-gate.sh`, where
/// the site's own `app.js` and a third implementation in Python read the same
/// bytes and are held against `declarations/name-map.json`. This exists so that
/// tests and tools can say what the bytes mean without a browser.
///
/// Every read is bounds-checked: a truncated or corrupt file returns `None`
/// rather than panicking, because the callers that matter are checking whether
/// a file is well-formed.
#[must_use]
#[expect(clippy::missing_panics_doc, reason = "the slice lengths are checked before every conversion")]
pub fn decode(bytes: &[u8]) -> Option<Decoded> {
    let u32_at = |at: usize| -> Option<usize> {
        let slice = bytes.get(at..at.checked_add(4)?)?;
        Some(u32::from_le_bytes(slice.try_into().expect("four bytes")) as usize)
    };
    let u16_at = |at: usize| -> Option<usize> {
        let slice = bytes.get(at..at.checked_add(2)?)?;
        Some(usize::from(u16::from_le_bytes(
            slice.try_into().expect("two bytes"),
        )))
    };
    if bytes.get(0..4)? != MAGIC || u32_at(4)? != VERSION as usize {
        return None;
    }
    let count = u32_at(8)?;
    let names_off = u32_at(16)?;
    let labels_off = u32_at(28)?;
    let kind_of_off = u32_at(36)?;
    let module_off = u32_at(40)?;

    let mut names = Vec::with_capacity(count);
    let mut at = names_off;
    let mut previous: Vec<u8> = Vec::new();
    for _ in 0..count {
        let shared = usize::from(*bytes.get(at)?);
        at += 1;
        let len = if *bytes.get(at)? == LONG_SUFFIX {
            let len = u16_at(at + 1)?;
            at += 3;
            len
        } else {
            let len = usize::from(*bytes.get(at)?);
            at += 1;
            len
        };
        if shared > previous.len() {
            return None;
        }
        previous.truncate(shared);
        previous.extend_from_slice(bytes.get(at..at.checked_add(len)?)?);
        at += len;
        names.push(String::from_utf8(previous.clone()).ok()?);
    }

    let label_count = u32_at(labels_off)?;
    let mut labels = Vec::with_capacity(label_count);
    let mut at = labels_off + 4;
    for _ in 0..label_count {
        let len = usize::from(*bytes.get(at)?);
        labels.push(String::from_utf8(bytes.get(at + 1..at + 1 + len)?.to_vec()).ok()?);
        at += 1 + len;
    }

    let mut kind_of = Vec::with_capacity(count);
    for i in 0..count {
        kind_of.push(usize::from(*bytes.get(kind_of_off + i)?));
    }
    let mut modules = Vec::with_capacity(count);
    for i in 0..count {
        modules.push(u16_at(module_off + i * 2)?);
    }
    Some(Decoded {
        names,
        labels,
        kind_of,
        modules,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries<'a>(names: &'a [&'a str]) -> Vec<Entry<'a>> {
        names
            .iter()
            .enumerate()
            .map(|(i, name)| Entry {
                name,
                kind: i % 2,
                module: i,
            })
            .collect()
    }

    #[test]
    fn the_names_come_back_out() {
        let names: Vec<&str> = vec![
            "Pkg.a",
            "Pkg.alpha",
            "Pkg.alphabet",
            "Pkg.b",
            "Pkg.Nested.deep.name",
            "Pkg.\u{1D49C}",
            "Pkg.\u{3B2}",
        ];
        let bytes = encode(&entries(&names), &["def", "theorem"]);
        let back = decode(&bytes).expect("the encoder's own output");
        assert_eq!(back.names, names);
        assert_eq!(back.labels, ["def", "theorem"]);
        assert_eq!(back.kind_of, (0..names.len()).map(|i| i % 2).collect::<Vec<_>>());
        assert_eq!(back.modules, (0..names.len()).collect::<Vec<_>>());
    }

    /// The block boundary is where front coding is easiest to get wrong: the
    /// 17th name shares everything with the 16th and still has to be written
    /// out whole.
    #[test]
    fn every_restart_block_stands_alone() {
        let names: Vec<String> = (0..40).map(|i| format!("Pkg.same.prefix.n{i:02}")).collect();
        let borrowed: Vec<&str> = names.iter().map(String::as_str).collect();
        let bytes = encode(&entries(&borrowed), &["def"]);
        assert_eq!(decode(&bytes).expect("the encoder's own output").names, names);
        let restarts_off = u32::from_le_bytes(bytes[24..28].try_into().expect("four")) as usize;
        let first_of_block = bytes[restarts_off + 4] as usize;
        assert_eq!(
            bytes[HEADER_BYTES + first_of_block],
            0,
            "the first name of the second block shares a prefix with the last of the first"
        );
    }

    /// 254 bytes is where the one-byte length runs out. A name that long is not
    /// in the measured corpus (129 B is), which is exactly why it is here.
    #[test]
    fn a_name_longer_than_the_short_length_survives() {
        let long = format!("Pkg.{}", "x".repeat(400));
        let names = vec!["Pkg.a", long.as_str(), "Pkg.b"];
        let bytes = encode(&entries(&names), &["def"]);
        assert_eq!(decode(&bytes).expect("the encoder's own output").names, names);
    }

    /// The fold section, and the fact that it is empty for ASCII.
    #[test]
    fn only_the_names_ascii_folding_is_wrong_for_are_carried() {
        let plain = encode(&entries(&["Pkg.Abc", "Pkg.dEF"]), &["def"]);
        let fold_off = u32::from_le_bytes(plain[44..48].try_into().expect("four")) as usize;
        assert_eq!(
            u32::from_le_bytes(plain[fold_off..fold_off + 4].try_into().expect("four")),
            0,
            "an all-ASCII package is carrying fold exceptions"
        );

        // `Γ` lowercases to `γ`, which adding 32 to A..Z does not do.
        let greek = encode(&entries(&["Pkg.\u{393}amma"]), &["def"]);
        let fold_off = u32::from_le_bytes(greek[44..48].try_into().expect("four")) as usize;
        assert_eq!(
            u32::from_le_bytes(greek[fold_off..fold_off + 4].try_into().expect("four")),
            1,
            "a name with an uppercase Greek letter is not in the fold section"
        );
        let at = u32::from_le_bytes(greek[fold_off + 4..fold_off + 8].try_into().expect("four"));
        let len = u16::from_le_bytes(greek[fold_off + 8..fold_off + 10].try_into().expect("two"));
        let folded = &greek[fold_off + 10..fold_off + 10 + usize::from(len)];
        assert_eq!(at, 0);
        assert_eq!(std::str::from_utf8(folded).expect("UTF-8"), "pkg.\u{3B3}amma");
    }

    #[test]
    fn an_empty_package_is_still_a_file() {
        let bytes = encode(&[], &[]);
        assert_eq!(&bytes[0..4], MAGIC);
        let back = decode(&bytes).expect("the encoder's own output");
        assert!(back.names.is_empty() && back.labels.is_empty());
    }
}
