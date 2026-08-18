#!/usr/bin/env -S deno run --allow-read --allow-write --allow-env
//
// `--allow-write` is only used by `--mismatches`, `--allow-env` only to read
// TARGET_REPO. Nothing is ever written to the measurement target.
//
// Diffs the constants litedoc4 collects from a signature (stage 3, `--refs`)
// against the links doc-gen4 actually put in its HTML, the same way
// `compare-modules.py` diffs the per-module collection against doc-gen4's
// database.
//
// The point of the comparison is stage 3's criterion (a): "links into Mathlib
// are correct", i.e. litedoc4 must reach the same targets doc-gen4 does. Note
// the direction: doc-gen4's HTML holds the *resolved* subset -- a constant whose
// name is not in `name2ModIdx` renders as `<span class="fn">` with no `href` --
// so the expected result is `html ⊆ refs`, not equality. A name in `html` that
// is missing from `refs` is a real defect; the other direction is the material
// increment 3 has to classify.
//
// Increment 1 compared *sets of names*. Increment 3 (`--decls`) drops to
// **(declaration, reference) pairs** and compares the generated `href` **string**
// with doc-gen4's, because "we reach the same names" is weaker than "we emit the
// same URL". Two URL strategies are generated and scored separately:
//
//   A. env      the defining module from `refs.jsonl` (Lean's `getModuleIdxFor?`,
//               which is what doc-gen4's `name2ModIdx` is) + the path rule
//   B. map      the `docLink` of `declaration-data.bmp`, i.e. no environment at
//               all on the dependency side -- the thing stage 3 is testing
//
// The path rule, read off `DocGen4/Output/Base.lean` (`getRoot`,
// `moduleNameToLink`, `declNameToLink`) and confirmed against the HTML:
//
//   href = "../" * (components(page module) - 1) + "./"
//        + module(target).replace(".", "/") + ".html#" + target
//
// usage:
//   compare-links.ts <refs.jsonl> [--doc <dir>] [--modules <list>]
//                    [--list-missing]
//                    [--decls <dump.jsonl>] [--bmp <file>] [--doc-root <dir>]
//                    [--mismatches <out.jsonl>] [--cap N] [--legacy-blocks]
//
//   <refs.jsonl>   output of `experiments/stage3/run.sh ... -- --refs --dump-refs`
//   --doc <dir>    doc-gen4's HTML for the package, default
//                  $TARGET_REPO/.lake/build/doc/InformationTheory
//   --modules      the target module list, default benchmarks/results/it-modules.txt
//   --decls        per-declaration dump (`--dump`), enables the increment-3
//                  per-pair URL comparison. 7 MB, so it is not committed:
//                  MODULES=$PWD/benchmarks/results/it-modules.txt RESULTS_DIR=<dir> \
//                    ./experiments/stage3/run.sh stage3-decls -- \
//                    --equations --refs --dump <dir>/stage3-decls-dump.jsonl
//   --bmp          doc-gen4's search index, default
//                  $TARGET_REPO/.lake/build/doc/declarations/declaration-data.bmp
//   --doc-root     doc root the `docLink`s are relative to, default
//                  $TARGET_REPO/.lake/build/doc (read only, to check anchors)
//   --mismatches   write the classified mismatches here as JSONL
//   --cap          per-class cap for that file (default 20)
//   --legacy-blocks  restore increment 1's block regexes (see below)
//
// Which HTML blocks count as "the signature" is the load-bearing choice here:
// a page's links also come from the docstring, the import list and the
// navigation, and counting those makes any match rate meaningless. Only these
// are read, and the declaration's own name link inside them is dropped:
//
//   div.decl_header             binders, result type, `extends` clause
//   ul.equations                the equation lemmas of a definition
//   div.structure_field_info    field types rendered inside their parent
//                               structure (`li.structure_field` under
//                               `--legacy-blocks`, which is what increment 1
//                               used -- see the note next to SOURCES)
//
// Blocks are delimited by walking tags with a depth counter, because they nest.
// Every such block is then attributed to the `div.decl` block that contains it;
// blocks that land outside every `div.decl` are counted and reported, since they
// would silently break the attribution.

const args = Deno.args.slice();
const refsPath = args.shift();
if (!refsPath || refsPath.startsWith("--")) {
  console.error("usage: compare-links.ts <refs.jsonl> [--doc <dir>] [--modules <list>] [--list-missing]");
  console.error("       [--decls <dump.jsonl>] [--bmp <file>] [--doc-root <dir>] [--mismatches <out.jsonl>] [--cap N] [--legacy-blocks]");
  Deno.exit(2);
}
const opt = (name: string, dflt: string) => {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : dflt;
};
const REPO = new URL("../..", import.meta.url).pathname.replace(/\/$/, "");
const TARGET = Deno.env.get("TARGET_REPO") ?? "/Users/haruka/dev/lean-projects";
const DOC = opt("--doc", `${TARGET}/.lake/build/doc/InformationTheory`);
const MODULES = opt("--modules", `${REPO}/benchmarks/results/it-modules.txt`);
const DECLS = opt("--decls", "");
const BMP = opt("--bmp", `${TARGET}/.lake/build/doc/declarations/declaration-data.bmp`);
const DOCROOT = opt("--doc-root", `${TARGET}/.lake/build/doc`);
const MISMATCHES = opt("--mismatches", "");
const CAP = Number(opt("--cap", "20"));
const listMissing = args.includes("--list-missing");

/**
 * Byte ranges of every `<tag ...>` block whose opening tag matches `open`,
 * plus the text of that opening tag (callers read attributes out of it).
 */
function blocks(html: string, open: RegExp, tag: string): [number, number, string][] {
  const out: [number, number, string][] = [];
  const both = new RegExp(`<${tag}\\b[^>]*>|</${tag}>`, "g");
  const re = new RegExp(open.source, "g");
  let m: RegExpExecArray | null;
  while ((m = re.exec(html))) {
    let depth = 0;
    both.lastIndex = m.index;
    let t: RegExpExecArray | null;
    while ((t = both.exec(html))) {
      if (t[0].startsWith("</")) {
        if (--depth === 0) {
          out.push([m.index, t.index, m[0]]);
          break;
        }
      } else depth++;
    }
  }
  return out;
}

const HREF = /<a\b[^>]*href="([^"]+)"/g;
// Increment 1 matched structure fields as `<li class="structure_field...">`.
// `DocGen4/Output/Structure.lean:fieldToHtml` emits `<li id={name}
// class="structure_field">` for a *direct* field and drops the `id` only in one
// branch of the inherited case, so that regex saw 4 of the 157 field blocks in
// this corpus. Matching `div.structure_field_info` instead is both
// order-independent and tighter: it is exactly the field's signature, without
// the sibling `div.structure_field_doc`, which is a docstring and out of scope.
// `--legacy-blocks` restores increment 1's regexes so its numbers stay
// reproducible from this tool.
const legacyBlocks = args.includes("--legacy-blocks");
const SOURCES: [RegExp, string, string][] = [
  [/<div class="decl_header"[^>]*>/, "div", "decl_header"],
  [/<ul class="equations"[^>]*>/, "ul", "equations"],
  legacyBlocks
    ? [/<li class="structure_field[^"]*"[^>]*>/, "li", "structure_field"]
    : [/<div class="structure_field_info"[^>]*>/, "div", "structure_field"],
];
const DECL_BLOCK = /<div class="decl" id="[^"]*"[^>]*>/;
// The field name is a link only in the inherited branch, where the `<a>` is the
// first child of `structure_field_info`; a direct field renders it as text.
const FIELD_NAME_LINK = /^<div class="structure_field_info"[^>]*>\s*<a\b[^>]*href="[^"]+"/;

const unescape = (s: string) =>
  s.replaceAll("&lt;", "<").replaceAll("&gt;", ">").replaceAll("&quot;", '"')
    .replaceAll("&#39;", "'").replaceAll("&amp;", "&");

async function* htmlFiles(dir: string): AsyncGenerator<string> {
  for await (const e of Deno.readDir(dir)) {
    if (e.isDirectory) yield* htmlFiles(`${dir}/${e.name}`);
    else if (e.name.endsWith(".html")) yield `${dir}/${e.name}`;
  }
}

/** `../.././Mathlib/A/B.html#Name` -> `Mathlib.A.B` / `Name`. */
function parseHref(href: string): { module: string; name: string } | null {
  const hash = href.indexOf("#");
  if (hash < 0) return null;
  const path = href.slice(0, hash).replace(/^(\.\.\/)*\.?\/?/, "").replace(/\.html$/, "");
  return { module: path.replaceAll("/", "."), name: href.slice(hash + 1) };
}

/** The three parts of a doc-gen4 declaration link, or null if it is not one. */
function splitHref(href: string): { prefix: string; path: string; anchor: string } | null {
  const m = /^((?:\.\.\/)*\.\/)(.*)\.html#(.*)$/.exec(href);
  return m ? { prefix: m[1], path: m[2], anchor: m[3] } : null;
}

/** `getRoot` in DocGen4/Output/Base.lean: one `../` per level below the root. */
const rootPrefix = (module: string) => "../".repeat(module.split(".").length - 1) + "./";

const htmlNames = new Set<string>();
const perSource = new Map<string, Set<string>>();
const htmlModules = new Set<string>();
let occurrences = 0;

// Increment 3: the same links, attributed to the `div.decl` they sit in.
type Target = { href: string; count: number; variants: Set<string> };
const htmlPairs = new Map<string, Map<string, Target>>(); // decl id -> anchor -> target
const declPage = new Map<string, string>(); // decl id -> module of the page it is on
let declBlocks = 0;
let nestedDecls = 0;
let duplicateDeclIds = 0;
let linksOutsideDecl = 0;
const outsideExamples: string[] = [];
// The self-links dropped above, kept as `decl id -> anchor`. They are not
// references, but they *are* `<a href>`s, so a (declaration, reference) pair
// that only exists as one of them must not be reported as "litedoc4 has a link
// doc-gen4 does not" -- that would be this tool's own filter talking.
const htmlSelfPairs = new Map<string, Set<string>>();

/** The `div.decl` range containing byte `pos`, or -1. Ranges are disjoint. */
function findDecl(ranges: [number, number, string][], pos: number): number {
  let lo = 0, hi = ranges.length - 1, best = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (ranges[mid][0] <= pos) {
      best = mid;
      lo = mid + 1;
    } else hi = mid - 1;
  }
  return best >= 0 && pos < ranges[best][1] ? best : -1;
}

for await (const path of htmlFiles(DOC)) {
  const pageModule = path.slice(DOC.lastIndexOf("/") + 1).replace(/\.html$/, "").replaceAll("/", ".");
  htmlModules.add(pageModule);
  const html = await Deno.readTextFile(path);

  const declRanges = blocks(html, DECL_BLOCK, "div");
  declBlocks += declRanges.length;
  const declIds = declRanges.map(([, , open]) => unescape(/id="([^"]*)"/.exec(open)![1]));
  for (let i = 0; i < declRanges.length; i++) {
    if (i > 0 && declRanges[i][0] < declRanges[i - 1][1]) nestedDecls++;
    if (declPage.has(declIds[i])) duplicateDeclIds++;
    declPage.set(declIds[i], pageModule);
    if (!htmlPairs.has(declIds[i])) htmlPairs.set(declIds[i], new Map());
  }

  for (const [open, tag, label] of SOURCES) {
    for (const [a, b] of blocks(html, open, tag)) {
      const seg = html.slice(a, b);
      // A declaration links to itself: `span.decl_name` in a header, and the
      // field name in front of the `:` in a structure field. Neither is a
      // reference to something else, so both are dropped.
      const own: [number, number][] = blocks(seg, /<span class="decl_name"[^>]*>/, "span")
        .map(([x, y]): [number, number] => [x, y]);
      if (legacyBlocks) {
        for (const [x, y] of blocks(seg, /<div class="structure_field_info"[^>]*>/, "div")) {
          const first = new RegExp(HREF.source).exec(seg.slice(x, y));
          if (first) own.push([x + first.index, x + first.index + first[0].length]);
        }
      } else {
        // anchored at 0, so an offset inside the match is an offset in `seg`
        const m = FIELD_NAME_LINK.exec(seg);
        if (m) own.push([m[0].lastIndexOf("<a"), m[0].length]);
      }
      HREF.lastIndex = 0;
      let h: RegExpExecArray | null;
      while ((h = HREF.exec(seg))) {
        const href = unescape(h[1]);
        const p = parseHref(href);
        if (own.some(([x, y]) => h!.index >= x && h!.index < y)) {
          const i = p ? findDecl(declRanges, a + h.index) : -1;
          if (i >= 0) {
            const s = htmlSelfPairs.get(declIds[i]) ?? new Set<string>();
            s.add(p!.name);
            htmlSelfPairs.set(declIds[i], s);
          }
          continue;
        }
        if (!p) continue;
        occurrences++;
        htmlNames.add(p.name);
        const s = perSource.get(label) ?? new Set<string>();
        s.add(p.name);
        perSource.set(label, s);

        const i = findDecl(declRanges, a + h.index);
        if (i < 0) {
          linksOutsideDecl++;
          if (outsideExamples.length < 5) outsideExamples.push(`${pageModule} ${label} -> ${p.name}`);
          continue;
        }
        const per = htmlPairs.get(declIds[i])!;
        const t = per.get(p.name);
        if (t) {
          t.count++;
          t.variants.add(href);
        } else per.set(p.name, { href, count: 1, variants: new Set([href]) });
      }
    }
  }
}

type Ref = { name: string; module: string | null; occurrences: number; own: boolean };
const refs: Ref[] = (await Deno.readTextFile(refsPath)).trim().split("\n").map((l) => JSON.parse(l));
const refNames = new Set(refs.map((r) => r.name));

const targetModules = new Set(
  (await Deno.readTextFile(MODULES)).split("\n").map((s) => s.trim()).filter(Boolean),
);
const covered = [...targetModules].filter((m) => htmlModules.has(m)).length;

const missing = [...htmlNames].filter((n) => !refNames.has(n)).sort();
const extra = [...refNames].filter((n) => !htmlNames.has(n)).sort();

const pct = (a: number, b: number) => (b === 0 ? "-" : `${((100 * a) / b).toFixed(1)}%`);
const num = (n: number) => n.toLocaleString("en-US");

console.log(`doc-gen4 HTML       ${DOC}`);
console.log(`litedoc4 refs       ${refsPath}`);
console.log();
console.log(`module coverage     ${covered} of ${targetModules.size} target modules have a page (${pct(covered, targetModules.size)})`);
if (covered < targetModules.size) {
  console.log(`                    the comparison below only covers those ${covered}`);
}
console.log();
console.log(`HTML link targets   ${htmlNames.size} unique, ${occurrences} occurrences`);
for (const [k, v] of perSource) console.log(`  ${k.padEnd(18)}${v.size} unique`);
console.log(`litedoc4 constants  ${refNames.size} unique (${refs.filter((r) => r.own).length} own, ${refs.filter((r) => !r.own).length} dependency)`);
console.log();
console.log(`in HTML, missing from litedoc4   ${missing.length}   <- defects`);
console.log(`in litedoc4, not linked in HTML  ${extra.length}   <- unresolved / not yet classified`);
console.log(`containment                      ${pct(htmlNames.size - missing.length, htmlNames.size)} of HTML targets are collected`);

if (listMissing) {
  for (const n of missing) console.log(`missing ${n}`);
  for (const n of extra) console.log(`extra   ${n}`);
}

// ---------------------------------------------------------------------------
// Increment 3: (declaration, reference) pairs and the generated URL string
// ---------------------------------------------------------------------------

if (!DECLS) Deno.exit(0);

type Decl = { name: string; module: string; refs: string[]; members: { label: string; name: string }[] };
const decls: Decl[] = (await Deno.readTextFile(DECLS)).trim().split("\n").map((l) => JSON.parse(l));
const byName = new Map(decls.map((d) => [d.name, d]));

// A Map, not the parsed object: declaration names are arbitrary strings and
// `obj["constructor"]` would answer for the prototype.
const bmp = new Map<string, { docLink: string }>(
  Object.entries(JSON.parse(await Deno.readTextFile(BMP)).declarations as Record<string, { docLink: string }>),
);
const refModule = new Map(refs.map((r) => [r.name, r.module]));

// Self-check: the per-declaration attribution has to reproduce the set-level
// numbers above. If it does not, some signature block sits outside every
// `div.decl` and the attribution is silently lossy.
const perDeclNames = new Set<string>();
for (const per of htmlPairs.values()) for (const n of per.keys()) perDeclNames.add(n);
const lostNames = [...htmlNames].filter((n) => !perDeclNames.has(n));
const perDeclMissing = [...perDeclNames].filter((n) => !refNames.has(n));

console.log();
console.log("=== increment 3: per-(declaration, reference) URL comparison ===");
console.log();
console.log("self-check (the per-decl attribution must lose nothing)");
console.log(`  block matching                       ${legacyBlocks ? "increment 1's regexes (--legacy-blocks)" : "fixed; rerun with --legacy-blocks for increment 1's"}`);
if (!legacyBlocks) {
  console.log(`    increment 1 matched structure fields as <li class="structure_field...">,`);
  console.log(`    which DocGen4/Output/Structure.lean only emits for one branch: it saw 4`);
  console.log(`    of the 157 field blocks here. Matching div.structure_field_info instead`);
  console.log(`    is order-independent and excludes the sibling field docstring.`);
}
console.log(`  div.decl blocks in the HTML          ${num(declBlocks)} (${num(declPage.size)} distinct ids, ${nestedDecls} nested, ${duplicateDeclIds} duplicate)`);
console.log(`  link targets, page-wide union        ${num(htmlNames.size)}`);
console.log(`  link targets, per-decl union         ${num(perDeclNames.size)}   ${lostNames.length === 0 ? "(same set)" : `LOST ${lostNames.length}`}`);
console.log(`  links in a signature block that is`);
console.log(`    outside every div.decl             ${num(linksOutsideDecl)}`);
for (const e of outsideExamples) console.log(`      e.g. ${e}`);
console.log(`  in HTML, missing from litedoc4       ${perDeclMissing.length}`);
if (lostNames.length) console.log(`  lost names: ${lostNames.slice(0, 10).join(" ")}`);

// --- population ------------------------------------------------------------
// The comparison can only speak about declarations that exist on both sides.
const population = decls.filter((d) => htmlPairs.has(d.name));
const dumpOnly = decls.filter((d) => !htmlPairs.has(d.name));
const htmlOnlyDecls = [...htmlPairs.keys()].filter((n) => !byName.has(n));

// Structure fields and constructors: the dump has them as their own records,
// but doc-gen4 renders them *inside* the parent's `div.decl` (as
// `li.structure_field`), so they never get a `div.decl` of their own. Their
// references are already part of the parent record's `refs` (the extractor
// folds `structureMembers` in), so dropping them here loses nothing.
const memberOf = new Map<string, string>();
for (const d of decls) for (const m of d.members ?? []) memberOf.set(m.name, d.name);
const dumpOnlyMembers = dumpOnly.filter((d) => memberOf.has(d.name));
const dumpOnlyNoPage = dumpOnly.filter((d) => !memberOf.has(d.name) && !htmlModules.has(d.module));
const dumpOnlyOther = dumpOnly.filter((d) => !memberOf.has(d.name) && htmlModules.has(d.module));

// Are the folded-in member references really in the parent's `refs`?
let memberRefsFolded = 0, memberRefsNotFolded = 0;
const notFoldedEx: string[] = [];
for (const d of dumpOnlyMembers) {
  const parent = byName.get(memberOf.get(d.name)!);
  if (!parent) continue;
  const pr = new Set(parent.refs);
  for (const r of new Set(d.refs)) {
    // the field's own type references; the field's *parent type* binder is not
    // rendered in the parent block, so only count what the parent block shows
    if (pr.has(r)) memberRefsFolded++;
    else {
      memberRefsNotFolded++;
      if (notFoldedEx.length < 5) notFoldedEx.push(`${d.name} -> ${r}`);
    }
  }
}

console.log();
console.log(`population (in the dump AND rendered as a div.decl; ${covered} of ${targetModules.size} modules)`);
console.log(`  declarations in the dump             ${num(decls.length)}`);
console.log(`  declarations with a div.decl         ${num(population.length)}   <- the population`);
console.log(`  dump only, rendered as a member of`);
console.log(`    their parent (field / ctor)        ${num(dumpOnlyMembers.length)}`);
console.log(`  dump only, module has no page        ${num(dumpOnlyNoPage.length)}`);
console.log(`  dump only, module has a page         ${num(dumpOnlyOther.length)}`);
console.log(`  div.decl with no dump record         ${num(htmlOnlyDecls.length)}`);
console.log(`  member refs also in the parent record  ${num(memberRefsFolded)} of ${num(memberRefsFolded + memberRefsNotFolded)} (${pct(memberRefsFolded, memberRefsFolded + memberRefsNotFolded)})`);
for (const e of notFoldedEx) console.log(`      only in the member record: ${e}`);
console.log(`    (the rest are refs of the member's own *signature*, which the parent`);
console.log(`     block does not render -- what matters is that section 4 stays at 0.)`);
if (htmlOnlyDecls.length) console.log(`  e.g. div.decl with no record: ${htmlOnlyDecls.slice(0, 5).join(" ")}`);

// page module of the dump vs page the HTML actually put the declaration on
let pageDisagree = 0;
const pageDisagreeEx: string[] = [];
for (const d of population) {
  if (declPage.get(d.name) !== d.module) {
    pageDisagree++;
    if (pageDisagreeEx.length < 5) pageDisagreeEx.push(`${d.name}: dump ${d.module}, HTML ${declPage.get(d.name)}`);
  }
}
console.log(`  page module disagreements            ${pageDisagree}`);
for (const e of pageDisagreeEx) console.log(`      ${e}`);

// --- pairs -----------------------------------------------------------------
// NUL separator: Lean names can contain spaces (notation such as `«term_ + _»`).
const SEP = "\u0000";
const key = (d: string, n: string) => d + SEP + n;
const htmlPairKeys = new Set<string>();
let htmlPairOccurrences = 0;
let multiHref = 0;
const multiHrefEx: string[] = [];
for (const d of population) {
  for (const [n, t] of htmlPairs.get(d.name)!) {
    htmlPairKeys.add(key(d.name, n));
    htmlPairOccurrences += t.count;
    if (t.variants.size > 1) {
      multiHref++;
      if (multiHrefEx.length < 3) multiHrefEx.push(`${d.name} -> ${n}: ${[...t.variants].join(" | ")}`);
    }
  }
}
const litedoc4PairKeys = new Set<string>();
for (const d of population) for (const r of new Set(d.refs)) litedoc4PairKeys.add(key(d.name, r));

const common = [...htmlPairKeys].filter((k) => litedoc4PairKeys.has(k));
const htmlOnlyPairs = [...htmlPairKeys].filter((k) => !litedoc4PairKeys.has(k));
const litedoc4OnlyPairs = [...litedoc4PairKeys].filter((k) => !htmlPairKeys.has(k));

console.log();
console.log("1. pairs");
console.log(`  HTML       ${num(htmlPairKeys.size)} distinct (declaration, reference) pairs, ${num(htmlPairOccurrences)} <a> occurrences`);
console.log(`  litedoc4   ${num(litedoc4PairKeys.size)} distinct pairs`);
console.log(`  common     ${num(common.length)}   (${pct(common.length, htmlPairKeys.size)} of HTML, ${pct(common.length, litedoc4PairKeys.size)} of litedoc4)`);
console.log(`  pairs whose <a> occurrences do not all share one href: ${multiHref}`);
for (const e of multiHrefEx) console.log(`      ${e}`);
// The `../` prefix is the only part of the rule that depends on the page, so
// record how many depths the comparison actually exercised.
const depths = new Map<number, number>();
for (const d of population) {
  const n = declPage.get(d.name)!.split(".").length;
  depths.set(n, (depths.get(n) ?? 0) + 1);
}
console.log(
  `  page depths exercised: ${
    [...depths].sort((a, b) => a[0] - b[0])
      .map(([n, c]) => `${"../".repeat(n - 1)}./ ${num(c)} decls`).join(", ")
  }`,
);

// --- URL generation and exact-string comparison ----------------------------
type Row = {
  cls: string;
  strategy: string;
  decl: string;
  ref: string;
  actual: string;
  generated: string | null;
  /** For the pairs of section 5: does the generated URL resolve? */
  fate?: string;
};
const rows: Row[] = [];
const capped = new Map<string, number>();
const push = (r: Row) => {
  const k = `${r.strategy}/${r.cls}`;
  const n = capped.get(k) ?? 0;
  capped.set(k, n + 1);
  if (n < CAP) rows.push(r);
};

function classify(actual: string, generated: string): string {
  const a = splitHref(actual), g = splitHref(generated);
  if (!a || !g) return "unparsable href";
  const diff: string[] = [];
  if (a.prefix !== g.prefix) diff.push("prefix");
  if (a.path !== g.path) diff.push("module path");
  if (a.anchor !== g.anchor) diff.push("anchor");
  return diff.join(" + ");
}

const score = { A: new Map<string, number>(), B: new Map<string, number>() };
const bump = (s: "A" | "B", c: string) => score[s].set(c, (score[s].get(c) ?? 0) + 1);
const examples = new Map<string, string[]>();
const addExample = (s: string, c: string, line: string) => {
  const k = `${s}/${c}`;
  const l = examples.get(k) ?? [];
  if (l.length < 3) l.push(line);
  examples.set(k, l);
};

for (const k of common) {
  const [decl, ref] = k.split(SEP);
  const actual = htmlPairs.get(decl)!.get(ref)!.href;
  const prefix = rootPrefix(declPage.get(decl)!);

  const mod = refModule.get(ref);
  if (!mod) {
    bump("A", "no defining module in refs.jsonl");
    addExample("A", "no defining module in refs.jsonl", `${decl} -> ${ref}`);
    push({ cls: "no defining module in refs.jsonl", strategy: "A", decl, ref, actual, generated: null });
  } else {
    const gen = `${prefix}${mod.replaceAll(".", "/")}.html#${ref}`;
    if (gen === actual) bump("A", "exact match");
    else {
      const c = classify(actual, gen);
      bump("A", c);
      addExample("A", c, `${decl} -> ${ref}\n        HTML ${actual}\n        env  ${gen}`);
      push({ cls: c, strategy: "A", decl, ref, actual, generated: gen });
    }
  }

  const entry = bmp.get(ref);
  if (!entry) {
    bump("B", "not in the .bmp");
    addExample("B", "not in the .bmp", `${decl} -> ${ref}   HTML ${actual}`);
    push({ cls: "not in the .bmp", strategy: "B", decl, ref, actual, generated: null });
  } else {
    const gen = prefix + entry.docLink.replace(/^\.\//, "");
    if (gen === actual) bump("B", "exact match");
    else {
      const c = classify(actual, gen);
      bump("B", c);
      addExample("B", c, `${decl} -> ${ref}\n        HTML ${actual}\n        map  ${gen}`);
      push({ cls: c, strategy: "B", decl, ref, actual, generated: gen });
    }
  }
}

console.log();
console.log("2. exact href match on the common pairs");
for (const [s, label] of [["A", "A. env  (refs.jsonl module + path rule)"], ["B", "B. map  (declaration-data.bmp docLink + prefix)"]] as const) {
  const m = score[s as "A" | "B"];
  const hit = m.get("exact match") ?? 0;
  console.log(`  ${label}`);
  console.log(
    `      exact match   ${num(hit)} of ${num(common.length)}   ` +
      `${((100 * hit) / common.length).toFixed(3)}%   (miss ${num(common.length - hit)})`,
  );
}
console.log(`  Read strategy A for what it is: doc-gen4's own name2ModIdx *is*`);
console.log(`  env.const2ModIdx (DocGen4/Process/Analyze.lean:243), the map litedoc4 read`);
console.log(`  with getModuleIdxFor?. A therefore tests the path rule, not the module`);
console.log(`  lookup -- the two sides share that input. B is the independent one.`);
console.log();
console.log("3. mismatch classification (common pairs only)");
for (const s of ["A", "B"] as const) {
  console.log(`  strategy ${s}`);
  const entries = [...score[s]].filter(([c]) => c !== "exact match").sort((a, b) => b[1] - a[1]);
  if (entries.length === 0) console.log("      (none)");
  for (const [c, n] of entries) {
    console.log(`      ${num(n).padStart(6)}  ${c}`);
    for (const e of examples.get(`${s}/${c}`) ?? []) console.log(`        ${e}`);
  }
}

// --- 4. pairs the HTML has and litedoc4 does not ---------------------------
// doc-gen4 does not always link the constant it was given: `findLinkableParent`
// (DocGen4/Output/Base.lean) strips trailing components until the remainder is
// in `name2ModIdx`, so the anchor can be an ancestor of the collected name.
const isAncestor = (anchor: string, ref: string) => ref.startsWith(anchor + ".");
const htmlOnlyClass = new Map<string, number>();
const htmlOnlyEx = new Map<string, string[]>();
for (const k of htmlOnlyPairs) {
  const [decl, anchor] = k.split(SEP);
  const declRefs = new Set(byName.get(decl)!.refs);
  let c: string;
  if ([...declRefs].some((r) => isAncestor(anchor, r))) c = "doc-gen4 linked an ancestor of a collected ref";
  else if (refNames.has(anchor)) c = "collected by litedoc4, but on another declaration";
  else c = "never collected by litedoc4 (defect)";
  htmlOnlyClass.set(c, (htmlOnlyClass.get(c) ?? 0) + 1);
  const l = htmlOnlyEx.get(c) ?? [];
  if (l.length < 3) l.push(`${decl} -> ${anchor}   ${htmlPairs.get(decl)!.get(anchor)!.href}`);
  htmlOnlyEx.set(c, l);
  push({ cls: c, strategy: "html-only", decl, ref: anchor, actual: htmlPairs.get(decl)!.get(anchor)!.href, generated: null });
}
console.log();
console.log(`4. in the HTML, not in litedoc4: ${num(htmlOnlyPairs.length)} pairs   <- defect candidates`);
for (const [c, n] of [...htmlOnlyClass].sort((a, b) => b[1] - a[1])) {
  console.log(`      ${num(n).padStart(6)}  ${c}`);
  for (const e of htmlOnlyEx.get(c) ?? []) console.log(`        ${e}`);
}

// --- 5. pairs litedoc4 has and the HTML does not ---------------------------
// Two independent questions are asked about these, and mixing them is how this
// section would start lying:
//
//   (i)  *why* is there no <a> -- a mechanism, only claimed when it is provable
//        from the name or from what this tool itself dropped;
//   (ii) *would it be a dead link* -- does the page the URL points at exist and
//        does it carry that anchor. This is the retreat-line material, and it is
//        reported over every pair litedoc4 has, not just these.
//
// The tempting third class, "doc-gen4 linked an ancestor of it instead", is
// deliberately not a class: an ancestor anchor being present in the same block
// is equally well explained by an unrelated reference, so it is printed as a
// note. doc-gen4's real reason is usually `innerHasAnchor` in
// `renderedCodeToHtmlAux` (a .const whose subtree already produced an <a> is
// left unwrapped), which leaves no trace in the HTML at all.

/** Anchors of the page a module would be rendered to; null = no such page. */
const anchorsOf = new Map<string, Set<string> | null>();
async function anchors(module: string): Promise<Set<string> | null> {
  if (anchorsOf.has(module)) return anchorsOf.get(module)!;
  let ids: Set<string> | null = null;
  try {
    const text = await Deno.readTextFile(`${DOCROOT}/${module.replaceAll(".", "/")}.html`);
    ids = new Set<string>();
    for (const m of text.matchAll(/\sid="([^"]*)"/g)) ids.add(unescape(m[1]));
  } catch { /* page absent from this partial build */ }
  anchorsOf.set(module, ids);
  return ids;
}

/** live | dead | unbuilt | no-module, for the URL litedoc4 would emit. */
type Fate = "live" | "dead" | "unbuilt" | "no-module";
const fateCache = new Map<string, Fate>();
async function fate(ref: string): Promise<Fate> {
  const hit = fateCache.get(ref);
  if (hit) return hit;
  const mod = refModule.get(ref);
  let f: Fate;
  if (!mod) f = "no-module";
  else {
    const ids = await anchors(mod);
    f = ids === null ? "unbuilt" : ids.has(ref) ? "live" : "dead";
  }
  fateCache.set(ref, f);
  return f;
}

const isAux = (n: string) =>
  n.split(".").some((c) => c.startsWith("_") || /^\d+$/.test(c));

const leanOnlyClass = new Map<string, number>();
const leanOnlyEx = new Map<string, string[]>();
let ancestorPresent = 0;
for (const k of litedoc4OnlyPairs) {
  const [decl, ref] = k.split(SEP);
  const per = htmlPairs.get(decl)!;
  const mod = refModule.get(ref) ?? null;
  let c: string;
  if (htmlSelfPairs.get(decl)?.has(ref)) {
    c = "rendered, but as the declaration's own name link (dropped by this tool)";
  } else if (ref.startsWith("_private.")) {
    c = "private name (doc-gen4 links these to the module page or gives up)";
  } else if (isAux(ref)) {
    c = "auxiliary name (_proof_*, match_*, numeric component)";
  } else {
    c = `no <a> at all (${targetModules.has(mod ?? "") ? "own package" : "dependency"}, ` +
      `${bmp.has(ref) ? "in the .bmp" : "NOT in the .bmp"})`;
    if ([...per.keys()].some((anchor) => isAncestor(anchor, ref))) ancestorPresent++;
  }
  leanOnlyClass.set(c, (leanOnlyClass.get(c) ?? 0) + 1);
  const l = leanOnlyEx.get(c) ?? [];
  if (l.length < 3) l.push(`${decl} -> ${ref}   module ${mod ?? "-"}, ${await fate(ref)}`);
  leanOnlyEx.set(c, l);
  push({
    cls: c,
    strategy: "litedoc4-only",
    decl,
    ref,
    actual: "",
    generated: mod ? `${rootPrefix(declPage.get(decl)!)}${mod.replaceAll(".", "/")}.html#${ref}` : null,
    fate: await fate(ref),
  });
}
console.log();
console.log(`5. in litedoc4, not linked in the HTML: ${num(litedoc4OnlyPairs.length)} pairs`);
for (const [c, n] of [...leanOnlyClass].sort((a, b) => b[1] - a[1])) {
  console.log(`      ${num(n).padStart(6)}  ${c}`);
  for (const e of leanOnlyEx.get(c) ?? []) console.log(`        ${e}`);
}
console.log(`   note: of the "no <a> at all" pairs, ${num(ancestorPresent)} have an ancestor of the name`);
console.log(`   anchored in the same block. Suggestive of findLinkableParent, not proof.`);

// (ii) dead-link exposure: what happens if litedoc4 emits every link it can.
console.log();
console.log("5b. dead-link exposure -- would the URL litedoc4 emits resolve?");
console.log("    (anchor = any id= on the target page. Pages absent from this 42%-cut");
console.log("    doc build are 'not built', which is a property of the build, not of the map.)");
const buckets = ["live", "dead", "unbuilt", "no-module"] as const;
for (const [label, keys] of [
  ["all pairs litedoc4 would emit (population)", [...litedoc4PairKeys]],
  ["  of those, the ones doc-gen4 also linked", common],
  ["  of those, the ones doc-gen4 did not link", litedoc4OnlyPairs],
] as const) {
  const byFate = new Map<Fate, number>();
  const namesByFate = new Map<Fate, Set<string>>();
  for (const k of keys) {
    const ref = k.split(SEP)[1];
    const f = await fate(ref);
    byFate.set(f, (byFate.get(f) ?? 0) + 1);
    const s = namesByFate.get(f) ?? new Set<string>();
    s.add(ref);
    namesByFate.set(f, s);
  }
  console.log(`  ${label}: ${num(keys.length)} pairs`);
  for (const f of buckets) {
    if (!byFate.get(f)) continue;
    console.log(`      ${num(byFate.get(f)!).padStart(6)} pairs / ${num(namesByFate.get(f)!.size).toString().padStart(4)} names   ${f}`);
  }
}
const deadNames = new Set<string>();
for (const k of litedoc4PairKeys) {
  const ref = k.split(SEP)[1];
  if (await fate(ref) === "dead") deadNames.add(ref);
}
const deadDep = [...deadNames].filter((n) => !targetModules.has(refModule.get(n) ?? ""));
const deadOwn = [...deadNames].filter((n) => targetModules.has(refModule.get(n) ?? ""));
console.log(`  dead names, dependency side: ${num(deadDep.length)}`);
for (const n of deadDep) console.log(`      ${n}   in .bmp: ${bmp.has(n) ? "yes" : "no"}, doc-gen4 links it: ${common.some((k) => k.split(SEP)[1] === n) ? "yes" : "no"}`);
console.log(`  dead names, own package:     ${num(deadOwn.length)} (${deadOwn.filter((n) => n.startsWith("_private.")).length} private)`);
for (const n of deadOwn.slice(0, 4)) console.log(`      ${n}`);
const depNames = new Set(
  [...litedoc4PairKeys].map((k) => k.split(SEP)[1]).filter((n) => !targetModules.has(refModule.get(n) ?? "")),
);
const depNotInBmp = [...depNames].filter((n) => !bmp.has(n));
console.log(`  dependency names referenced from the population: ${num(depNames.size)}`);
console.log(`  of those absent from the .bmp: ${num(depNotInBmp.length)}  ${depNotInBmp.join(" ")}`);
console.log(`  -> with strategy B those simply produce no link; with strategy A they`);
console.log(`     produce doc-gen4's link, whatever its fate above says. Absence from`);
console.log(`     the .bmp is exactly doc-gen4's own blacklist: a name it refuses to`);
console.log(`     give a page also never enters the index it ships.`);

if (MISMATCHES) {
  await Deno.writeTextFile(MISMATCHES, rows.map((r) => JSON.stringify(r)).join("\n") + "\n");
  const over = [...capped].filter(([, n]) => n > CAP);
  console.log();
  console.log(`mismatch records -> ${MISMATCHES}`);
  console.log(`  ${num(rows.length)} of ${num([...capped.values()].reduce((a, b) => a + b, 0))} records, capped at ${CAP} per (strategy, class)`);
  for (const [k, n] of over.sort((a, b) => b[1] - a[1])) console.log(`    truncated: ${k} (${num(n)} total)`);
}
