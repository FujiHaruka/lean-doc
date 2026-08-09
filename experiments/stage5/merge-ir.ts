#!/usr/bin/env -S deno run --allow-read --allow-write
//
// Stage 5: fold a partial extraction back into the package IR.
//
// The extractor (experiments/stage4b) writes a *complete* IR tree for whatever
// module list it was given, so a one-module run produces a one-module tree. Two
// things therefore have to be repaired before that tree is usable as an update:
//
//   1. `index.json` must keep the other 431 modules' entries. The entry for the
//      re-extracted module is taken verbatim from the partial run — including
//      its `contentHash`, which only Lean can compute (it is `String.hash` of
//      the module JSON).
//
//   2. `deps/*.json` is **package-global and cannot be produced by a partial
//      run at all.** The extractor decides "dependency" as "defining module not
//      in the target list", so with a one-module target list the package's own
//      other modules are misfiled as dependencies (the one-module run of
//      `…Kolmogorov.OmegaNoncomputable` writes a `deps/InformationTheory.json`,
//      which the full run does not have). The fix is not to merge that file but
//      to recompute the slice from the merged module files, which is where the
//      `refs` live anyway.
//
// This is the first place where "one module changed" stops being local, and it
// is worth being precise about why: it is not a dependency of the *change*, it
// is an artefact of the extractor's interface. A driver that knows the package
// module list can pass it separately from the target list.
//
// usage:
//   merge-ir.ts --base <ir> --inc <ir> [--out <ir>] [--modules <list>]
//   merge-ir.ts --verify <ir> --against <ir>
//
//   --base    the IR to update (not modified unless --out is omitted... it is
//             never modified in place: --out defaults to --base + ".merged")
//   --inc     the partial extraction's IR tree
//   --modules the package's module list; defaults to the base index's modules
//   --verify  compare two IR trees: module files byte for byte, index entries
//             field by field, dependency slices as name -> module maps (their
//             JSON key order comes out of a Lean HashMap and is not
//             reproducible outside Lean, so the comparison is on the mapping).

const argv = Deno.args.slice();
const opt = (name: string, dflt = "") => {
  const i = argv.indexOf(name);
  return i >= 0 ? argv[i + 1] : dflt;
};

interface IndexEntry {
  module: string;
  file: string;
  bytes: number;
  declarations: number;
  contentHash: string;
}

async function readJson(p: string): Promise<any> {
  return JSON.parse(await Deno.readTextFile(p));
}

/** First component of a module name — the extractor's `moduleRoot`. */
function moduleRoot(m: string): string {
  return m.split(".")[0];
}

// ---------------------------------------------------------------- verify

async function verify(a: string, b: string): Promise<number> {
  const ia = await readJson(`${a}/index.json`);
  const ib = await readJson(`${b}/index.json`);
  let bad = 0;
  const ma = new Map<string, IndexEntry>(ia.modules.map((e: IndexEntry) => [e.module, e]));
  const mb = new Map<string, IndexEntry>(ib.modules.map((e: IndexEntry) => [e.module, e]));
  if (ma.size !== mb.size) {
    console.log(`FAIL module count ${ma.size} vs ${mb.size}`);
    bad++;
  }
  let same = 0;
  for (const [name, ea] of ma) {
    const eb = mb.get(name);
    if (!eb) {
      console.log(`FAIL missing in B: ${name}`);
      bad++;
      continue;
    }
    for (const k of ["file", "bytes", "declarations", "contentHash"] as const) {
      if ((ea as any)[k] !== (eb as any)[k]) {
        console.log(`FAIL index.${k} ${name}: ${(ea as any)[k]} vs ${(eb as any)[k]}`);
        bad++;
      }
    }
    const fa = await Deno.readFile(`${a}/${ea.file}`);
    const fb = await Deno.readFile(`${b}/${eb.file}`);
    if (fa.length !== fb.length || !fa.every((x, i) => x === fb[i])) {
      console.log(`FAIL bytes differ: ${name}`);
      bad++;
    } else same++;
  }
  console.log(`module files byte-identical: ${same}/${ma.size}`);

  // Dependency slices, compared as mappings.
  const depMap = async (root: string) => {
    const idx = await readJson(`${root}/index.json`);
    const out = new Map<string, string>();
    for (const d of idx.dependencyMaps ?? []) {
      const f = await readJson(`${root}/${d.file}`);
      for (const [n, m] of Object.entries(f.declarations as Record<string, string>)) out.set(n, m);
    }
    return out;
  };
  const da = await depMap(a);
  const db = await depMap(b);
  let dbad = 0;
  for (const [n, m] of da) {
    if (db.get(n) !== m) {
      if (dbad < 10) console.log(`FAIL dep ${n}: ${m} vs ${db.get(n)}`);
      dbad++;
    }
  }
  for (const n of db.keys()) {
    if (!da.has(n)) {
      if (dbad < 10) console.log(`FAIL dep only in B: ${n}`);
      dbad++;
    }
  }
  console.log(`dependency map entries: ${da.size} vs ${db.size}, mismatches ${dbad}`);
  bad += dbad;
  console.log(bad === 0 ? "VERIFY OK" : `VERIFY FAILED (${bad} problems)`);
  return bad;
}

// ---------------------------------------------------------------- merge

async function merge(base: string, inc: string, out: string): Promise<void> {
  const t0 = performance.now();
  const baseIndex = await readJson(`${base}/index.json`);
  const incIndex = await readJson(`${inc}/index.json`);
  const entries = new Map<string, IndexEntry>(
    baseIndex.modules.map((e: IndexEntry) => [e.module, e]),
  );
  const order: string[] = baseIndex.modules.map((e: IndexEntry) => e.module);

  await Deno.mkdir(`${out}/modules`, { recursive: true });
  await Deno.mkdir(`${out}/deps`, { recursive: true });
  if (out !== base) {
    // Copy the untouched part. This is the cost of not updating in place; a
    // real driver keeps one directory and rewrites only the changed files.
    for (const e of baseIndex.modules as IndexEntry[]) {
      if (incIndex.modules.some((x: IndexEntry) => x.module === e.module)) continue;
      await Deno.copyFile(`${base}/${e.file}`, `${out}/${e.file}`);
    }
  }
  const updated: string[] = [];
  // The *output*-side hash: `contentHash` is `String.hash` of the module JSON,
  // computed by the extractor (§5.5). A re-extracted module whose hash did not
  // move produces the same page, so it does not enter the render set. This is
  // the second ledger, and it is the one that decides what to re-render.
  const irChanged: string[] = [];
  for (const e of incIndex.modules as IndexEntry[]) {
    await Deno.copyFile(`${inc}/${e.file}`, `${out}/${e.file}`);
    const before = entries.get(e.module);
    if (!before) order.push(e.module);
    if (!before || before.contentHash !== e.contentHash) irChanged.push(e.module);
    entries.set(e.module, e);
    updated.push(e.module);
  }
  const tModules = performance.now();

  // Recompute the dependency slice from the merged module files: a reference is
  // a dependency iff its defining module is not one of the package's.
  const own = new Set(order);
  const dep = new Map<string, string>();
  let declarations = 0;
  for (const m of order) {
    const e = entries.get(m)!;
    const mod = await readJson(`${out}/${e.file}`);
    declarations += mod.declarations.length;
    for (const d of mod.declarations) {
      for (const [defMod, name] of d.refs as [string, string][]) {
        if (!own.has(defMod)) dep.set(name, defMod);
      }
    }
  }
  const byRoot = new Map<string, [string, string][]>();
  for (const [n, m] of dep) {
    const r = moduleRoot(m);
    if (!byRoot.has(r)) byRoot.set(r, []);
    byRoot.get(r)!.push([n, m]);
  }
  const depMaps: unknown[] = [];
  for (const [root, es] of [...byRoot].sort((x, y) => (x[0] < y[0] ? -1 : 1))) {
    const body = JSON.stringify({
      schemaVersion: baseIndex.schemaVersion,
      package: root,
      declarations: Object.fromEntries(es),
    });
    const file = `deps/${root}.json`;
    await Deno.writeTextFile(`${out}/${file}`, body);
    depMaps.push({
      package: root,
      file,
      entries: es.length,
      bytes: new TextEncoder().encode(body).length,
    });
  }
  // Drop dependency files that no longer belong (a package that stopped being
  // referenced, or the own-package slice a partial run wrongly produced).
  for await (const f of Deno.readDir(`${out}/deps`)) {
    if (!depMaps.some((d: any) => d.file === `deps/${f.name}`)) {
      await Deno.remove(`${out}/deps/${f.name}`);
    }
  }
  const index = {
    ...baseIndex,
    moduleCount: order.length,
    declarationCount: declarations,
    modules: order.map((m) => entries.get(m)!),
    dependencyMaps: depMaps,
  };
  await Deno.writeTextFile(`${out}/index.json`, JSON.stringify(index));
  const t1 = performance.now();
  console.log(
    `merged ${updated.length} module(s) into ${order.length}: ` +
      `modules ${((tModules - t0) / 1000).toFixed(4)} s, ` +
      `deps+index ${((t1 - tModules) / 1000).toFixed(4)} s, ` +
      `total ${((t1 - t0) / 1000).toFixed(4)} s -> ${out}`,
  );
  console.log(
    `IR content hash moved for ${irChanged.length} of ${updated.length} re-extracted module(s)` +
      (irChanged.length ? `: ${irChanged.join(", ")}` : ""),
  );
  const CHANGED_OUT = opt("--changed-out");
  if (CHANGED_OUT) {
    await Deno.writeTextFile(CHANGED_OUT, irChanged.join("\n") + (irChanged.length ? "\n" : ""));
  }
  const TIMINGS = opt("--timings");
  if (TIMINGS) {
    await Deno.writeTextFile(
      TIMINGS,
      JSON.stringify({
        command: "merge",
        updated: updated.length,
        irChanged: irChanged.length,
        modules: order.length,
        copySeconds: (tModules - t0) / 1000,
        depsSeconds: (t1 - tModules) / 1000,
        totalSeconds: (t1 - t0) / 1000,
      }) + "\n",
    );
  }
}

const VERIFY = opt("--verify");
if (VERIFY) {
  const AGAINST = opt("--against");
  if (!AGAINST) {
    console.error("usage: merge-ir.ts --verify <ir> --against <ir>");
    Deno.exit(2);
  }
  Deno.exit((await verify(VERIFY, AGAINST)) === 0 ? 0 : 1);
} else {
  const BASE = opt("--base");
  const INC = opt("--inc");
  if (!BASE || !INC) {
    console.error("usage: merge-ir.ts --base <ir> --inc <ir> [--out <ir>]");
    Deno.exit(2);
  }
  await merge(BASE, INC, opt("--out", BASE + ".merged"));
}
