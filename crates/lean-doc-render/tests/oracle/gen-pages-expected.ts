#!/usr/bin/env -S deno run --allow-read --allow-write --allow-run
// gen-pages-expected.ts -- run the frozen prototype *as a program* over small
// synthetic IR trees and record the pages it writes, so that `tests/pages.rs`
// compares the Rust page builder and main loop against the prototype's bytes
// rather than against a reading of it.
//
// WHY A SUBPROCESS AND NOT A SLICE
// --------------------------------
// The other generators in this directory slice functions out of `render.ts` by
// line range and call them. That works for a leaf like `declHeader`. It does
// not work for what M1-d3 ports, because the thing under test *is* the top
// level of `render.ts`: how `known`, `suppressed` and `knownModules` are built,
// in what order, and which modules get a page. A slice would have to
// re-implement that top level in order to test it, and would then be testing
// the re-implementation. Running the whole program on an IR tree tests it.
//
// The price is that each case needs a complete, valid IR tree. That is what
// makes the cases small and hand-written: they exist to reach the corners of
// the ordering, suppression and module-selection rules, and the real corpus is
// covered by `pages_match_the_reference_tree` (env-gated) and by
// `tools/render-compare.sh`.
//
// ONE CASE RECORDS A DISAGREEMENT ON PURPOSE
// ------------------------------------------
// `render.ts` cannot express "render no modules": `ONLY.length > 0 ? ... : null`
// (2088) makes zero `--only` flags mean *every* module, which is why the
// incremental pipeline had to guard the call in shell (`incremental.sh:367`).
// The case named "empty render set" records what the prototype does, and
// `tests/pages.rs` asserts the port does the opposite. See plan §5.
//
// npm/node are broken in this environment; this must run under deno.
//
// usage:
//   deno run --allow-read --allow-write --allow-run \
//     crates/lean-doc-render/tests/oracle/gen-pages-expected.ts
//   ... --check      fail if the committed fixture is stale
//   ... --ir DIR     take the corpus branch profile from another IR tree

const FIXTURE = new URL("../data/pages-expected.json", import.meta.url);
const RENDER_TS = new URL("../../../../experiments/stage7d/render.ts", import.meta.url);

const DEFAULT_IR = "/private/tmp/lean-doc-relay/w7h/base-ir";
/**
 * The same base `tools/render-reference.sh` uses. **40 hex digits, not a tag**
 * (plan 決定 1): `coverage.ts` normalises `/blob/[0-9a-f]{40}/` and nothing
 * else, so anything shorter silently lowers the acceptance score.
 */
const SOURCE_URL =
  "https://github.com/FujiHaruka/information-theory/blob/573793b243fb1343636088eb62d1789ab2b14cec";

const argv = Deno.args.slice();
const opt = (name: string, dflt = "") => {
  const i = argv.indexOf(name);
  return i >= 0 ? argv[i + 1] : dflt;
};
const check = argv.includes("--check");
const irDir = opt("--ir", DEFAULT_IR);

// --------------------------------------------------------------- IR builders

type Json = Record<string, unknown>;

/** A declaration with every schema-4 key the reader requires. */
function decl(o: {
  name: string;
  kind?: string;
  line?: number;
  col?: number;
  index?: number;
  doc?: string | null;
  members?: Json[];
  refs?: [string, string][];
}): Json {
  const line = o.line ?? 1;
  return {
    name: o.name,
    kind: o.kind ?? "theorem",
    modifiers: [],
    binders: [],
    implicits: [],
    binderCode: [],
    type: "Prop",
    typeCode: [],
    line,
    col: o.col ?? 0,
    endLine: line,
    endCol: 1,
    index: o.index ?? 0,
    members: o.members ?? [],
    doc: o.doc ?? null,
    equations: [],
    equationCode: [],
    refs: o.refs ?? [],
  };
}

function ctorMember(name: string): Json {
  return { label: "ctor", name, text: "", code: [] };
}

function moduleFile(o: {
  module: string;
  imports?: string[];
  moduleDocs?: { line: number; col: number; text: string }[];
  declarations?: Json[];
}): Json {
  return {
    schemaVersion: 4,
    module: o.module,
    imports: o.imports ?? [],
    moduleDocs: o.moduleDocs ?? [],
    tactics: [],
    declarations: o.declarations ?? [],
  };
}

type Case = {
  what: string;
  /** Which of the branches below this case reaches. */
  branches: string[];
  modules: Json[];
  deps?: { package: string; declarations: Record<string, string> }[];
  lidx?: string;
  /** `null` = no `--only` flag; `[]` = a render set that came out empty. */
  only?: string[] | null;
  /** Whether the run passes `--link-index`. The product always does. */
  linkIndex?: boolean;
  note?: string;
};

// ------------------------------------------------------------------- the cases

const cases: Case[] = [
  {
    what: "curated: a module docstring and a declaration at the same position",
    // The tie-breaker is a running sequence number, not `Decl.index`: the
    // docstrings take 0..k and a declaration takes k + index. Here the second
    // docstring and `Pkg.Two.b` (index 0) share 7:0, so `index` alone would put
    // the declaration first. This is the *only* shape that tells the two apart.
    branches: [
      "onlyAll",
      "moduleDocItems",
      "pageDeclItems",
      "tieDocVsDecl",
      "tieDeclVsDecl",
      "docAfterFirstDecl",
      "nestedPagePath",
    ],
    modules: [
      moduleFile({
        module: "Pkg.Two",
        imports: ["Pkg.One"],
        moduleDocs: [
          { line: 1, col: 0, text: "the first section" },
          { line: 7, col: 0, text: "the second section" },
        ],
        declarations: [
          decl({ name: "Pkg.Two.b", line: 7, col: 0, index: 0 }),
          decl({ name: "Pkg.Two.a", kind: "structure", line: 5, col: 0, index: 1 }),
          decl({ name: "Pkg.Two.c", line: 5, col: 0, index: 2 }),
        ],
      }),
    ],
  },
  {
    what: "curated: a member of another module's declaration",
    // `suppressed` is collected across every module (`render.ts:2043-2048`).
    // `Pkg.Two.b` is declared in `Pkg.Two` and is a member of a structure in
    // `Pkg.One`, so a per-module set leaves it on `Pkg.Two`'s page.
    branches: ["onlyAll", "suppressedSkipped", "pageDeclItems", "nestedPagePath"],
    modules: [
      moduleFile({
        module: "Pkg.One",
        declarations: [
          decl({
            name: "Pkg.One.s",
            kind: "structure",
            line: 1,
            index: 0,
            members: [ctorMember("Pkg.Two.b")],
          }),
        ],
      }),
      moduleFile({
        module: "Pkg.Two",
        declarations: [
          decl({ name: "Pkg.Two.a", line: 1, index: 0 }),
          decl({ name: "Pkg.Two.b", kind: "constructor", line: 3, index: 1 }),
        ],
      }),
    ],
  },
  {
    what: "curated: --only names one module of two",
    // The unselected module is still read: `known` and `suppressed` are
    // site-wide, and a page rendered against a partial map differs in the links
    // it draws rather than failing.
    branches: ["onlyThese", "onlySkipped", "suppressedSkipped", "nestedPagePath"],
    only: ["Pkg.Two"],
    modules: [
      moduleFile({
        module: "Pkg.One",
        declarations: [
          decl({
            name: "Pkg.One.s",
            kind: "structure",
            line: 1,
            index: 0,
            members: [ctorMember("Pkg.Two.b")],
          }),
        ],
      }),
      moduleFile({
        module: "Pkg.Two",
        declarations: [
          decl({ name: "Pkg.Two.a", line: 1, index: 0, doc: "See `Pkg.One.s`." }),
          decl({ name: "Pkg.Two.b", kind: "constructor", line: 3, index: 1 }),
        ],
      }),
    ],
  },
  {
    what: "curated: --only names a module the IR does not have",
    branches: ["onlyThese", "onlySkipped"],
    only: ["Pkg.Absent"],
    modules: [moduleFile({ module: "Pkg.One", declarations: [decl({ name: "Pkg.One.a" })] })],
  },
  {
    what: "curated: empty render set",
    // What the *prototype* does with it, which is the opposite of right. See
    // the header comment.
    branches: ["emptyOnly"],
    only: [],
    note: "render.ts renders every module when --only is passed zero times",
    modules: [
      moduleFile({ module: "Pkg.One", declarations: [decl({ name: "Pkg.One.a" })] }),
      moduleFile({ module: "Pkg.Two", declarations: [decl({ name: "Pkg.Two.a" })] }),
    ],
  },
  {
    what: "curated: a module with no declarations, no docs and no imports",
    branches: [
      "onlyAll",
      "emptyPage",
      "moduleWithoutModuleDocs",
      "moduleWithoutPageEntries",
      "moduleWithoutImports",
      "topLevelPagePath",
    ],
    modules: [moduleFile({ module: "Pkg" })],
  },
  {
    what: "curated: a top level module and a deeply nested one",
    branches: ["onlyAll", "topLevelPagePath", "nestedPagePath", "moduleWithoutModuleDocs"],
    modules: [
      moduleFile({ module: "Pkg", imports: ["Pkg.A.B.C.D"], declarations: [decl({ name: "Pkg.x" })] }),
      moduleFile({ module: "Pkg.A.B.C.D", declarations: [decl({ name: "Pkg.A.B.C.D.y" })] }),
    ],
  },
  {
    what: "curated: a module name that needs escaping",
    branches: ["onlyAll", "moduleNameNeedsEscaping", "nestedPagePath"],
    modules: [
      moduleFile({
        module: 'Pkg.A<B&C"D',
        declarations: [decl({ name: 'Pkg.A<B&C"D.f' })],
      }),
    ],
  },
  {
    what: "curated: a reference does not overwrite a declaration",
    // `known` takes declarations with `set` and references with
    // `if (!known.has(n))` (`render.ts:2031-2032`). `Pkg.One` is read first and
    // declares `Pkg.One.a`; `Pkg.Two`'s reference claims it lives in
    // `Wrong.Module`. The docstring link says which one won.
    branches: ["pageDeclItems", "nestedPagePath"],
    modules: [
      moduleFile({
        module: "Pkg.One",
        declarations: [decl({ name: "Pkg.One.a", line: 1, index: 0 })],
      }),
      moduleFile({
        module: "Pkg.Two",
        declarations: [
          decl({
            name: "Pkg.Two.b",
            line: 1,
            index: 0,
            doc: "See `Pkg.One.a`.",
            refs: [["Wrong.Module", "Pkg.One.a"]],
          }),
        ],
      }),
    ],
  },
  {
    what: "curated: a module name that only the .lidx knows",
    // `knownModules` is the union of three sources; this one is reachable only
    // through the `.lidx`'s `@` section (plan §5, pitfall 6). The second link
    // comes from the `\t` section, so both halves of the file are load-bearing.
    branches: ["pageDeclItems", "linkIndexUsed", "nestedPagePath"],
    lidx: "#lidx1\n@Dep.Only.Module\nDep.M\n\tDep.M.thing\n",
    modules: [
      moduleFile({
        module: "Pkg.One",
        declarations: [
          decl({
            name: "Pkg.One.a",
            doc: "See `Dep.Only.Module` and `Dep.M.thing`.",
          }),
        ],
      }),
    ],
  },
  {
    what: "curated: the same corpus without a link index",
    branches: ["pageDeclItems", "linkIndexAbsent", "nestedPagePath"],
    linkIndex: false,
    lidx: "#lidx1\n@Dep.Only.Module\nDep.M\n\tDep.M.thing\n",
    modules: [
      moduleFile({
        module: "Pkg.One",
        declarations: [
          decl({
            name: "Pkg.One.a",
            doc: "See `Dep.Only.Module` and `Dep.M.thing`.",
          }),
        ],
      }),
    ],
  },
  {
    what: "curated: the suffix scan reads declaration-range order, not IR order",
    // `nameToLink?`'s last branch walks the module's declarations in
    // declaration-range order with the private ones removed
    // (`render.ts:1912-1916`). Here the IR order, the range order and the name
    // order all disagree, and a bare `f` resolves to whichever comes first.
    branches: ["pageDeclItems", "privateNameSkipped", "nestedPagePath"],
    modules: [
      moduleFile({
        module: "Pkg.M",
        declarations: [
          decl({ name: "Pkg.M.X.f", line: 10, index: 0, doc: "See `f`." }),
          decl({ name: "Pkg.M.Y.f", line: 5, index: 1 }),
          decl({ name: "_private.0.Pkg.M.f", line: 1, index: 2 }),
        ],
      }),
    ],
  },
  {
    what: "curated: a dependency slice supplies the module a name is documented in",
    branches: ["pageDeclItems", "depMapEntries", "nestedPagePath"],
    deps: [{ package: "Dep", declarations: { "Dep.thing": "Dep.Home" } }],
    modules: [
      moduleFile({
        module: "Pkg.One",
        declarations: [decl({ name: "Pkg.One.a", doc: "See `Dep.thing`." })],
      }),
    ],
  },
];

// ------------------------------------------------------------- running render.ts

/**
 * Writes one case's IR tree and returns its directory and the exact file
 * contents, so that the Rust side can rebuild the same tree byte for byte
 * rather than re-deriving it from a description of it.
 */
async function writeIr(root: string, c: Case): Promise<[string, Record<string, string>]> {
  const ir = `${root}/ir`;
  const files: Record<string, string> = {};
  await Deno.mkdir(`${ir}/modules`, { recursive: true });
  await Deno.mkdir(`${ir}/deps`, { recursive: true });
  const modules = [];
  for (const m of c.modules) {
    const name = m.module as string;
    const file = `modules/${name}.json`;
    const text = JSON.stringify(m);
    await Deno.writeTextFile(`${ir}/${file}`, text);
    files[file] = text;
    modules.push({
      bytes: new TextEncoder().encode(text).length,
      contentHash: "0".repeat(16),
      declarations: (m.declarations as Json[]).length,
      file,
      module: name,
    });
  }
  const dependencyMaps = [];
  for (const d of c.deps ?? []) {
    const file = `deps/${d.package}.json`;
    const text = JSON.stringify({
      schemaVersion: 4,
      package: d.package,
      declarations: d.declarations,
    });
    await Deno.writeTextFile(`${ir}/${file}`, text);
    files[file] = text;
    dependencyMaps.push({
      bytes: new TextEncoder().encode(text).length,
      entries: Object.keys(d.declarations).length,
      file,
      package: d.package,
    });
  }
  const index = JSON.stringify({
    declarationCount: modules.reduce((n, m) => n + m.declarations, 0),
    dependencyMaps,
    generator: "gen-pages-expected.ts",
    hashAlgorithm: "lean-string-hash-64/hex16",
    leanVersion: "4.31.0",
    moduleCount: modules.length,
    modules,
    schemaVersion: 4,
  });
  await Deno.writeTextFile(`${ir}/index.json`, index);
  files["index.json"] = index;
  return [ir, files];
}

/**
 * Every `.html` under `dir`, keyed by its path relative to it.
 *
 * A missing directory is an empty tree, not an error: a run that renders
 * nothing creates nothing, which is itself one of the cases.
 */
async function readTree(dir: string): Promise<Record<string, string>> {
  const out: Record<string, string> = {};
  const walk = async (at: string, prefix: string) => {
    try {
      await Deno.stat(at);
    } catch {
      return;
    }
    for await (const entry of Deno.readDir(at)) {
      if (entry.isDirectory) await walk(`${at}/${entry.name}`, `${prefix}${entry.name}/`);
      else if (entry.name.endsWith(".html")) {
        out[`${prefix}${entry.name}`] = await Deno.readTextFile(`${at}/${entry.name}`);
      }
    }
  };
  await walk(dir, "");
  return Object.fromEntries(Object.entries(out).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)));
}

const root = await Deno.makeTempDir({ prefix: "lean-doc-pages-oracle-" });
const out = [];
for (const [i, c] of cases.entries()) {
  const dir = `${root}/case${i}`;
  await Deno.mkdir(dir, { recursive: true });
  const [ir, files] = await writeIr(dir, c);
  const lidx = `${dir}/link-index.lidx`;
  await Deno.writeTextFile(lidx, c.lidx ?? "#lidx1\n");
  const pages = `${dir}/pages`;
  const args = [
    "run",
    "--allow-read",
    "--allow-write",
    RENDER_TS.pathname,
    "--ir",
    ir,
    "--pages",
    pages,
    "--source-url",
    SOURCE_URL,
  ];
  if (c.linkIndex !== false) args.push("--link-index", lidx);
  for (const m of c.only ?? []) args.push("--only", m);
  const run = await new Deno.Command(Deno.execPath(), { args, stdout: "piped", stderr: "piped" })
    .output();
  if (!run.success) {
    console.error(new TextDecoder().decode(run.stderr));
    console.error(`case ${i} (${c.what}) failed`);
    Deno.exit(3);
  }
  out.push({
    what: c.what,
    branches: c.branches,
    note: c.note,
    /** The IR tree as bytes, keyed by path under the tree root. */
    ir: Object.fromEntries(
      Object.entries(files).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)),
    ),
    lidx: c.lidx ?? "#lidx1\n",
    linkIndex: c.linkIndex !== false,
    only: c.only ?? null,
    pages: await readTree(pages),
  });
}
await Deno.remove(root, { recursive: true });

// ------------------------------------------------- the real corpus's branch profile

// Counted over one full run of the target package: no `--only`, with a
// `.lidx`. Every number is an event of that run, so they can be compared with
// each other.
const index = JSON.parse(await Deno.readTextFile(`${irDir}/index.json`));
const irModules: Json[] = [];
for (const entry of index.modules) {
  irModules.push(JSON.parse(await Deno.readTextFile(`${irDir}/${entry.file}`)));
}
const suppressed = new Set<string>();
for (const m of irModules) {
  for (const d of m.declarations as Json[]) {
    for (const mem of d.members as Json[]) suppressed.add(mem.name as string);
  }
}

const branchTotals: Record<string, number> = {
  moduleDocItems: 0,
  pageDeclItems: 0,
  suppressedSkipped: 0,
  moduleWithoutModuleDocs: 0,
  moduleWithoutPageEntries: 0,
  moduleWithoutImports: 0,
  emptyPage: 0,
  docAfterFirstDecl: 0,
  tieDocVsDecl: 0,
  tieDeclVsDecl: 0,
  topLevelPagePath: 0,
  nestedPagePath: 0,
  moduleNameNeedsEscaping: 0,
  privateNameSkipped: 0,
  depMapEntries: index.dependencyMaps.length,
  linkIndexUsed: 1,
  linkIndexAbsent: 0,
  onlyAll: 0,
  onlyThese: 0,
  onlySkipped: 0,
  emptyOnly: 0,
};

for (const m of irModules) {
  const name = m.module as string;
  const docs = (m.moduleDocs ?? []) as { line: number; col: number }[];
  const decls = (m.declarations as Json[]).filter((d) => !suppressed.has(d.name as string));
  branchTotals.onlyAll++;
  branchTotals.moduleDocItems += docs.length;
  branchTotals.pageDeclItems += decls.length;
  branchTotals.suppressedSkipped += (m.declarations as Json[]).length - decls.length;
  branchTotals.privateNameSkipped +=
    (m.declarations as Json[]).filter((d) => (d.name as string).startsWith("_private.")).length;
  if (docs.length === 0) branchTotals.moduleWithoutModuleDocs++;
  if (decls.length === 0) branchTotals.moduleWithoutPageEntries++;
  if ((m.imports as string[]).length === 0) branchTotals.moduleWithoutImports++;
  if (docs.length === 0 && decls.length === 0) branchTotals.emptyPage++;
  if (name.includes(".")) branchTotals.nestedPagePath++;
  else branchTotals.topLevelPagePath++;
  if (/[<>&"]/.test(name)) branchTotals.moduleNameNeedsEscaping++;

  // The page order, built exactly as `pageHtml` builds it, so the two
  // tie-breaker counters mean what they say.
  type Item = { line: number; col: number; seq: number; doc: boolean };
  const items: Item[] = [];
  let seq = 0;
  for (const d of docs) items.push({ line: d.line, col: d.col, seq: seq++, doc: true });
  for (const d of decls) {
    items.push({ line: d.line as number, col: d.col as number, seq: seq + (d.index as number), doc: false });
  }
  items.sort((a, b) => a.line - b.line || a.col - b.col || a.seq - b.seq);
  let seenDecl = false;
  for (const [i, it] of items.entries()) {
    if (it.doc && seenDecl) branchTotals.docAfterFirstDecl++;
    if (!it.doc) seenDecl = true;
    if (i === 0) continue;
    const prev = items[i - 1];
    if (prev.line === it.line && prev.col === it.col) {
      if (prev.doc !== it.doc) branchTotals.tieDocVsDecl++;
      else if (!it.doc) branchTotals.tieDeclVsDecl++;
    }
  }
}

const neverFires = Object.entries(branchTotals).filter(([, n]) => n === 0).map(([k]) => k).sort();
const curatedBranches: Record<string, number> = {};
for (const c of cases) for (const b of c.branches) curatedBranches[b] = (curatedBranches[b] ?? 0) + 1;

const uncovered = neverFires.filter((b) => !curatedBranches[b]);
if (uncovered.length > 0) {
  console.error(`branches that fire nowhere at all: ${uncovered.join(" ")}`);
  Deno.exit(7);
}
const unknown = Object.keys(curatedBranches).filter((b) => !(b in branchTotals));
if (unknown.length > 0) {
  console.error(`curated cases claim branches that are not counted: ${unknown.join(" ")}`);
  Deno.exit(7);
}

const fixture = JSON.stringify({
  generatedBy: "crates/lean-doc-render/tests/oracle/gen-pages-expected.ts",
  oracle:
    "experiments/stage7d/render.ts run as a program (pageHtml + the main loop) over synthetic IR trees",
  sourceUrl: SOURCE_URL,
  ir: irDir,
  irModules: irModules.length,
  irDeclarations: irModules.reduce((n, m) => n + (m.declarations as Json[]).length, 0),
  pageDeclarations: branchTotals.pageDeclItems,
  suppressedDeclarations: suppressed.size,
  branchTotals,
  curatedBranches,
  neverFires,
  deno: Deno.version.deno,
  cases: out,
}) + "\n";

if (check) {
  const committed = await Deno.readTextFile(FIXTURE);
  if (committed !== fixture) {
    console.error(`${FIXTURE.pathname} is not what this script produces`);
    Deno.exit(1);
  }
  console.error(`${FIXTURE.pathname} is current`);
} else {
  await Deno.writeTextFile(FIXTURE, fixture);
  const pages = out.reduce((n, c) => n + Object.keys(c.pages).length, 0);
  console.error(
    `${out.length} cases / ${pages} pages -> ${FIXTURE.pathname} ` +
      `(${new TextEncoder().encode(fixture).length} B)`,
  );
  console.error(`branch totals over ${irModules.length} real modules:`);
  for (const [k, n] of Object.entries(branchTotals)) console.error(`  ${k} ${n}`);
  console.error(`never fires: ${neverFires.join(" ")}`);
}
