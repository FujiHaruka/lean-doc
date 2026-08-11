#!/usr/bin/env -S deno run --allow-read --allow-write
// v6-token-separators.ts -- measure verification item V6 (plan §8).
//
// WHY THIS FILE EXISTS
// --------------------
// `autolinkTokens` splits the inside of a code span on a separator set. The
// frozen prototype (`experiments/stage7h/global.ts:120`) uses **V8's**
// `/[\p{Z}\p{C}]/u`; the Rust port uses **UnicodeBasic's** `Z | C`, because that
// is the table the renderer's `autoLinkInline` splits on. The tokens are the
// filter in front of the whole-package map delta, so a code point that is a
// separator for one table and not the other is a place where the two
// implementations disagree about which modules are stale.
//
// This script measures the disagreement rather than believing it:
//
//   1. the symmetric difference of the two sets, per direction, over the whole
//      code point space (0..=0x10FFFF);
//   2. how many of those code points actually occur **inside a code span** in
//      the target package's declaration docstrings.
//
// V8's answer is taken from this runtime. UnicodeBasic's is read out of
// `crates/lean-doc-md/src/gc.rs`, which is generated from the build doc-gen4
// links by `crates/lean-doc-md/tests/oracle/gen-gc-table.ts` — not a second copy
// of the UCD.
//
// npm/node are broken in this environment; this must run under deno.
//
// usage:
//   deno run --allow-read --allow-write benchmarks/tools/v6-token-separators.ts
//   ... --ir <dir>       the IR tree to scan   (default: the w7h base IR)
//   ... --out <file>     where to write the log (default: the path below)

const REPO = new URL("../..", import.meta.url).pathname;
const DEFAULT_IR = "/private/tmp/lean-doc-relay/w7h/base-ir";
const DEFAULT_OUT = `${REPO}benchmarks/results/m2b-v6-token-separators.json`;
const GC_RS = `${REPO}crates/lean-doc-md/src/gc.rs`;

const args = Deno.args;
const flag = (name: string, fallback: string) => {
  const at = args.indexOf(name);
  return at >= 0 && at + 1 < args.length ? args[at + 1] : fallback;
};
const irRoot = flag("--ir", DEFAULT_IR);
const outPath = flag("--out", DEFAULT_OUT);

// ------------------------------------------------- UnicodeBasic's Z | C table

/** The `Z_C` ranges of the generated `gc.rs`, as a membership predicate. */
function readUnicodeBasicZC(source: string): (cp: number) => boolean {
  const at = source.indexOf("static Z_C:");
  if (at < 0) throw new Error("gc.rs has no Z_C table");
  const body = source.slice(at, source.indexOf("];", at));
  const ranges: [number, number][] = [];
  for (const m of body.matchAll(/\(0x([0-9A-Fa-f]+), 0x([0-9A-Fa-f]+)\)/g)) {
    ranges.push([parseInt(m[1], 16), parseInt(m[2], 16)]);
  }
  const declared = body.match(/\[\(u32, u32\); (\d+)\]/);
  if (!declared || Number(declared[1]) !== ranges.length) {
    throw new Error(`gc.rs declares ${declared?.[1]} Z_C ranges, parsed ${ranges.length}`);
  }
  const member = new Uint8Array(0x110000);
  for (const [lo, hi] of ranges) for (let cp = lo; cp <= hi; cp++) member[cp] = 1;
  return (cp: number) => member[cp] === 1;
}

const gcSource = await Deno.readTextFile(GC_RS);
const isUnicodeBasicZC = readUnicodeBasicZC(gcSource);

// ------------------------------------------------------------ V8's \p{Z}\p{C}

const V8_ZC = /[\p{Z}\p{C}]/u;
const isV8ZC = (cp: number) => V8_ZC.test(String.fromCodePoint(cp));

// --------------------------------------------------- the symmetric difference

/** Code points one table calls a separator and the other does not. */
const v8Only: number[] = [];
const unicodeBasicOnly: number[] = [];
let v8Total = 0;
let unicodeBasicTotal = 0;
let surrogateDisagreements = 0;
for (let cp = 0; cp <= 0x10ffff; cp++) {
  const a = isV8ZC(cp);
  const b = isUnicodeBasicZC(cp);
  if (a) v8Total++;
  if (b) unicodeBasicTotal++;
  if (a === b) continue;
  if (cp >= 0xd800 && cp <= 0xdfff) surrogateDisagreements++;
  (a ? v8Only : unicodeBasicOnly).push(cp);
}

/** `[lo, hi]` runs of a sorted code point list, for a log a human can read. */
function toRanges(points: number[]): [number, number][] {
  const out: [number, number][] = [];
  for (const cp of points) {
    const last = out[out.length - 1];
    if (last && last[1] + 1 === cp) last[1] = cp;
    else out.push([cp, cp]);
  }
  return out;
}

const disagreeing = new Set([...v8Only, ...unicodeBasicOnly]);

// ------------------------------------------- occurrences in the target package

/** The prototype's own two regexes (`global.ts:119-123`). */
const CODE_SPAN = /`([^`\n]+)`/g;

interface Occurrence {
  module: string;
  decl: string;
  codePoint: number;
  direction: "v8Only" | "unicodeBasicOnly";
}

const index = JSON.parse(await Deno.readTextFile(`${irRoot}/index.json`));
const occurrences: Occurrence[] = [];
let modulesScanned = 0;
let declsScanned = 0;
let docstringsScanned = 0;
let codeSpansScanned = 0;
let codeSpanCodePoints = 0;
for (const entry of index.modules) {
  modulesScanned++;
  const module = JSON.parse(await Deno.readTextFile(`${irRoot}/${entry.file}`));
  for (const decl of module.declarations ?? []) {
    declsScanned++;
    if (!decl.doc) continue;
    docstringsScanned++;
    for (const m of decl.doc.matchAll(CODE_SPAN)) {
      codeSpansScanned++;
      for (const ch of m[1]) {
        codeSpanCodePoints++;
        const cp = ch.codePointAt(0)!;
        if (!disagreeing.has(cp)) continue;
        occurrences.push({
          module: entry.module,
          decl: decl.name,
          codePoint: cp,
          direction: v8Only.includes(cp) ? "v8Only" : "unicodeBasicOnly",
        });
      }
    }
  }
}

// ------------------------------------------------------------------- the log

const hex = (cp: number) => `U+${cp.toString(16).toUpperCase().padStart(4, "0")}`;
const record = {
  measurement: "V6: the two autolinkTokens separator sets",
  date: new Date().toISOString().slice(0, 10),
  label: "実測",
  runtime: { deno: Deno.version.deno, v8: Deno.version.v8 },
  unicodeBasicFrom: "crates/lean-doc-md/src/gc.rs (generated from the build doc-gen4 links)",
  sets: {
    v8Members: v8Total,
    unicodeBasicMembers: unicodeBasicTotal,
    agree: 0x110000 - disagreeing.size,
    disagree: disagreeing.size,
    // The direction that costs correctness: V8 splits here and UnicodeBasic
    // does not, so the Rust port keeps a longer token and never offers the
    // pieces the prototype would have offered.
    v8OnlySeparator: v8Only.length,
    unicodeBasicOnlySeparator: unicodeBasicOnly.length,
    surrogateDisagreements,
  },
  v8OnlyRanges: toRanges(v8Only).map(([lo, hi]) => [hex(lo), hex(hi)]),
  unicodeBasicOnlyRanges: toRanges(unicodeBasicOnly).map(([lo, hi]) => [hex(lo), hex(hi)]),
  corpus: {
    ir: irRoot,
    modulesScanned,
    declsScanned,
    docstringsScanned,
    codeSpansScanned,
    codeSpanCodePoints,
    occurrences: occurrences.length,
    where: occurrences.slice(0, 50),
  },
};

await Deno.writeTextFile(outPath, `${JSON.stringify(record, null, 2)}\n`);
console.error(
  [
    `V8 |Z∪C| = ${v8Total}, UnicodeBasic |Z∪C| = ${unicodeBasicTotal}`,
    `disagree on ${disagreeing.size} code points`,
    `  V8 only (Rust under-splits): ${v8Only.length}`,
    `  UnicodeBasic only:           ${unicodeBasicOnly.length}`,
    `corpus: ${modulesScanned} modules, ${declsScanned} declarations, ` +
      `${docstringsScanned} docstrings, ${codeSpansScanned} code spans, ` +
      `${codeSpanCodePoints} code points inside them`,
    `  occurrences of a disagreeing code point: ${occurrences.length}`,
    `-> ${outPath}`,
  ].join("\n"),
);
