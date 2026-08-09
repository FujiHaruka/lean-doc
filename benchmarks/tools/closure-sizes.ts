#!/usr/bin/env -S deno run -A
/**
 * Computes the transitive import-closure size of every module in a package, by parsing the
 * `import` headers of its `.lean` sources. The closure size is what drives `doc-gen4 single`:
 * each per-module process pays for loading its whole closure into an `Environment`.
 *
 * Usage: closure-sizes.ts <src-root> <module-prefix> [<src-root> <module-prefix> ...]
 */
const roots: [string, string][] = [];
for (let i = 0; i < Deno.args.length; i += 2) roots.push([Deno.args[i], Deno.args[i + 1]]);
if (roots.length === 0) {
  console.error("usage: closure-sizes.ts <src-root> <module-prefix> ...");
  Deno.exit(1);
}

const imports = new Map<string, string[]>();

for (const [root, prefix] of roots) {
  for await (const entry of walk(root)) {
    if (!entry.endsWith(".lean")) continue;
    const rel = entry.slice(root.length + 1, -".lean".length);
    const mod = prefix + (rel === "" ? "" : "." + rel.replaceAll("/", "."));
    const text = await Deno.readTextFile(entry);
    const deps: string[] = [];
    let inBlockComment = false;
    for (const line of text.split("\n")) {
      const t = line.trim();
      if (inBlockComment) {
        if (t.includes("-/")) inBlockComment = false;
        continue;
      }
      if (t.startsWith("/-")) {
        if (!t.includes("-/")) inBlockComment = true;
        continue;
      }
      if (t === "" || t.startsWith("--")) continue;
      // Lean 4.31 module system: `module`, `public import`, `meta import`, ...
      const m = /^(?:public\s+|meta\s+|private\s+|all\s+)*import\s+([A-Za-z0-9_.«»']+)/.exec(t);
      if (m) deps.push(m[1]);
      else if (t !== "prelude" && t !== "module") break; // first real declaration ends the header
    }
    imports.set(mod, deps);
  }
}

async function* walk(dir: string): AsyncGenerator<string> {
  for await (const e of Deno.readDir(dir)) {
    const p = `${dir}/${e.name}`;
    if (e.isDirectory) yield* walk(p);
    else yield p;
  }
}

const mods = [...imports.keys()].sort();
const idx = new Map(mods.map((m, i) => [m, i]));
const N = mods.length;
const words = Math.ceil(N / 32);
const closures = new Array<Int32Array | null>(N).fill(null);

function closure(i: number, stack: Set<number>): Int32Array {
  const cached = closures[i];
  if (cached) return cached;
  if (stack.has(i)) return new Int32Array(words); // import cycle guard
  stack.add(i);
  const bits = new Int32Array(words);
  bits[i >> 5] |= 1 << (i & 31);
  for (const dep of imports.get(mods[i]) ?? []) {
    const j = idx.get(dep);
    if (j === undefined) continue; // out-of-package import (e.g. Lean core)
    const sub = closure(j, stack);
    for (let w = 0; w < words; w++) bits[w] |= sub[w];
  }
  stack.delete(i);
  closures[i] = bits;
  return bits;
}

const popcount = (x: number) => {
  x = x - ((x >> 1) & 0x55555555);
  x = (x & 0x33333333) + ((x >> 2) & 0x33333333);
  return (((x + (x >> 4)) & 0x0f0f0f0f) * 0x01010101) >> 24;
};

let total = 0;
const sizes: number[] = [];
for (let i = 0; i < N; i++) {
  const b = closure(i, new Set());
  let c = 0;
  for (let w = 0; w < words; w++) c += popcount(b[w]);
  sizes.push(c);
  total += c;
}

sizes.sort((a, b) => a - b);
const pct = (p: number) => sizes[Math.floor((sizes.length - 1) * p)];
console.log(`modules:            ${N.toLocaleString()}`);
console.log(`sum of closures:    ${total.toLocaleString()}  (module-loads a per-module process pays for)`);
console.log(`mean closure:       ${(total / N).toFixed(0)}`);
console.log(`p50 / p90 / max:    ${pct(0.5)} / ${pct(0.9)} / ${sizes[sizes.length - 1]}`);
