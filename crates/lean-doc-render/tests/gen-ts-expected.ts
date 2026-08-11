#!/usr/bin/env -S deno run --allow-read --allow-write
// gen-ts-expected.ts -- produce the expected answers for `tests/differential.rs`
// by running the *prototype's own* code.
//
// WHY THIS FILE EXISTS
// --------------------
// `tests/differential.rs` checks that the Rust port of five small functions
// returns what `experiments/stage7d/render.ts` returns. An expected value that
// came from reading the Rust implementation, or from re-deriving what the
// TypeScript "should" do, proves nothing: the two would just share whatever
// mistake was made. So the expected values are produced here, by TypeScript, and
// committed as `tests/data/ts-expected.json`.
//
// HOW IT GETS AT render.ts WITHOUT IMPORTING IT
// ---------------------------------------------
// `render.ts` cannot be imported: it is a script, and its top level parses argv
// and exits. It also must not be edited (it is frozen -- the byte-reproduction
// numbers are measured on it). So this script *slices the function definitions
// out of its text* by line range, assembles them into a module, and imports that
// through a `data:` URL. The functions that answer below are therefore the
// prototype's own bytes, not a transcription of them; `checkRanges` fails loudly
// if a range stops naming the function it claims to name.
//
// npm/node are broken in this environment; this must run under deno.
//
// usage:
//   deno run --allow-read --allow-write crates/lean-doc-render/tests/gen-ts-expected.ts
//   deno run --allow-read           ... --check   (verify the committed file)

// ---------------------------------------------------- slicing render.ts

const RENDER_TS = new URL("../../../experiments/stage7d/render.ts", import.meta.url);
const renderLines = (await Deno.readTextFile(RENDER_TS)).split("\n");

/** Lines `from..to` of render.ts, 1-based and inclusive. */
const slice = (from: number, to: number) => renderLines.slice(from - 1, to).join("\n");

/**
 * Line ranges in render.ts. `head` is text the first line must start with, so
 * that a range which has drifted onto other code is a failure rather than a
 * silently different expectation.
 */
const RANGES = {
  spanType: { from: 167, to: 170, head: "type Span =" },
  escapeHtml: { from: 331, to: 341, head: "function escapeHtml(" },
  applyWsWidths: { from: 555, to: 593, head: "function applyWsWidths(" },
  leanQuote: { from: 785, to: 797, head: "function leanQuote(" },
  stringLt: { from: 800, to: 807, head: "function stringLt(" },
  nameLt: { from: 816, to: 825, head: "function nameLt(" },
  importSort: { from: 903, to: 903, head: "  uniq.sort((x, y) => (nameLt(" },
} as const;

function checkRanges() {
  for (const [what, { from, to, head }] of Object.entries(RANGES)) {
    const text = slice(from, to);
    if (!text.startsWith(head)) {
      console.error(
        `render.ts:${from}-${to} no longer starts with ${JSON.stringify(head)} (${what}).\n` +
          `Found:\n${text.split("\n")[0]}`,
      );
      Deno.exit(3);
    }
    // A function range must be balanced, i.e. end at the closing brace.
    if (what !== "spanType" && what !== "importSort" && !text.endsWith("\n}")) {
      console.error(`render.ts:${from}-${to} (${what}) does not end at a closing brace`);
      Deno.exit(3);
    }
  }
}
checkRanges();

const at = (k: keyof typeof RANGES) => slice(RANGES[k].from, RANGES[k].to);

/**
 * The module assembled out of those slices. `sink` is the only thing
 * `applyWsWidths` needs from render.ts's surroundings, and it is a counter, not
 * a behaviour. `sortNames` wraps the import-list sort expression so it can be
 * called; the expression itself is render.ts's line.
 */
const MODULE_SOURCE = [
  at("spanType"),
  "export const sink = { wsWidthChars: 0, wsWidthFragments: 0 };",
  `export ${at("escapeHtml")}`,
  `export ${at("applyWsWidths")}`,
  `export ${at("leanQuote")}`,
  `export ${at("stringLt")}`,
  `export ${at("nameLt")}`,
  ["export function sortNames(uniq: string[]): string[] {", at("importSort"), "  return uniq;", "}"]
    .join("\n"),
].join("\n\n");

const prototype = await import(
  `data:text/typescript;charset=utf-8,${encodeURIComponent(MODULE_SOURCE)}`
) as {
  sink: { wsWidthChars: number; wsWidthFragments: number };
  escapeHtml(s: string): string;
  applyWsWidths(text: string, spans: Span[]): string;
  leanQuote(s: string): string;
  stringLt(a: string, b: string): boolean;
  nameLt(a: string[], b: string[]): boolean;
  sortNames(uniq: string[]): string[];
};

type Span =
  | [number, number, number]
  | [number, number, 1, string]
  | [number, number, 1, string, number, number];

// ---------------------------------------------------------------- corpus

const chr = (cp: number) => String.fromCodePoint(cp);
/** Every ASCII punctuation character, in code point order. */
const ASCII_PUNCT = "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~";
/** cp 0-31 and 127, the two ranges `String.quote` writes as `\xNN`. */
const CONTROLS = [...Array(32).keys(), 127].map(chr);

/**
 * Strings fed to escapeHtml / leanQuote / the UTF-16 sort. Non-BMP scalars are
 * present on purpose: they are the case where UTF-16 order and byte order
 * disagree (plan §7, U1), and `𝒜`/`𝓧` really occur in the target package.
 */
const STRINGS: string[] = [
  "",
  " ",
  "  ",
  ASCII_PUNCT,
  ...ASCII_PUNCT.split(""),
  ...CONTROLS,
  ...CONTROLS.map((c) => `a${c}b`),
  "a & b < c > d \" e ' f",
  "&amp;",
  "&lt;&gt;&quot;&amp;",
  "<script>alert('x')</script>",
  "a<b>c&d\"e'f",
  "\\",
  "\\n",
  "a\\b",
  '"',
  '""',
  'say "hi"',
  "a\r\nb",
  "line1\nline2\ttabbed",
  "\n\t\\\"",
  "日本語のテキスト",
  "テスト。カタカナ、ひらがな",
  "ℕ",
  "∑",
  "≐",
  "μ",
  "ℕ ∑ ≐ μ",
  "∀ (x : ℕ), x ≤ x",
  "Set.univ ⊆ ⋃ i, s i",
  "α → β",
  "𝒜",
  "𝓧",
  "𝒜𝓧",
  "x𝒜y",
  "𝒜\t<&>",
  "μ𝒜 ℕ",
  "Foo.𝓧'",
  "Mathlib.Analysis.𝒜.foo",
  "𝟙",
  chr(0x10ffff),
  chr(0x10000),
  chr(0xffff),
  chr(0xfffd),
  chr(0xfb00),
  chr(0xe000),
  chr(0xd7ff),
  chr(0x80),
  chr(0xa0),
  `a${chr(0xa0)}b`,
  "Nat.succ",
  "Nat.succ'",
  "Nat.«foo bar»",
  "_private.Mathlib.Foo.0.bar",
  "./Mathlib/Order/Basic.html#Nat.succ",
  "https://example.test/a?b=c&d=e",
];

/** Dotted names, for `Name.lt` and its sort. */
const NAMES: string[] = [
  "",
  ".",
  "A",
  "A.B",
  "A.B.C",
  "A..B",
  "Aaa.Bbb",
  "Zzz",
  "Init",
  "Init.Core",
  "Init.Core.Basic",
  "Mathlib",
  "Mathlib.Order",
  "Mathlib.Order.Basic",
  "Mathlib.Algebra.Group.Defs",
  "Mathlib.Data.Nat.Defs",
  "Nat.succ",
  "Nat.succ'",
  "Nat.«foo bar»",
  "_private.Mathlib.Foo.0.bar",
  "Foo.Bar",
  "Foo.Bar'",
  "Foo.ℕ",
  "Foo.μ",
  "Foo.日本語",
  "Foo.𝒜",
  "Foo.𝓧",
  `Foo.${chr(0xfb00)}`,
  `Foo.${chr(0xe000)}`,
  "M.𝓧.x",
  "𝒜",
  "𝒜.B",
];

/** Pairs for the two strict orders: every ordered pair of these. */
const ORDER_CORPUS: string[] = [
  ...NAMES,
  "a",
  "b",
  "ab",
  chr(0xffff),
  chr(0x10000),
  chr(0x10ffff),
  "\t",
  " ",
  "0",
  "~",
];

/** `applyWsWidths` cases: the schema-3 widths replayed over a fragment. */
const WS_CASES: { what: string; text: string; spans: Span[] }[] = [
  { what: "no spans at all", text: "f x", spans: [] },
  { what: "spans without widths", text: "f\n x", spans: [[0, 1, 0], [2, 3, 1, "x"]] },
  { what: "newline and tab around a tag", text: "a =\n\tb", spans: [[2, 3, 1, "e", 1, 2]] },
  { what: "already spaces", text: "a = b", spans: [[2, 3, 1, "e", 1, 1]] },
  {
    what: "several runs, given out of order",
    text: "a\tb\tc\td",
    spans: [[4, 5, 1, "c", 0, 1], [2, 3, 1, "b", 1, 1]],
  },
  {
    what: "surrogate pair before the run",
    text: "𝓧\n:\tType",
    spans: [[3, 4, 1, ":", 1, 1]],
  },
  {
    what: "run at the very start and the very end",
    text: "\nf\t",
    spans: [[1, 2, 1, "f", 1, 1]],
  },
  {
    what: "non-breaking space is not a space",
    text: `a${chr(0xa0)}b`,
    spans: [[2, 3, 1, "b", 1, 0]],
  },
  {
    what: "astral scalars on both sides of a rewritten run",
    text: "𝒜\n𝓧",
    spans: [[3, 5, 1, "X", 1, 0]],
  },
  {
    what: "a run of several whitespace kinds",
    text: "x \n\t y",
    spans: [[5, 6, 1, "y", 4, 0]],
  },
];

// ---------------------------------------------------------------- generate

const dedupe = (xs: string[]) => [...new Set(xs)];
const strings = dedupe(STRINGS);
const names = dedupe(NAMES);
const orderCorpus = dedupe(ORDER_CORPUS);

const pairs: [string, string][] = [];
for (const a of orderCorpus) for (const b of orderCorpus) pairs.push([a, b]);

const out = {
  README: [
    "Generated by crates/lean-doc-render/tests/gen-ts-expected.ts, which imports the",
    "function definitions sliced out of experiments/stage7d/render.ts and records",
    "their answers. Do not edit by hand; regenerate with",
    "`deno run --allow-read --allow-write crates/lean-doc-render/tests/gen-ts-expected.ts`.",
  ].join(" "),
  generatedFrom: "experiments/stage7d/render.ts",
  lineRanges: Object.fromEntries(
    Object.entries(RANGES).map(([k, v]) => [k, `${v.from}-${v.to}`]),
  ),
  deno: Deno.version.deno,
  v8: Deno.version.v8,
  escapeHtml: strings.map((s) => [s, prototype.escapeHtml(s)]),
  leanQuote: strings.map((s) => [s, prototype.leanQuote(s)]),
  stringLt: pairs.map(([a, b]) => [a, b, prototype.stringLt(a, b)]),
  nameLt: pairs.map(([a, b]) => [a, b, prototype.nameLt(a.split("."), b.split("."))]),
  nameSort: { input: names, output: prototype.sortNames([...names]) },
  // `Array.prototype.sort()` with no comparator: UTF-16 code unit order. This
  // is the sort every global artifact's byte layout depends on (plan §7, U1).
  utf16Sort: { input: strings, output: [...strings].sort() },
  applyWsWidths: WS_CASES.map((c) => {
    const before = prototype.sink.wsWidthChars;
    const output = prototype.applyWsWidths(c.text, c.spans);
    return { ...c, output, changed: prototype.sink.wsWidthChars - before };
  }),
};

const OUT = new URL("./data/ts-expected.json", import.meta.url);
const text = JSON.stringify(out, null, 1) + "\n";

if (Deno.args.includes("--check")) {
  const have = await Deno.readTextFile(OUT);
  if (have !== text) {
    console.error(`${OUT.pathname} is stale: re-run without --check`);
    Deno.exit(1);
  }
  console.log("ts-expected.json is up to date");
} else {
  await Deno.mkdir(new URL("./data/", import.meta.url), { recursive: true });
  await Deno.writeTextFile(OUT, text);
  console.log(
    `wrote ${OUT.pathname}: ${out.escapeHtml.length} strings, ${out.stringLt.length} order pairs, ` +
      `${out.utf16Sort.input.length} sorted, ${out.applyWsWidths.length} whitespace cases`,
  );
}
