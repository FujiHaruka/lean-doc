#!/usr/bin/env -S deno run --allow-read --allow-write
//
// Stage 5: the *output* side of incrementality — given a set of changed
// modules, which module IRs may change and which HTML pages have to be
// rewritten.
//
// This program never starts Lean and never touches the measurement target's
// build tree. Its only input is the schema-2 IR, which already carries both
// halves of the graph:
//
//   * `imports`            — the direct import edges (module -> module)
//   * `declarations[].refs — the `(defining module, name)` pairs that appear in
//                            a printed signature/equation
//
// Two closures are computed because they answer different questions and their
// sizes are what decides whether incremental generation is worth anything:
//
//   IMPORTERS(M)  the reverse *transitive* import closure of M, restricted to
//                 the package's own modules. This is the **sound** bound: a
//                 module that does not transitively import M cannot observe
//                 anything M declares — not its constants, not its notation,
//                 not its instances. Nothing else in Lean reaches across.
//
//   REFERRERS(M)  the modules that have at least one `refs` entry whose
//                 defining module is M. This is a **subset** of IMPORTERS(M)
//                 (to mention M's constant you must import M) and it is the set
//                 whose *printed text* names something of M's.
//
// Neither is "the answer" on its own; see README.md "Dependency propagation".
//
// usage:
//   impact.ts --ir <dir> [--changed <Module>]... [--changed-file <path>]
//             [--mode importers|referrers|self] [--census <path>]
//             [--print-set <path>] [--json <path>]
//
//   --mode self        the changed modules themselves (the rule the olean-hash
//                      ledger implements on its own)
//   --mode referrers   self + REFERRERS
//   --mode importers   self + IMPORTERS (the sound bound)
//   --census <path>    write a per-module census of |IMPORTERS| / |REFERRERS| /
//                      declaration count, TSV, for choosing the modules to
//                      measure with.
//   --print-set <path> write the resulting module set, one name per line.
//   --json <path>      write the summary as JSON.

const argv = Deno.args.slice();
const opt = (name: string, dflt = "") => {
  const i = argv.indexOf(name);
  return i >= 0 ? argv[i + 1] : dflt;
};
const opts = (name: string): string[] => {
  const out: string[] = [];
  for (let i = 0; i < argv.length; i++) if (argv[i] === name) out.push(argv[i + 1]);
  return out;
};

const IR = opt("--ir");
if (!IR) {
  console.error(
    "usage: impact.ts --ir <dir> [--changed <M>]... [--changed-file <p>] " +
      "[--mode self|referrers|importers] [--census <p>] [--print-set <p>] [--json <p>]",
  );
  Deno.exit(2);
}
const MODE = opt("--mode", "importers");
const CENSUS = opt("--census");
const PRINT_SET = opt("--print-set");
const JSON_OUT = opt("--json");

const changed: string[] = opts("--changed");
const changedFile = opt("--changed-file");
if (changedFile) {
  for (const line of (await Deno.readTextFile(changedFile)).split("\n")) {
    const s = line.trim();
    if (s && !s.startsWith("#")) changed.push(s);
  }
}

// ---------------------------------------------------------------- read IR

interface IndexEntry {
  module: string;
  file: string;
  bytes: number;
  declarations: number;
  contentHash: string;
}
interface ModuleFile {
  module: string;
  imports: string[];
  declarations: { name: string; refs: [string, string][] }[];
}

const index = JSON.parse(await Deno.readTextFile(`${IR}/index.json`)) as {
  modules: IndexEntry[];
};
const own = new Set(index.modules.map((m) => m.module));

const directImports = new Map<string, string[]>(); // module -> own-package imports
const refModules = new Map<string, Set<string>>(); // module -> own-package modules it names
const declCount = new Map<string, number>();
const definedIn = new Map<string, string>(); // declaration name -> module

for (const entry of index.modules) {
  const mod = JSON.parse(await Deno.readTextFile(`${IR}/${entry.file}`)) as ModuleFile;
  directImports.set(mod.module, mod.imports.filter((i) => own.has(i)));
  declCount.set(mod.module, mod.declarations.length);
  const r = new Set<string>();
  for (const d of mod.declarations) {
    definedIn.set(d.name, mod.module);
    for (const [m] of d.refs) if (own.has(m) && m !== mod.module) r.add(m);
  }
  refModules.set(mod.module, r);
}

// ---------------------------------------------------------------- graphs

/** Reverse direct-import edges, restricted to the package's own modules. */
const importedBy = new Map<string, string[]>();
for (const m of own) importedBy.set(m, []);
for (const [m, imps] of directImports) {
  for (const i of imps) importedBy.get(i)!.push(m);
}

/** Reverse direct-reference edges: M -> modules whose printed text names M. */
const referredBy = new Map<string, string[]>();
for (const m of own) referredBy.set(m, []);
for (const [m, rs] of refModules) {
  for (const r of rs) referredBy.get(r)!.push(m);
}

function closure(seeds: string[], edges: Map<string, string[]>): Set<string> {
  const seen = new Set<string>();
  const stack = [...seeds];
  while (stack.length) {
    const cur = stack.pop()!;
    for (const nxt of edges.get(cur) ?? []) {
      if (!seen.has(nxt)) {
        seen.add(nxt);
        stack.push(nxt);
      }
    }
  }
  return seen;
}

// ---------------------------------------------------------------- census

if (CENSUS) {
  const rows: string[] = [
    "module\tdeclarations\tdirectImports\timportedByDirect\timportersTransitive\treferrersDirect",
  ];
  for (const entry of index.modules) {
    const m = entry.module;
    const imps = closure([m], importedBy);
    rows.push(
      [
        m,
        declCount.get(m) ?? 0,
        (directImports.get(m) ?? []).length,
        (importedBy.get(m) ?? []).length,
        imps.size,
        (referredBy.get(m) ?? []).length,
      ].join("\t"),
    );
  }
  await Deno.writeTextFile(CENSUS, rows.join("\n") + "\n");
  console.log(`census -> ${CENSUS} (${index.modules.length} modules)`);
}

// ---------------------------------------------------------------- impact

if (changed.length > 0) {
  for (const m of changed) {
    if (!own.has(m)) {
      console.error(`not a module of this package: ${m}`);
      Deno.exit(3);
    }
  }
  const self = new Set(changed);
  const importers = closure(changed, importedBy);
  const referrers = closure(changed, referredBy); // transitive over reference edges
  const referrersDirect = new Set<string>();
  for (const m of changed) for (const r of referredBy.get(m) ?? []) referrersDirect.add(r);

  let set: Set<string>;
  switch (MODE) {
    case "self":
      set = self;
      break;
    case "referrers":
      set = new Set([...self, ...referrersDirect]);
      break;
    case "importers":
      set = new Set([...self, ...importers]);
      break;
    default:
      console.error(`unknown --mode ${MODE}`);
      Deno.exit(2);
  }

  const list = [...set!].sort();
  const summary = {
    ir: IR,
    changed,
    mode: MODE,
    ownModules: own.size,
    self: self.size,
    referrersDirect: referrersDirect.size,
    referrersTransitive: referrers.size,
    importersTransitive: importers.size,
    selected: list.length,
    selectedDeclarations: list.reduce((a, m) => a + (declCount.get(m) ?? 0), 0),
    selectedIrBytes: index.modules.filter((e) => set!.has(e.module)).reduce(
      (a, e) => a + e.bytes,
      0,
    ),
  };
  console.log(JSON.stringify(summary, null, 2));
  if (JSON_OUT) await Deno.writeTextFile(JSON_OUT, JSON.stringify(summary, null, 2) + "\n");
  if (PRINT_SET) await Deno.writeTextFile(PRINT_SET, list.join("\n") + "\n");
}
