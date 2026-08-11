#!/usr/bin/env -S deno run --allow-read --allow-write --allow-run
// gen-delta-expected.ts -- the whole-package map delta, answered by the frozen
// prototype over the real corpus, so that the Rust port has something to be
// wrong against.
//
// WHY A DIFFERENTIAL TEST AND NOT A UNIT TEST
// -------------------------------------------
// `--print-set` is the incremental pipeline's input (plan §6, ordering rule 2:
// "global before impact"). A module missing from it keeps a page whose links
// point at the module a name used to live in, and nothing downstream notices:
// the site builds, the page is well formed, the link is wrong. The six
// artifacts cannot see this file at all -- it is derived from `ModuleFacts.
// tokens`, which reaches no artifact -- so the only check with any weight is
// "the prototype and the port name the same modules for the same mutation".
//
// THE MUTATION
// ------------
// `before` is the reference `name-map.json` with three edits, one of each kind
// the union rule has to cover:
//
//   moved     a name that is in both maps under different modules
//   dropped   a name that is in `after` and not in `before`   (a new name)
//   inserted  a name that is in `before` and not in `after`   (a deleted name)
//
// The three names are chosen by the generator, not by hand: `moved` and
// `dropped` are the names the most modules mention, so the affected set is
// large enough to exercise the 20-witness cap, and `inserted` is the most
// mentioned token that the map does not contain. The choice is recorded in the
// fixture and the Rust side rebuilds `before` from it, so both sides start from
// the same file without either one deciding what the answer is.
//
// npm/node are broken in this environment; this must run under deno.
//
// usage:
//   deno run --allow-read --allow-write --allow-run \
//     crates/lean-doc-global/tests/oracle/gen-delta-expected.ts
//   ... --check      fail if the committed fixture is stale
//   ... --ir DIR     another IR tree
//   ... --reference DIR   another reference artifact tree

const FIXTURE = new URL("../data/delta-expected.json", import.meta.url);
const GLOBAL_TS = new URL("../../../../experiments/stage7h/global.ts", import.meta.url);

const DEFAULT_IR = "/private/tmp/lean-doc-relay/w7h/base-ir";
const DEFAULT_REFERENCE = "/private/tmp/lean-doc-relay/m2/ref-global";

const argv = Deno.args.slice();
const opt = (name: string, dflt = "") => {
  const i = argv.indexOf(name);
  return i >= 0 ? argv[i + 1] : dflt;
};
const check = argv.includes("--check");
const irDir = opt("--ir", DEFAULT_IR);
const refDir = opt("--reference", DEFAULT_REFERENCE);

// ------------------------------------------------------ slicing global.ts

const globalLines = (await Deno.readTextFile(GLOBAL_TS)).split("\n");
const slice = (from: number, to: number) => globalLines.slice(from - 1, to).join("\n");

/** The same ranges `gen-global-expected.ts` slices, with the same head guards. */
const RANGES = {
  declType: { from: 65, to: 71, head: "interface Decl {" },
  moduleFileType: { from: 72, to: 78, head: "interface ModuleFile {" },
  headConst: { from: 97, to: 106, head: "function headConst(" },
  autolinkTokens: { from: 111, to: 125, head: "function autolinkTokens(" },
  moduleFactsType: { from: 136, to: 144, head: "interface ModuleFacts {" },
  factsOf: { from: 148, to: 170, head: "function factsOf(" },
} as const;

for (const [what, { from, to, head }] of Object.entries(RANGES)) {
  const text = slice(from, to);
  if (!text.startsWith(head)) {
    console.error(`global.ts:${from}-${to} no longer starts with ${JSON.stringify(head)} (${what})`);
    Deno.exit(3);
  }
}

const at = (k: keyof typeof RANGES) => slice(RANGES[k].from, RANGES[k].to);
const MODULE_SOURCE = [
  at("declType"),
  at("moduleFileType"),
  at("moduleFactsType"),
  at("headConst"),
  at("autolinkTokens"),
  at("factsOf"),
  "export { autolinkTokens, factsOf, headConst };",
].join("\n\n");

// deno-lint-ignore no-explicit-any
type Json = any;

const prototype = await import(
  `data:text/typescript;charset=utf-8,${encodeURIComponent(MODULE_SOURCE)}`
) as { factsOf(m: Json, contentHash: string): { module: string; tokens: string[] } };

// ------------------------------------------------------------- the corpus

const index = JSON.parse(await Deno.readTextFile(`${irDir}/index.json`));
const tokensByModule: { module: string; tokens: string[] }[] = [];
for (const entry of index.modules) {
  const m = JSON.parse(await Deno.readTextFile(`${irDir}/${entry.file}`));
  const facts = prototype.factsOf(m, entry.contentHash);
  tokensByModule.push({ module: facts.module, tokens: facts.tokens });
}

/** How many modules mention each token. The unit is the module, not the token:
 *  what makes a mutation interesting is how many pages it would restage. */
const mentions = new Map<string, number>();
for (const { tokens } of tokensByModule) {
  for (const t of new Set(tokens)) mentions.set(t, (mentions.get(t) ?? 0) + 1);
}

const after = JSON.parse(
  await Deno.readTextFile(`${refDir}/declarations/name-map.json`),
) as Record<string, string>;

/** Tokens sorted by how many modules mention them, ties broken by name so that
 *  the choice does not depend on iteration order. */
const ranked = [...mentions.entries()]
  .sort((a, b) => (b[1] - a[1]) || (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0))
  .map(([name, n]) => ({ name, modules: n }));

const inMap = ranked.filter((t) => t.name in after);
const notInMap = ranked.filter((t) => !(t.name in after) && t.name !== "");
if (inMap.length < 2 || notInMap.length < 1) {
  console.error("the corpus does not offer the three names the mutation needs");
  Deno.exit(4);
}

const mutation = {
  // In both maps, different modules.
  movedName: inMap[0].name,
  movedTo: "Fake.Moved.Module",
  // In `after` only: `before` never had it, so it reads as a new name.
  droppedName: inMap[1].name,
  // In `before` only: it is gone from the package now.
  insertedName: notInMap[0].name,
  insertedModule: "Fake.Deleted.Module",
  mentions: {
    moved: inMap[0].modules,
    dropped: inMap[1].modules,
    inserted: notInMap[0].modules,
  },
};

const before: Record<string, string> = { ...after };
before[mutation.movedName] = mutation.movedTo;
delete before[mutation.droppedName];
before[mutation.insertedName] = mutation.insertedModule;

// ------------------------------------------------------- running the delta

const root = await Deno.makeTempDir({ prefix: "lean-doc-delta-oracle-" });
const beforePath = `${root}/before.json`;
await Deno.writeTextFile(beforePath, JSON.stringify(before));
const printSet = `${root}/set.txt`;
const deltaJson = `${root}/delta.json`;
const run = await new Deno.Command(Deno.execPath(), {
  args: [
    "run",
    "--allow-read",
    "--allow-write",
    GLOBAL_TS.pathname,
    "build",
    "--ir",
    irDir,
    "--out",
    `${root}/site`,
    "--before",
    beforePath,
    "--print-set",
    printSet,
    "--delta-json",
    deltaJson,
  ],
  stdout: "piped",
  stderr: "piped",
}).output();
if (!run.success) {
  console.error(new TextDecoder().decode(run.stderr));
  Deno.exit(3);
}

const summary = JSON.parse(await Deno.readTextFile(deltaJson));
const printSetBytes = await Deno.readFile(printSet);

if (summary.changedNames === 0 || summary.affected === 0) {
  console.error(
    `the mutation changed ${summary.changedNames} name(s) and affected ` +
      `${summary.affected} module(s); a differential over an empty scan proves nothing`,
  );
  Deno.exit(5);
}

/** FNV-1a 64 -- the same ten lines `tests/global.rs` has. */
function fnv1a64(bytes: Uint8Array): string {
  let h = 0xcbf2_9ce4_8422_2325n;
  const prime = 0x0000_0100_0000_01b3n;
  const mask = 0xffff_ffff_ffff_ffffn;
  for (const b of bytes) h = (h ^ BigInt(b)) * prime & mask;
  return h.toString(16).padStart(16, "0");
}

// The empty-set case, which has no bytes at all and is the one the pipeline
// takes on most runs: `before` equal to `after` must write a zero-byte file.
const emptySet = `${root}/empty-set.txt`;
const unchanged = await new Deno.Command(Deno.execPath(), {
  args: [
    "run",
    "--allow-read",
    "--allow-write",
    GLOBAL_TS.pathname,
    "build",
    "--ir",
    irDir,
    "--out",
    `${root}/site2`,
    "--before",
    `${refDir}/declarations/name-map.json`,
    "--print-set",
    emptySet,
  ],
  stdout: "piped",
  stderr: "piped",
}).output();
if (!unchanged.success) {
  console.error(new TextDecoder().decode(unchanged.stderr));
  Deno.exit(3);
}
const emptySetBytes = (await Deno.readFile(emptySet)).length;

await Deno.remove(root, { recursive: true });

const fixture = JSON.stringify(
  {
    generatedBy: "crates/lean-doc-global/tests/oracle/gen-delta-expected.ts",
    oracle: "experiments/stage7h/global.ts: `build --before` run as a program over the real IR",
    globalTsRanges: RANGES,
    ir: irDir,
    reference: refDir,
    modules: tokensByModule.length,
    mutation,
    beforeNames: summary.beforeNames,
    afterNames: summary.afterNames,
    changedNames: summary.changedNames,
    changedSample: summary.changedSample,
    affected: summary.affected,
    affectedModules: summary.affectedModules,
    witnesses: summary.witnesses,
    printSetBytes: printSetBytes.length,
    printSetFnv1a64: fnv1a64(printSetBytes),
    unchangedPrintSetBytes: emptySetBytes,
    deno: Deno.version.deno,
  },
  null,
  1,
) + "\n";

if (check) {
  const committed = await Deno.readTextFile(FIXTURE);
  if (committed !== fixture) {
    console.error(`${FIXTURE.pathname} is not what this script produces`);
    Deno.exit(1);
  }
  console.error(`${FIXTURE.pathname} is current`);
} else {
  await Deno.writeTextFile(FIXTURE, fixture);
  console.error(
    `moved ${mutation.movedName} (${mutation.mentions.moved} modules mention it), ` +
      `dropped ${mutation.droppedName} (${mutation.mentions.dropped}), ` +
      `inserted ${mutation.insertedName} (${mutation.mentions.inserted})`,
  );
  console.error(
    `${summary.changedNames} changed -> ${summary.affected} affected, ` +
      `${printSetBytes.length} B of print-set; unchanged run wrote ${emptySetBytes} B`,
  );
}
