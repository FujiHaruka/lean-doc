#!/usr/bin/env -S deno run -A
/**
 * Aggregates the JSONL emitted by the instrumented doc-gen4 (`DOCGEN_TIMING=<file>`).
 *
 * Usage: analyze.ts <file.jsonl> [--by-module]
 */
type Rec = { phase: string; pid: number; us: number; [k: string]: unknown };

const [path, ...flags] = Deno.args;
if (!path) {
  console.error("usage: analyze.ts <file.jsonl> [--by-module]");
  Deno.exit(1);
}

const recs: Rec[] = Deno.readTextFileSync(path)
  .trim()
  .split("\n")
  .filter((l) => l.length > 0)
  .map((l) => JSON.parse(l));

/** Phases whose duration is contained in another phase; excluded from the "share of wall" view. */
const NESTED = new Set([
  "load.initSearchPath",
  "load.importModules",
  "load.process",
  "process.getAllModuleDocs",
  "process.constantLoop",
  "process.sortMembers",
  "single.updateModuleDb",
  "batch.updateModuleDb",
  "genCore.updateModuleDb",
  "index.collectBackrefs",
  "index.htmlOutputSetup",
  "index.readDiskModules",
  "index.buildIndex",
  "index.writeIndex",
  "navbar.scanHtmlFiles",
  // lean-doc's own extractor (experiments/stage1): everything below is inside `stage1.total`.
  "stage1.initSearchPath",
  "stage1.importModules",
  "stage1.envStats",
  "stage1.indexLookup",
  "stage1.scanLookup",
  "stage1.compare",
  // experiments/stage2: everything below is inside `stage2.total`.
  "stage2.initSearchPath",
  "stage2.importModules",
  "stage2.envStats",
  "stage2.indexLookup",
  "stage2.moduleDocs",
  "stage2.tactics",
  "stage2.analyze",
  "stage2.dump",
  "stage2.dumpModules",
  // Diagnosis only (`--tactics-emulate` / `--tactics-probe`): doc-gen4's shape and
  // the breakdown of one `allTacticDocs` call. Inside `stage2.total` as well.
  "stage2.tacticsPerModule",
  "stage2.tacticsProbe",
  // experiments/stage3: everything below is inside `stage3.total`.
  "stage3.initSearchPath",
  "stage3.importModules",
  "stage3.envStats",
  "stage3.indexLookup",
  "stage3.moduleDocs",
  "stage3.tactics",
  "stage3.analyze",
  "stage3.dump",
  "stage3.dumpModules",
  "stage3.dumpRefs",
  // Diagnosis only (`--tactics-emulate` / `--tactics-probe`), as in stage 2.
  "stage3.tacticsPerModule",
  "stage3.tacticsProbe",
]);

const byPhase = new Map<string, { n: number; us: number; max: number }>();
for (const r of recs) {
  const e = byPhase.get(r.phase) ?? { n: 0, us: 0, max: 0 };
  e.n += 1;
  e.us += r.us;
  e.max = Math.max(e.max, r.us);
  byPhase.set(r.phase, e);
}

const fmt = (us: number) => (us / 1e6).toFixed(2) + "s";

console.log(`\n=== ${path} — ${recs.length} records, ${new Set(recs.map((r) => r.pid)).size} processes ===\n`);
const rows = [...byPhase.entries()]
  .sort((a, b) => b[1].us - a[1].us)
  .map(([phase, e]) => ({
    phase,
    calls: e.n,
    total: fmt(e.us),
    mean: fmt(e.us / e.n),
    max: fmt(e.max),
    nested: NESTED.has(phase) ? "yes" : "",
  }));
console.table(rows);

// Sum of counters carried on the records (scanned constants, modules rendered, ...).
const counters: Record<string, number> = {};
for (const r of recs) {
  for (const [k, v] of Object.entries(r)) {
    if (["phase", "pid", "us", "module"].includes(k)) continue;
    if (typeof v === "number") counters[`${r.phase}.${k}`] = (counters[`${r.phase}.${k}`] ?? 0) + v;
  }
}
console.log("counters (summed over all records):");
for (const [k, v] of Object.entries(counters).sort()) console.log("  " + k.padEnd(46), v.toLocaleString());

if (flags.includes("--by-module")) {
  const byPid = new Map<number, Record<string, Rec>>();
  for (const r of recs) {
    if (!byPid.has(r.pid)) byPid.set(r.pid, {});
    byPid.get(r.pid)![r.phase] = r;
  }
  const perMod = [];
  for (const p of byPid.values()) {
    const t = p["single.total"] ?? p["batch.total"] ?? p["genCore.total"];
    if (!t) continue;
    perMod.push({
      module: String(t.module ?? `${t.modules} modules`),
      closure: p["load.envStats"]?.loadedModules as number,
      total: fmt(t.us),
      import: fmt(p["load.importModules"]?.us ?? 0),
      modDocs: fmt(p["process.getAllModuleDocs"]?.us ?? 0),
      loop: fmt(p["process.constantLoop"]?.us ?? 0),
      relevant: p["process.constantLoop"]?.relevant as number,
      db: fmt((p["single.updateModuleDb"] ?? p["batch.updateModuleDb"] ?? p["genCore.updateModuleDb"])?.us ?? 0),
    });
  }
  perMod.sort((a, b) => parseFloat(b.total) - parseFloat(a.total));
  console.log("\nper-process breakdown (slowest 25):");
  console.table(perMod.slice(0, 25));

  const totalUs = [...byPid.values()].reduce(
    (s, p) => s + ((p["single.total"] ?? p["batch.total"] ?? p["genCore.total"])?.us ?? 0),
    0,
  );
  const importUs = [...byPid.values()].reduce((s, p) => s + (p["load.importModules"]?.us ?? 0), 0);
  console.log(`\nprocesses: ${perMod.length}`);
  console.log(`sum of per-process totals: ${fmt(totalUs)}`);
  console.log(`  of which importModules:  ${fmt(importUs)}  (${((importUs / totalUs) * 100).toFixed(1)}%)`);
}
