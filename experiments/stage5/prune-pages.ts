#!/usr/bin/env -S deno run --allow-read --allow-write
//
// Stage 5: the deletion path, page third.
//
// A module that no longer exists leaves three things behind, and the renderer
// cleans up none of them because it only ever writes: `render.ts --only` writes
// the pages it is asked for and never looks at what else is in the tree. So a
// deleted module's page survives every subsequent incremental run, and it looks
// exactly like a live page — the failure is silent, which is why stage 5b's S4
// found the pipeline exiting rather than pretending to succeed.
//
// The other two thirds:
//   IR      `merge-ir.ts --remove`
//   ledger  rebuilt outright. `build` costs the same ~0.05 s as `check`, so
//           there is no reason to write an incremental ledger-update path.
//
// It also reports orphans — pages with no module in the IR at all — because
// that is the state a run that crashed halfway leaves behind, and because it is
// the same question `.lake`'s 659 ghost oleans answer wrongly (stage 5c): a
// directory listing is not a list of what exists.
//
// usage:
//   prune-pages.ts --pages <dir> [--remove <file>] [--ir <dir>] [--dry-run]
//                  [--json <path>]
//
//   --remove <file>  modules to delete, one per line
//   --ir <dir>       also delete every page with no module in this IR (orphans)
//   --dry-run        report, delete nothing

const argv = Deno.args.slice();
const opt = (name: string, dflt = "") => {
  const i = argv.indexOf(name);
  return i >= 0 ? argv[i + 1] : dflt;
};

const PAGES = opt("--pages");
const REMOVE = opt("--remove");
const IR = opt("--ir");
const JSON_OUT = opt("--json");
const DRY = argv.includes("--dry-run");
if (!PAGES || (!REMOVE && !IR)) {
  console.error(
    "usage: prune-pages.ts --pages <dir> [--remove <file>] [--ir <dir>] [--dry-run]",
  );
  Deno.exit(2);
}

function readLines(path: string): string[] {
  return Deno.readTextFileSync(path).split("\n").map((s) => s.trim())
    .filter((s) => s && !s.startsWith("#"));
}

/** The renderer's path rule: dots become directory separators. */
function pageOf(module: string): string {
  return module.split(".").join("/") + ".html";
}

const t0 = performance.now();
const removed = REMOVE ? readLines(REMOVE) : [];

const deleted: string[] = [];
const missing: string[] = []; // asked to delete, no page there — already gone
for (const m of removed) {
  const p = `${PAGES}/${pageOf(m)}`;
  try {
    Deno.statSync(p);
  } catch {
    missing.push(m);
    continue;
  }
  if (!DRY) await Deno.remove(p);
  deleted.push(m);
}

const orphans: string[] = [];
if (IR) {
  const idx = JSON.parse(await Deno.readTextFile(`${IR}/index.json`)) as {
    modules: { module: string }[];
  };
  const live = new Set(idx.modules.map((e) => pageOf(e.module)));
  const walk = async (dir: string): Promise<void> => {
    for await (const e of Deno.readDir(dir)) {
      const full = `${dir}/${e.name}`;
      if (e.isDirectory) {
        await walk(full);
      } else if (e.name.endsWith(".html")) {
        const rel = full.slice(PAGES.length + 1);
        if (!live.has(rel)) {
          orphans.push(rel);
          if (!DRY) await Deno.remove(full);
        }
      }
    }
  };
  await walk(PAGES);
}

// Directories left empty by the deletions. Left behind they are harmless but
// they make a page tree that is not equal to a from-scratch one, and byte
// equality with a from-scratch build is the only oracle this project trusts.
const emptied: string[] = [];
if (!DRY) {
  const prune = async (dir: string): Promise<boolean> => {
    let any = false;
    for await (const e of Deno.readDir(dir)) {
      if (e.isDirectory) {
        if (!(await prune(`${dir}/${e.name}`))) any = true;
      } else any = true;
    }
    if (!any && dir !== PAGES) {
      await Deno.remove(dir);
      emptied.push(dir.slice(PAGES.length + 1));
      return true;
    }
    return false;
  };
  await prune(PAGES);
}

const summary = {
  pages: PAGES,
  dryRun: DRY,
  requested: removed.length,
  deleted: deleted.length,
  deletedModules: deleted,
  alreadyAbsent: missing.length,
  orphans: orphans.length,
  orphanPages: orphans.slice(0, 20),
  emptiedDirectories: emptied.length,
  totalSeconds: (performance.now() - t0) / 1000,
};
console.log(
  `prune-pages${DRY ? " (dry run)" : ""}: deleted ${deleted.length}/${removed.length} ` +
    `requested, ${orphans.length} orphan(s), ${emptied.length} empty dir(s) ` +
    `— ${summary.totalSeconds.toFixed(4)} s`,
);
for (const o of orphans.slice(0, 10)) console.log(`  orphan  ${o}`);
if (JSON_OUT) await Deno.writeTextFile(JSON_OUT, JSON.stringify(summary, null, 2) + "\n");
