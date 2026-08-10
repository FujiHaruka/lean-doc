#!/usr/bin/env -S deno run --allow-read --allow-write --allow-env
//
// Stage 5: the *input* side of incrementality — a per-module hash ledger over
// the extractor's actual inputs, so that "which modules have to be
// re-extracted" is answered without starting Lean.
//
// WHAT IS HASHED, AND WHY (see README.md "What is hashed")
//   The extractor's only view of a module is its `.olean` — `importModules`
//   reads nothing else. So the olean is what the ledger hashes:
//
//     * `.lean` source is one step too early. It carries changes the olean does
//       not (whitespace inside a proof) and misses changes the olean has
//       (anything a rebuild of a dependency puts into it).
//     * The IR's own content hash is one step too late: computing it requires
//       running the extraction the ledger exists to skip. That hash has a
//       different job — deciding which *pages* to rewrite (`--ir-ledger`).
//
//   Modules built with Lean's module system have three olean files
//   (`.olean` / `.olean.server` / `.olean.private`). All three are hashed when
//   present: the extractor reads docstrings and declaration ranges, which live
//   in the server data, and private declarations, which live in the private
//   one. On the measurement target the package's own modules have only
//   `.olean`; Mathlib's have all three.
//
// TWO SOURCES FOR THE SAME HASH
//   `--algorithm sha256` reads the olean bytes and hashes them here.
//   `--algorithm lake` instead reads the `<file>.hash` that Lake already wrote
//   next to every olean. That file is `computeBinFileHash` of the olean
//   (Lake/Build/Common.lean `cacheFileHash`), i.e. a content hash of the very
//   same bytes, already paid for by the build. It is a 64-bit non-cryptographic
//   hash and an implementation detail of Lake; `sha256` is the reference and
//   `lake` is measured as the free alternative.
//
// usage:
//   ledger.ts build --modules <list> --target <repo> --out <ledger.json>
//                   [--algorithm sha256|lake] [--concurrency N]
//                   [--ir <dir>] [--source-url <base>] [--timings <path>]
//   ledger.ts check --ledger <ledger.json> [--algorithm ...] [--concurrency N]
//                   [--ir <dir>] [--source-url <base>] [--changed-out <path>]
//                   [--removed-out <path>] [--render-all-out <path>]
//                   [--timings <path>]
//   ledger.ts touch --ledger <ledger.json> --module <M> [--out <path>]
//
//   `touch` is the honest fake this experiment is built on: the measurement
//   target must not be modified (its `.lake/build` is the baseline for every
//   number in this repository), so the *fact* "module M changed" is injected by
//   invalidating M's ledger entry. Everything downstream — the comparison, the
//   re-extraction, the page set, the timings — is real.

const argv = Deno.args.slice();
const cmd = argv[0] ?? "";
const opt = (name: string, dflt = "") => {
  const i = argv.indexOf(name);
  return i >= 0 ? argv[i + 1] : dflt;
};

const OLEAN_SUFFIXES = [".olean", ".olean.server", ".olean.private"];

interface FileEntry {
  path: string; // relative to the target repo
  bytes: number;
  hash: string;
}
interface ModuleEntry {
  module: string;
  files: FileEntry[];
  hash: string; // over the per-file hashes, so a missing/extra file shows up
}
interface Ledger {
  ledgerSchema: number;
  algorithm: string;
  target: string;
  libDir: string;
  extractKey: Record<string, string>;
  renderKey: Record<string, string>;
  modules: ModuleEntry[];
}

const enc = new TextEncoder();
function hex(buf: ArrayBuffer): string {
  return Array.from(new Uint8Array(buf)).map((b) => b.toString(16).padStart(2, "0")).join("");
}
async function sha256(bytes: Uint8Array): Promise<string> {
  return hex(await crypto.subtle.digest("SHA-256", bytes as BufferSource));
}
async function sha256Text(s: string): Promise<string> {
  return sha256(enc.encode(s));
}

function modulePaths(libDir: string, module: string): string[] {
  const base = `${libDir}/${module.split(".").join("/")}`;
  const out: string[] = [];
  for (const suffix of OLEAN_SUFFIXES) {
    const p = base + suffix;
    try {
      if (Deno.statSync(p).isFile) out.push(p);
    } catch { /* absent: this build does not use the module system */ }
  }
  return out;
}

/** One module's entry, or null when the module has no olean at all.
 *
 * Null is a real answer, not an error: a module can be deleted between `build`
 * and `check`, and the deletion is exactly what the caller needs to hear about.
 * Throwing here is what made `check` die with an exception instead of
 * reporting a removed module (stage 5b, S4). */
async function hashModule(
  target: string,
  libDir: string,
  module: string,
  algorithm: string,
): Promise<ModuleEntry | null> {
  const files: FileEntry[] = [];
  for (const p of modulePaths(libDir, module)) {
    if (algorithm === "lake") {
      // Lake's own content hash of exactly this file, already on disk.
      const h = (await Deno.readTextFile(p + ".hash")).trim();
      files.push({ path: p.slice(target.length + 1), bytes: -1, hash: h });
    } else {
      const bytes = await Deno.readFile(p);
      files.push({
        path: p.slice(target.length + 1),
        bytes: bytes.length,
        hash: await sha256(bytes),
      });
    }
  }
  if (files.length === 0) return null;
  const combined = await sha256Text(files.map((f) => `${f.path} ${f.hash}`).join("\n"));
  return { module, files, hash: combined };
}

function readModuleList(path: string): string[] {
  return Deno.readTextFileSync(path).split("\n").map((s) => s.trim())
    .filter((s) => s && !s.startsWith("#"));
}

/** Bounded-concurrency map, so the read path can be measured at 1 and at N. */
async function mapPool<A, B>(items: A[], n: number, f: (a: A) => Promise<B>): Promise<B[]> {
  if (n <= 1) {
    const out: B[] = [];
    for (const it of items) out.push(await f(it));
    return out;
  }
  const out: B[] = new Array(items.length);
  let next = 0;
  const workers = Array.from({ length: Math.min(n, items.length) }, async () => {
    for (;;) {
      const i = next++;
      if (i >= items.length) return;
      out[i] = await f(items[i]);
    }
  });
  await Promise.all(workers);
  return out;
}

/* L1, THE GLOBAL KEY, IS TWO KEYS — SPLIT BY BLAST RADIUS
 *
 * One global key was correct but far too coarse: any change to it re-extracted
 * all 432 modules. That is the wrong answer for the input that changes most
 * often. `--source-url` carries a 40-hex git revision, so it changes on *every
 * commit* — which is exactly when an incremental build runs. Folding it into a
 * single key would make every real incremental build pay a full re-extraction
 * (27 s, Lean started) on top of the full re-render it genuinely needs.
 *
 *   extractKey   inputs that can change the IR bytes.
 *                changed => re-extract everything. Which pages are then stale
 *                still follows from the IR diff, as usual — a re-extraction
 *                that lands byte-identical rewrites no page.
 *   renderKey    inputs that change the page bytes with the IR held fixed.
 *                changed => re-extract *nothing*, re-render *everything*.
 *
 * The test for which side an input belongs on is not "does it change the
 * output" (both do) but "can it change the IR" — i.e. does answering it require
 * starting Lean. `--source-url` cannot: render.ts refuses `--pages` without it
 * precisely because it is configuration doc-gen4 reads from lake + git and the
 * IR does not carry it.
 *
 * Both keys are compared as the *union* of the stored and current key sets, so
 * a key that vanished counts as a change (letting an absent `--ir` silently
 * hide the IR schema was the S1 failure). A forgotten `--source-url` therefore
 * reports the render key as changed and re-renders everything: over-rendering
 * is the safe direction, and the reason is printed rather than swallowed. */
async function extractKey(target: string, ir: string): Promise<Record<string, string>> {
  const key: Record<string, string> = {};
  key.leanToolchain = (await Deno.readTextFile(`${target}/lean-toolchain`)).trim();
  key.manifestSha256 = await sha256Text(await Deno.readTextFile(`${target}/lake-manifest.json`));
  key.extractor = "lean-doc/experiments/stage4b";
  if (ir) {
    const idx = JSON.parse(await Deno.readTextFile(`${ir}/index.json`));
    key.irSchemaVersion = String(idx.schemaVersion);
    key.irGenerator = String(idx.generator);
  }
  return key;
}

/** The generator id stands in for the renderer's configuration that has no flag
 * of its own; everything that does have a flag and reaches the output bytes
 * belongs here beside it. Values are stored in the clear, not hashed, so that a
 * mismatch names itself in the log. */
function renderKey(sourceUrl: string): Record<string, string> {
  const key: Record<string, string> = {};
  key.renderer = "lean-doc/experiments/stage4c";
  if (sourceUrl) key.sourceUrl = sourceUrl.replace(/\/+$/, "");
  return key;
}

/** Keys present in either set whose values differ. */
function keyDiff(was: Record<string, string>, now: Record<string, string>): string[] {
  return [...new Set([...Object.keys(was), ...Object.keys(now)])]
    .filter((k) => was[k] !== now[k]).sort();
}

const t0 = performance.now();
let timings: Record<string, unknown> = {};

if (cmd === "build") {
  const MODULES = opt("--modules");
  const TARGET = opt("--target").replace(/\/+$/, "");
  const OUT = opt("--out");
  const IR = opt("--ir");
  const ALGO = opt("--algorithm", "sha256");
  const CONC = Number(opt("--concurrency", "1"));
  if (!MODULES || !TARGET || !OUT) {
    console.error("usage: ledger.ts build --modules <list> --target <repo> --out <ledger.json>");
    Deno.exit(2);
  }
  const libDir = `${TARGET}/.lake/build/lib/lean`;
  const modules = readModuleList(MODULES);
  const tKey = performance.now();
  const xKey = await extractKey(TARGET, IR);
  const rKey = renderKey(opt("--source-url"));
  const tHash0 = performance.now();
  const maybe = await mapPool(modules, CONC, (m) => hashModule(TARGET, libDir, m, ALGO));
  const tHash1 = performance.now();
  const missing = modules.filter((_m, i) => maybe[i] === null);
  if (missing.length) {
    console.error(`no olean under ${libDir} for: ${missing.join(", ")}`);
    Deno.exit(3);
  }
  const entries = maybe as ModuleEntry[];
  const ledger: Ledger = {
    ledgerSchema: 2, // 1 had a single `envKey`; 2 splits it into extract/render
    algorithm: ALGO,
    target: TARGET,
    libDir,
    extractKey: xKey,
    renderKey: rKey,
    modules: entries,
  };
  await Deno.writeTextFile(OUT, JSON.stringify(ledger) + "\n");
  const tEnd = performance.now();
  const bytes = entries.reduce((a, e) => a + e.files.reduce((b, f) => b + Math.max(f.bytes, 0), 0), 0);
  timings = {
    command: "build",
    algorithm: ALGO,
    concurrency: CONC,
    modules: entries.length,
    files: entries.reduce((a, e) => a + e.files.length, 0),
    hashedBytes: bytes,
    keySeconds: (tHash0 - tKey) / 1000,
    hashSeconds: (tHash1 - tHash0) / 1000,
    writeSeconds: (tEnd - tHash1) / 1000,
    totalSeconds: (tEnd - t0) / 1000,
  };
  console.log(
    `build ${entries.length} modules, ${bytes.toLocaleString("en-US")} B hashed in ` +
      `${((tHash1 - tHash0) / 1000).toFixed(4)} s -> ${OUT}`,
  );
} else if (cmd === "check") {
  const LEDGER = opt("--ledger");
  const CHANGED = opt("--changed-out");
  const REMOVED = opt("--removed-out");
  const RENDER_ALL = opt("--render-all-out");
  const MODULES = opt("--modules");
  const CONC = Number(opt("--concurrency", "1"));
  if (!LEDGER) {
    console.error(
      "usage: ledger.ts check --ledger <ledger.json> [--modules <list>] " +
        "[--changed-out <path>] [--removed-out <path>] [--render-all-out <path>]",
    );
    Deno.exit(2);
  }
  const tRead0 = performance.now();
  const ledger = JSON.parse(await Deno.readTextFile(LEDGER)) as Ledger;
  const ALGO = opt("--algorithm", ledger.algorithm);
  const tRead1 = performance.now();
  if ((ledger.ledgerSchema ?? 1) < 2) {
    console.error(
      `${LEDGER} is ledgerSchema ${ledger.ledgerSchema ?? 1}; this build needs 2 ` +
        `(the single envKey was split into extractKey/renderKey). Rebuild the ledger.`,
    );
    Deno.exit(3);
  }
  const xKey = await extractKey(ledger.target, opt("--ir"));
  const rKey = renderKey(opt("--source-url"));
  const tKey = performance.now();

  // L1 in two halves (see the comment on extractKey): one invalidates the IR,
  // the other only the pages rendered from it.
  const extractKeyChanged = keyDiff(ledger.extractKey, xKey);
  const renderKeyChanged = keyDiff(ledger.renderKey ?? {}, rKey);

  // L2's module list must not be frozen at build time. With `--modules` the
  // current list is re-read, so a module that appeared since `build` is visible
  // as `added` and one that vanished as `removed`, instead of being invisible
  // and throwing respectively (stage 5b, S4).
  const previous = new Map(ledger.modules.map((e) => [e.module, e]));
  const current = MODULES ? readModuleList(MODULES) : [...previous.keys()];
  const now = await mapPool(
    current,
    CONC,
    (m) => hashModule(ledger.target, ledger.libDir, m, ALGO),
  );
  const tHash = performance.now();

  const present: ModuleEntry[] = [];
  const changed: string[] = [];
  const added: string[] = [];
  const removed: string[] = [];
  for (let i = 0; i < current.length; i++) {
    const m = current[i], e = now[i], was = previous.get(m);
    if (e === null) { removed.push(m); continue; } // in the list, no olean on disk
    present.push(e);
    if (!was) added.push(m);
    else if (e.hash !== was.hash) changed.push(m);
  }
  for (const m of previous.keys()) {
    if (!current.includes(m) && !removed.includes(m)) removed.push(m);
  }
  // A changed extract key invalidates every module's IR (L1). Computing the key
  // diff and then not acting on it was the whole failure of S1.
  const extractInvalidated = extractKeyChanged.length > 0;
  const reExtract = extractInvalidated
    ? present.map((e) => e.module)
    : [...changed, ...added].sort();
  // A changed render key invalidates no IR at all, so it is deliberately *not*
  // folded into `reExtract`. It is a separate signal because the re-extract set
  // and the re-render set are derived separately (approach.md §5.5).
  const renderAll = renderKeyChanged.length > 0;
  const tEnd = performance.now();

  if (CHANGED) {
    await Deno.writeTextFile(CHANGED, reExtract.join("\n") + (reExtract.length ? "\n" : ""));
  }
  if (REMOVED) {
    await Deno.writeTextFile(REMOVED, removed.join("\n") + (removed.length ? "\n" : ""));
  }
  if (RENDER_ALL) {
    // The reasons, one per line: an empty file means "the render set follows
    // from the IR diff as usual", which is what the caller tests for.
    const lines = renderKeyChanged.map((k) => `renderKey:${k}`);
    await Deno.writeTextFile(RENDER_ALL, lines.join("\n") + (lines.length ? "\n" : ""));
  }
  const bytes = present.reduce((a, e) => a + e.files.reduce((b, f) => b + Math.max(f.bytes, 0), 0), 0);
  timings = {
    command: "check",
    algorithm: ALGO,
    concurrency: CONC,
    modules: present.length,
    moduleListSource: MODULES ? "list" : "ledger",
    files: present.reduce((a, e) => a + e.files.length, 0),
    hashedBytes: bytes,
    extractKeyChanged,
    extractInvalidated,
    renderKeyChanged,
    renderAll,
    changed: changed.length,
    changedModules: changed,
    added: added.length,
    addedModules: added,
    removed: removed.length,
    removedModules: removed,
    reExtract: reExtract.length,
    readLedgerSeconds: (tRead1 - tRead0) / 1000,
    keySeconds: (tKey - tRead1) / 1000,
    hashSeconds: (tHash - tKey) / 1000,
    compareSeconds: (tEnd - tHash) / 1000,
    totalSeconds: (tEnd - t0) / 1000,
  };
  console.log(
    `check ${present.length} modules (${ALGO}, concurrency ${CONC}): ` +
      `${changed.length} changed, ${added.length} added, ${removed.length} removed` +
      (extractInvalidated
        ? `; extract key changed (${extractKeyChanged.join(",")}) -> all ${reExtract.length} re-extracted`
        : "") +
      (renderAll
        ? `; render key changed (${renderKeyChanged.join(",")}) -> re-render all, re-extract ${reExtract.length}`
        : "") +
      ` — ${((tEnd - t0) / 1000).toFixed(4)} s`,
  );
  for (const m of changed) console.log(`  changed  ${m}`);
  for (const m of added) console.log(`  added    ${m}`);
  for (const m of removed) console.log(`  removed  ${m}`);
} else if (cmd === "touch") {
  const LEDGER = opt("--ledger");
  const MODULE = opt("--module");
  const OUT = opt("--out", LEDGER);
  if (!LEDGER || !MODULE) {
    console.error("usage: ledger.ts touch --ledger <ledger.json> --module <M> [--out <path>]");
    Deno.exit(2);
  }
  const ledger = JSON.parse(await Deno.readTextFile(LEDGER)) as Ledger;
  const e = ledger.modules.find((m) => m.module === MODULE);
  if (!e) {
    console.error(`no such module in the ledger: ${MODULE}`);
    Deno.exit(3);
  }
  // Invalidate rather than delete: the entry has to stay so `check` compares it
  // and reports a *changed* module rather than a new one.
  e.hash = "injected-change:" + e.hash;
  await Deno.writeTextFile(OUT, JSON.stringify(ledger) + "\n");
  console.log(`touched ${MODULE} in ${OUT} (injected change; the olean is untouched)`);
} else {
  console.error("usage: ledger.ts build|check|touch ...");
  Deno.exit(2);
}

const TIMINGS = opt("--timings");
if (TIMINGS) await Deno.writeTextFile(TIMINGS, JSON.stringify(timings) + "\n");
