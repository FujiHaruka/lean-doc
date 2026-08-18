#!/usr/bin/env -S deno run --allow-read --allow-env --allow-run
//
// Stage 4 preparation: reads the module-granular IR `experiments/stage4` writes
// and measures how long that takes. Schema 2 (`experiments/stage4b`, the
// positional span lists) is read by the same code path — see "Schema 2" below.
//
// This is the stand-in for the stage-4 consumer (`approach.md` §5.6: HTML,
// search index, cross references are all built outside Lean from the IR). The
// point of the tool is not what it computes — declaration, reference and byte
// totals are just enough work to force every file to be parsed — but that it
// **never starts Lean**. If the numbers below are small, the "Lean is an
// extractor, output lives outside it" boundary is affordable; if reading the IR
// cost as much as the 2.5 s warm `importModules` floor, the split would be
// paying for itself twice.
//
// usage:
//   read-ir.ts [--ir <dir>] [--runs N] [--standalone N] [--json <out>]
//              [--verify-hashes] [--note <line>]...
//
//   --ir        IR root written by `experiments/stage4/run.sh --write-ir`,
//               default $IR_DIR, else the session scratchpad path below
//   --runs      repetitions (default 7); run 1 is dropped as page-cache cold
//   --standalone N  repeat with one fresh process per read as a cross-check
//                   (default 0 = off; needs --allow-run)
//   --json      also write a machine-readable summary here (needs --allow-write)
//   --verify-hashes  recompute each module's content hash and compare with the
//               index. Off by default: it is not part of what a consumer pays,
//               and it would inflate the read time being reported.
//   --note      extra condition line to print in the header (repeatable)
//
// Timing note: run 1 of every variant is discarded, so the reported number is
// warm by construction. The spread of the remaining runs is printed so that a
// single-run number is never what gets quoted. Nothing is written to the
// measurement target.
//
// Schema 2: the tagged IR adds a flat span list per printed fragment
// (`binderCode` / `typeCode` / `equationCode` / `members[].code`, see
// `experiments/stage4b/README.md`). This reader walks those lists and tallies
// them by kind, for the same reason it sums references: to force the parsed
// objects to be touched rather than dropped. The extension is **additive** — the
// span walk is behind `schemaVersion >= 2`, so a schema-1 IR takes exactly the
// code path it took before and must produce exactly the same counts.

const args = Deno.args.slice();
const opt = (name: string, dflt: string) => {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : dflt;
};
const opts = (name: string): string[] => {
  const out: string[] = [];
  for (let i = 0; i < args.length; i++) if (args[i] === name) out.push(args[i + 1]);
  return out;
};
const has = (name: string) => args.includes(name);

const DEFAULT_IR =
  "/private/tmp/claude-502/-Users-haruka-dev-litedoc4/3db6b213-b50d-48cb-a16b-16df93b5f009/scratchpad/ir";

let envIr = "";
try {
  envIr = Deno.env.get("IR_DIR") ?? "";
} catch { /* --allow-env not granted */ }

const IR = opt("--ir", envIr || DEFAULT_IR);
const RUNS = Number(opt("--runs", "7"));
const STANDALONE = Number(opt("--standalone", "0"));
const JSON_OUT = opt("--json", "");
const NOTES = opts("--note");

/** The 2.5 s warm `importModules` floor measured in stage 1 — the yardstick for
 *  "is reading the IR cheap compared to loading the Lean environment". */
const ENV_LOAD_FLOOR_S = 2.5;

type IndexEntry = {
  module: string;
  file: string;
  bytes: number;
  declarations: number;
  contentHash: string;
};
type DepEntry = { package: string; file: string; entries: number; bytes: number };
type Index = {
  schemaVersion: number;
  generator: string;
  leanVersion: string;
  hashAlgorithm: string;
  moduleCount: number;
  declarationCount: number;
  modules: IndexEntry[];
  dependencyMaps: DepEntry[];
};

type Totals = {
  modules: number;
  declarations: number;
  refPairs: number;
  refsUnique: number;
  moduleDocs: number;
  imports: number;
  equations: number;
  members: number;
  bytesRead: number;
  depPackages: number;
  depEntries: number;
  /** Schema 2 only; stays 0 on a schema-1 IR. */
  spanFragments: number;
  spans: number;
  spansConst: number;
  spansSort: number;
  spansOther: number;
  spansNamed: number;
};

/** `[start, stop, kind]`, or `[start, stop, 1, name]` when kind is 1. */
type Span = [number, number, number] | [number, number, 1, string];

/** Tally one fragment's span list. Counts the fragment even when it has no
 *  spans, which is how the writer counts `spanFragments` — so the two numbers
 *  are comparable (`experiments/stage4b/Extract.lean`, `IrStats.spanFragments`). */
function tallySpans(t: Totals, spans: Span[]): void {
  t.spanFragments++;
  for (const s of spans) {
    t.spans++;
    const kind = s[2];
    if (kind === 1) {
      t.spansConst++;
      // Touch the name: it is the half of a const span a link renderer needs,
      // and leaving it unread would time a parse whose result is thrown away.
      if (s.length > 3) t.spansNamed++;
    } else if (kind === 2) t.spansSort++;
    else t.spansOther++;
  }
}

/** One full pass over the IR: index, every module file, every dependency map. */
async function readAll(): Promise<Totals> {
  const indexText = await Deno.readTextFile(`${IR}/index.json`);
  const index: Index = JSON.parse(indexText);
  const tagged = index.schemaVersion >= 2;
  // Byte counts come from the index, which the writer filled in with
  // `String.utf8ByteSize`. Re-encoding the text here to measure it would be work
  // a real consumer never does, and it would land inside the timer.
  const t: Totals = {
    modules: 0,
    declarations: 0,
    refPairs: 0,
    refsUnique: 0,
    moduleDocs: 0,
    imports: 0,
    equations: 0,
    members: 0,
    bytesRead: 0, // filled from the index below (UTF-8 bytes recorded by the writer)
    depPackages: 0,
    depEntries: 0,
    spanFragments: 0,
    spans: 0,
    spansConst: 0,
    spansSort: 0,
    spansOther: 0,
    spansNamed: 0,
  };
  const uniqueRefs = new Set<string>();
  for (const entry of index.modules) {
    const mod = JSON.parse(await Deno.readTextFile(`${IR}/${entry.file}`));
    t.bytesRead += entry.bytes;
    t.modules++;
    t.imports += mod.imports.length;
    t.moduleDocs += mod.moduleDocs.length;
    for (const d of mod.declarations) {
      t.declarations++;
      t.equations += d.equations.length;
      t.members += d.members.length;
      for (const [refModule, refName] of d.refs) {
        t.refPairs++;
        // Forces both halves of the pair; also what a link resolver would key on.
        uniqueRefs.add(`${refModule} ${refName}`);
      }
      if (tagged) {
        for (const spans of d.binderCode) tallySpans(t, spans);
        tallySpans(t, d.typeCode);
        for (const spans of d.equationCode) tallySpans(t, spans);
        for (const m of d.members) tallySpans(t, m.code);
      }
    }
  }
  t.refsUnique = uniqueRefs.size;
  for (const dep of index.dependencyMaps) {
    const map = JSON.parse(await Deno.readTextFile(`${IR}/${dep.file}`));
    t.bytesRead += dep.bytes;
    t.depPackages++;
    t.depEntries += Object.keys(map.declarations).length;
  }
  return t;
}

// `--standalone <n>`: the tool re-invoking itself so that each read starts from
// a cold V8 heap. Handled before anything else so the child does no other work.
const child = args.indexOf("--read-once");
if (child >= 0) {
  const t0 = performance.now();
  const t = await readAll();
  const t1 = performance.now();
  console.log(`${((t1 - t0) / 1000).toFixed(4)} ${t.declarations} ${t.bytesRead}`);
  Deno.exit(0);
}

const median = (xs: number[]) => {
  const s = [...xs].sort((a, b) => a - b);
  return s.length % 2 ? s[(s.length - 1) / 2] : (s[s.length / 2 - 1] + s[s.length / 2]) / 2;
};
const num = (n: number) => n.toLocaleString("en-US");
const mib = (n: number) => `${(n / 1024 / 1024).toFixed(2)} MiB`;

const out: string[] = [];
const say = (s = "") => out.push(s);

const indexStat = await Deno.stat(`${IR}/index.json`).catch(() => null);
if (!indexStat) {
  console.error(
    `no index.json under ${IR} — this reader wants the schema-2 IR that ` +
      `experiments/stage4/run.sh --write-ir produced. That script only exists at tag ` +
      `experiments-frozen; the product writes schema 4, which this reader does not parse.`,
  );
  Deno.exit(1);
}
const index: Index = JSON.parse(await Deno.readTextFile(`${IR}/index.json`));

say(`# read-ir — reading the stage-4 IR without starting Lean`);
say();
say(`date              ${new Date().toISOString().replace(/\.\d+Z$/, "Z")}`);
say(`ir                ${IR}`);
say(`written           ${indexStat.mtime?.toISOString().replace(/\.\d+Z$/, "Z") ?? "?"}`);
say(`deno              ${Deno.version.deno} / V8 ${Deno.version.v8}`);
say(`schemaVersion     ${index.schemaVersion}`);
say(`generator         ${index.generator}`);
say(`leanVersion       ${index.leanVersion}`);
say(`hashAlgorithm     ${index.hashAlgorithm}`);
say(`runs              ${RUNS} (run 1 dropped as cold)`);
for (const n of NOTES) say(`note              ${n}`);
say();

const times: number[] = [];
let totals: Totals | null = null;
for (let i = 0; i < RUNS; i++) {
  const t0 = performance.now();
  const t = await readAll();
  const t1 = performance.now();
  times.push((t1 - t0) / 1000);
  totals = t;
}
const warm = times.slice(1);
const t = totals!;

say(`## What was read (実測)`);
say();
say(`| | |`);
say(`|---|---:|`);
say(`| module files | ${num(t.modules)} |`);
say(`| declarations | ${num(t.declarations)} |`);
say(`| (declaration, reference) pairs | ${num(t.refPairs)} |`);
say(`| distinct (module, name) references | ${num(t.refsUnique)} |`);
say(`| equations | ${num(t.equations)} |`);
say(`| structure members | ${num(t.members)} |`);
say(`| module docstrings | ${num(t.moduleDocs)} |`);
say(`| direct imports | ${num(t.imports)} |`);
say(`| dependency map packages | ${num(t.depPackages)} |`);
say(`| dependency map entries | ${num(t.depEntries)} |`);
say(`| index.json bytes | ${num(indexStat.size)} |`);
say(`| bytes read | ${num(t.bytesRead + indexStat.size)} (${mib(t.bytesRead + indexStat.size)}) |`);
say();

if (index.schemaVersion >= 2) {
  // Rows only a schema-2 IR has. Kept in their own table so the table above
  // stays byte-comparable with the schema-1 runs.
  say(`## Spans read back (実測, schema ${index.schemaVersion})`);
  say();
  say(`| | |`);
  say(`|---|---:|`);
  say(`| fragments (binders + type + equations + members) | ${num(t.spanFragments)} |`);
  say(`| spans | ${num(t.spans)} |`);
  say(`| of which kind 1 (const) | ${num(t.spansConst)} |`);
  say(`| of which kind 2 (sort) | ${num(t.spansSort)} |`);
  say(`| of which kind 0 (other) | ${num(t.spansOther)} |`);
  say(`| const spans carrying a name | ${num(t.spansNamed)} |`);
  say();
}

const med = median(warm);
say(`## Read time (実測, warm — run 1 dropped)`);
say();
say(`| | seconds |`);
say(`|---|---:|`);
say(`| run 1 (cold-ish, discarded) | ${times[0].toFixed(4)} |`);
warm.forEach((x, i) => say(`| run ${i + 2} | ${x.toFixed(4)} |`));
say(`| **median of runs 2-${RUNS}** | **${med.toFixed(4)}** |`);
say(`| spread (min-max, warm) | ${Math.min(...warm).toFixed(4)}-${Math.max(...warm).toFixed(4)} |`);
say();
say(
  `Against the warm \`importModules\` floor (${ENV_LOAD_FLOOR_S.toFixed(1)} s, stage 1): ` +
    `**${((100 * med) / ENV_LOAD_FLOOR_S).toFixed(1)}%**.`,
);
say(`Throughput: ${mib((t.bytesRead + indexStat.size) / med)}/s.`);
say();

if (STANDALONE > 0) {
  const self = new URL(import.meta.url).pathname;
  const rows: number[] = [];
  for (let i = 0; i < STANDALONE; i++) {
    const cmd = new Deno.Command(Deno.execPath(), {
      args: ["run", "--allow-read", "--allow-env", self, "--ir", IR, "--read-once"],
      stdout: "piped",
    });
    const res = await cmd.output();
    const [secs] = new TextDecoder().decode(res.stdout).trim().split(/\s+/);
    rows.push(Number(secs));
  }
  const w = rows.slice(1);
  say(`## Cross-check: one fresh process per read (実測)`);
  say();
  say(`In-process repetition can be flattered by a warm V8 heap, so the same read`);
  say(`was repeated with a new process each time (the process start itself is not`);
  say(`counted; the timer is inside the child).`);
  say();
  say(`| | seconds |`);
  say(`|---|---:|`);
  rows.forEach((x, i) => say(`| run ${i + 1}${i === 0 ? " (discarded)" : ""} | ${x.toFixed(4)} |`));
  say(`| **median of runs 2-${STANDALONE}** | **${median(w).toFixed(4)}** |`);
  say();
}

if (has("--verify-hashes")) {
  // FNV-1a-ish is not what Lean uses; the digest is `lean_string_hash`, which is
  // not reproducible in TypeScript without transcribing it. So this only checks
  // that every module file exists at the recorded size — the hash itself is
  // verified on the Lean side by re-running the extractor and diffing the tree.
  let sizeMismatch = 0;
  for (const e of index.modules) {
    const st = await Deno.stat(`${IR}/${e.file}`);
    if (st.size !== e.bytes) sizeMismatch++;
  }
  say(`## Index cross-check (実測)`);
  say();
  say(`byte counts in index vs on disk: ${index.modules.length - sizeMismatch}/${index.modules.length} match`);
  say();
}

const text = out.join("\n") + "\n";
console.log(text);

if (JSON_OUT) {
  await Deno.writeTextFile(
    JSON_OUT,
    JSON.stringify(
      {
        ir: IR,
        deno: Deno.version.deno,
        v8: Deno.version.v8,
        schemaVersion: index.schemaVersion,
        leanVersion: index.leanVersion,
        runs: RUNS,
        times,
        warmMedian: med,
        warmMin: Math.min(...warm),
        warmMax: Math.max(...warm),
        indexBytes: indexStat.size,
        totals: t,
      },
      null,
      2,
    ) + "\n",
  );
}
