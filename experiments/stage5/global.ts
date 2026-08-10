#!/usr/bin/env -S deno run --allow-read --allow-write
//
// Stage 5, layer L3-3: the artifacts that are functions of *every* module, and
// the global name -> module map's delta.
//
// WHAT THESE ARE
//   doc-gen4 writes 23 non-module files; 5 of them depend on the whole package
//   (`declarations/declaration-data.bmp`, `navbar.html`, `tactics.html`,
//   `references.html`, `references.bib`). `render.ts` writes none of them, which
//   judgement point 3 reclassified from "not incremental" to "not implemented".
//
//   Display compatibility with doc-gen4 is explicitly out of scope
//   (approach.md §9), so these are *our* artifacts with the same job, not
//   byte-compatible copies. The oracle is that an incrementally updated artifact
//   equals a from-scratch one, not that either equals doc-gen4's.
//
// WHY THE MAP AND THE ARTIFACT ARE THE SAME PROGRAM
//   `declaration-data.bmp` serialises name -> (module, kind). That is the same
//   map a docstring's autolink resolves against, so the artifact's *delta* is
//   exactly the set of names whose links can have changed anywhere in the site.
//   Stage 5f measured a deletion dropping six names out of it and leaving a live
//   link to a now-nonexistent anchor in a module that shares nothing with the
//   deleted one. Computing the delta here means the render set can be closed
//   without guessing (§5.5 L3-2).
//
// usage:
//   global.ts build  --ir <dir> --out <pages dir> [--timings <p>]
//   global.ts delta  --before <map.json> --after <map.json> --ir <dir>
//                    [--print-set <path>] [--json <path>]
//
//   `build` also writes `<out>/declarations/name-map.json`, the flat map used by
//   `delta`. It is ours, not doc-gen4's: the .bmp carries the same information
//   in the shape doc-gen4's JavaScript expects, and re-deriving the flat map
//   from it would be parsing our own output back.

const argv = Deno.args.slice();
const cmd = argv[0] ?? "";
const opt = (name: string, dflt = "") => {
  const i = argv.indexOf(name);
  return i >= 0 ? argv[i + 1] : dflt;
};

interface IndexEntry {
  module: string;
  file: string;
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

async function readIndex(dir: string): Promise<IndexEntry[]> {
  const idx = JSON.parse(await Deno.readTextFile(`${dir}/index.json`)) as {
    modules: IndexEntry[];
  };
  return idx.modules;
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

/** The renderer's path rule: dots become directory separators. */
function pagePath(module: string): string {
  return module.split(".").join("/") + ".html";
}

/** The head constant of a printed type, from its tagged spans.
 *
 * `typeCode` spans are `[start, stop, kind]` with a fourth element naming the
 * constant when kind is 1. The head of an instance's type is the class, which
 * is the earliest such span — the same thing doc-gen4 keys `instances` on. */
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

// ------------------------------------------------------------------ build

interface NameEntry {
  module: string;
  kind: string;
}

async function build(ir: string, out: string, timingsPath: string): Promise<void> {
  const t0 = performance.now();
  const entries = await readIndex(ir);
  const mods: ModuleFile[] = [];
  for (const e of entries) {
    mods.push(JSON.parse(await Deno.readTextFile(`${ir}/${e.file}`)) as ModuleFile);
  }
  const tRead = performance.now();

  // name -> (module, kind), the map everything else is derived from.
  const nameMap = new Map<string, NameEntry>();
  const instances = new Map<string, string[]>();
  let tactics = 0;
  for (const m of mods) {
    tactics += (m.tactics ?? []).length;
    for (const d of m.declarations) {
      nameMap.set(d.name, { module: m.module, kind: d.kind });
      if (d.kind === "instance") {
        const cls = headConst(d);
        if (cls) {
          if (!instances.has(cls)) instances.set(cls, []);
          instances.get(cls)!.push(d.name);
        }
      }
    }
  }
  const own = new Set(mods.map((m) => m.module));
  const importedBy = new Map<string, string[]>();
  for (const m of own) importedBy.set(m, []);
  for (const m of mods) {
    for (const i of m.imports) if (own.has(i)) importedBy.get(i)!.push(m.module);
  }

  // Everything is emitted in sorted order. Not cosmetic: an artifact that is a
  // function of a *set* must not depend on the order the set was read in, or
  // "the incremental one equals the from-scratch one" stops being testable.
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

  // The dependency half of the map. It is not decoration: `render.ts` resolves a
  // backticked docstring token against own declarations *and* the dependency
  // slice, so a name that leaves the slice stops being a link. Stage 5f's one
  // wrong page was exactly that — `Nat.find` left the slice when the only module
  // referencing it was deleted — and a map holding only own declarations misses
  // it completely (4,750 -> 4,698 own names changed, and none of them was the
  // one that mattered).
  const depIndex = JSON.parse(await Deno.readTextFile(`${ir}/index.json`)) as {
    dependencyMaps?: { file: string }[];
  };
  const deps: Record<string, string> = {};
  for (const d of depIndex.dependencyMaps ?? []) {
    const f = JSON.parse(await Deno.readTextFile(`${ir}/${d.file}`)) as {
      declarations: Record<string, string>;
    };
    for (const [n, m] of Object.entries(f.declarations)) deps[n] = m;
  }
  const depNames = Object.keys(deps).sort();
  const dependencies: Record<string, string> = {};
  for (const n of depNames) dependencies[n] = deps[n];

  await Deno.mkdir(`${out}/declarations`, { recursive: true });
  // `declarations` holds what this site hosts; `dependencies` holds what it
  // links out to (§5.3). doc-gen4 puts both in one map because it documents the
  // whole dependency tree; we do not, and merging them would claim pages we do
  // not generate.
  const bmp = JSON.stringify({
    declarations,
    dependencies,
    instances: instancesOut,
    modules: modulesOut,
  });
  await Deno.writeTextFile(`${out}/declarations/declaration-data.bmp`, bmp);
  // The flat map `delta` reads: everything a docstring token can resolve to.
  // Kept beside the .bmp rather than re-parsed out of it, and sorted, so two
  // runs over the same module set are byte-equal.
  const flat: Record<string, string> = {};
  for (const n of [...sortedNames, ...depNames].sort()) {
    flat[n] = nameMap.get(n)?.module ?? deps[n];
  }
  await Deno.writeTextFile(`${out}/declarations/name-map.json`, JSON.stringify(flat));

  // navbar: the module tree. doc-gen4 rebuilds this by walking the emitted HTML
  // (Output.lean:357-383); walking the IR instead means it does not depend on
  // which pages happen to exist, which is the same reason the module list comes
  // from a source glob and not from `.lake/build`.
  const navRows = [...own].sort().map((m) =>
    `<li><a href="./${escapeHtml(pagePath(m))}">${escapeHtml(m)}</a></li>`
  ).join("");
  await Deno.writeTextFile(
    `${out}/navbar.html`,
    `<html lang="en"><head><meta charset="UTF-8"></meta>` +
      `<link rel="stylesheet" href="./style.css"></link><title>Modules</title></head>` +
      `<body><nav class="nav"><ul>${navRows}</ul></nav></body></html>`,
  );

  // tactics.html: the package contributes nothing to it. The tactic docs live in
  // an environment extension filled by core and Mathlib, not by these modules,
  // and the extractor records `tactics: []` for all of them. Emitting an empty
  // shell states that rather than hiding it.
  await Deno.writeTextFile(
    `${out}/tactics.html`,
    `<html lang="en"><head><meta charset="UTF-8"></meta>` +
      `<link rel="stylesheet" href="./style.css"></link><title>Tactics</title></head>` +
      `<body><main><p>This package declares no tactics ` +
      `(${tactics} tactic docstrings across ${mods.length} modules).</p></main></body></html>`,
  );

  // references.*: a function of the package's bibliography, which is empty here.
  await Deno.writeTextFile(`${out}/references.bib`, "");
  await Deno.writeTextFile(
    `${out}/references.html`,
    `<html lang="en"><head><meta charset="UTF-8"></meta>` +
      `<link rel="stylesheet" href="./style.css"></link><title>References</title></head>` +
      `<body><main><p>No references.</p></main></body></html>`,
  );
  const t1 = performance.now();

  const timings = {
    command: "build",
    modules: mods.length,
    declarations: sortedNames.length,
    dependencyNames: depNames.length,
    instanceClasses: Object.keys(instancesOut).length,
    tacticDocs: tactics,
    bmpBytes: new TextEncoder().encode(bmp).length,
    readSeconds: (tRead - t0) / 1000,
    writeSeconds: (t1 - tRead) / 1000,
    totalSeconds: (t1 - t0) / 1000,
  };
  console.log(
    `global build: ${mods.length} modules, ${sortedNames.length} declarations ` +
      `+ ${depNames.length} dependency names, ` +
      `${timings.bmpBytes.toLocaleString("en-US")} B of declaration data, ` +
      `${tactics} tactic docs — read ${timings.readSeconds.toFixed(4)} s, ` +
      `write ${timings.writeSeconds.toFixed(4)} s`,
  );
  if (timingsPath) await Deno.writeTextFile(timingsPath, JSON.stringify(timings) + "\n");
}

// ------------------------------------------------------------------ delta

/** The names a docstring can autolink, tokenised the way the renderer does.
 *
 * `render.ts`'s `autoLinkInline` takes the text inside a code span, splits it on
 * whitespace and control characters, and tries each part as a name — then, if
 * that fails, the part after its last dot. So the unit is **not** the code span:
 * `` `bAbsorbed = Nat.find` `` offers `bAbsorbed`, `=` and `Nat.find`, and it is
 * the third that resolves. Matching on whole code spans finds nothing here,
 * which is how the first version of this rule reported "0 pages to re-render"
 * while a page was demonstrably wrong.
 *
 * Over-approximating is safe (one page re-rendered for nothing);
 * under-approximating is the L3-2 bug itself, so the post-dot tail is included
 * as well even though the renderer only reaches for it as a fallback. */
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

async function delta(
  beforePath: string,
  afterPath: string,
  ir: string,
  printSet: string,
  jsonOut: string,
): Promise<void> {
  const t0 = performance.now();
  const before = JSON.parse(await Deno.readTextFile(beforePath)) as Record<string, string>;
  const after = JSON.parse(await Deno.readTextFile(afterPath)) as Record<string, string>;
  const changed = new Set<string>();
  for (const n of new Set([...Object.keys(before), ...Object.keys(after)])) {
    if (before[n] !== after[n]) changed.add(n);
  }
  const tDiff = performance.now();

  // Which pages mention one of those names in a docstring? A scan, not a guess.
  const affected: string[] = [];
  const witnesses: { module: string; name: string }[] = [];
  if (changed.size > 0) {
    for (const e of await readIndex(ir)) {
      const m = JSON.parse(await Deno.readTextFile(`${ir}/${e.file}`)) as ModuleFile;
      const docs: string[] = [];
      for (const md of m.moduleDocs ?? []) if (md.doc) docs.push(md.doc);
      for (const d of m.declarations) if (d.doc) docs.push(d.doc);
      let hit: string | null = null;
      for (const doc of docs) {
        for (const tok of autolinkTokens(doc)) {
          if (changed.has(tok)) { hit = tok; break; }
        }
        if (hit) break;
      }
      if (hit) {
        affected.push(m.module);
        if (witnesses.length < 20) witnesses.push({ module: m.module, name: hit });
      }
    }
  }
  const t1 = performance.now();

  affected.sort();
  const summary = {
    command: "delta",
    beforeNames: Object.keys(before).length,
    afterNames: Object.keys(after).length,
    changedNames: changed.size,
    changedSample: [...changed].sort().slice(0, 20),
    affected: affected.length,
    affectedModules: affected,
    witnesses,
    diffSeconds: (tDiff - t0) / 1000,
    scanSeconds: (t1 - tDiff) / 1000,
    totalSeconds: (t1 - t0) / 1000,
  };
  console.log(
    `global delta: ${changed.size} name(s) moved in or out of the map ` +
      `(${Object.keys(before).length} -> ${Object.keys(after).length}) -> ` +
      `${affected.length} page(s) to re-render — ${((t1 - t0) / 1000).toFixed(4)} s`,
  );
  for (const w of witnesses.slice(0, 10)) console.log(`  ${w.module}  (mentions \`${w.name}\`)`);
  if (printSet) {
    await Deno.writeTextFile(printSet, affected.join("\n") + (affected.length ? "\n" : ""));
  }
  if (jsonOut) await Deno.writeTextFile(jsonOut, JSON.stringify(summary, null, 2) + "\n");
}

// ------------------------------------------------------------------ main

if (cmd === "build") {
  const IR = opt("--ir"), OUT = opt("--out");
  if (!IR || !OUT) {
    console.error("usage: global.ts build --ir <dir> --out <pages dir> [--timings <p>]");
    Deno.exit(2);
  }
  await build(IR, OUT, opt("--timings"));
} else if (cmd === "delta") {
  const B = opt("--before"), A = opt("--after"), IR = opt("--ir");
  if (!B || !A || !IR) {
    console.error(
      "usage: global.ts delta --before <map.json> --after <map.json> --ir <dir> " +
        "[--print-set <p>] [--json <p>]",
    );
    Deno.exit(2);
  }
  await delta(B, A, IR, opt("--print-set"), opt("--json"));
} else {
  console.error("usage: global.ts build|delta ...");
  Deno.exit(2);
}
