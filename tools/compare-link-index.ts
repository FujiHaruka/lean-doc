#!/usr/bin/env -S deno run --allow-read --allow-write
// compare-link-index.ts -- set-compare two `.lidx` name -> module maps.
//
// WHY THIS EXISTS
// ---------------
// M5 replaces the producer of the renderer's `name -> module` map: it used to
// be derived from a doc-gen4 site's `declarations/declaration-data.bmp`
// (`experiments/stage7d/build-link-index.ts`), and it is now a scan of the
// environment the extractor already holds (`extractor/Extract.lean`,
// `writeLinkIndex`). Verification item V1 asks whether the two agree, and the
// question that matters most is not "how many names moved" but "does any name
// they share point at a *different* module" — that failure mode is a link that
// silently goes somewhere else rather than a link that disappears.
//
// The classification of the differences is coarse on purpose: it names the
// buckets a reader can act on (a whole module missing on one side is a
// different problem from a name missing inside a module both sides have).
//
// usage: compare-link-index.ts --a <a.lidx> --b <b.lidx> [--out <report.md>]
//                              [--samples <n>] [--dump-dir <dir>]

const argv = Deno.args.slice();
const opt = (name: string, dflt = "") => {
  const i = argv.indexOf(name);
  return i >= 0 ? argv[i + 1] : dflt;
};
const A = opt("--a");
const B = opt("--b");
const OUT = opt("--out");
const DUMP = opt("--dump-dir");
const SAMPLES = Number(opt("--samples", "10"));
if (!A || !B) {
  console.error("usage: compare-link-index.ts --a <a.lidx> --b <b.lidx> [--out <p>]");
  Deno.exit(2);
}

type Index = {
  names: Map<string, string>;
  modules: Set<string>; // group headers
  known: Set<string>; // the `@` section
};

/** The parser is the renderer's (`crates/lean-doc-render/src/link_index.rs`):
 * first byte decides, no error path, a repeated name takes the later module. */
function parse(text: string): Index {
  const names = new Map<string, string>();
  const modules = new Set<string>();
  const known = new Set<string>();
  let current = "";
  for (const line of text.split("\n")) {
    if (line.length === 0) continue;
    const c = line[0];
    if (c === "\t") names.set(line.slice(1), current);
    else if (c === "@") known.add(line.slice(1));
    else if (c === "#") continue;
    else {
      current = line;
      modules.add(line);
    }
  }
  return { names, modules, known };
}

const a = parse(await Deno.readTextFile(A));
const b = parse(await Deno.readTextFile(B));

const onlyA: string[] = [];
const onlyB: string[] = [];
const moved: [string, string, string][] = [];
let same = 0;
for (const [n, m] of a.names) {
  const other = b.names.get(n);
  if (other === undefined) onlyA.push(n);
  else if (other === m) same++;
  else moved.push([n, m, other]);
}
for (const [n] of b.names) if (!a.names.has(n)) onlyB.push(n);

/** Which module a name is missing "because of": the module it lives in on the
 * side that has it, split by whether the other side knows that module at all. */
function classify(names: string[], from: Index, to: Index) {
  const byModule = new Map<string, number>();
  let moduleAbsent = 0;
  let moduleShared = 0;
  const absentModules = new Set<string>();
  for (const n of names) {
    const m = from.names.get(n)!;
    byModule.set(m, (byModule.get(m) ?? 0) + 1);
    if (to.known.has(m)) moduleShared++;
    else {
      moduleAbsent++;
      absentModules.add(m);
    }
  }
  return { byModule, moduleAbsent, moduleShared, absentModules };
}

/** Shape buckets, so that "8,000 names" does not have to be read one by one. */
function shapes(names: string[]) {
  const bucket = new Map<string, number>();
  const put = (k: string) => bucket.set(k, (bucket.get(k) ?? 0) + 1);
  for (const n of names) {
    if (n.startsWith("_private.")) put("_private.*");
    else if (/(^|\.)_/.test(n)) put("component starting with _");
    else if (/\.match_\d+$/.test(n)) put(".match_N");
    else if (/\.eq_\d+$|\.eq_def$/.test(n)) put(".eq_N / .eq_def");
    else if (/\.proof_\d+$/.test(n)) put(".proof_N");
    else if (/\.rec$|\.recOn$|\.brecOn$|\.casesOn$|\.below$|\.ibelow$/.test(n)) put("recursor-ish");
    else if (/\.noConfusion(Type)?$/.test(n)) put("noConfusion");
    else if (/«/.test(n)) put("escaped (notation)");
    else put("plain");
  }
  return bucket;
}

const clsA = classify(onlyA, a, b);
const clsB = classify(onlyB, b, a);

const n = (x: number) => x.toLocaleString("en-US");
const top = (m: Map<string, number>, k: number) =>
  [...m].sort((x, y) => y[1] - x[1]).slice(0, k);

const lines: string[] = [];
const say = (s = "") => lines.push(s);

say(`# link-index の突き合わせ (集合比較)`);
say();
say(`A = ${A}`);
say(`B = ${B}`);
say(`date ${new Date().toISOString()}`);
say();
say(`| | A | B |`);
say(`|---|---:|---:|`);
say(`| 宣言 (名前 → モジュール) | ${n(a.names.size)} | ${n(b.names.size)} |`);
say(`| グループ (宣言を持つモジュール) | ${n(a.modules.size)} | ${n(b.modules.size)} |`);
say(`| \`@\` 節 (モジュール名) | ${n(a.known.size)} | ${n(b.known.size)} |`);
say();
say(`## 名前の集合`);
say();
say(`| | 件数 |`);
say(`|---|---:|`);
say(`| 両方にあり、モジュールも一致 | ${n(same)} |`);
say(`| **両方にあるがモジュールが食い違う** | **${n(moved.length)}** |`);
say(`| A にしかない | ${n(onlyA.length)} |`);
say(`| B にしかない | ${n(onlyB.length)} |`);
say();
for (const [label, names, cls, from] of [
  ["A にしかない", onlyA, clsA, a],
  ["B にしかない", onlyB, clsB, b],
] as [string, string[], ReturnType<typeof classify>, Index][]) {
  if (names.length === 0) continue;
  say(`### ${label} — ${n(names.length)} 件の内訳`);
  say();
  say(`| 相手側にそのモジュールが | 件数 |`);
  say(`|---|---:|`);
  say(`| **無い** (モジュールごと欠けている: ${n(cls.absentModules.size)} モジュール) | ${n(cls.moduleAbsent)} |`);
  say(`| ある (同じモジュールの中で欠けている) | ${n(cls.moduleShared)} |`);
  say();
  say(`名前の形:`);
  say();
  say(`| 形 | 件数 |`);
  say(`|---|---:|`);
  for (const [k, v] of top(shapes(names), 12)) say(`| ${k} | ${n(v)} |`);
  say();
  say(`上位モジュール:`);
  say();
  say(`| モジュール | 件数 | 相手側にある |`);
  say(`|---|---:|---|`);
  for (const [m, c] of top(cls.byModule, SAMPLES)) {
    const other = label.startsWith("A") ? b : a;
    say(`| ${m} | ${n(c)} | ${other.known.has(m) ? "あり" : "**なし**"} |`);
  }
  say();
  say(`例: ${names.slice(0, 5).join(", ")}`);
  say();
}
if (moved.length > 0) {
  say(`### モジュールが食い違う ${n(moved.length)} 件`);
  say();
  say(`| 名前 | A | B |`);
  say(`|---|---|---|`);
  for (const [nm, ma, mb] of moved.slice(0, 40)) say(`| ${nm} | ${ma} | ${mb} |`);
  say();
}
say(`## \`@\` 節 (モジュール名)`);
say();
const knownOnlyA = [...a.known].filter((m) => !b.known.has(m));
const knownOnlyB = [...b.known].filter((m) => !a.known.has(m));
say(`| | 件数 |`);
say(`|---|---:|`);
say(`| 両方 | ${n(a.known.size - knownOnlyA.length)} |`);
say(`| A のみ | ${n(knownOnlyA.length)} |`);
say(`| B のみ | ${n(knownOnlyB.length)} |`);
say();
if (knownOnlyA.length) say(`A のみ (先頭 20): ${knownOnlyA.slice(0, 20).join(", ")}`);
if (knownOnlyB.length) say(`B のみ (先頭 20): ${knownOnlyB.slice(0, 20).join(", ")}`);
say();

const out = lines.join("\n");
console.log(out);
if (OUT) await Deno.writeTextFile(OUT, out);
if (DUMP) {
  await Deno.mkdir(DUMP, { recursive: true });
  await Deno.writeTextFile(`${DUMP}/only-a.txt`, onlyA.sort().join("\n") + "\n");
  await Deno.writeTextFile(`${DUMP}/only-b.txt`, onlyB.sort().join("\n") + "\n");
  await Deno.writeTextFile(
    `${DUMP}/moved.tsv`,
    moved.map(([nm, ma, mb]) => `${nm}\t${ma}\t${mb}`).sort().join("\n") + "\n",
  );
  await Deno.writeTextFile(
    `${DUMP}/known-only-a.txt`,
    knownOnlyA.sort().join("\n") + "\n",
  );
  await Deno.writeTextFile(
    `${DUMP}/known-only-b.txt`,
    knownOnlyB.sort().join("\n") + "\n",
  );
}
