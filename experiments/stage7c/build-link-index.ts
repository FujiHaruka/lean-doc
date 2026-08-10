#!/usr/bin/env -S deno run --allow-read --allow-write --allow-env --allow-hrtime
// build-link-index.ts -- derive the "name -> module" index of the whole
// dependency closure from a doc-gen4 site's `declarations/declaration-data.bmp`.
//
// WHY THIS FILE EXISTS
// --------------------
// The renderer resolves docstring autolinks the way doc-gen4's
// `Output/DocString.lean:nameToLink?` does: look the token up in a global
// name -> module map. doc-gen4 has `env.name2ModIdx` because it holds the whole
// environment; the IR does not, and `deps/` only carries the names the
// signatures referred to (32 KB). `approach.md` §5.3 measured that this map is
// all that is needed and that `kind` is not.
//
// WHAT IS ASSUMED (and is NOT settled -- `approach.md` §8)
// -------------------------------------------------------
// This reads a *local* doc-gen4 output tree. That is a measurement convenience,
// not a distribution decision: it presumes the same file can be fetched from the
// upstream package's published documentation site, and whether that holds (and
// whether versions can be matched up) is **unverified**. If it does not hold the
// index has to be produced by the generator instead, which is a different cost.
//
// FORMATS
// -------
// Two, so that the size question of §5.3 ("53 KB lower bound / 34.3 MB upper
// bound", a 647x spread) gets a number from an actual implementation rather than
// from a bound:
//
//   .lidx  grouped text. A module name on its own line, then one TAB-prefixed
//          declaration name per line. Read = split + one branch per line.
//   .json  {"schemaVersion":1,"modules":[...],"declarations":{name: moduleIdx}}
//          the same content in the shape the rest of the IR uses.
//
// Both are written so their sizes can be compared; the renderer reads the .lidx.
//
// usage: build-link-index.ts --bmp <declaration-data.bmp> --out <dir> [--report <path>]

const argv = Deno.args.slice();
const opt = (name: string, dflt = "") => {
  const i = argv.indexOf(name);
  return i >= 0 ? argv[i + 1] : dflt;
};
const BMP = opt(
  "--bmp",
  "/Users/haruka/dev/lean-projects/.lake/build/doc/declarations/declaration-data.bmp",
);
const OUT = opt("--out");
const REPORT = opt("--report");
const REPEAT = Number(opt("--repeat", "1"));
if (!OUT) {
  console.error("usage: build-link-index.ts [--bmp <path>] --out <dir> [--report <path>]");
  Deno.exit(2);
}
await Deno.mkdir(OUT, { recursive: true });

type Decl = { docLink: string; kind: string };

/** `./Mathlib/A/B.html#Name` -> `Mathlib.A.B`. */
function moduleOfDocLink(docLink: string): string | null {
  const hash = docLink.indexOf("#");
  const path = hash >= 0 ? docLink.slice(0, hash) : docLink;
  if (!path.startsWith("./") || !path.endsWith(".html")) return null;
  return path.slice(2, -".html".length).split("/").join(".");
}

const gz = async (bytes: Uint8Array): Promise<number> => {
  const cs = new CompressionStream("gzip");
  const stream = new Blob([bytes as BlobPart]).stream().pipeThrough(cs);
  const buf = await new Response(stream).arrayBuffer();
  return buf.byteLength;
};

const timings: { run: number; readMs: number; parseMs: number; buildMs: number; writeMs: number; totalMs: number }[] = [];
let lidxBytes = new Uint8Array();
let jsonBytes = new Uint8Array();
let nDecls = 0;
let nModules = 0;
let nSkipped = 0;
let nModuleNames = 0;
let bmpBytes = 0;

for (let run = 1; run <= REPEAT; run++) {
  const t0 = performance.now();
  const raw = await Deno.readFile(BMP);
  const t1 = performance.now();
  const text = new TextDecoder().decode(raw);
  const bmp = JSON.parse(text) as {
    declarations: Record<string, Decl>;
    modules: Record<string, unknown>;
  };
  const t2 = performance.now();

  // name -> module, grouped by module. Module order = first appearance, so the
  // output is deterministic for a given input.
  const byModule = new Map<string, string[]>();
  let skipped = 0;
  for (const [name, d] of Object.entries(bmp.declarations)) {
    const mod = moduleOfDocLink(d.docLink);
    if (mod === null) {
      skipped++;
      continue;
    }
    let arr = byModule.get(mod);
    if (arr === undefined) byModule.set(mod, arr = []);
    arr.push(name);
  }
  // Module names that are link targets in their own right (doc-gen4 checks
  // `res.moduleNames.contains name` before falling back to the module-local
  // search). Kept as a section of the same file: it is part of the same mapping.
  const moduleNames = Object.keys(bmp.modules);

  const parts: string[] = [`#lidx1`];
  for (const m of moduleNames) parts.push(`@${m}`);
  for (const [mod, names] of byModule) {
    parts.push(mod);
    for (const n of names) parts.push(`\t${n}`);
  }
  const lidx = parts.join("\n") + "\n";
  const t3 = performance.now();

  const enc = new TextEncoder();
  lidxBytes = enc.encode(lidx);
  await Deno.writeFile(`${OUT}/link-index.lidx`, lidxBytes);
  const t4 = performance.now();

  timings.push({
    run,
    readMs: t1 - t0,
    parseMs: t2 - t1,
    buildMs: t3 - t2,
    writeMs: t4 - t3,
    totalMs: t4 - t0,
  });

  if (run === REPEAT) {
    nDecls = 0;
    for (const a of byModule.values()) nDecls += a.length;
    nModules = byModule.size;
    nSkipped = skipped;
    nModuleNames = moduleNames.length;
    bmpBytes = raw.byteLength;
    // The JSON shape, for the size comparison only.
    const modules = [...byModule.keys()];
    const idx = new Map(modules.map((m, i) => [m, i]));
    const declarations: Record<string, number> = {};
    for (const [mod, names] of byModule) for (const n of names) declarations[n] = idx.get(mod)!;
    const json = JSON.stringify({
      schemaVersion: 1,
      modules,
      moduleNames,
      declarations,
    });
    jsonBytes = enc.encode(json);
    await Deno.writeFile(`${OUT}/link-index.json`, jsonBytes);
  }
}

const med = (xs: number[]) => {
  const s = [...xs].sort((a, b) => a - b);
  return s.length % 2 ? s[(s.length - 1) / 2] : (s[s.length / 2 - 1] + s[s.length / 2]) / 2;
};
const warm = timings.slice(1).length > 0 ? timings.slice(1) : timings;

const lidxGz = await gz(lidxBytes);
const jsonGz = await gz(jsonBytes);

const n = (x: number) => x.toLocaleString("en-US");
const s = (ms: number) => (ms / 1000).toFixed(4);
const lines = [
  `# build-link-index — 依存クロージャ全体の「名前 → モジュール」索引 (実測)`,
  ``,
  `bmp        ${BMP}`,
  `out        ${OUT}`,
  `date       ${new Date().toISOString()}`,
  `deno       ${Deno.version.deno} / V8 ${Deno.version.v8}`,
  `runs       ${REPEAT} (run 1 は捨て、残りの median)`,
  ``,
  `## 中身`,
  ``,
  `| | |`,
  `|---|---:|`,
  `| 入力 (declaration-data.bmp) | ${n(bmpBytes)} B |`,
  `| 宣言 (名前 → モジュール) | ${n(nDecls)} |`,
  `| … 出現するモジュール | ${n(nModules)} |`,
  `| docLink が解釈できず落とした宣言 | ${n(nSkipped)} |`,
  `| モジュール名 (それ自体がリンク先になる) | ${n(nModuleNames)} |`,
  ``,
  `## サイズ`,
  ``,
  `| 形式 | 生 | gzip | 入力比 |`,
  `|---|---:|---:|---:|`,
  `| .lidx (グループ化テキスト) | ${n(lidxBytes.length)} B | ${n(lidxGz)} B | ${
    (lidxBytes.length / bmpBytes * 100).toFixed(1)
  }% |`,
  `| .json (modules[] + declarations{}) | ${n(jsonBytes.length)} B | ${n(jsonGz)} B | ${
    (jsonBytes.length / bmpBytes * 100).toFixed(1)
  }% |`,
  ``,
  `## 作る時間 (秒)`,
  ``,
  `| | read | JSON.parse | 組み立て | 書き出し | 合計 |`,
  `|---|---:|---:|---:|---:|---:|`,
  `| median (run 2 以降) | ${s(med(warm.map((t) => t.readMs)))} | ${
    s(med(warm.map((t) => t.parseMs)))
  } | ${s(med(warm.map((t) => t.buildMs)))} | ${s(med(warm.map((t) => t.writeMs)))} | ${
    s(med(warm.map((t) => t.totalMs)))
  } |`,
  ``,
  `各 run (合計秒): ${timings.map((t) => s(t.totalMs)).join(" ")}`,
  ``,
];
const out = lines.join("\n");
console.log(out);
if (REPORT) await Deno.writeTextFile(REPORT, out);
