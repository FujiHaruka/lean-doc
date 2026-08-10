#!/usr/bin/env -S deno run --allow-read --allow-write --allow-env
//
// Stage 7h, step 1: how many times does one incremental run read the whole IR?
//
// The question L3-3's ceiling depends on is not "how long does `global.ts` take"
// but "how much of that time is a full IR read that some other step is doing
// anyway". Counting the reads by reading the code is possible (six scripts) but
// not checkable; counting them by *running* the real scripts is.
//
// This wrapper patches `Deno.readTextFile` / `readTextFileSync` / `readFile` /
// `readFileSync` / `copyFile`, overrides `Deno.args`, and then imports the
// script under test. The script itself is untouched — that is the point: a
// modified copy would count the copy's reads, not the pipeline's.
//
// It measures reads, so its own timings are worthless (the counting adds work
// per call). Timings come from the uninstrumented runs.
//
// usage:
//   count-reads.ts --tag <name> --out <jsonl> -- <script.ts> [args...]

const argv = Deno.args.slice();
const sep = argv.indexOf("--");
if (sep < 0) {
  console.error("usage: count-reads.ts --tag <t> --out <jsonl> -- <script.ts> [args...]");
  Deno.exit(2);
}
const head = argv.slice(0, sep);
const rest = argv.slice(sep + 1);
const opt = (n: string, d = "") => {
  const i = head.indexOf(n);
  return i >= 0 ? head[i + 1] : d;
};
const TAG = opt("--tag", "?");
const OUT = opt("--out");
const script = rest[0];
if (!script) {
  console.error("count-reads.ts: no script");
  Deno.exit(2);
}

interface Bucket {
  moduleFiles: number;
  depFiles: number;
  indexJson: number;
  other: number;
  otherPaths: string[];
}
const b: Bucket = { moduleFiles: 0, depFiles: 0, indexJson: 0, other: 0, otherPaths: [] };
const seenModules = new Set<string>();

function note(p: string | URL) {
  const s = String(p);
  if (s.endsWith("/index.json")) b.indexJson++;
  else if (/\/modules\/[^/]+\.json$/.test(s)) {
    b.moduleFiles++;
    seenModules.add(s.replace(/^.*\/modules\//, ""));
  } else if (/\/deps\/[^/]+\.json$/.test(s)) b.depFiles++;
  else {
    b.other++;
    if (b.otherPaths.length < 40) b.otherPaths.push(s);
  }
}

// deno-lint-ignore no-explicit-any
const D = Deno as any;
for (const name of ["readTextFile", "readFile"]) {
  const orig = D[name].bind(Deno);
  D[name] = (p: string | URL, o?: unknown) => {
    note(p);
    return orig(p, o);
  };
}
for (const name of ["readTextFileSync", "readFileSync"]) {
  const orig = D[name].bind(Deno);
  D[name] = (p: string | URL, o?: unknown) => {
    note(p);
    return orig(p, o);
  };
}
// `merge-ir.ts` moves module files with `copyFile`, which reads them in the
// kernel rather than in JS. It is a read of the same bytes and is counted
// separately so it can be told apart from a parse.
let copies = 0;
{
  const orig = D.copyFile.bind(Deno);
  D.copyFile = (from: string | URL, to: string | URL) => {
    copies++;
    return orig(from, to);
  };
}

Object.defineProperty(Deno, "args", { value: rest.slice(1), configurable: true });

const report = () => {
  const rec = {
    tag: TAG,
    script: script.replace(/^.*\/experiments\//, "experiments/"),
    moduleFileReads: b.moduleFiles,
    distinctModuleFiles: seenModules.size,
    depFileReads: b.depFiles,
    indexJsonReads: b.indexJson,
    copyFileCalls: copies,
    otherReads: b.other,
    otherPathsSample: b.otherPaths,
  };
  const line = JSON.stringify(rec);
  console.error(
    `[count-reads] ${TAG}: module-file reads ${b.moduleFiles} ` +
      `(${seenModules.size} distinct), deps ${b.depFiles}, index.json ${b.indexJson}, ` +
      `copyFile ${copies}, other ${b.other}`,
  );
  if (OUT) Deno.writeTextFileSync(OUT, line + "\n", { append: true });
};
globalThis.addEventListener("unload", report);

const url = new URL("file://" + (script.startsWith("/") ? script : Deno.cwd() + "/" + script));
await import(url.href);
