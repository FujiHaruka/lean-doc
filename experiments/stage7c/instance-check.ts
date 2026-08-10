#!/usr/bin/env -S deno run --allow-read --allow-write --allow-env
//
// Stage 7b: check the schema-4 **instance type index** against doc-gen4's own.
//
// The index never reaches a module page -- the browser fills the "Instances" and
// "Instances For" lists from `declarations/declaration-data.bmp` -- so
// `coverage.ts` cannot see it, and "we compute it too" would otherwise be an
// unverified claim. doc-gen4 writes that file from exactly the two fields
// schema 4 stores (`Output/ToJson.lean:88-97`):
//
//     instances[className]  ∋ instanceName
//     instancesFor[typeName] ∋ instanceName        for every typeName
//
// so the two can be compared directly, in both directions:
//
//   forward   every (name, className) and (name, typeName) pair in the IR is in
//             doc-gen4's file;
//   backward  every entry in doc-gen4's file that mentions one of the IR's
//             instances is a pair the IR has. This is the half that catches a
//             *missing* type name, which the forward direction cannot.
//
// doc-gen4's file covers the whole environment (258,760 declarations); only the
// rows naming one of this package's instances are in scope here.
//
// **Scope, the same one `coverage.ts` uses.** The committed doc-gen4 output has
// pages for 348 of the 432 modules; the other 84 were written after that build.
// An instance in one of those 84 is not in `declaration-data.bmp` either, and
// that says nothing about this extractor. So the comparison is made on the
// documented modules, and the rest are counted separately rather than dropped.
//
// usage: instance-check.ts --ir <dir> [--bmp <path>] [--doc-root <dir>]
//                          [--report <path.txt>]

const argv = Deno.args.slice();
const opt = (n: string, d = "") => {
  const i = argv.indexOf(n);
  return i >= 0 ? argv[i + 1] : d;
};
const IR = opt("--ir");
const TARGET = Deno.env.get("TARGET_REPO") ?? "/Users/haruka/dev/lean-projects";
const BMP = opt("--bmp", `${TARGET}/.lake/build/doc/declarations/declaration-data.bmp`);
const DOC_ROOT = opt("--doc-root", `${TARGET}/.lake/build/doc/InformationTheory`);
const REPORT = opt("--report");
if (!IR) {
  console.error("usage: instance-check.ts --ir <dir> [--bmp <path>] [--report <txt>]");
  Deno.exit(2);
}

type Decl = { name: string; kind: string; instClass?: string; instTypes?: string[] };
type ModuleFile = { module: string; declarations: Decl[] };
type Index = { schemaVersion: number; modules: { file: string }[] };

/** The modules doc-gen4 actually wrote a page for. */
async function* walk(dir: string): AsyncGenerator<string> {
  for await (const e of Deno.readDir(dir)) {
    const p = `${dir}/${e.name}`;
    if (e.isDirectory) yield* walk(p);
    else if (e.name.endsWith(".html")) yield p;
  }
}
const documented = new Set<string>();
for await (const p of walk(DOC_ROOT)) {
  const rel = p.slice(DOC_ROOT.length + 1, -".html".length);
  documented.add("InformationTheory." + rel.split("/").join("."));
}

const index: Index = JSON.parse(await Deno.readTextFile(`${IR}/index.json`));
if (index.schemaVersion < 4) {
  console.error(`schemaVersion ${index.schemaVersion}: the instance index is schema 4`);
  Deno.exit(2);
}

/** name -> (className, typeNames) from the IR, for the documented modules. */
const mine = new Map<string, { cls: string; types: string[] }>();
/** The same, for the modules doc-gen4 has no page for -- out of scope. */
const undocumented = new Map<string, { cls: string; types: string[] }>();
let declarations = 0;
let instanceKind = 0;
for (const e of index.modules) {
  const m: ModuleFile = JSON.parse(await Deno.readTextFile(`${IR}/${e.file}`));
  const inScope = documented.has(m.module);
  for (const d of m.declarations) {
    declarations++;
    if (d.kind === "instance") instanceKind++;
    if (d.instClass === undefined) continue;
    (inScope ? mine : undocumented).set(d.name, { cls: d.instClass, types: d.instTypes ?? [] });
  }
}

const bmp = JSON.parse(await Deno.readTextFile(BMP)) as {
  instances: Record<string, string[]>;
  instancesFor: Record<string, string[]>;
};

const problems: string[] = [];
let forwardClass = 0, forwardType = 0, notInBmpAtAll = 0;
for (const [name, { cls, types }] of mine) {
  const list = bmp.instances[cls];
  if (list && list.includes(name)) forwardClass++;
  else problems.push(`forward: instances["${cls}"] does not contain ${name}`);
  const seen = new Set<string>();
  for (const t of types) {
    if (seen.has(t)) continue; // doc-gen4 stores a set
    seen.add(t);
    const l = bmp.instancesFor[t];
    if (l && l.includes(name)) forwardType++;
    else problems.push(`forward: instancesFor["${t}"] does not contain ${name}`);
  }
}

let backwardClass = 0, backwardType = 0;
for (const [cls, list] of Object.entries(bmp.instances)) {
  for (const name of list) {
    const m = mine.get(name);
    if (m === undefined) continue;
    backwardClass++;
    if (m.cls !== cls) problems.push(`backward: doc-gen4 files ${name} under class ${cls}, IR says ${m.cls}`);
  }
}
for (const [t, list] of Object.entries(bmp.instancesFor)) {
  for (const name of list) {
    const m = mine.get(name);
    if (m === undefined) continue;
    backwardType++;
    if (!m.types.includes(t)) problems.push(`backward: doc-gen4 files ${name} under type ${t}, IR does not`);
  }
}
for (const name of mine.keys()) {
  const anywhere = Object.values(bmp.instances).some((l) => l.includes(name));
  if (!anywhere) notInBmpAtAll++;
}

const n = (x: number) => x.toLocaleString("en-US");
const out: string[] = [];
const say = (s = "") => out.push(s);
say("# stage7c — instance 索引を doc-gen4 の declaration-data.bmp と突き合わせる (実測)");
say();
say(`ir           ${IR}`);
say(`bmp          ${BMP}`);
say(`date         ${new Date().toISOString().replace(/\.\d+Z$/, "Z")}`);
say();
say("この索引はモジュールページに 1 バイトも出ない (ブラウザが bmp から埋める)。");
say("`coverage.ts` では検証できないので、doc-gen4 が書いた索引そのものと比べる。");
say();
say("| | |");
say("|---|---:|");
say(`| IR の宣言 | ${n(declarations)} |`);
say(`| … kind = instance | ${n(instanceKind)} |`);
say(`| … doc-gen4 がページを出したモジュールにあるもの (採点対象) | ${n(mine.size)} |`);
say(`| … doc-gen4 がページを出していない 84 モジュールにあるもの (対象外) | ${n(undocumented.size)} |`);
say(`| 対象のうち doc-gen4 の \`instances\` に出てこないもの | ${n(notInBmpAtAll)} |`);
say(`| 順方向: (name, className) が bmp にある | ${n(forwardClass)} / ${n(mine.size)} |`);
say(`| 順方向: (name, typeName) が bmp にある | ${n(forwardType)} |`);
say(`| 逆方向: bmp の \`instances\` 行が IR と一致 | ${n(backwardClass)} |`);
say(`| 逆方向: bmp の \`instancesFor\` 行が IR と一致 | ${n(backwardType)} |`);
say();
if (problems.length === 0) {
  say("**食い違い 0**。両方向で一致した。");
} else {
  say(`**食い違い ${n(problems.length)} 件:**`);
  say();
  for (const p of problems.slice(0, 40)) say(`  * ${p}`);
}
say();

const text = out.join("\n") + "\n";
console.log(text);
if (REPORT) await Deno.writeTextFile(REPORT, text);
if (problems.length > 0) Deno.exit(1);
