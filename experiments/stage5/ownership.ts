#!/usr/bin/env -S deno run --allow-read --allow-write
//
// Stage 5, layer L3-1: **name ownership**.
//
// THE HOLE THIS CLOSES, AND WHY NOTHING ELSE CLOSES IT
//   The IR stores every reference as a `(defining module, name)` pair, because
//   the printed token and the constant it links to often have no textual
//   relation (`ℕ` -> `Nat`). That pair is a fact about *where a name lives*, and
//   it goes stale when the name moves — even though nothing about the referring
//   module changed.
//
//   Stage 5c measured that this is invisible to every other layer: moving a
//   declaration from A to X leaves the referring module C's `.olean` **byte
//   identical** (and Lake's hash unmoved), so L2 cannot see it, and no widening
//   of the *render* set can fix it either — the stale bytes are in C's IR, so C
//   has to be re-**extracted**. Renaming, by contrast, does change C's olean,
//   because the new name is embedded in C's terms.
//
//   So this program answers: given the modules just re-extracted, whose IR now
//   holds a `(module, name)` pair that the fresh IR contradicts?
//
// HOW
//   1. For every module M in the incremental IR, diff the set of names M
//      defines against the set it defined in the base IR.
//        lost(M)    names M no longer defines — moved away, or deleted
//        gained(M)  names M now defines — moved in, or new
//   2. Scan every *other* module's references for those names. A reference
//      `(O, n)` is stale when
//        (a) n ∈ lost(O)                  O no longer defines what it points at
//        (b) n ∈ gained(M), M ≠ O         n now lives somewhere else
//      (b) is not implied by (a): (a) needs O itself to be in the re-extracted
//      set, which holds for a move (removing a declaration changes A's olean)
//      but is worth checking separately rather than assuming.
//   3. The stale modules are the *second round* of re-extraction. Rounds are
//      the caller's business (`incremental.sh`); this program reports one round.
//
// Reads the IR only. Lean is never started and the measurement target is never
// touched.
//
// usage:
//   ownership.ts --base <base ir> [--inc <inc ir>] [--removed <file>]
//                [--exclude <file>] [--print-set <path>] [--json <path>]
//
//   --exclude <file>  modules already scheduled for re-extraction, one per
//                     line; they are fresh by definition and are never
//                     reported. Normally the changed-set file.
//   --removed <file>  modules that no longer exist. Every name they defined is
//                     lost, so this is the same computation as a move with an
//                     empty "gained" side — deletion and relocation are one
//                     mechanism, not two.

const argv = Deno.args.slice();
const opt = (name: string, dflt = "") => {
  const i = argv.indexOf(name);
  return i >= 0 ? argv[i + 1] : dflt;
};

const BASE = opt("--base");
const INC = opt("--inc");
const REMOVED = opt("--removed");
const EXCLUDE = opt("--exclude");
const PRINT_SET = opt("--print-set");
const JSON_OUT = opt("--json");
if (!BASE || (!INC && !REMOVED)) {
  console.error(
    "usage: ownership.ts --base <base ir> [--inc <inc ir>] [--removed <file>] " +
      "[--exclude <file>] [--print-set <path>] [--json <path>]",
  );
  Deno.exit(2);
}

const t0 = performance.now();

interface IndexEntry {
  module: string;
  file: string;
}
interface ModuleFile {
  module: string;
  declarations: { name: string; refs: [string, string][] }[];
}

function readLines(path: string): string[] {
  return Deno.readTextFileSync(path).split("\n").map((s) => s.trim())
    .filter((s) => s && !s.startsWith("#"));
}

async function readIndex(dir: string): Promise<IndexEntry[]> {
  const idx = JSON.parse(await Deno.readTextFile(`${dir}/index.json`)) as {
    modules: IndexEntry[];
  };
  return idx.modules;
}

async function readModule(dir: string, file: string): Promise<ModuleFile> {
  return JSON.parse(await Deno.readTextFile(`${dir}/${file}`)) as ModuleFile;
}

// ------------------------------------------------------------ 1. the diff

const baseEntries = await readIndex(BASE);
const baseFileOf = new Map(baseEntries.map((e) => [e.module, e.file]));
const incEntries = INC ? await readIndex(INC) : [];
const fresh = new Set(incEntries.map((e) => e.module));
const removedModules = REMOVED ? readLines(REMOVED).filter((m) => baseFileOf.has(m)) : [];

/** name -> the modules that lost it / the modules that gained it. */
const lostOwners = new Map<string, Set<string>>();
const gainedOwners = new Map<string, Set<string>>();
let lostCount = 0, gainedCount = 0;

for (const e of incEntries) {
  const now = new Set((await readModule(INC, e.file)).declarations.map((d) => d.name));
  const baseFile = baseFileOf.get(e.module);
  // A module absent from the base IR is new: it has no ownership history, so
  // nothing can be pointing at it wrongly yet.
  const was = baseFile
    ? new Set((await readModule(BASE, baseFile)).declarations.map((d) => d.name))
    : new Set<string>();
  for (const n of was) {
    if (!now.has(n)) {
      if (!lostOwners.has(n)) lostOwners.set(n, new Set());
      lostOwners.get(n)!.add(e.module);
      lostCount++;
    }
  }
  for (const n of now) {
    if (!was.has(n)) {
      if (!gainedOwners.has(n)) gainedOwners.set(n, new Set());
      gainedOwners.get(n)!.add(e.module);
      gainedCount++;
    }
  }
}

// A deleted module is a module whose whole name set was lost. Same computation,
// empty "gained" side.
for (const m of removedModules) {
  for (const d of (await readModule(BASE, baseFileOf.get(m)!)).declarations) {
    if (!lostOwners.has(d.name)) lostOwners.set(d.name, new Set());
    lostOwners.get(d.name)!.add(m);
    lostCount++;
  }
}
const tDiff = performance.now();

// ------------------------------------------------------------ 2. the scan

const exclude = new Set<string>(EXCLUDE ? readLines(EXCLUDE) : []);
for (const m of fresh) exclude.add(m);
// A removed module must not be reported as needing re-extraction: it is gone.
for (const m of removedModules) exclude.add(m);

const staleByRule = { lostOwner: new Set<string>(), movedElsewhere: new Set<string>() };
const witnesses: { module: string; rule: string; ref: [string, string] }[] = [];

// Nothing moved and nothing was deleted: no module can be pointing anywhere
// wrong, and the whole base IR does not have to be read.
const watching = lostOwners.size > 0 || gainedOwners.size > 0;
if (watching) {
  for (const e of baseEntries) {
    if (exclude.has(e.module)) continue;
    const mod = await readModule(BASE, e.file);
    for (const d of mod.declarations) {
      for (const [owner, name] of d.refs) {
        if (lostOwners.get(name)?.has(owner)) {
          if (!staleByRule.lostOwner.has(e.module)) {
            witnesses.push({ module: e.module, rule: "lostOwner", ref: [owner, name] });
          }
          staleByRule.lostOwner.add(e.module);
        } else {
          const g = gainedOwners.get(name);
          if (g && !g.has(owner)) {
            if (!staleByRule.movedElsewhere.has(e.module)) {
              witnesses.push({ module: e.module, rule: "movedElsewhere", ref: [owner, name] });
            }
            staleByRule.movedElsewhere.add(e.module);
          }
        }
      }
    }
  }
}
const tScan = performance.now();

const stale = [...new Set([...staleByRule.lostOwner, ...staleByRule.movedElsewhere])].sort();

const summary = {
  base: BASE,
  inc: INC,
  incModules: incEntries.length,
  removedModules: removedModules.length,
  scannedBaseModules: watching ? baseEntries.length - exclude.size : 0,
  lostNames: lostCount,
  gainedNames: gainedCount,
  lostNamesDistinct: lostOwners.size,
  gainedNamesDistinct: gainedOwners.size,
  staleByLostOwner: staleByRule.lostOwner.size,
  staleByMovedElsewhere: staleByRule.movedElsewhere.size,
  stale: stale.length,
  staleModules: stale,
  witnesses: witnesses.slice(0, 20),
  diffSeconds: (tDiff - t0) / 1000,
  scanSeconds: (tScan - tDiff) / 1000,
  totalSeconds: (tScan - t0) / 1000,
};

console.log(
  `ownership: ${lostCount} name(s) lost, ${gainedCount} gained across ${incEntries.length} ` +
    `re-extracted module(s) -> ${stale.length} module(s) need re-extraction ` +
    `— ${((tScan - t0) / 1000).toFixed(4)} s`,
);
for (const w of witnesses.slice(0, 10)) {
  console.log(`  ${w.rule.padEnd(15)} ${w.module}  (ref ${w.ref[0]} :: ${w.ref[1]})`);
}
if (JSON_OUT) await Deno.writeTextFile(JSON_OUT, JSON.stringify(summary, null, 2) + "\n");
if (PRINT_SET) {
  await Deno.writeTextFile(PRINT_SET, stale.join("\n") + (stale.length ? "\n" : ""));
}
