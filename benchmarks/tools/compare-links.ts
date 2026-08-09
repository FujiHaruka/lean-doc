#!/usr/bin/env -S deno run --allow-read
//
// Diffs the constants lean-doc collects from a signature (stage 3, `--refs`)
// against the links doc-gen4 actually put in its HTML, the same way
// `compare-modules.py` diffs the per-module collection against doc-gen4's
// database.
//
// The point of the comparison is stage 3's criterion (a): "links into Mathlib
// are correct", i.e. lean-doc must reach the same targets doc-gen4 does. Note
// the direction: doc-gen4's HTML holds the *resolved* subset -- a constant whose
// name is not in `name2ModIdx` renders as `<span class="fn">` with no `href` --
// so the expected result is `html ⊆ refs`, not equality. A name in `html` that
// is missing from `refs` is a real defect; the other direction is the material
// increment 3 has to classify.
//
// usage:
//   compare-links.ts <refs.jsonl> [--doc <dir>] [--modules <list>] [--list-missing]
//
//   <refs.jsonl>   output of `experiments/stage3/run.sh ... -- --refs --dump-refs`
//   --doc <dir>    doc-gen4's HTML for the package, default
//                  $TARGET_REPO/.lake/build/doc/InformationTheory
//   --modules      the target module list, default benchmarks/results/it-modules.txt
//
// Which HTML blocks count as "the signature" is the load-bearing choice here:
// a page's links also come from the docstring, the import list and the
// navigation, and counting those makes any match rate meaningless. Only these
// are read, and the declaration's own name link inside them is dropped:
//
//   div.decl_header        binders, result type, `extends` clause
//   ul.equations           the equation lemmas of a definition
//   li.structure_field     field types rendered inside their parent structure
//
// Blocks are delimited by walking tags with a depth counter, because they nest.

const args = Deno.args.slice();
const refsPath = args.shift();
if (!refsPath || refsPath.startsWith("--")) {
  console.error("usage: compare-links.ts <refs.jsonl> [--doc <dir>] [--modules <list>] [--list-missing]");
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
const listMissing = args.includes("--list-missing");

/** Byte ranges of every `<tag ...>` block whose opening tag matches `open`. */
function blocks(html: string, open: RegExp, tag: string): [number, number][] {
  const out: [number, number][] = [];
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
          out.push([m.index, t.index]);
          break;
        }
      } else depth++;
    }
  }
  return out;
}

const HREF = /<a\b[^>]*href="([^"]+)"/g;
const SOURCES: [RegExp, string, string][] = [
  [/<div class="decl_header"[^>]*>/, "div", "decl_header"],
  [/<ul class="equations"[^>]*>/, "ul", "equations"],
  [/<li class="structure_field[^"]*"[^>]*>/, "li", "structure_field"],
];

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

const htmlNames = new Set<string>();
const perSource = new Map<string, Set<string>>();
const htmlModules = new Set<string>();
let occurrences = 0;

for await (const path of htmlFiles(DOC)) {
  htmlModules.add(path.slice(DOC.lastIndexOf("/") + 1).replace(/\.html$/, "").replaceAll("/", "."));
  const html = await Deno.readTextFile(path);
  for (const [open, tag, label] of SOURCES) {
    for (const [a, b] of blocks(html, open, tag)) {
      const seg = html.slice(a, b);
      // A declaration links to itself: `span.decl_name` in a header, and the
      // field name in front of the `:` in a structure field. Neither is a
      // reference to something else, so both are dropped.
      const own = blocks(seg, /<span class="decl_name"[^>]*>/, "span");
      for (const [x, y] of blocks(seg, /<div class="structure_field_info"[^>]*>/, "div")) {
        const first = new RegExp(HREF.source).exec(seg.slice(x, y));
        if (first) own.push([x + first.index, x + first.index + first[0].length]);
      }
      HREF.lastIndex = 0;
      let h: RegExpExecArray | null;
      while ((h = HREF.exec(seg))) {
        if (own.some(([x, y]) => h!.index >= x && h!.index < y)) continue;
        const p = parseHref(h[1]);
        if (!p) continue;
        occurrences++;
        htmlNames.add(p.name);
        const s = perSource.get(label) ?? new Set<string>();
        s.add(p.name);
        perSource.set(label, s);
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

console.log(`doc-gen4 HTML       ${DOC}`);
console.log(`lean-doc refs       ${refsPath}`);
console.log();
console.log(`module coverage     ${covered} of ${targetModules.size} target modules have a page (${pct(covered, targetModules.size)})`);
if (covered < targetModules.size) {
  console.log(`                    the comparison below only covers those ${covered}`);
}
console.log();
console.log(`HTML link targets   ${htmlNames.size} unique, ${occurrences} occurrences`);
for (const [k, v] of perSource) console.log(`  ${k.padEnd(18)}${v.size} unique`);
console.log(`lean-doc constants  ${refNames.size} unique (${refs.filter((r) => r.own).length} own, ${refs.filter((r) => !r.own).length} dependency)`);
console.log();
console.log(`in HTML, missing from lean-doc   ${missing.length}   <- defects`);
console.log(`in lean-doc, not linked in HTML  ${extra.length}   <- unresolved / not yet classified`);
console.log(`containment                      ${pct(htmlNames.size - missing.length, htmlNames.size)} of HTML targets are collected`);

if (listMissing) {
  for (const n of missing) console.log(`missing ${n}`);
  for (const n of extra) console.log(`extra   ${n}`);
}
