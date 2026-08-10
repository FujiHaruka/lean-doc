#!/usr/bin/env -S deno run --allow-read --allow-write
//
// Stage 7h, L3-3: the whole-package artifacts, without reading the whole IR.
//
// A copy of `stage5/global.ts` with two changes and nothing else. Stage 5's copy
// is untouched because it is the control in stage 7h's A/B and docs quote its
// numbers.
//
// WHAT IS THE SAME
//   The artifacts and the way they are derived. Every line from "name -> (module,
//   kind)" down to the last `writeTextFile` is stage 5's, unchanged, and it still
//   runs over *all* 433 modules on every invocation. The oracle is byte equality
//   with a from-scratch build (`global.ts` header, stage 5), and the cheapest way
//   to keep it is to not touch the code that produces the bytes.
//
// WHAT IS DIFFERENT
//   (a) A cache. The derivation needs six facts per module — imports, tactic
//       count, (name, kind) per declaration, the head constant of each instance's
//       type, and the autolink tokens of its docstrings. Those are ~800 KB across
//       the package; the module IR they are read out of is 16 MB. `--state <dir>`
//       keeps them in one file and re-reads a module's IR only when the module's
//       `contentHash` in `index.json` has moved.
//
//       **The cache is keyed on the IR's own hash, not on the caller's idea of
//       what changed.** A driver that passes a wrong changed-set cannot corrupt
//       it; a stale entry is impossible unless the extractor emits the same hash
//       for different bytes, which is the same assumption `merge-ir.ts` already
//       makes when it decides which pages are stale.
//
//   (b) One process instead of two. `build` and `delta` were separate commands,
//       so the pipeline paid `deno` startup twice (measured: 0.052 s each,
//       benchmarks/results/stage7h-probe.txt). They need the same facts, so they
//       are one command here: `--before <map>` turns the delta on.
//
//   The delta's *scan* is the other full-IR read (0.116 s measured) and it goes
//   the same way: the tokens are in the cache. The scan's predicate is unchanged
//   — a module is affected iff any autolink token of any of its docstrings is in
//   the changed set — so the affected set is identical. The `witnesses` field of
//   the JSON summary can name a different token of the same module, because the
//   cache holds the tokens sorted and de-duplicated rather than in the order the
//   docstrings produced them. That field is diagnostic output; `--print-set`, the
//   thing the pipeline consumes, is not affected.
//
// usage:
//   global.ts build --ir <dir> --out <pages dir> [--state <dir>]
//                   [--before <map.json>] [--print-set <p>] [--delta-json <p>]
//                   [--timings <p>]
//
//   Without `--state` this reads every module file, i.e. it is stage 5's `build`
//   with stage 5's cost. That is not a fallback, it is the from-scratch build the
//   oracle compares against, and it has to be the same program.

const argv = Deno.args.slice();
const cmd = argv[0] ?? "";
const opt = (name: string, dflt = "") => {
  const i = argv.indexOf(name);
  return i >= 0 ? argv[i + 1] : dflt;
};

interface IndexEntry {
  module: string;
  file: string;
  contentHash: string;
}
interface Decl {
  name: string;
  kind: string;
  doc?: string;
  type?: string;
  typeCode?: (number | string)[][];
}
interface ModuleFile {
  module: string;
  imports: string[];
  moduleDocs: { doc?: string }[];
  tactics: unknown[];
  declarations: Decl[];
}
interface IndexFile {
  schemaVersion: number;
  generator: string;
  modules: IndexEntry[];
  dependencyMaps?: { file: string }[];
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

/** The renderer's path rule: dots become directory separators. */
function pagePath(module: string): string {
  return module.split(".").join("/") + ".html";
}

/** The head constant of a printed type, from its tagged spans. (stage 5) */
function headConst(d: Decl): string | null {
  let best: { start: number; name: string } | null = null;
  for (const span of d.typeCode ?? []) {
    if (span.length >= 4 && span[2] === 1 && typeof span[3] === "string") {
      const start = span[0] as number;
      if (!best || start < best.start) best = { start, name: span[3] };
    }
  }
  return best?.name ?? null;
}

/** The names a docstring can autolink, tokenised the way the renderer does.
 *  Transcribed unchanged from stage 5; see its comment for why the unit is the
 *  whitespace-split part and not the code span. */
function autolinkTokens(doc: string): string[] {
  const out: string[] = [];
  const push = (part: string) => {
    if (!part) return;
    out.push(part);
    const dot = part.lastIndexOf(".");
    if (dot >= 0) out.push(part.slice(dot + 1));
  };
  for (const m of doc.matchAll(/`([^`\n]+)`/g)) {
    for (const part of m[1].split(/[\p{Z}\p{C}]/u)) push(part);
  }
  // `[text](Target)` goes through the same name resolution (`extendLink`).
  for (const m of doc.matchAll(/\]\(([^)\s]+)\)/g)) push(m[1]);
  return out;
}

// ------------------------------------------------------------------ the cache

/** Everything the artifacts and the delta need from one module.
 *
 * This is the whole contract between the cache and the derivation: if a fact the
 * derivation reads is not here, adding it is a `STATE_DERIVATION` bump, not an
 * edit. Bumping it makes every entry a miss, which is correct and slow, rather
 * than keeping entries that were built by a different rule, which is fast and
 * wrong. */
interface ModuleFacts {
  module: string;
  contentHash: string;
  imports: string[];
  tactics: number;
  decls: [string, string][];
  instances: [string, string][];
  tokens: string[];
}
const STATE_VERSION = 1;
const STATE_DERIVATION = "stage7h/global.ts facts v1";

function factsOf(m: ModuleFile, contentHash: string): ModuleFacts {
  const decls: [string, string][] = [];
  const instances: [string, string][] = [];
  const tokens = new Set<string>();
  for (const md of m.moduleDocs ?? []) if (md.doc) for (const t of autolinkTokens(md.doc)) tokens.add(t);
  for (const d of m.declarations) {
    decls.push([d.name, d.kind]);
    if (d.kind === "instance") {
      const cls = headConst(d);
      if (cls) instances.push([cls, d.name]);
    }
    if (d.doc) for (const t of autolinkTokens(d.doc)) tokens.add(t);
  }
  return {
    module: m.module,
    contentHash,
    imports: m.imports,
    tactics: (m.tactics ?? []).length,
    decls,
    instances,
    tokens: [...tokens].sort(),
  };
}

interface StateFile {
  stateVersion: number;
  derivation: string;
  schemaVersion: number;
  generator: string;
  modules: Record<string, ModuleFacts>;
}

async function loadState(dir: string, index: IndexFile): Promise<Map<string, ModuleFacts>> {
  if (!dir) return new Map();
  let raw: string;
  try {
    raw = await Deno.readTextFile(`${dir}/global-state.json`);
  } catch {
    return new Map();
  }
  let st: StateFile;
  try {
    st = JSON.parse(raw) as StateFile;
  } catch {
    return new Map();
  }
  // A state written by a different rule, a different IR schema or a different
  // extractor is not a cache, it is a guess. Drop it whole.
  if (
    st.stateVersion !== STATE_VERSION || st.derivation !== STATE_DERIVATION ||
    st.schemaVersion !== index.schemaVersion || st.generator !== index.generator
  ) return new Map();
  return new Map(Object.entries(st.modules ?? {}));
}

async function saveState(
  dir: string,
  index: IndexFile,
  facts: Map<string, ModuleFacts>,
): Promise<number> {
  if (!dir) return 0;
  await Deno.mkdir(dir, { recursive: true });
  // Emitted in index order so that two runs over the same module set write the
  // same bytes; the state is not an oracle-checked artifact, but a file that
  // churns for no reason is a file nobody can diff.
  const modules: Record<string, ModuleFacts> = {};
  for (const e of index.modules) {
    const f = facts.get(e.module);
    if (f) modules[e.module] = f;
  }
  const body = JSON.stringify(
    {
      stateVersion: STATE_VERSION,
      derivation: STATE_DERIVATION,
      schemaVersion: index.schemaVersion,
      generator: index.generator,
      modules,
    } satisfies StateFile,
  );
  await Deno.writeTextFile(`${dir}/global-state.json`, body);
  return new TextEncoder().encode(body).length;
}

// ------------------------------------------------------------------ build

interface NameEntry {
  module: string;
  kind: string;
}

async function build(
  ir: string,
  out: string,
  state: string,
  beforePath: string,
  printSet: string,
  deltaJson: string,
  timingsPath: string,
): Promise<void> {
  const t0 = performance.now();
  // Read once. Stage 5 read `index.json` twice — once for the module list and
  // once for `dependencyMaps` — which is 0.09 MB of parse for nothing.
  const index = JSON.parse(await Deno.readTextFile(`${ir}/index.json`)) as IndexFile;
  const entries = index.modules;
  const cached = await loadState(state, index);
  const tState = performance.now();

  const facts = new Map<string, ModuleFacts>();
  let hits = 0, misses = 0;
  for (const e of entries) {
    const c = cached.get(e.module);
    // The hash is the extractor's `String.hash` of the module JSON, the same one
    // `merge-ir.ts` uses to decide staleness. Equal hash, equal bytes, equal facts.
    if (c && c.contentHash === e.contentHash) {
      facts.set(e.module, c);
      hits++;
      continue;
    }
    const m = JSON.parse(await Deno.readTextFile(`${ir}/${e.file}`)) as ModuleFile;
    facts.set(e.module, factsOf(m, e.contentHash));
    misses++;
  }
  const tRead = performance.now();

  // --- from here to the end of the writes: stage 5's derivation, unchanged ---
  // `entries` is in the order `index.json` lists, which is the order stage 5's
  // `mods` array had. That matters for exactly one thing — `nameMap.set` is
  // last-write-wins if two modules declare the same name — and it is the reason
  // the facts are iterated in index order rather than in `Map` order.
  const nameMap = new Map<string, NameEntry>();
  const instances = new Map<string, string[]>();
  let tactics = 0;
  for (const e of entries) {
    const f = facts.get(e.module)!;
    tactics += f.tactics;
    for (const [name, kind] of f.decls) nameMap.set(name, { module: f.module, kind });
    for (const [cls, n] of f.instances) {
      if (!instances.has(cls)) instances.set(cls, []);
      instances.get(cls)!.push(n);
    }
  }
  const own = new Set(entries.map((e) => facts.get(e.module)!.module));
  const importedBy = new Map<string, string[]>();
  for (const m of own) importedBy.set(m, []);
  for (const e of entries) {
    const f = facts.get(e.module)!;
    for (const i of f.imports) if (own.has(i)) importedBy.get(i)!.push(f.module);
  }

  const sortedNames = [...nameMap.keys()].sort();
  const declarations: Record<string, { docLink: string; kind: string }> = {};
  for (const n of sortedNames) {
    const e = nameMap.get(n)!;
    declarations[n] = { docLink: `./${pagePath(e.module)}#${n}`, kind: e.kind };
  }
  const instancesOut: Record<string, string[]> = {};
  for (const c of [...instances.keys()].sort()) instancesOut[c] = instances.get(c)!.sort();
  const modulesOut: Record<string, { importedBy: string[] }> = {};
  for (const m of [...own].sort()) {
    modulesOut[m] = { importedBy: importedBy.get(m)!.sort() };
  }

  // The dependency half of the map: 32 KB across three files, read in full every
  // run. Caching it would save a millisecond and add a second thing that can go
  // stale.
  const deps: Record<string, string> = {};
  for (const d of index.dependencyMaps ?? []) {
    const f = JSON.parse(await Deno.readTextFile(`${ir}/${d.file}`)) as {
      declarations: Record<string, string>;
    };
    for (const [n, m] of Object.entries(f.declarations)) deps[n] = m;
  }
  const depNames = Object.keys(deps).sort();
  const dependencies: Record<string, string> = {};
  for (const n of depNames) dependencies[n] = deps[n];

  await Deno.mkdir(`${out}/declarations`, { recursive: true });
  const bmp = JSON.stringify({
    declarations,
    dependencies,
    instances: instancesOut,
    modules: modulesOut,
  });
  await Deno.writeTextFile(`${out}/declarations/declaration-data.bmp`, bmp);
  const flat: Record<string, string> = {};
  for (const n of [...sortedNames, ...depNames].sort()) {
    flat[n] = nameMap.get(n)?.module ?? deps[n];
  }
  const flatBody = JSON.stringify(flat);
  await Deno.writeTextFile(`${out}/declarations/name-map.json`, flatBody);

  const navRows = [...own].sort().map((m) =>
    `<li><a href="./${escapeHtml(pagePath(m))}">${escapeHtml(m)}</a></li>`
  ).join("");
  await Deno.writeTextFile(
    `${out}/navbar.html`,
    `<html lang="en"><head><meta charset="UTF-8"></meta>` +
      `<link rel="stylesheet" href="./style.css"></link><title>Modules</title></head>` +
      `<body><nav class="nav"><ul>${navRows}</ul></nav></body></html>`,
  );
  await Deno.writeTextFile(
    `${out}/tactics.html`,
    `<html lang="en"><head><meta charset="UTF-8"></meta>` +
      `<link rel="stylesheet" href="./style.css"></link><title>Tactics</title></head>` +
      `<body><main><p>This package declares no tactics ` +
      `(${tactics} tactic docstrings across ${entries.length} modules).</p></main></body></html>`,
  );
  await Deno.writeTextFile(`${out}/references.bib`, "");
  await Deno.writeTextFile(
    `${out}/references.html`,
    `<html lang="en"><head><meta charset="UTF-8"></meta>` +
      `<link rel="stylesheet" href="./style.css"></link><title>References</title></head>` +
      `<body><main><p>No references.</p></main></body></html>`,
  );
  const t1 = performance.now();
  // --- end of stage 5's derivation ---

  // --- the delta, in the same process ---------------------------------------
  let deltaSummary: Record<string, unknown> | null = null;
  if (beforePath) {
    const before = JSON.parse(await Deno.readTextFile(beforePath)) as Record<string, string>;
    // `after` is the map this run just built. Stage 5 read it back off disk
    // because it was a separate process; here it is still in memory, and reading
    // our own output back would be the only way to get a *different* answer.
    const after = flat;
    const changed = new Set<string>();
    for (const n of new Set([...Object.keys(before), ...Object.keys(after)])) {
      if (before[n] !== after[n]) changed.add(n);
    }
    const tDiff = performance.now();

    const affected: string[] = [];
    const witnesses: { module: string; name: string }[] = [];
    if (changed.size > 0) {
      for (const e of entries) {
        const f = facts.get(e.module)!;
        let hit: string | null = null;
        for (const tok of f.tokens) {
          if (changed.has(tok)) { hit = tok; break; }
        }
        if (hit) {
          affected.push(f.module);
          if (witnesses.length < 20) witnesses.push({ module: f.module, name: hit });
        }
      }
    }
    const tScan = performance.now();
    affected.sort();
    deltaSummary = {
      command: "delta",
      beforeNames: Object.keys(before).length,
      afterNames: Object.keys(after).length,
      changedNames: changed.size,
      changedSample: [...changed].sort().slice(0, 20),
      affected: affected.length,
      affectedModules: affected,
      witnesses,
      diffSeconds: (tDiff - t1) / 1000,
      scanSeconds: (tScan - tDiff) / 1000,
      totalSeconds: (tScan - t1) / 1000,
    };
    console.log(
      `global delta: ${changed.size} name(s) moved in or out of the map ` +
        `(${Object.keys(before).length} -> ${Object.keys(after).length}) -> ` +
        `${affected.length} page(s) to re-render — ${((tScan - t1) / 1000).toFixed(4)} s`,
    );
    for (const w of witnesses.slice(0, 10)) console.log(`  ${w.module}  (mentions \`${w.name}\`)`);
    if (printSet) {
      await Deno.writeTextFile(printSet, affected.join("\n") + (affected.length ? "\n" : ""));
    }
    if (deltaJson) {
      await Deno.writeTextFile(deltaJson, JSON.stringify(deltaSummary, null, 2) + "\n");
    }
  }
  const t2 = performance.now();

  const stateBytes = await saveState(state, index, facts);
  const t3 = performance.now();

  const timings = {
    command: "build",
    state: state ? "on" : "off",
    cacheHits: hits,
    cacheMisses: misses,
    stateBytes,
    modules: entries.length,
    declarations: sortedNames.length,
    dependencyNames: depNames.length,
    instanceClasses: Object.keys(instancesOut).length,
    tacticDocs: tactics,
    bmpBytes: new TextEncoder().encode(bmp).length,
    nameMapBytes: new TextEncoder().encode(flatBody).length,
    // Same phase boundaries as stage 5's `build`, so the two records subtract.
    // `readSeconds` covers the state load *and* the module files that missed:
    // together they are what stage 5 spent reading all 433.
    stateLoadSeconds: (tState - t0) / 1000,
    readSeconds: (tRead - t0) / 1000,
    writeSeconds: (t1 - tRead) / 1000,
    deltaSeconds: (t2 - t1) / 1000,
    stateSaveSeconds: (t3 - t2) / 1000,
    totalSeconds: (t3 - t0) / 1000,
    delta: deltaSummary
      ? {
        changedNames: deltaSummary.changedNames,
        affected: deltaSummary.affected,
        diffSeconds: deltaSummary.diffSeconds,
        scanSeconds: deltaSummary.scanSeconds,
      }
      : null,
  };
  console.log(
    `global build: ${entries.length} modules (${hits} cached, ${misses} read), ` +
      `${sortedNames.length} declarations + ${depNames.length} dependency names, ` +
      `${timings.bmpBytes.toLocaleString("en-US")} B of declaration data, ` +
      `${tactics} tactic docs — read ${timings.readSeconds.toFixed(4)} s, ` +
      `write ${timings.writeSeconds.toFixed(4)} s, ` +
      `state ${(stateBytes / 1024).toFixed(0)} KB`,
  );
  if (timingsPath) await Deno.writeTextFile(timingsPath, JSON.stringify(timings) + "\n");
}

// ------------------------------------------------------------------ main

if (cmd === "build") {
  const IR = opt("--ir"), OUT = opt("--out");
  if (!IR || !OUT) {
    console.error(
      "usage: global.ts build --ir <dir> --out <pages dir> [--state <dir>] " +
        "[--before <map.json>] [--print-set <p>] [--delta-json <p>] [--timings <p>]",
    );
    Deno.exit(2);
  }
  await build(
    IR,
    OUT,
    opt("--state"),
    opt("--before"),
    opt("--print-set"),
    opt("--delta-json"),
    opt("--timings"),
  );
} else {
  console.error("usage: global.ts build ...");
  Deno.exit(2);
}
