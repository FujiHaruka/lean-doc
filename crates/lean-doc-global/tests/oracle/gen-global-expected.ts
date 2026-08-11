#!/usr/bin/env -S deno run --allow-read --allow-write --allow-run
// gen-global-expected.ts -- produce everything `tests/global.rs` compares the
// Rust port against, from the frozen prototype rather than from a reading of it.
//
// TWO ORACLES, ONE FIXTURE
// ------------------------
// The six artifacts and the per-module facts need different oracles, and the
// coverage invariant needs both in one place:
//
//   cases      `experiments/stage7h/global.ts build` run **as a program** over
//              small synthetic IR trees, recording the six files it writes.
//              This is what "byte identical with the prototype" means for M2-a.
//
//   factCases  `factsOf` / `autolinkTokens` / `headConst` **sliced out** of the
//              same file by line range and called directly. `ModuleFacts.tokens`
//              reaches no artifact -- it is the delta's input, M2-b's business --
//              so the program oracle above cannot see it at all, and a port that
//              dropped tokens entirely would pass every byte comparison.
//
// Both feed one branch profile, so `the_curated_cases_cover_what_the_package_
// does_not` can assert that every branch the target package never reaches is
// reached by a curated case *somewhere*.
//
// WHY A SUBPROCESS FOR THE ARTIFACTS
// ----------------------------------
// Same reason as `gen-pages-expected.ts`: what M2-a ports is the prototype's top
// level -- the order the maps are built in, which sorts happen where, which
// collisions resolve which way. Slicing that would mean re-implementing it in
// order to test it.
//
// npm/node are broken in this environment; this must run under deno.
//
// usage:
//   deno run --allow-read --allow-write --allow-run \
//     crates/lean-doc-global/tests/oracle/gen-global-expected.ts
//   ... --check      fail if the committed fixture is stale
//   ... --ir DIR     take the corpus branch profile from another IR tree

const FIXTURE = new URL("../data/global-expected.json", import.meta.url);
const GLOBAL_TS = new URL("../../../../experiments/stage7h/global.ts", import.meta.url);

const DEFAULT_IR = "/private/tmp/lean-doc-relay/w7h/base-ir";

const argv = Deno.args.slice();
const opt = (name: string, dflt = "") => {
  const i = argv.indexOf(name);
  return i >= 0 ? argv[i + 1] : dflt;
};
const check = argv.includes("--check");
const irDir = opt("--ir", DEFAULT_IR);

// ------------------------------------------------------ slicing global.ts

const globalLines = (await Deno.readTextFile(GLOBAL_TS)).split("\n");

/** Lines `from..to` of global.ts, 1-based and inclusive. */
const slice = (from: number, to: number) => globalLines.slice(from - 1, to).join("\n");

/**
 * Line ranges in global.ts. `head` is text the first line must start with, so
 * that a range which has drifted onto other code fails loudly instead of
 * quietly producing a different expectation.
 */
const RANGES = {
  declType: { from: 65, to: 71, head: "interface Decl {" },
  moduleFileType: { from: 72, to: 78, head: "interface ModuleFile {" },
  headConst: { from: 97, to: 106, head: "function headConst(" },
  autolinkTokens: { from: 111, to: 125, head: "function autolinkTokens(" },
  moduleFactsType: { from: 136, to: 144, head: "interface ModuleFacts {" },
  factsOf: { from: 148, to: 170, head: "function factsOf(" },
} as const;

for (const [what, { from, to, head }] of Object.entries(RANGES)) {
  const text = slice(from, to);
  if (!text.startsWith(head)) {
    console.error(
      `global.ts:${from}-${to} no longer starts with ${JSON.stringify(head)} (${what}).\n` +
        `Found:\n${text.split("\n")[0]}`,
    );
    Deno.exit(3);
  }
}

const at = (k: keyof typeof RANGES) => slice(RANGES[k].from, RANGES[k].to);

const MODULE_SOURCE = [
  at("declType"),
  at("moduleFileType"),
  at("moduleFactsType"),
  at("headConst"),
  at("autolinkTokens"),
  at("factsOf"),
  "export { autolinkTokens, factsOf, headConst };",
  "export type { Decl, ModuleFacts, ModuleFile };",
].join("\n\n");

const prototype = await import(
  `data:text/typescript;charset=utf-8,${encodeURIComponent(MODULE_SOURCE)}`
) as {
  headConst(d: Json): string | null;
  autolinkTokens(doc: string): string[];
  factsOf(m: Json, contentHash: string): Facts;
};

// deno-lint-ignore no-explicit-any
type Json = any;

type Facts = {
  module: string;
  contentHash: string;
  imports: string[];
  tactics: number;
  decls: [string, string][];
  instances: [string, string][];
  tokens: string[];
};

// --------------------------------------------------------------- IR builders

/** A declaration with every schema-4 key `lean_doc_ir` requires. */
function decl(o: {
  name: string;
  kind?: string;
  doc?: string | null;
  type?: string;
  typeCode?: (number | string)[][];
}): Json {
  return {
    name: o.name,
    kind: o.kind ?? "theorem",
    modifiers: [],
    binders: [],
    implicits: [],
    binderCode: [],
    type: o.type ?? "Prop",
    typeCode: o.typeCode ?? [],
    line: 1,
    col: 0,
    endLine: 1,
    endCol: 1,
    index: 0,
    members: [],
    doc: o.doc ?? null,
    equations: [],
    equationCode: [],
    refs: [],
  };
}

function tactic(name: string, doc: string): Json {
  return { internalName: name, userName: name, tags: [], docString: doc };
}

function moduleFile(o: {
  module: string;
  imports?: string[];
  moduleDocs?: { line: number; col: number; text: string }[];
  tactics?: Json[];
  declarations?: Json[];
}): Json {
  return {
    schemaVersion: 4,
    module: o.module,
    imports: o.imports ?? [],
    moduleDocs: o.moduleDocs ?? [],
    tactics: o.tactics ?? [],
    declarations: o.declarations ?? [],
  };
}

type DepSlice = { package: string; declarations: Record<string, string> };

// ------------------------------------------------------------ the corners

/**
 * U+1D49C MATHEMATICAL SCRIPT CAPITAL A and U+FB00 LATIN SMALL LIGATURE FF.
 *
 * `𝒜` is a surrogate pair (D835 DC9C), so it sorts **below** `ﬀ` in UTF-16 and
 * **above** it by code point. This pair is the whole of U1: the target package
 * has no name above the BMP 【実測: 0 of 4,750 declarations, 0 of 533 dependency
 * names, 0 of 432 modules】, so nothing derived from the corpus can tell
 * `cmp_utf16` from `str::cmp`.
 */
const ASTRAL = "\u{1D49C}";
const LIGATURE = "\u{FB00}";

type Case = {
  what: string;
  modules: Json[];
  deps?: DepSlice[];
  note?: string;
};

const cases: Case[] = [
  {
    what: "curated: names above the BMP, in every artifact that sorts",
    // Seven argument-less `.sort()` calls decide these bytes (plan §7 U1):
    // declaration names, instance classes, each class's instance list, the
    // module list twice (navbar and `modules`), each `importedBy` list, and the
    // declaration+dependency merge in `name-map.json`. Every one of them gets
    // an astral/BMP pair here, so a UTF-8 sort anywhere shows up.
    modules: [
      moduleFile({
        module: `Pkg.${ASTRAL}`,
        imports: ["Pkg.Base"],
        declarations: [
          decl({ name: `Pkg.${ASTRAL}.${ASTRAL}` }),
          decl({ name: `Pkg.${ASTRAL}.${LIGATURE}` }),
          decl({
            name: `Pkg.${ASTRAL}.inst${ASTRAL}`,
            kind: "instance",
            type: `Cls.${ASTRAL} Nat`,
            typeCode: [[0, 5, 1, `Cls.${ASTRAL}`]],
          }),
          decl({
            name: `Pkg.${ASTRAL}.inst${LIGATURE}`,
            kind: "instance",
            type: `Cls.${ASTRAL} Int`,
            typeCode: [[0, 5, 1, `Cls.${ASTRAL}`]],
          }),
        ],
      }),
      moduleFile({
        module: `Pkg.${LIGATURE}`,
        imports: ["Pkg.Base"],
        declarations: [
          decl({
            name: `Pkg.${LIGATURE}.inst`,
            kind: "instance",
            type: `Cls.${LIGATURE} Nat`,
            typeCode: [[0, 5, 1, `Cls.${LIGATURE}`]],
          }),
        ],
      }),
      moduleFile({ module: "Pkg.Base", declarations: [decl({ name: "Pkg.Base.a" })] }),
    ],
    deps: [
      {
        package: "Dep",
        declarations: { [`Dep.${ASTRAL}`]: "Dep.Home", [`Dep.${LIGATURE}`]: "Dep.Home" },
      },
    ],
  },
  {
    what: "curated: a package that declares tactics",
    // `tactics.html` embeds two counts. The target package's first is always
    // zero 【実測: 0 across all 432 modules】, so the sentence never distinguishes
    // "sum the per-module counts" from "print a literal 0".
    modules: [
      moduleFile({
        module: "Pkg.One",
        imports: ["Pkg.Two"],
        tactics: [tactic("Pkg.tacA", "does a"), tactic("Pkg.tacB", "does b")],
        declarations: [decl({ name: "Pkg.One.a" })],
      }),
      moduleFile({
        module: "Pkg.Two",
        imports: ["Pkg.One"],
        tactics: [tactic("Pkg.tacC", "does c")],
      }),
      moduleFile({ module: "Pkg.Three", imports: ["Foreign.Module"] }),
    ],
  },
  {
    what: "curated: instances whose head constant is not the obvious one",
    // Four shapes the corpus has none of: no constant span at all, a constant
    // span that is not the first element but starts earliest, a constant named
    // by the empty string (falsy, so the instance is dropped rather than
    // recorded under ""), and two instances of one class.
    modules: [
      moduleFile({
        module: "Pkg.Inst",
        imports: [],
        declarations: [
          decl({
            name: "Pkg.Inst.noConst",
            kind: "instance",
            type: "Type u",
            typeCode: [[0, 4, 2]],
          }),
          decl({
            name: "Pkg.Inst.reordered",
            kind: "instance",
            type: "Cls.B (Cls.A n)",
            typeCode: [[6, 11, 1, "Cls.A"], [0, 5, 1, "Cls.B"]],
          }),
          decl({
            // A name on a span that is not a constant. The extractor never
            // writes one, so only this case says the kind test does anything.
            name: "Pkg.Inst.namedSort",
            kind: "instance",
            type: "Type (Cls.C n)",
            typeCode: [[0, 4, 2, "Not.A.Constant"], [5, 10, 1, "Cls.C"]],
          }),
          decl({
            name: "Pkg.Inst.emptyName",
            kind: "instance",
            type: "whatever",
            typeCode: [[0, 3, 1, ""]],
          }),
          decl({
            name: "Pkg.Inst.alsoB",
            kind: "instance",
            type: "Cls.B Int",
            typeCode: [[0, 5, 1, "Cls.B"]],
          }),
          decl({
            name: "Pkg.Inst.notAnInstance",
            kind: "def",
            type: "Cls.B Int",
            typeCode: [[0, 5, 1, "Cls.B"]],
          }),
        ],
      }),
    ],
  },
  {
    what: "curated: two modules declare the same name",
    // `nameMap.set` is last-write-wins over the index order, so the second
    // module's page is the one every artifact points at. The corpus has no
    // duplicate at all 【実測: 4,750 declarations, 4,750 distinct names】.
    modules: [
      moduleFile({
        module: "Pkg.First",
        declarations: [decl({ name: "Pkg.shared", kind: "theorem" })],
      }),
      moduleFile({
        module: "Pkg.Second",
        declarations: [decl({ name: "Pkg.shared", kind: "def" })],
      }),
    ],
  },
  {
    what: "curated: a module name that needs HTML escaping",
    // The name goes into `navbar.html` twice: once as the `href`, where the
    // path rule has already turned dots into slashes, and once as text. The
    // apostrophe is there because `Html.escape` leaves it alone and every
    // general-purpose HTML escaper does not (plan §7).
    modules: [
      moduleFile({
        module: "Pkg.A<B&C\"D'E",
        declarations: [decl({ name: "Pkg.A<B&C\"D'E.f" })],
      }),
      moduleFile({ module: "Pkg", imports: ["Pkg.A<B&C\"D'E"] }),
    ],
  },
  {
    what: "curated: a dependency name that a module also declares",
    // `flat[n] = nameMap.get(n)?.module ?? deps[n]`: the declaration wins, and
    // the merged array holds the name twice without the object doing so.
    modules: [
      moduleFile({
        module: "Pkg.One",
        declarations: [decl({ name: "Shared.name" }), decl({ name: "Pkg.One.a" })],
      }),
    ],
    deps: [
      { package: "Dep", declarations: { "Shared.name": "Dep.Wrong", "Dep.only": "Dep.Home" } },
      { package: "Dep2", declarations: { "Dep.only": "Dep2.Wins" } },
    ],
  },
  {
    what: "curated: an IR with no modules",
    // Every artifact degenerates: two empty JSON objects, an empty `<ul>`, and
    // "0 tactic docstrings across 0 modules".
    modules: [],
    deps: [{ package: "Dep", declarations: { "Dep.thing": "Dep.Home" } }],
  },
  {
    what: "curated: an IR with no dependency slices and one top level module",
    modules: [
      moduleFile({ module: "Pkg", imports: [], declarations: [decl({ name: "Pkg.a" })] }),
      moduleFile({ module: "Pkg.Deep.Down.Here", imports: ["Pkg"] }),
    ],
  },
];

// ------------------------------------------------------------- fact cases

/**
 * Cases for the sliced `factsOf`. These exist for `tokens`, which no artifact
 * carries; the shapes below are all invisible to a byte comparison of the six
 * files.
 */
const factCases: { what: string; module: Json; contentHash: string; note?: string }[] = [
  {
    what: "curated: a module docstring is never tokenised",
    // `global.ts:152` reads `md.doc`; the extractor writes `line`/`col`/`text`
    // (`Extract.lean:2004-2006`). The loop body has never run. This case has a
    // module docstring stuffed with things a fixed version would harvest, and
    // the expected token list is the declaration's alone.
    note: "the module docstring's `Mod.doc.name` and [x](Mod.link) contribute nothing",
    contentHash: "0f0f0f0f0f0f0f0f",
    module: moduleFile({
      module: "Pkg.Docs",
      moduleDocs: [
        { line: 1, col: 0, text: "See `Mod.doc.name` and [x](Mod.link)." },
        { line: 9, col: 0, text: "`Another.one`" },
      ],
      declarations: [decl({ name: "Pkg.Docs.a", doc: "See `Decl.only`." })],
    }),
  },
  {
    what: "curated: what a docstring offers the delta",
    // Whitespace-separated parts of code spans, link targets, and the last
    // component of anything with a dot -- including the empty last component of
    // a part that ends in one, which `push` emits unconditionally.
    contentHash: "1111111111111111",
    module: moduleFile({
      module: "Pkg.Tokens",
      declarations: [
        decl({
          name: "Pkg.Tokens.a",
          doc: [
            "`Nat.succ n` and `Nat.add`.",
            "A trailing dot: `Foo.`",
            "A link [text](Bar.baz) and a broken one [t](a b) and [t]().",
            "Not a span: `across\na newline`, nor an empty one ``.",
            "Non-breaking\u{a0}space and a line\u{2028}separator inside `x\u{a0}y`.",
          ].join("\n"),
        }),
        decl({ name: "Pkg.Tokens.b", doc: "" }),
        decl({ name: "Pkg.Tokens.c" }),
      ],
    }),
  },
  {
    what: "curated: tokens are deduplicated and sorted above the BMP",
    // The set is emptied into an array and sorted with the same argument-less
    // `.sort()` as the artifacts use.
    contentHash: "2222222222222222",
    module: moduleFile({
      module: "Pkg.Sorted",
      declarations: [
        decl({ name: "Pkg.Sorted.a", doc: `\`z ${ASTRAL} ${LIGATURE} z\`` }),
        decl({ name: "Pkg.Sorted.b", doc: `\`${ASTRAL}\`` }),
      ],
    }),
  },
  {
    what: "curated: instances, tactics and imports as facts",
    contentHash: "3333333333333333",
    module: moduleFile({
      module: "Pkg.Facts",
      imports: ["Pkg.One", "Foreign.Module"],
      tactics: [tactic("Pkg.tac", "a tactic")],
      declarations: [
        decl({
          name: "Pkg.Facts.reordered",
          kind: "instance",
          type: "Cls.B (Cls.A n)",
          typeCode: [[6, 11, 1, "Cls.A"], [0, 5, 1, "Cls.B"]],
        }),
        decl({ name: "Pkg.Facts.bare", kind: "instance", type: "Type", typeCode: [] }),
      ],
    }),
  },
];

// ------------------------------------------------------------ the profile

const COUNTERS = [
  // the facts
  "moduleFacts",
  "moduleWithTactics",
  "tacticDocs",
  "moduleWithoutImports",
  "importOwn",
  "importForeign",
  "moduleDocDropped",
  "moduleDocWithCodeSpan",
  "declFacts",
  "declWithDoc",
  "declWithoutDoc",
  "declWithEmptyDoc",
  "docWithCodeSpan",
  "docWithLinkTarget",
  "tokensTotal",
  "tokenWithDot",
  "tokenEmptyString",
  "instanceDecl",
  "instanceWithHeadConst",
  "instanceWithoutHeadConst",
  "instanceHeadNotFirstSpan",
  "namedNonConstSpan",
  // the derivation
  "nameMapOverwrite",
  "instanceClassFirstSeen",
  "instanceClassAgain",
  "moduleImportedByNone",
  "depMapFiles",
  "depMapEntries",
  "depNameAlsoDeclared",
  "depNameOnly",
  "topLevelPagePath",
  "nestedPagePath",
  "moduleNameNeedsEscaping",
  "nameAboveBmp",
  "moduleNameAboveBmp",
  "runWithoutModules",
  "runWithoutDepMaps",
] as const;

type Counters = Record<(typeof COUNTERS)[number], number>;

const ABOVE_BMP = /[\u{10000}-\u{10FFFF}]/u;
const CODE_SPAN = /`[^`\n]+`/;
const LINK_TARGET = /\]\([^)\s]+\)/;

/**
 * How often each branch fires over one run.
 *
 * Every counter is a property of the *inputs* or of what the sliced prototype
 * returns for them -- never a re-derivation of the branch structure, which
 * would be a second definition of the thing under test (plan §7).
 */
function profile(modules: Json[], deps: DepSlice[]): Counters {
  const b = Object.fromEntries(COUNTERS.map((k) => [k, 0])) as Counters;
  b.runWithoutModules = modules.length === 0 ? 1 : 0;
  b.runWithoutDepMaps = deps.length === 0 ? 1 : 0;
  b.depMapFiles = deps.length;

  const own = new Set<string>(modules.map((m) => m.module as string));
  const declared = new Set<string>();
  const classes = new Set<string>();
  const importedBy = new Map<string, number>();
  for (const m of own) importedBy.set(m, 0);

  for (const m of modules) {
    b.moduleFacts++;
    const name = m.module as string;
    if (ABOVE_BMP.test(name)) b.moduleNameAboveBmp++;
    if (/[<>&"]/.test(name)) b.moduleNameNeedsEscaping++;
    if (name.includes(".")) b.nestedPagePath++;
    else b.topLevelPagePath++;

    const tactics = (m.tactics ?? []).length;
    b.tacticDocs += tactics;
    if (tactics > 0) b.moduleWithTactics++;

    const imports = m.imports as string[];
    if (imports.length === 0) b.moduleWithoutImports++;
    for (const i of imports) {
      if (own.has(i)) {
        b.importOwn++;
        importedBy.set(i, importedBy.get(i)! + 1);
      } else b.importForeign++;
    }

    for (const md of (m.moduleDocs ?? []) as { text: string }[]) {
      b.moduleDocDropped++;
      if (CODE_SPAN.test(md.text) || LINK_TARGET.test(md.text)) b.moduleDocWithCodeSpan++;
    }

    for (const d of m.declarations as Json[]) {
      b.declFacts++;
      if (ABOVE_BMP.test(d.name as string)) b.nameAboveBmp++;
      if (declared.has(d.name as string)) b.nameMapOverwrite++;
      declared.add(d.name as string);
      const doc = d.doc as string | null;
      if (doc) {
        b.declWithDoc++;
        if (CODE_SPAN.test(doc)) b.docWithCodeSpan++;
        if (LINK_TARGET.test(doc)) b.docWithLinkTarget++;
      } else {
        b.declWithoutDoc++;
        if (doc === "") b.declWithEmptyDoc++;
      }
      // `headConst` tests the kind *and* the fourth element. The extractor only
      // ever names kind-1 spans, so dropping the kind test is invisible unless
      // something puts a name on a span that is not a constant.
      for (const s of (d.typeCode ?? []) as (number | string)[][]) {
        if (s.length >= 4 && typeof s[3] === "string" && s[2] !== 1) b.namedNonConstSpan++;
      }
      if (d.kind === "instance") {
        b.instanceDecl++;
        const head = prototype.headConst(d);
        const firstConst = ((d.typeCode ?? []) as (number | string)[][])
          .find((s) => s.length >= 4 && s[2] === 1 && typeof s[3] === "string");
        if (head !== (firstConst?.[3] ?? null)) b.instanceHeadNotFirstSpan++;
        if (head) {
          b.instanceWithHeadConst++;
          if (classes.has(head)) b.instanceClassAgain++;
          else {
            b.instanceClassFirstSeen++;
            classes.add(head);
          }
        } else b.instanceWithoutHeadConst++;
      }
    }

    // The tokens, from the prototype's own `factsOf`.
    const facts = prototype.factsOf(m, "0".repeat(16));
    b.tokensTotal += facts.tokens.length;
    for (const t of facts.tokens) {
      if (t.includes(".")) b.tokenWithDot++;
      if (t === "") b.tokenEmptyString++;
    }
  }

  for (const [, n] of importedBy) if (n === 0) b.moduleImportedByNone++;

  const depNames = new Set<string>();
  for (const d of deps) for (const n of Object.keys(d.declarations)) depNames.add(n);
  b.depMapEntries = depNames.size;
  for (const n of depNames) {
    if (ABOVE_BMP.test(n)) b.nameAboveBmp++;
    if (declared.has(n)) b.depNameAlsoDeclared++;
    else b.depNameOnly++;
  }
  return b;
}

// -------------------------------------------------------- running global.ts

const ARTIFACTS = [
  "declarations/declaration-data.bmp",
  "declarations/name-map.json",
  "navbar.html",
  "tactics.html",
  "references.bib",
  "references.html",
];

/** Writes one case's IR tree and returns its root and its exact file bytes. */
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
    generator: "gen-global-expected.ts",
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

const root = await Deno.makeTempDir({ prefix: "lean-doc-global-oracle-" });
const out = [];
for (const [i, c] of cases.entries()) {
  const dir = `${root}/case${i}`;
  await Deno.mkdir(dir, { recursive: true });
  const [ir, files] = await writeIr(dir, c);
  const site = `${dir}/site`;
  const run = await new Deno.Command(Deno.execPath(), {
    args: [
      "run",
      "--allow-read",
      "--allow-write",
      GLOBAL_TS.pathname,
      "build",
      "--ir",
      ir,
      "--out",
      site,
    ],
    stdout: "piped",
    stderr: "piped",
  }).output();
  if (!run.success) {
    console.error(new TextDecoder().decode(run.stderr));
    console.error(`case ${i} (${c.what}) failed`);
    Deno.exit(3);
  }
  const artifacts: Record<string, string> = {};
  for (const a of ARTIFACTS) artifacts[a] = await Deno.readTextFile(`${site}/${a}`);
  out.push({
    what: c.what,
    note: c.note,
    branches: reached(profile(c.modules, c.deps ?? [])),
    ir: Object.fromEntries(Object.entries(files).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))),
    artifacts,
  });
}
await Deno.remove(root, { recursive: true });

const factsOut = factCases.map((c) => ({
  what: c.what,
  note: c.note,
  branches: reached(profile([c.module], [])),
  contentHash: c.contentHash,
  module: JSON.stringify(c.module),
  facts: prototype.factsOf(c.module, c.contentHash),
}));

function reached(counters: Counters): string[] {
  return COUNTERS.filter((k) => counters[k] > 0);
}

// ------------------------------------------- the real corpus's branch profile

const index = JSON.parse(await Deno.readTextFile(`${irDir}/index.json`));
const irModules: Json[] = [];
for (const entry of index.modules) {
  irModules.push(JSON.parse(await Deno.readTextFile(`${irDir}/${entry.file}`)));
}
const irDeps: DepSlice[] = [];
for (const entry of index.dependencyMaps ?? []) {
  const f = JSON.parse(await Deno.readTextFile(`${irDir}/${entry.file}`));
  irDeps.push({ package: f.package, declarations: f.declarations });
}
const branchTotals = profile(irModules, irDeps);

/**
 * A digest of `factsOf` over every module of the corpus, so that the port's
 * `tokens` are checked against the prototype on 3,394 real docstrings and not
 * only on the four cases above -- without committing ~800 KB of tokens.
 *
 * FNV-1a 64 rather than SHA-256 because both sides have to implement it and it
 * is ten lines; this is a staleness check, not a security property. The two
 * plain counts beside it are what a digest cannot tell you when it disagrees.
 */
function fnv1a64(bytes: Uint8Array): string {
  let h = 0xcbf2_9ce4_8422_2325n;
  // 2^40 + 2^8 + 0xb3. Grouped in fours from the right, as the Rust side is.
  const prime = 0x0000_0100_0000_01b3n;
  const mask = 0xffff_ffff_ffff_ffffn;
  for (const b of bytes) {
    h = (h ^ BigInt(b)) * prime & mask;
  }
  return h.toString(16).padStart(16, "0");
}

const corpusFactsText = index.modules
  .map((e: Json, i: number) => JSON.stringify(prototype.factsOf(irModules[i], e.contentHash)))
  .join("\n");
const corpusFactsBytes = new TextEncoder().encode(corpusFactsText);
const corpusFacts = {
  modules: irModules.length,
  bytes: corpusFactsBytes.length,
  tokens: branchTotals.tokensTotal,
  fnv1a64: fnv1a64(corpusFactsBytes),
  moduleDocsWithDocKey: irModules.reduce(
    (n, m) => n + ((m.moduleDocs ?? []) as Json[]).filter((md) => md.doc !== undefined).length,
    0,
  ),
};

// ------------------------------------------------------------- coverage

const neverFires = COUNTERS.filter((k) => branchTotals[k] === 0).sort();
const curatedBranches: Record<string, number> = {};
for (const c of [...out, ...factsOut]) {
  for (const b of c.branches) curatedBranches[b] = (curatedBranches[b] ?? 0) + 1;
}
const uncovered = neverFires.filter((b) => !curatedBranches[b]);
if (uncovered.length > 0) {
  console.error(`branches that fire nowhere at all: ${uncovered.join(" ")}`);
  Deno.exit(7);
}
const unknown = Object.keys(curatedBranches).filter((b) => !(COUNTERS as readonly string[]).includes(b));
if (unknown.length > 0) {
  console.error(`curated cases claim branches that are not counted: ${unknown.join(" ")}`);
  Deno.exit(7);
}

// ------------------------------- the artifacts the real corpus produces

/**
 * The six real artifacts are 1.9 MB together, which is not a fixture. Their
 * sizes and digests are, and `tools/global-reference.sh` regenerates the files
 * themselves whenever the corpus is on the machine.
 */
const refDir = opt("--reference", "/private/tmp/lean-doc-relay/m2/ref-global");
let reference: Record<string, { bytes: number; fnv1a64: string }> | null = null;
try {
  reference = {};
  for (const a of ARTIFACTS) {
    const bytes = await Deno.readFile(`${refDir}/${a}`);
    reference[a] = { bytes: bytes.length, fnv1a64: fnv1a64(bytes) };
  }
} catch {
  reference = null;
  console.error(`no reference tree at ${refDir}; the fixture will not pin the real artifacts`);
}

const fixture = JSON.stringify({
  generatedBy: "crates/lean-doc-global/tests/oracle/gen-global-expected.ts",
  oracle: "experiments/stage7h/global.ts: `build` run as a program for the artifacts, " +
    "and factsOf / autolinkTokens / headConst sliced out for the facts",
  globalTsRanges: RANGES,
  ir: irDir,
  irModules: irModules.length,
  irDeclarations: irModules.reduce((n, m) => n + (m.declarations as Json[]).length, 0),
  corpusFacts,
  reference,
  branchTotals,
  curatedBranches,
  neverFires,
  deno: Deno.version.deno,
  cases: out,
  factCases: factsOut,
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
  console.error(
    `${out.length} artifact cases / ${factsOut.length} fact cases -> ${FIXTURE.pathname} ` +
      `(${new TextEncoder().encode(fixture).length} B)`,
  );
  console.error(`branch totals over ${irModules.length} real modules:`);
  for (const k of COUNTERS) console.error(`  ${k} ${branchTotals[k]}`);
  console.error(`never fires: ${neverFires.join(" ")}`);
}
