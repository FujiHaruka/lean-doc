#!/usr/bin/env -S deno run --allow-read --allow-write --allow-env --allow-run
//
// Stage 3, increment 2: matches the demand side (the constants lean-doc collects
// from a signature, `stage3-refs.jsonl`) against the supply side (doc-gen4's
// `declarations/declaration-data.bmp`), and measures how big that map actually
// is in three cuts: everything, the dependency side only, and just the
// constants this package really references.
//
// The map is what stage 3 is about: if links into Mathlib can be resolved from
// a name -> module -> URL table alone, nobody has to build an IR for the 8k
// dependency modules. This tool answers "does the table cover the demand" and
// "how many bytes and how many seconds is the table".
//
// usage:
//   map-size.ts [--bmp <file>] [--refs <jsonl>] [--modules <list>] [--doc <dir>]
//               [--runs N] [--standalone N] [--sample N] [--json <out>]
//               [--work <dir>] [--note <line>]...
//   map-size.ts --load-once <file>          (internal: one read + parse, then exit)
//
//   --bmp      doc-gen4's search index, default
//              $TARGET_REPO/.lake/build/doc/declarations/declaration-data.bmp
//   --refs     unique reference set from increment 1, default
//              benchmarks/results/stage3-refs.jsonl
//   --modules  the target package's module list, default
//              benchmarks/results/it-modules.txt
//   --doc      doc-gen4's HTML root, default $TARGET_REPO/.lake/build/doc
//   --pkg      the target package's directory under --doc, default InformationTheory
//   --runs     load-time repetitions per variant (default 7, run 1 dropped as cold)
//   --standalone N  repeat the load timing with one process per read as a
//                   cross-check (default 0 = off; needs --allow-run)
//   --sample   dependency declarations to resolve to a real file (default 50)
//   --json     also write a machine-readable summary here
//   --work     scratch dir for the derived variants (default: a temp dir)
//   --note     extra condition line to print in the header (repeatable)
//
// `--allow-write` is only used for the derived variants under `--work`: load
// time has to be measured against a real file, not an in-memory string, because
// that is what a consumer of the map would pay. `--allow-run` is only used by
// `--standalone`, which re-invokes this same file. Nothing is written to the
// target repository; the .bmp and the HTML are read-only inputs.
//
// Timing note: run 1 of every variant is discarded, so the reported numbers are
// warm (page cache holds the file by construction). The spread of the remaining
// runs is printed so a single-run number is never what gets quoted.

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

// `--load-once <file>`: the tool re-invoking itself, one read + parse per
// process. Handled before anything else so the child does no other work.
const loadOnce = args.indexOf("--load-once");
if (loadOnce >= 0) {
  const p = args[loadOnce + 1];
  const t0 = performance.now();
  const v = JSON.parse(await Deno.readTextFile(p));
  const t1 = performance.now();
  console.log(`${((t1 - t0) / 1000).toFixed(4)} ${Object.keys(v).length}`);
  Deno.exit(0);
}

const REPO = new URL("../..", import.meta.url).pathname.replace(/\/$/, "");
let TARGET = "/Users/haruka/dev/lean-projects";
try {
  TARGET = Deno.env.get("TARGET_REPO") ?? TARGET;
} catch { /* --allow-env not granted; the default is right for this project */ }

const BMP = opt("--bmp", `${TARGET}/.lake/build/doc/declarations/declaration-data.bmp`);
const REFS = opt("--refs", `${REPO}/benchmarks/results/stage3-refs.jsonl`);
const MODULES = opt("--modules", `${REPO}/benchmarks/results/it-modules.txt`);
const DOC = opt("--doc", `${TARGET}/.lake/build/doc`);
const RUNS = Number(opt("--runs", "7"));
const STANDALONE = Number(opt("--standalone", "0"));
const PKG = opt("--pkg", "InformationTheory");
const SAMPLE = Number(opt("--sample", "50"));
const JSON_OUT = opt("--json", "");
const WORK = opt("--work", "");
const NOTES = opts("--note");

/** The 2.5 s warm `importModules` floor measured in stage 1; the yardstick for
 *  "is loading the map cheap compared to loading the Lean environment". */
const ENV_LOAD_FLOOR_S = 2.5;

type IndexedDecl = { docLink: string; kind: string };
type Ref = { module: string; name: string; occurrences: number; own: boolean };

const out: string[] = [];
const say = (s = "") => out.push(s);
const num = (n: number) => n.toLocaleString("en-US");
const pct = (a: number, b: number) => (b === 0 ? "-" : `${((100 * a) / b).toFixed(2)}%`);
const mib = (n: number) => `${(n / 1024 / 1024).toFixed(2)} MiB`;

/** `./Mathlib/A/B.html#Name` -> `Mathlib.A.B`. */
function moduleOfDocLink(docLink: string): string | null {
  const h = docLink.indexOf(".html");
  if (h < 0) return null;
  return docLink.slice(0, h).replace(/^(\.\.\/)*\.?\/?/, "").replaceAll("/", ".");
}

async function gzipBytes(s: string): Promise<number> {
  const stream = new Blob([s]).stream().pipeThrough(new CompressionStream("gzip"));
  let n = 0;
  for await (const chunk of stream) n += (chunk as Uint8Array).byteLength;
  return n;
}

/** Reads + JSON.parses `path` `RUNS` times, drops run 1, returns seconds. */
async function loadTimes(path: string): Promise<number[]> {
  const ts: number[] = [];
  for (let i = 0; i < RUNS; i++) {
    const t0 = performance.now();
    const text = await Deno.readTextFile(path);
    const v = JSON.parse(text);
    const t1 = performance.now();
    if (typeof v !== "object") throw new Error("parse produced a non-object");
    ts.push((t1 - t0) / 1000);
  }
  return ts.slice(1);
}

const median = (xs: number[]) => {
  const s = [...xs].sort((a, b) => a - b);
  return s.length % 2 ? s[(s.length - 1) / 2] : (s[s.length / 2 - 1] + s[s.length / 2]) / 2;
};

// ---------------------------------------------------------------- inputs

const bmpBytes = (await Deno.stat(BMP)).size;
const bmpText = await Deno.readTextFile(BMP);
const bmp = JSON.parse(bmpText);
const decls = bmp.declarations as Record<string, IndexedDecl>;
const declNames = Object.keys(decls);

const refs: Ref[] = (await Deno.readTextFile(REFS)).trim().split("\n").map((l) => JSON.parse(l));
const targetModules = new Set(
  (await Deno.readTextFile(MODULES)).split("\n").map((s) => s.trim()).filter(Boolean),
);

// The `own` flag in the refs file means "defining module is in the target
// module list". Recompute it here instead of trusting it: the dependency count
// is the headline number of this increment, so it gets a second route.
const ownFlagMismatches = refs.filter((r) => r.own !== targetModules.has(r.module));
const depRefs = refs.filter((r) => !targetModules.has(r.module));
const ownRefs = refs.filter((r) => targetModules.has(r.module));

// Which modules did *this* .bmp actually get declarations from? A module of the
// target package that contributed nothing is a module the build never reached
// (the HTML build was stopped at 42%), and that is a different miss cause from
// "doc-gen4 deliberately does not publish this name".
const declModule = new Map<string, string>();
const modulesWithDecls = new Set<string>();
for (const n of declNames) {
  const m = moduleOfDocLink(decls[n].docLink);
  if (m === null) continue;
  declModule.set(n, m);
  modulesWithDecls.add(m);
}

// ---------------------------------------------------------------- coverage

const AUX_LEAF = new Set([
  "rec", "recOn", "brecOn", "binductionOn", "below", "ibelow", "casesOn", "ndrec", "ndrecOn",
  "noConfusion", "noConfusionType", "inj", "injEq", "sizeOf_spec", "toCtorIdx",
]);
const AUX_NUMBERED = /^(match|eq|proof|def|sunfold|fun)_\d+$/;

/** doc-gen4's `isBlackListed` (Process/DocInfo.lean) rejects internal names,
 *  aux recursors, noConfusion, recursors and matchers. Those never reach the
 *  index. This is the name-shape half of that test -- the semantic half needs
 *  an environment, so a name that fails here is *reported as such*, not assumed. */
function blacklistShaped(name: string): boolean {
  const parts = name.split(".");
  if (parts.some((p) => p.startsWith("_"))) return true;
  const leaf = parts[parts.length - 1];
  return AUX_LEAF.has(leaf) || AUX_NUMBERED.test(leaf);
}

type Miss = { name: string; module: string; own: boolean; cause: string };
const causeOf = (r: Ref): string => {
  if (r.name.startsWith("_private.")) return "private name";
  if (blacklistShaped(r.name)) return "blacklisted by name shape";
  if (!modulesWithDecls.has(r.module)) return "module absent from this .bmp (partial build)";
  return "unclassified";
};

const hits = refs.filter((r) => decls[r.name] !== undefined);
const misses: Miss[] = refs
  .filter((r) => decls[r.name] === undefined)
  .map((r) => ({ name: r.name, module: r.module, own: targetModules.has(r.module), cause: causeOf(r) }));

const byCause = (pred: (m: Miss) => boolean) => {
  const m = new Map<string, number>();
  for (const x of misses.filter(pred)) m.set(x.cause, (m.get(x.cause) ?? 0) + 1);
  return [...m.entries()].sort((a, b) => b[1] - a[1]);
};
// A name can satisfy more than one rule; `cause` reports the first that fires,
// so the overlap is printed too rather than hidden behind the ordering.
const shapedAndAbsent = misses.filter(
  (m) => m.cause === "blacklisted by name shape" && !modulesWithDecls.has(m.module),
).length;

// Independent check of the join key. lean-doc gets the defining module from the
// Lean environment (`const2ModIdx`); doc-gen4 gets it from whichever module it
// rendered the declaration into. If the two disagree, the coverage number above
// is measuring the wrong thing -- a name could "hit" while pointing at a page
// that does not hold it.
const moduleAgree: string[] = [];
const moduleDisagree: string[] = [];
for (const r of refs) {
  if (!decls[r.name]) continue;
  const m = declModule.get(r.name);
  if (m === r.module) moduleAgree.push(r.name);
  else moduleDisagree.push(`${r.name}: lean-doc says ${r.module}, .bmp says ${m}`);
}

// Weight the misses by how many link sites they would actually break.
const occTotal = refs.reduce((a, r) => a + r.occurrences, 0);
const occDepTotal = depRefs.reduce((a, r) => a + r.occurrences, 0);
const occDepMissed = depRefs.filter((r) => !decls[r.name]).reduce((a, r) => a + r.occurrences, 0);

// ---------------------------------------------------------------- docLink resolution

const depHits = hits.filter((r) => !targetModules.has(r.module));
const step = Math.max(1, Math.floor(depHits.length / SAMPLE));
const sampled = depHits.filter((_, i) => i % step === 0).slice(0, SAMPLE);
let fileOk = 0, anchorOk = 0;
const brokenLinks: string[] = [];
for (const r of sampled) {
  const link = decls[r.name].docLink;
  const hash = link.indexOf("#");
  const rel = (hash < 0 ? link : link.slice(0, hash)).replace(/^\.\//, "");
  const anchor = hash < 0 ? "" : link.slice(hash + 1);
  const path = `${DOC}/${rel}`;
  let html = "";
  try {
    html = await Deno.readTextFile(path);
    fileOk++;
  } catch {
    brokenLinks.push(`${r.name} -> ${link} (no file)`);
    continue;
  }
  if (html.includes(`id="${anchor}"`)) anchorOk++;
  else brokenLinks.push(`${r.name} -> ${link} (file exists, anchor missing)`);
}

// ---------------------------------------------------------------- dead links doc-gen4 already emits

// The retreat line for stage 3 is "the dependency-side blacklist cannot be
// recovered from the map, so we link to declarations that have no page". Before
// accepting that as a cost of the map, check what doc-gen4 does today: its link
// renderer consults `name2ModIdx`, which is `env.const2ModIdx` (Analyze.lean:243)
// -- every constant in the environment, blacklisted ones included -- while the
// index only holds what it published. So a name can be linked and have no
// anchor. Search the package's own HTML for exactly the names that are missing
// from the index.
async function* htmlFiles(dir: string): AsyncGenerator<string> {
  for await (const e of Deno.readDir(dir)) {
    if (e.isDirectory) yield* htmlFiles(`${dir}/${e.name}`);
    else if (e.name.endsWith(".html")) yield `${dir}/${e.name}`;
  }
}
const depMissNames = misses.filter((m) => !m.own).map((m) => m.name);
const deadLinks = new Map<string, number>();
const pkgHtml = `${DOC}/${PKG}`;
try {
  for await (const p of htmlFiles(pkgHtml)) {
    const html = await Deno.readTextFile(p);
    for (const n of depMissNames) {
      const c = html.split(`#${n}"`).length - 1;
      if (c) deadLinks.set(n, (deadLinks.get(n) ?? 0) + c);
    }
  }
} catch { /* no package HTML: leave the map empty and say so */ }
const deadLinkAnchors = new Map<string, boolean>();
for (const n of deadLinks.keys()) {
  const ref = refs.find((r) => r.name === n)!;
  const page = `${DOC}/${ref.module.replaceAll(".", "/")}.html`;
  try {
    deadLinkAnchors.set(n, (await Deno.readTextFile(page)).includes(`id="${n}"`));
  } catch {
    deadLinkAnchors.set(n, false);
  }
}

// ---------------------------------------------------------------- size variants

type Slice = { label: string; names: string[] };
const slices: Slice[] = [
  { label: "all", names: declNames },
  { label: "dependency only", names: declNames.filter((n) => !targetModules.has(declModule.get(n) ?? "")) },
  { label: "referenced only", names: depRefs.filter((r) => decls[r.name]).map((r) => r.name) },
];

/** The four shapes the map could take, from doc-gen4's own to the real floor. */
function shapes(names: string[]) {
  const withKind: Record<string, IndexedDecl> = {};
  const noKind: Record<string, string> = {};
  const byModuleName: Record<string, string> = {};
  const modIndex = new Map<string, number>();
  const modList: string[] = [];
  const byModuleIdx: Record<string, number> = {};
  for (const n of names) {
    const d = decls[n];
    withKind[n] = { docLink: d.docLink, kind: d.kind };
    noKind[n] = d.docLink;
    const m = declModule.get(n) ?? "";
    byModuleName[n] = m;
    let i = modIndex.get(m);
    if (i === undefined) {
      i = modList.length;
      modIndex.set(m, i);
      modList.push(m);
    }
    byModuleIdx[n] = i;
  }
  return {
    "docLink + kind (as shipped)": JSON.stringify(withKind),
    "docLink only": JSON.stringify(noKind),
    "name -> module (string)": JSON.stringify(byModuleName),
    "name -> module (index) + module table": JSON.stringify({ modules: modList, names: byModuleIdx }),
    _modules: modList.length,
  };
}

const workDir = WORK || (await Deno.makeTempDir({ prefix: "lean-doc-mapsize-" }));
await Deno.mkdir(workDir, { recursive: true });

type SizeRow = {
  slice: string;
  shape: string;
  count: number;
  raw: number;
  gzip: number;
};
const sizeRows: SizeRow[] = [];
const sliceModuleCount = new Map<string, number>();
const timedFiles: { label: string; path: string; bytes: number }[] = [
  { label: "whole .bmp file (as shipped)", path: BMP, bytes: bmpBytes },
];

for (const s of slices) {
  const sh = shapes(s.names);
  sliceModuleCount.set(s.label, sh._modules);
  for (const [shape, text] of Object.entries(sh)) {
    if (shape.startsWith("_")) continue;
    const raw = new TextEncoder().encode(text as string).byteLength;
    sizeRows.push({ slice: s.label, shape, count: s.names.length, raw, gzip: await gzipBytes(text as string) });
    if (shape === "docLink + kind (as shipped)") {
      const path = `${workDir}/decls-${s.label.replaceAll(" ", "-")}.json`;
      await Deno.writeTextFile(path, text as string);
      timedFiles.push({ label: `declarations only, ${s.label}`, path, bytes: raw });
    }
  }
}

// ---------------------------------------------------------------- load time

type TimeRow = { label: string; bytes: number; times: number[] };
const timeRows: TimeRow[] = [];
for (const f of timedFiles) timeRows.push({ label: f.label, bytes: f.bytes, times: await loadTimes(f.path) });

// Cross-check: the numbers above are taken inside a long-running process whose
// heap is already warm, which is exactly the shape that flatters a parse
// benchmark. Repeat them one process per read, so a JIT/heap artifact would
// show up as a disagreement.
const standaloneRows: TimeRow[] = [];
if (STANDALONE > 0) {
  for (const f of timedFiles) {
    const times: number[] = [];
    for (let i = 0; i < STANDALONE; i++) {
      const cmd = new Deno.Command(Deno.execPath(), {
        args: ["run", "--allow-read", new URL(import.meta.url).pathname, "--load-once", f.path],
        stdout: "piped",
      });
      const { stdout } = await cmd.output();
      times.push(Number(new TextDecoder().decode(stdout).trim().split(" ")[0]));
    }
    standaloneRows.push({ label: f.label, bytes: f.bytes, times: times.slice(1) });
  }
}

// ---------------------------------------------------------------- kind usage

// Where `kind` is consumed, checked in the doc-gen4 tree rather than guessed:
//   DocGen4/Output/ToJson.lean          `JsonIndexedDeclarationInfo := { kind, docLink }`
//   static/declaration-data.js:177-191  the only reader; feeds `allowedKinds`
//   static/search.js:111,181            `.kind_checkbox`, the search filter UI
//   DocGen4/Output/Base.lean:338-381    link rendering -- reads `name2ModIdx` only
// i.e. the *link* never looks at `kind`; the search page's type filter does.

// ---------------------------------------------------------------- report

say("stage 3 / increment 2 -- demand vs supply, and the real size of the map");
say();
say("conditions");
say(`  date                ${new Date().toISOString()}`);
say(`  runtime             deno ${Deno.version.deno} / v8 ${Deno.version.v8} on ${Deno.build.target}`);
say(`  load-time runs      ${RUNS} per variant, run 1 dropped -> ${RUNS - 1} warm runs reported`);
say(`  warm/cold           warm by construction (the same file is re-read in-process)`);
for (const n of NOTES) say(`  ${n}`);
say();
say("inputs");
say(`  bmp                 ${BMP}`);
say(`                      ${num(bmpBytes)} bytes on disk, ${num(bmpText.length)} UTF-16 code units, 1 line`);
say(`                      top-level keys: ${Object.keys(bmp).join(" / ")}`);
say(`                      declarations ${num(declNames.length)}, modules ${num(Object.keys(bmp.modules).length)}`);
say(`  refs                ${REFS} (${num(refs.length)} unique constants)`);
say(`  target modules      ${MODULES} (${num(targetModules.size)})`);
say(`  doc root            ${DOC}`);
say(`  work dir            ${workDir}`);
say();

say("A. coverage -- is the demand in the supply");
say();
say("  set                            total      in .bmp     missing   coverage");
const covRow = (label: string, rs: Ref[]) => {
  const h = rs.filter((r) => decls[r.name]).length;
  say(
    `  ${label.padEnd(28)}${String(rs.length).padStart(6)}${String(h).padStart(12)}${
      String(rs.length - h).padStart(12)
    }   ${pct(h, rs.length)}`,
  );
};
covRow("all referenced constants", refs);
covRow("dependency side", depRefs);
covRow("own package", ownRefs);
say();
say(`  own-flag cross-check          the refs file's \`own\` disagrees with the module list on ${ownFlagMismatches.length} of ${refs.length}`);
say(`  dependency count, 2nd route   ${depRefs.length} constants whose defining module is not one of the ${targetModules.size}`);
say(`  join-key cross-check          defining module agrees between lean-doc (const2ModIdx) and the .bmp`);
say(`                                on ${num(moduleAgree.length)} of ${num(moduleAgree.length + moduleDisagree.length)} hits, ${moduleDisagree.length} disagreements`);
for (const d of moduleDisagree.slice(0, 20)) say(`                                  ${d}`);
say();
say("  weighted by occurrences (how many link sites would actually break)");
say(`    all referenced         ${num(occTotal)} occurrences`);
say(`    dependency side        ${num(occDepTotal)} occurrences, ${num(occDepMissed)} of them unresolvable (${pct(occDepMissed, occDepTotal)})`);
say();

say("B. what is missing, and why");
say();
say("  dependency side");
for (const [c, n] of byCause((m) => !m.own)) say(`    ${c.padEnd(46)}${String(n).padStart(5)}`);
for (const m of misses.filter((x) => !x.own)) say(`      ${m.name}  [${m.module}]  ${m.cause}`);
say();
say("  own package");
for (const [c, n] of byCause((m) => m.own)) say(`    ${c.padEnd(46)}${String(n).padStart(5)}`);
for (const m of misses.filter((x) => x.own && x.cause === "blacklisted by name shape")) {
  say(`      ${m.name}  [${m.module}]`);
}
say();
say(`  causes overlap: ${shapedAndAbsent} of the "blacklisted by name shape" misses are also in a module`);
say(`  that this .bmp never indexed, so those two causes are not exclusive.`);
say();
say(`  target-package modules that contributed declarations to this .bmp: ${
  [...targetModules].filter((m) => modulesWithDecls.has(m)).length
} of ${targetModules.size}`);
say("  (the HTML build was stopped at 42%, so the rest of the package was never indexed --");
say("   this is a property of the build that produced the .bmp, not of the mapping.)");
say();

const wholeSet = sampled.length === depHits.length;
say(
  `C. do the docLinks point at real pages (${sampled.length} of ${depHits.length} dependency-side hits${
    wholeSet ? ", i.e. all of them" : ", evenly spaced"
  })`,
);
say();
say(`  file exists                   ${fileOk} / ${sampled.length}`);
say(`  anchor id="<name>" present    ${anchorOk} / ${sampled.length}`);
if (brokenLinks.length) {
  say("  broken:");
  for (const b of brokenLinks.slice(0, 20)) say(`    ${b}`);
} else {
  say("  no broken links");
}
say();

say("C2. what doc-gen4 itself does with the names the index does not hold");
say();
say(`  doc-gen4's link renderer reads name2ModIdx = env.const2ModIdx (Analyze.lean:243),`);
say(`  i.e. every constant in the environment, not the ${num(declNames.length)} it published.`);
say(`  Searched ${pkgHtml} for the ${depMissNames.length} dependency-side misses:`);
if (deadLinks.size === 0) {
  say("    none of them is linked from the package HTML");
} else {
  for (const [n, c] of deadLinks) {
    say(`    ${n.padEnd(24)}${String(c).padStart(4)} href(s)   anchor on target page: ${deadLinkAnchors.get(n) ? "present" : "ABSENT -> dead link"}`);
  }
  say();
  say("  So doc-gen4 already emits hrefs with no anchor to land on. A map-based");
  say("  resolver that links only names present in the index is strictly better here:");
  say("  those names render as <span class=\"fn\"> instead of a dead link.");
}
say();

say("D. size of the map");
say();
say("  slice             shape                                   entries        raw       gzip");
for (const r of sizeRows) {
  say(
    `  ${r.slice.padEnd(18)}${r.shape.padEnd(38)}${num(r.count).padStart(9)}${
      num(r.raw).padStart(12)
    }${num(r.gzip).padStart(11)}`,
  );
}
say();
say(`  whole .bmp file (declarations + instances + instancesFor + modules): ${num(bmpBytes)} bytes = ${mib(bmpBytes)}`);
say(`  gzip of the whole file: ${num(await gzipBytes(bmpText))} bytes`);
say();
for (const [k, v] of sliceModuleCount) say(`  distinct defining modules, ${k}: ${num(v)}`);
say();
say("  Reading of the shapes:");
say("  - `docLink` repeats the module path for every declaration of that module and then");
say("    repeats the declaration's own name after the `#`. Both are derivable, so");
say("    `name -> module` is the same information with the duplication removed.");
say("  - Interning the module into an index pays off at Mathlib scale (raw halves again)");
say("    but not on a 530-entry slice, where the module table costs more than it saves");
say("    -- there, gzip on the string form is the smaller of the two.");
say("  - Turning `module -> URL` back into a path is increment 3's job. Until that is");
say("    implemented the `name -> module` rows are a measured size, not a working map.");
say();

say("E. load time (read + JSON.parse), seconds");
say();
say("  variant                                        bytes    median      min      max   vs 2.5s env load");
for (const r of timeRows) {
  const m = median(r.times);
  say(
    `  ${r.label.padEnd(40)}${num(r.bytes).padStart(11)}${m.toFixed(4).padStart(10)}${
      Math.min(...r.times).toFixed(4).padStart(9)
    }${Math.max(...r.times).toFixed(4).padStart(9)}   ${pct(m, ENV_LOAD_FLOOR_S)}`,
  );
}
say();
say(`  runs per variant: ${RUNS} (run 1 discarded). All values are wall clock inside one process.`);
if (standaloneRows.length) {
  say();
  say(`  cross-check, one process per read (${STANDALONE} runs, run 1 discarded):`);
  for (const r of standaloneRows) {
    say(
      `  ${r.label.padEnd(40)}${num(r.bytes).padStart(11)}${median(r.times).toFixed(4).padStart(10)}${
        Math.min(...r.times).toFixed(4).padStart(9)
      }${Math.max(...r.times).toFixed(4).padStart(9)}   ${pct(median(r.times), ENV_LOAD_FLOOR_S)}`,
    );
  }
} else {
  say("  (re-run with --standalone N --allow-run for the one-process-per-read cross-check)");
}
say();

say("F. is the `kind` column needed for links");
say();
say("  doc-gen4 renders a link in DocGen4/Output/Base.lean:338-381 (`renderedCodeToHtmlAux`,");
say("  the `.const` case). It consults `name2ModIdx` and emits `<a href=...>` or");
say("  `<span class=\"fn\">`. `kind` is never read there, and `declNameToLink`");
say("  (Base.lean:231) builds the href from the module name alone.");
say("  The only reader of `kind` is static/declaration-data.js:177-191, which feeds");
say("  the `allowedKinds` filter of the search page (static/search.js:111,181,");
say("  `.kind_checkbox`). So `kind` is a search-UI column, not a link column.");
say("  -> a link-resolution map needs 2 columns (name, module), not 3.");
say("  The size table above measures the `docLink only` shape for exactly this reason.");
say();

const jsonSummary = {
  conditions: {
    date: new Date().toISOString(),
    deno: Deno.version.deno,
    target: Deno.build.target,
    runs: RUNS,
    runsReported: RUNS - 1,
    warm: true,
    notes: NOTES,
  },
  inputs: { bmp: BMP, bmpBytes, refs: REFS, modules: MODULES, doc: DOC },
  coverage: {
    all: { total: refs.length, hit: hits.length },
    dependency: { total: depRefs.length, hit: depHits.length },
    own: { total: ownRefs.length, hit: hits.length - depHits.length },
    ownFlagMismatches: ownFlagMismatches.length,
  },
  misses,
  joinKey: { agree: moduleAgree.length, disagree: moduleDisagree },
  occurrences: { all: occTotal, dependency: occDepTotal, dependencyUnresolved: occDepMissed },
  docLinkSample: { sampled: sampled.length, fileOk, anchorOk, broken: brokenLinks },
  deadLinksInDocGen4: [...deadLinks].map(([name, hrefs]) => ({ name, hrefs, anchorPresent: deadLinkAnchors.get(name) })),
  sizes: sizeRows,
  wholeFile: { bytes: bmpBytes },
  loadTimes: timeRows.map((r) => ({ ...r, median: median(r.times) })),
  loadTimesStandalone: standaloneRows.map((r) => ({ ...r, median: median(r.times) })),
  moduleCounts: Object.fromEntries(sliceModuleCount),
};

console.log(out.join("\n"));
if (JSON_OUT) await Deno.writeTextFile(JSON_OUT, JSON.stringify(jsonSummary, null, 2) + "\n");
