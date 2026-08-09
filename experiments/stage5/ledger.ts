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
//                   [--ir <dir>] [--timings <path>]
//   ledger.ts check --ledger <ledger.json> [--algorithm ...] [--concurrency N]
//                   [--changed-out <path>] [--timings <path>]
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
  envKey: Record<string, string>;
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

/** One module's entry. `algorithm` decides where the per-file hash comes from. */
async function hashModule(
  target: string,
  libDir: string,
  module: string,
  algorithm: string,
): Promise<ModuleEntry> {
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
  if (files.length === 0) throw new Error(`no olean for ${module} under ${libDir}`);
  const combined = await sha256Text(files.map((f) => `${f.path} ${f.hash}`).join("\n"));
  return { module, files, hash: combined };
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

async function envKey(target: string, ir: string): Promise<Record<string, string>> {
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
  const modules = (await Deno.readTextFile(MODULES)).split("\n").map((s) => s.trim())
    .filter((s) => s && !s.startsWith("#"));
  const tKey = performance.now();
  const key = await envKey(TARGET, IR);
  const tHash0 = performance.now();
  const entries = await mapPool(modules, CONC, (m) => hashModule(TARGET, libDir, m, ALGO));
  const tHash1 = performance.now();
  const ledger: Ledger = {
    ledgerSchema: 1,
    algorithm: ALGO,
    target: TARGET,
    libDir,
    envKey: key,
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
    envKeySeconds: (tHash0 - tKey) / 1000,
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
  const CONC = Number(opt("--concurrency", "1"));
  if (!LEDGER) {
    console.error("usage: ledger.ts check --ledger <ledger.json> [--changed-out <path>]");
    Deno.exit(2);
  }
  const tRead0 = performance.now();
  const ledger = JSON.parse(await Deno.readTextFile(LEDGER)) as Ledger;
  const ALGO = opt("--algorithm", ledger.algorithm);
  const tRead1 = performance.now();
  const key = await envKey(ledger.target, opt("--ir"));
  const tKey = performance.now();
  const envChanged = Object.entries(ledger.envKey)
    .filter(([k, v]) => key[k] !== undefined && key[k] !== v)
    .map(([k]) => k);
  const now = await mapPool(
    ledger.modules,
    CONC,
    (e) => hashModule(ledger.target, ledger.libDir, e.module, ALGO),
  );
  const tHash = performance.now();
  const changed: string[] = [];
  for (let i = 0; i < now.length; i++) {
    if (now[i].hash !== ledger.modules[i].hash) changed.push(now[i].module);
  }
  const tEnd = performance.now();
  if (CHANGED) await Deno.writeTextFile(CHANGED, changed.join("\n") + (changed.length ? "\n" : ""));
  const bytes = now.reduce((a, e) => a + e.files.reduce((b, f) => b + Math.max(f.bytes, 0), 0), 0);
  timings = {
    command: "check",
    algorithm: ALGO,
    concurrency: CONC,
    modules: now.length,
    files: now.reduce((a, e) => a + e.files.length, 0),
    hashedBytes: bytes,
    envChanged,
    changed: changed.length,
    changedModules: changed,
    readLedgerSeconds: (tRead1 - tRead0) / 1000,
    envKeySeconds: (tKey - tRead1) / 1000,
    hashSeconds: (tHash - tKey) / 1000,
    compareSeconds: (tEnd - tHash) / 1000,
    totalSeconds: (tEnd - t0) / 1000,
  };
  console.log(
    `check ${now.length} modules (${ALGO}, concurrency ${CONC}): ${changed.length} changed` +
      (envChanged.length ? `, env key changed: ${envChanged.join(",")}` : "") +
      ` — ${((tEnd - t0) / 1000).toFixed(4)} s`,
  );
  for (const m of changed) console.log(`  changed  ${m}`);
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
