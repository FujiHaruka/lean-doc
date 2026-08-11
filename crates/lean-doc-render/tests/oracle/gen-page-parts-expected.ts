#!/usr/bin/env -S deno run --allow-read --allow-write
// gen-page-parts-expected.ts -- render every declaration block and every page
// frame of the target package with the *prototype's* own functions, so that
// `tests/page_parts.rs` compares the Rust port against the prototype's bytes
// rather than against a reading of it.
//
// WHY THE PROTOTYPE IS THE ORACLE
// -------------------------------
// Same reason as M1-d1 (`gen-fragment-expected.ts`): doc-gen4's `docInfoToHtml`
// takes `DocInfo`s built from a live environment, and the IR carries a
// flattened, already-printed form of them. There is no input the two could
// both be handed. The prototype is the other consumer of that IR and it is the
// one scoring 439/439 (plan §1).
//
// HOW IT GETS AT render.ts WITHOUT IMPORTING IT
// ---------------------------------------------
// render.ts is a script whose top level parses argv and exits, and it is
// frozen, so the definitions are sliced out by line range, assembled into a
// module and imported through a `data:` URL. Each range declares what its first
// line must start with, so a range that has drifted onto other code is a hard
// failure rather than a silently different expectation.
//
// WHAT MAKES THE COMMITTED SAMPLE SELF-CONTAINED
// ----------------------------------------------
// A declaration block resolves against three maps totalling 8.5 MB. All three
// are handed to the prototype as tracing collections, so the exact set of names
// each declaration looks up is recorded, and the fixture carries the answers
// for those and nothing else. It also carries a *reduced module file*: the
// rendered declaration verbatim, plus name/range/index stubs for the module's
// other declarations, because `containedNames` and `moduleDeclNames` read
// those. Every reduced case is re-rendered and required to produce the same
// bytes, so a reduction that lost something fails here rather than weakening a
// test later.
//
// npm/node are broken in this environment; this must run under deno.
//
// usage:
//   deno run --allow-read --allow-write \
//     crates/lean-doc-render/tests/oracle/gen-page-parts-expected.ts
//   ... --full /tmp/page-parts-full.json   also write every header, every
//                                          declaration and every frame
//   ... --check                            fail if the committed file is stale
//   ... --ir DIR --link-index FILE --source-url URL   point at another corpus

const FIXTURE = new URL("../data/page-parts-expected.json", import.meta.url);
const RENDER_TS = new URL("../../../../experiments/stage7d/render.ts", import.meta.url);

const DEFAULT_IR = "/private/tmp/lean-doc-relay/w7h/base-ir";
const DEFAULT_LINK_INDEX = "/private/tmp/lean-doc-relay/w7c/linkindex/link-index.lidx";
/**
 * The same base `tools/render-reference.sh` uses.
 *
 * **40 hex digits, not a tag or a branch** (plan 決定 1): `coverage.ts`
 * normalises the revision away with `/blob/[0-9a-f]{40}/`, so anything else
 * silently lowers the acceptance score.
 */
const DEFAULT_SOURCE_URL =
  "https://github.com/FujiHaruka/information-theory/blob/573793b243fb1343636088eb62d1789ab2b14cec";

/** Roughly how many real declarations the committed fixture aims for. */
const DECL_TARGET = 110;
/** Roughly how many real module frames it aims for. */
const FRAME_TARGET = 24;
/**
 * Above this many characters a declaration is not taken for variety alone.
 *
 * The whole corpus is reachable with `--full`, so what is committed should be
 * the variety, not the volume. Coverage still overrides it.
 */
const COMPACT_CHARS = 1500;
/**
 * Above this many characters a module's nav is not committed.
 *
 * The package root imports all 432 modules and its nav alone is 59 KB. The
 * frame is a template with no data-dependent branching beyond the two lists,
 * so committing the biggest one buys nothing the curated frames do not.
 */
const COMPACT_NAV = 6000;

// ---------------------------------------------------- slicing render.ts

const renderLines = (await Deno.readTextFile(RENDER_TS)).split("\n");

/** Lines `from..to` of render.ts, 1-based and inclusive. */
const slice = (from: number, to: number) => renderLines.slice(from - 1, to).join("\n");

/**
 * Line ranges in render.ts. `head` is text the first line must start with.
 *
 * The first block is M1-b/M1-c/M1-d1 scaffolding that this step's functions
 * call; the second is the whole of M1-d2's scope.
 */
const RANGES = {
  escapeHtml: { from: 331, to: 341, head: "function escapeHtml(" },
  kindDescription: { from: 349, to: 380, head: "function kindDescription(" },
  links: { from: 385, to: 393, head: "function moduleLink(" },
  findLinkableParent: { from: 402, to: 413, head: "function findLinkableParent(" },
  privatePrefix: { from: 415, to: 415, head: "const PRIVATE_PREFIX =" },
  spanTree: { from: 419, to: 443, head: "type Node = {" },
  applyWsWidths: { from: 555, to: 593, head: "function applyWsWidths(" },
  renderer: { from: 597, to: 702, head: "type Rendered = {" },
  privateToUserName: { from: 705, to: 708, head: "function privateToUserName(" },
  moduleFromPrivatePrefix: { from: 711, to: 714, head: "function moduleFromPrivatePrefix(" },
  breakWithin: { from: 719, to: 724, head: "function breakWithin(" },
  leanQuote: { from: 785, to: 797, head: "function leanQuote(" },
  order: { from: 800, to: 825, head: "function stringLt(" },
  cssKind: { from: 832, to: 843, head: "function cssKind(" },
  docstrings: { from: 932, to: 1672, head: "function nameToLink(" },

  // --- M1-d2 itself ---
  declHeader: { from: 726, to: 777, head: "function declHeader(" },
  headHtml: { from: 846, to: 866, head: "function headHtml(" },
  pageHeaderHtml: { from: 873, to: 880, head: "function pageHeaderHtml(" },
  internalNavHtml: { from: 883, to: 921, head: "function internalNavHtml(" },
  declParts: { from: 1677, to: 1897, head: "function equationsHtml(" },

  // --- inputs lifted out of the main loop (M1-d3's scope, needed to call the above) ---
  pageCtxDeclNames: { from: 1912, to: 1916, head: "    moduleDeclNames: mod.declarations" },
  suppressed: { from: 2043, to: 2048, head: "const suppressed = new Set<string>();" },
  pageDecls: { from: 2097, to: 2100, head: "  const decls = mod.declarations" },
} as const;

for (const [what, { from, to, head }] of Object.entries(RANGES)) {
  const text = slice(from, to);
  if (!text.startsWith(head)) {
    console.error(
      `render.ts:${from}-${to} no longer starts with ${JSON.stringify(head)} (${what}).\n` +
        `Found:\n${text.split("\n")[0]}`,
    );
    Deno.exit(3);
  }
}

const at = (k: keyof typeof RANGES) => slice(RANGES[k].from, RANGES[k].to);

/** `pageHtml`'s `moduleDeclNames` expression (`render.ts:1912-1916`), lifted. */
const declNamesExpr = (() => {
  const text = at("pageCtxDeclNames").trim();
  const prefix = "moduleDeclNames: ";
  if (!text.startsWith(prefix) || !text.endsWith(",")) {
    console.error("render.ts:1912-1916 is no longer a `moduleDeclNames: <expr>,` entry");
    Deno.exit(3);
  }
  return text.slice(prefix.length, -1);
})();

/**
 * The module assembled out of those slices.
 *
 * `known`, `linkIndex` and the per-page `knownModules` are the three maps the
 * docstring path reads (`render.ts:2001-2085`), and `known` is also what the
 * code renderer resolves signatures against. They are tracing collections here,
 * which is what lets the committed fixture carry a reduced world that is
 * provably enough.
 *
 * `stats` and `pageStats` are the prototype's own counters. They reach no byte
 * -- the Rust port drops them (M1-d1's judgement) -- but they *are* a report of
 * which branch each call took, so they are read here as per-case deltas and
 * summed into `branchTotals`. That is how this generator can say which branches
 * the real corpus never reaches, instead of assuming it reaches all of them.
 */
const MODULE_SOURCE = [
  "// deno-lint-ignore-file no-explicit-any",
  "type Span = any[];",
  "type Decl = any; type Member = any; type ModuleFile = any; type ModuleDoc = any;",
  "export const traced = new Set<string>();",
  "let tracing = false;",
  "export const setTracing = (on: boolean) => { tracing = on; };",
  "class TracingMap extends Map<string, string> {",
  "  get(k: string) { if (tracing) traced.add(k); return super.get(k); }",
  "  has(k: string) { if (tracing) traced.add(k); return super.has(k); }",
  "}",
  "export class TracingSet extends Set<string> {",
  "  has(k: string) { if (tracing) traced.add(k); return super.has(k); }",
  "}",
  "export const known = new TracingMap();",
  "export const linkIndex = new TracingMap();",
  "const abl = (_name: string) => false;",
  "const T = { docstring: 0 };",
  "export const stats = {",
  "  anchorsBreakWithin: 0, implArgs: 0, extendsRendered: 0,",
  "  anchorsConst: 0, anchorsSort: 0, anchorsModuleFallback: 0,",
  "  constSpansSuppressedByNesting: 0, constSpansUnlinkable: 0,",
  "  constSpansViaParent: 0, constSpansPrivate: 0, constSpansNameNotInRefs: 0,",
  "  wsWidthChars: 0, wsWidthFragments: 0,",
  "};",
  "export const pageStats = {",
  "  navLinks: 0, importListItems: 0, importDuplicatesDropped: 0,",
  "  ghLinks: 0, docstringsRendered: 0, docstringChars: 0,",
  "  equationBlocks: 0, equationItems: 0, equationsOmittedNotices: 0,",
  "  equationsDroppedOverLimit: 0, memberTables: 0, memberFields: 0,",
  "  memberFieldArgs: 0, memberFieldDocs: 0, memberFieldsInherited: 0,",
  "  memberFieldsInheritedWithId: 0, attributeBlocks: 0, attributeItems: 0,",
  "  instancesForStubs: 0, classInstancesStubs: 0,",
  "  autolinkAttempts: 0, autolinkResolved: 0,",
  "  anchorsConst: 0, anchorsSort: 0, anchorsModuleFallback: 0,",
  "  constSpansSuppressedByNesting: 0, constSpansUnlinkable: 0,",
  "  constSpansViaParent: 0, constSpansPrivate: 0, constSpansNameNotInRefs: 0,",
  "  wsWidthChars: 0, wsWidthFragments: 0,",
  "};",
  "let sink: any = stats;",
  "export const sinkPageStats = () => { sink = pageStats; };",
  "export const sinkStats = () => { sink = stats; };",
  at("privatePrefix"),
  at("escapeHtml"),
  at("kindDescription"),
  at("links"),
  at("findLinkableParent"),
  at("spanTree"),
  at("applyWsWidths"),
  at("renderer"),
  at("privateToUserName"),
  at("moduleFromPrivatePrefix"),
  at("breakWithin"),
  at("declHeader"),
  at("leanQuote"),
  at("order"),
  at("cssKind"),
  at("headHtml"),
  at("pageHeaderHtml"),
  at("internalNavHtml"),
  at("docstrings"),
  at("declParts"),
  `export const moduleDeclNamesOf = (mod: any): string[] => (${declNamesExpr});`,
  [
    "export function suppressedOf(modules: any[]): Set<string> {",
    at("suppressed"),
    "  return suppressed;",
    "}",
  ].join("\n"),
  [
    "export function pageDeclsOf(mod: any, suppressed: Set<string>): any[] {",
    at("pageDecls"),
    "  return decls;",
    "}",
  ].join("\n"),
  "export { Renderer, declHeader, declHtml, headHtml, internalNavHtml, moduleLink,",
  "  pageHeaderHtml, pageRoot };",
].join("\n\n");

// deno-lint-ignore no-explicit-any
type Any = any;
type Sink = Record<string, number>;
type Ctx = { root: string; moduleDeclNames: string[]; knownModules: Set<string> };

const prototype = await import(
  `data:text/typescript;charset=utf-8,${encodeURIComponent(MODULE_SOURCE)}`
) as {
  Renderer: new (known: Map<string, string>) => Any;
  declHeader(d: Any, module: string, r: Any): string;
  declHtml(
    d: Any,
    mod: Any,
    root: string,
    moduleSourceUrl: string,
    header: string,
    ctx: Ctx,
    r: Any,
  ): string;
  headHtml(module: string, root: string): string;
  pageHeaderHtml(module: string, root: string): string;
  internalNavHtml(
    module: string,
    root: string,
    moduleSourceUrl: string,
    imports: string[],
    memberNames: string[],
  ): string;
  moduleLink(root: string, module: string): string;
  pageRoot(module: string): string;
  moduleDeclNamesOf(mod: Any): string[];
  suppressedOf(modules: Any[]): Set<string>;
  pageDeclsOf(mod: Any, suppressed: Set<string>): Any[];
  known: Map<string, string>;
  linkIndex: Map<string, string>;
  TracingSet: new (values?: Iterable<string>) => Set<string>;
  traced: Set<string>;
  setTracing(on: boolean): void;
  stats: Sink;
  pageStats: Sink;
  sinkPageStats(): void;
  sinkStats(): void;
};

// -------------------------------------------------------------------- inputs

const args = Deno.args;
const flag = (name: string, fallback: string | null = null) => {
  const i = args.indexOf(name);
  return i >= 0 && i + 1 < args.length ? args[i + 1] : fallback;
};
const ir = flag("--ir", DEFAULT_IR)!;
const linkIndexPath = flag("--link-index", DEFAULT_LINK_INDEX)!;
const sourceUrlBase = flag("--source-url", DEFAULT_SOURCE_URL)!.replace(/\/+$/, "");
const full = flag("--full");
const check = args.includes("--check");

const readJson = async (path: string): Promise<Any> => JSON.parse(await Deno.readTextFile(path));

const index = await readJson(`${ir}/index.json`);

/** `render.ts:2008-2036`: dependency slices, then declarations, then references. */
for (const dep of index.dependencyMaps) {
  const map = await readJson(`${ir}/${dep.file}`);
  for (const [n, m] of Object.entries(map.declarations as Record<string, string>)) {
    prototype.known.set(n, m);
  }
}
const irModules: Any[] = [];
for (const entry of index.modules) {
  const mod = await readJson(`${ir}/${entry.file}`);
  irModules.push(mod);
  for (const d of mod.declarations) {
    prototype.known.set(d.name, mod.module);
    for (const [m, n] of d.refs) if (!prototype.known.has(n)) prototype.known.set(n, m);
  }
}

const irModuleNames = new Set<string>(irModules.map((m) => m.module));
const fullKnownModules = new prototype.TracingSet(irModuleNames);
for (const m of prototype.known.values()) fullKnownModules.add(m);

/** `render.ts:2067-2084`, the `.lidx` reader, transcribed as it stands. */
const lidxModuleNames = new Set<string>();
{
  const text = await Deno.readTextFile(linkIndexPath);
  let module = "";
  let pos = 0;
  const n = text.length;
  while (pos < n) {
    let nl = text.indexOf("\n", pos);
    if (nl < 0) nl = n;
    const c = text.charCodeAt(pos);
    if (c === 9 /* tab */) prototype.linkIndex.set(text.slice(pos + 1, nl), module);
    else if (c === 64 /* @ */) {
      const name = text.slice(pos + 1, nl);
      fullKnownModules.add(name);
      lidxModuleNames.add(name);
    } else if (c === 35 /* # */) { /* format marker */ }
    else if (nl > pos) module = text.slice(pos, nl);
    pos = nl + 1;
  }
}

/**
 * `render.ts:2043-2048` -- **across every module**, not per module.
 *
 * Built with the prototype's own lines. Building it per module would leave the
 * projection functions and constructors of other modules on their pages (plan
 * §5).
 */
const suppressed = prototype.suppressedOf(irModules);

const renderer = new prototype.Renderer(prototype.known);

console.error(
  `known ${prototype.known.size} / linkIndex ${prototype.linkIndex.size} / ` +
    `knownModules ${fullKnownModules.size} / suppressed ${suppressed.size}`,
);

// ------------------------------------------------------------------ branches

/**
 * How often each branch of M1-d2's scope fires over the whole corpus.
 *
 * Two sources, marked per key below: `counter:` is one of the prototype's own
 * counters, read as a per-case delta; `derived:` is computed here from the same
 * input or output the prototype saw. **Neither is a reading of the Rust port.**
 *
 * This exists because M1-d1 found that three of its ten branches never fire on
 * the target package: "every real case matched" is not a statement about branch
 * coverage, and a branch with a zero here can only be defended by a curated
 * case. `tests/page_parts.rs` pins these numbers and requires the curated cases
 * to cover every zero.
 */
const COUNTER_BRANCHES = [
  // declHeader
  "anchorsBreakWithin",
  "implArgs",
  "extendsRendered",
  // declHtml
  "ghLinks",
  "docstringsRendered",
  "attributeBlocks",
  "attributeItems",
  // equationsHtml
  "equationBlocks",
  "equationItems",
  "equationsOmittedNotices",
  "equationsDroppedOverLimit",
  // structureHtml / fieldToHtml
  "memberTables",
  "memberFields",
  "memberFieldArgs",
  "memberFieldDocs",
  "memberFieldsInherited",
  "memberFieldsInheritedWithId",
  // the two stubs
  "instancesForStubs",
  "classInstancesStubs",
  // internalNavHtml
  "navLinks",
  "importListItems",
  "importDuplicatesDropped",
] as const;

const DERIVED_BRANCHES = [
  "headerExplicitArg",
  "headerNoBinders",
  "headerStructureWithoutParents",
  "declWithoutAttributes",
  "declWithoutDocstring",
  "kindStructure",
  "kindClass",
  "kindDefinition",
  "kindInstance",
  "kindInductive",
  "kindClassInductive",
  "kindOther",
  "equationsAbsent",
  "structureFieldsMk",
  "structureExt",
  "structureCtorMissing",
  "fieldDirect",
  "fieldImplicitArg",
  "navWithoutImports",
  "navWithoutMembers",
] as const;

const branchTotals: Sink = Object.fromEntries(
  [...COUNTER_BRANCHES, ...DERIVED_BRANCHES].map((k) => [k, 0]),
);

const zeroCounters = () => {
  for (const k of Object.keys(prototype.stats)) prototype.stats[k] = 0;
  for (const k of Object.keys(prototype.pageStats)) prototype.pageStats[k] = 0;
};

/** The prototype's counters, merged the way a page merges them. */
const counterDelta = (): Sink => {
  const out: Sink = {};
  for (const k of COUNTER_BRANCHES) {
    out[k] = (prototype.stats[k] ?? 0) + (prototype.pageStats[k] ?? 0);
  }
  return out;
};

const bump = (into: Sink, k: string, n = 1) => {
  into[k] = (into[k] ?? 0) + n;
};

// -------------------------------------------------------------------- corpus

type DeclCase = {
  what: string;
  module: string;
  root: string;
  sourceUrl: string;
  mod: Any;
  decl: Any;
  header: string;
  html: string;
  branches: Sink;
  traced: Set<string>;
};

type FrameCase = {
  module: string;
  root: string;
  sourceUrl: string;
  imports: string[];
  memberNames: string[];
  head: string;
  header: string;
  nav: string;
  branches: Sink;
};

const headers: { what: string; html: string }[] = [];
const declCases: DeclCase[] = [];
const frameCases: FrameCase[] = [];

/** Branches decided by the declaration's own shape rather than by a counter. */
function derivedDeclBranches(d: Any, header: string, html: string): Sink {
  const out: Sink = {};
  const explicit = d.binders.filter((_: string, i: number) => !d.implicits?.[i]).length;
  if (explicit > 0) bump(out, "headerExplicitArg", explicit);
  if (d.binders.length === 0) bump(out, "headerNoBinders");
  if (
    (d.kind === "structure" || d.kind === "class") &&
    !d.members.some((m: Any) => m.label === "parent")
  ) bump(out, "headerStructureWithoutParents");
  if (!d.attrs || d.attrs.length === 0) bump(out, "declWithoutAttributes");
  if (!d.doc) bump(out, "declWithoutDocstring");
  const kinds: Record<string, string> = {
    structure: "kindStructure",
    class: "kindClass",
    definition: "kindDefinition",
    instance: "kindInstance",
    inductive: "kindInductive",
    class_inductive: "kindClassInductive",
  };
  bump(out, kinds[d.kind] ?? "kindOther");
  if ((d.kind === "definition" || d.kind === "instance") && !html.includes("<ul class=\"equations\"")) {
    bump(out, "equationsAbsent");
  }
  if (d.kind === "structure" || d.kind === "class") {
    if (html.includes("<ul class=\"structure_ext\">")) bump(out, "structureExt");
    else bump(out, "structureFieldsMk");
    if (!d.members.some((m: Any) => m.label === "ctor")) bump(out, "structureCtorMissing");
    const fields = d.members.filter((m: Any) => m.label === "field");
    const direct = fields.filter((f: Any) => f.isDirect !== false).length;
    if (direct > 0) bump(out, "fieldDirect", direct);
    let implicitArgs = 0;
    for (const f of fields) {
      implicitArgs += (f.implicits ?? []).filter(Boolean).length;
    }
    if (implicitArgs > 0) bump(out, "fieldImplicitArg", implicitArgs);
  }
  // `header` is compared on its own; touching it here keeps the linter honest
  // about the argument being part of the case.
  if (header.length === 0) bump(out, "headerEmpty");
  return out;
}

for (const mod of irModules) {
  const module: string = mod.module;
  const root = prototype.pageRoot(module);
  const moduleSourceUrl = `${sourceUrlBase}/${module.split(".").join("/")}.lean`;
  const ctx: Ctx = {
    root,
    moduleDeclNames: prototype.moduleDeclNamesOf(mod),
    knownModules: fullKnownModules,
  };

  // --- every declaration's header, in IR order (a superset of the page's) ---
  prototype.sinkStats();
  const headerOf = new Map<string, string>();
  for (const d of mod.declarations) {
    zeroCounters();
    const html = prototype.declHeader(d, module, renderer);
    headerOf.set(d.name, html);
    headers.push({ what: `${module} ${d.name} header`, html });
    const delta = counterDelta();
    for (const [k, n] of Object.entries(delta)) branchTotals[k] += n;
  }

  // --- every declaration that gets a page entry, in page order ---
  prototype.sinkPageStats();
  const pageDecls = prototype.pageDeclsOf(mod, suppressed);
  const memberNames: string[] = pageDecls.map((d: Any) => d.name);
  for (const d of pageDecls) {
    zeroCounters();
    prototype.traced.clear();
    prototype.setTracing(true);
    const header = headerOf.get(d.name)!;
    const html = prototype.declHtml(d, mod, root, moduleSourceUrl, header, ctx, renderer);
    prototype.setTracing(false);
    const branches = counterDelta();
    for (const [k, n] of Object.entries(derivedDeclBranches(d, header, html))) bump(branches, k, n);
    for (const [k, n] of Object.entries(branches)) {
      if (k in branchTotals) branchTotals[k] += n;
      else branchTotals[k] = n;
    }
    declCases.push({
      what: `${module} ${d.name}`,
      module,
      root,
      sourceUrl: moduleSourceUrl,
      mod,
      decl: d,
      header,
      html,
      branches,
      traced: new Set(prototype.traced),
    });
  }

  // --- the frame ---
  zeroCounters();
  const head = prototype.headHtml(module, root);
  const pageHeader = prototype.pageHeaderHtml(module, root);
  const nav = prototype.internalNavHtml(module, root, moduleSourceUrl, mod.imports ?? [], memberNames);
  const branches = counterDelta();
  if ((mod.imports ?? []).length === 0) bump(branches, "navWithoutImports");
  if (memberNames.length === 0) bump(branches, "navWithoutMembers");
  for (const [k, n] of Object.entries(branches)) {
    if (k in branchTotals) branchTotals[k] += n;
    else branchTotals[k] = n;
  }
  frameCases.push({
    module,
    root,
    sourceUrl: moduleSourceUrl,
    imports: mod.imports ?? [],
    memberNames,
    head,
    header: pageHeader,
    nav,
    branches,
  });
}

console.error(
  `${headers.length} headers / ${declCases.length} declaration blocks / ` +
    `${frameCases.length} frames`,
);

const neverFires = Object.entries(branchTotals).filter(([, n]) => n === 0).map(([k]) => k);
console.error(
  neverFires.length === 0
    ? "every branch fires on the real corpus"
    : `branches the real corpus never reaches: ${neverFires.join(" ")}`,
);

// ------------------------------------------------------------------ reduction

type World = {
  known: Record<string, string>;
  lidxEntries: Record<string, string>;
  irModules: string[];
  lidxModules: string[];
};

/**
 * A case's world reduced to the names it actually looked up.
 *
 * `knownModules` is not stored: it is *derived* from the three sources, and a
 * fixture that carried the derived set could not tell a port that forgot one of
 * them from a correct one. Each module name the case asked about is put back
 * into a source that supplies it -- the IR's own names, the `.lidx`'s `@`
 * section, or a carrier entry of `known` whose value it is. A carrier's key
 * contains a space, so it is not a name literal and can never be looked up.
 */
function reduce(traced: Set<string>): World {
  const world: World = { known: {}, lidxEntries: {}, irModules: [], lidxModules: [] };
  const names = [...traced].sort();
  for (const name of names) {
    const owner = prototype.known.get(name);
    if (owner !== undefined) world.known[name] = owner;
    const viaLidx = prototype.linkIndex.get(name);
    if (viaLidx !== undefined) world.lidxEntries[name] = viaLidx;
  }
  for (const name of names) {
    if (!fullKnownModules.has(name)) continue;
    if (irModuleNames.has(name)) world.irModules.push(name);
    else if (lidxModuleNames.has(name)) world.lidxModules.push(name);
    else if (!Object.values(world.known).includes(name)) {
      world.known[`carrier for ${name}`] = name;
    }
  }
  return world;
}

/** The `.lidx` text a world stands on. */
function lidxText(world: World): string {
  const lines = ["#lidx1"];
  for (const m of world.lidxModules) lines.push(`@${m}`);
  const byModule = new Map<string, string[]>();
  for (const [name, module] of Object.entries(world.lidxEntries)) {
    const list = byModule.get(module) ?? [];
    list.push(name);
    byModule.set(module, list);
  }
  for (const [module, names] of byModule) {
    lines.push(module);
    for (const name of names) lines.push(`\t${name}`);
  }
  return lines.join("\n") + "\n";
}

/**
 * A declaration reduced to what `containedNames` and `moduleDeclNames` read.
 *
 * Everything else is emptied, so a module of 200 declarations costs a few
 * hundred bytes in the fixture rather than a few hundred kilobytes. The keys
 * are the ones the schema-4 reader requires -- it is `deny_unknown_fields` and
 * these are not `#[serde(default)]`.
 */
const stubDecl = (d: Any) => ({
  name: d.name,
  kind: "theorem",
  modifiers: [],
  binders: [],
  implicits: [],
  binderCode: [],
  type: "",
  typeCode: [],
  line: d.line,
  col: d.col,
  endLine: d.endLine,
  endCol: d.endCol,
  index: d.index,
  members: [],
  doc: null,
  equations: [],
  equationCode: [],
  refs: [],
});

/**
 * The module file a case carries: the rendered declaration, plus stubs.
 *
 * `all = false` drops the siblings entirely. That is only used when re-rendering
 * without them produces the same bytes -- i.e. when neither `containedNames`
 * nor `nameToLink`'s last branch consulted the list -- which is most cases,
 * and it is what keeps the fixture from carrying 200 stubs per theorem.
 */
function reduceModule(mod: Any, decl: Any, all: boolean) {
  return {
    schemaVersion: 4,
    module: mod.module,
    imports: [],
    moduleDocs: [],
    tactics: [],
    declarations: all
      ? mod.declarations.map((d: Any) => (d.name === decl.name ? d : stubDecl(d)))
      : [decl],
  };
}

/**
 * Renders inside a world, the way the Rust fixture test will: the global maps
 * are emptied first, so nothing outside the world can answer.
 */
function renderInWorld(
  mod: Any,
  decl: Any,
  root: string,
  sourceUrl: string,
  world: World,
): { header: string; html: string } {
  prototype.known.clear();
  prototype.linkIndex.clear();
  for (const [n, m] of Object.entries(world.known)) prototype.known.set(n, m);
  for (const [n, m] of Object.entries(world.lidxEntries)) prototype.linkIndex.set(n, m);
  const knownModules = new Set<string>(world.irModules);
  for (const m of prototype.known.values()) knownModules.add(m);
  for (const m of world.lidxModules) knownModules.add(m);
  const ctx: Ctx = {
    root,
    moduleDeclNames: prototype.moduleDeclNamesOf(mod),
    knownModules,
  };
  const header = prototype.declHeader(decl, mod.module, renderer);
  const html = prototype.declHtml(decl, mod, root, sourceUrl, header, ctx, renderer);
  return { header, html };
}

// ------------------------------------------------------------------ selection

/** What a declaration case exercises: its branches, plus shapes of its output. */
function declFeatures(c: DeclCase): Set<string> {
  const out = new Set<string>(Object.keys(c.branches).filter((k) => c.branches[k] > 0));
  out.add(`root:${c.root}`);
  if (c.html.includes("<a href=\"") === false) out.add("no-anchor");
  if (c.decl.members.length === 0) out.add("no-members");
  if (/[\u{10000}-\u{10FFFF}]/u.test(c.html)) out.add("astral");
  if (/[&<>"]/.test(c.decl.name)) out.add("escapable-name");
  if (c.decl.modifiers.length > 0) out.add(`modifiers:${[...c.decl.modifiers].sort().join(",")}`);
  if (c.html.includes("<code>")) out.add("docstring-code");
  if (c.html.includes("structure_field_doc")) out.add("field-doc");
  if (c.decl.equations.length > 1) out.add("many-equations");
  return out;
}

/**
 * Weighted greedy set cover: at each step take the case with the best
 * *newly covered features per byte*, not the one with the most new features.
 *
 * The unweighted rule picks whichever declaration happens to be biggest, and
 * the biggest of this package are 28 KB apiece; the weighted one covers the
 * same feature set out of cases an order of magnitude smaller. It still
 * terminates with full coverage, because any case with a positive gain beats
 * every case with none.
 */
function greedyCover<T>(items: T[], featuresOf: (t: T) => Set<string>, size: (t: T) => number) {
  const chosen = new Set<number>();
  const covered = new Set<string>();
  const fs = items.map(featuresOf);
  for (;;) {
    let best = -1;
    let bestScore = 0;
    for (let i = 0; i < items.length; i++) {
      if (chosen.has(i)) continue;
      let gain = 0;
      for (const f of fs[i]) if (!covered.has(f)) gain++;
      if (gain === 0) continue;
      const score = gain / Math.max(1, size(items[i]));
      if (score > bestScore) {
        best = i;
        bestScore = score;
      }
    }
    if (best < 0) break;
    chosen.add(best);
    for (const f of fs[best]) covered.add(f);
  }
  return { chosen, covered, featuresOf: fs };
}

/** Roughly how many bytes a case would cost the fixture. */
const declCost = (c: DeclCase) =>
  c.html.length + c.header.length + 160 * c.mod.declarations.length + 60 * c.decl.refs.length;

const declCover = greedyCover(declCases, declFeatures, declCost);
{
  const compact = (i: number) => declCases[i].html.length <= COMPACT_CHARS;
  const rest = declCases.map((_, i) => i).filter((i) => !declCover.chosen.has(i) && compact(i));
  const want = Math.max(0, DECL_TARGET - declCover.chosen.size);
  if (want > 0 && rest.length > 0) {
    const stride = Math.max(1, Math.floor(rest.length / want));
    for (let i = 0; i < rest.length; i += stride) declCover.chosen.add(rest[i]);
  }
}
const selectedDecls = [...declCover.chosen].sort((a, b) => a - b);

function frameFeatures(f: FrameCase): Set<string> {
  const out = new Set<string>(Object.keys(f.branches).filter((k) => f.branches[k] > 0));
  out.add(`root:${f.root}`);
  out.add(`depth:${f.module.split(".").length}`);
  if (f.imports.length !== new Set(f.imports).size) out.add("duplicate-imports");
  if (f.memberNames.length === 0) out.add("no-members");
  if (/[^\x20-\x7e]/.test(f.module)) out.add("non-ascii-module");
  return out;
}

/** Frames small enough to commit; the rest are covered by `--full`. */
const frameCandidates = frameCases
  .map((f, i) => ({ f, i }))
  .filter(({ f }) => f.nav.length <= COMPACT_NAV);

const frameCover = greedyCover(
  frameCandidates,
  ({ f }) => frameFeatures(f),
  ({ f }) => f.nav.length,
);
{
  const rest = frameCandidates.map((_, i) => i).filter((i) => !frameCover.chosen.has(i));
  const want = Math.max(0, FRAME_TARGET - frameCover.chosen.size);
  if (want > 0 && rest.length > 0) {
    const stride = Math.max(1, Math.floor(rest.length / want));
    for (let i = 0; i < rest.length; i += stride) frameCover.chosen.add(rest[i]);
  }
}
const selectedFrames = [...frameCover.chosen]
  .map((i) => frameCandidates[i].i)
  .sort((a, b) => a - b);

const declWorlds = new Map<number, World>(
  selectedDecls.map((i) => [i, reduce(declCases[i].traced)]),
);

// The full maps are emptied by `renderInWorld` below; the sizes are part of
// what the fixture records, so they are read here for the last time.
const fullKnownEntries = prototype.known.size;
const fullLinkIndexEntries = prototype.linkIndex.size;
const fullKnownModuleCount = fullKnownModules.size;

// --------------------------------------------------------------------- curated

/**
 * Hand-written declarations with hand-written worlds.
 *
 * The corpus is real data, so it covers what the target package happens to
 * contain. These cover what it does not -- see `neverFires` above, printed by
 * every run: an `extends` clause, a non-`mk` constructor (`structure_ext`), an
 * inherited field with and without the `id` `containedNames` gives it, an
 * equation over the 200-code-point limit, a `class_inductive`, a field
 * docstring, an implicit field binder, and a member with `isDirect` **absent**,
 * which the target package has none of and which the prototype treats as
 * direct.
 */
type Curated = {
  what: string;
  module?: string;
  imports?: string[];
  decls: Any[];
  /** Which of `decls` is rendered. Defaults to the first. */
  at?: number;
} & Partial<World>;

/** A schema-4 declaration with everything empty. */
const mk = (name: string, kind: string, extra: Record<string, Any> = {}) => ({
  name,
  kind,
  modifiers: [],
  binders: [],
  implicits: [],
  binderCode: [],
  type: "",
  typeCode: [],
  line: 1,
  col: 0,
  endLine: 1,
  endCol: 1,
  index: 0,
  members: [],
  doc: null,
  equations: [],
  equationCode: [],
  refs: [],
  ...extra,
});

const field = (name: string, extra: Record<string, Any> = {}) => ({
  label: "field",
  name,
  text: "Nat",
  code: [],
  binders: [],
  implicits: [],
  binderCode: [],
  doc: null,
  ...extra,
});

const CURATED: Curated[] = [
  {
    what: "curated: a structure with an extends clause and a parent member",
    decls: [
      mk("S", "structure", {
        endLine: 9,
        endCol: 0,
        members: [
          { label: "parent", name: "S.toP", text: "P", code: [[0, 1, 1, "P"]] },
          { label: "ctor", name: "S.mk", text: "", code: [] },
          field("S.x", { isDirect: true }),
        ],
      }),
    ],
    known: { "P": "Pkg.Parent" },
  },
  {
    what: "curated: two parents are joined with a comma",
    decls: [
      mk("S", "class", {
        members: [
          { label: "parent", name: "S.toA", text: "A", code: [] },
          { label: "parent", name: "S.to<B", text: "B", code: [] },
        ],
      }),
    ],
  },
  {
    what: "curated: a constructor that is not `mk` prints as structure_ext",
    decls: [
      mk("S", "structure", {
        members: [
          { label: "ctor", name: "S.make", text: "", code: [] },
          field("S.x", { isDirect: true }),
        ],
      }),
    ],
  },
  {
    what: "curated: a structure with no ctor member falls back to <name>.mk",
    decls: [mk("S", "structure", { members: [field("S.x", { isDirect: true })] })],
  },
  {
    what: "curated: an inherited field links out and gets no id",
    decls: [
      mk("S", "structure", {
        endLine: 9,
        endCol: 0,
        members: [field("P.y", { isDirect: false })],
      }),
    ],
    known: { "P.y": "Pkg.Parent" },
  },
  {
    what: "curated: an inherited field whose projection is inside the range gets one",
    decls: [
      mk("S", "structure", {
        endLine: 9,
        endCol: 0,
        members: [field("P.y", { isDirect: false })],
      }),
      mk("S.y", "definition", { line: 2, endLine: 2, index: 1 }),
    ],
    known: { "P.y": "Pkg.Parent" },
  },
  {
    // `containedNames` compares `col >= parent.col` and `endCol <= parent.endCol`,
    // and no real structure of this package has an inherited field at all, let
    // alone one whose projection shares a boundary. Making either comparison
    // strict is invisible everywhere except here.
    what: "curated: an inherited field whose projection shares the structure's exact range",
    decls: [
      mk("S", "structure", {
        col: 2,
        endCol: 40,
        members: [field("P.y", { isDirect: false })],
      }),
      mk("S.y", "definition", { col: 2, endCol: 40, index: 1 }),
    ],
    known: { "P.y": "Pkg.Parent" },
  },
  {
    what: "curated: a field with a docstring, an implicit binder and an explicit one",
    decls: [
      mk("S", "structure", {
        members: [
          field("S.x", {
            isDirect: true,
            doc: "the field's own `doc`",
            binders: ["{n : Nat}", "(m : Nat)"],
            implicits: [true, false],
            binderCode: [[], []],
          }),
        ],
      }),
    ],
  },
  {
    what: "curated: a member with isDirect absent is direct, not inherited",
    decls: [mk("S", "structure", { members: [field("S.x")] })],
  },
  {
    what: "curated: an equation over the 200-code-point limit",
    decls: [
      mk("f", "definition", {
        equations: ["f x = " + "𝒜".repeat(200), "f y = 1"],
        equationCode: [[], []],
      }),
    ],
  },
  {
    what: "curated: every equation is over the limit, so only the notice remains",
    decls: [mk("f", "definition", { equations: ["𝒜".repeat(400)], equationCode: [[]] })],
  },
  {
    what: "curated: an equation of exactly 199 code points is kept",
    decls: [mk("f", "instance", { equations: ["𝒜".repeat(199)], equationCode: [[]] })],
  },
  {
    what: "curated: a class_inductive gets the class instance stub",
    decls: [mk("C", "class_inductive", { doc: "a class inductive" })],
  },
  {
    what: "curated: an inductive gets the instances-for stub",
    decls: [mk("I", "inductive", { modifiers: ["unsafe"] })],
  },
  {
    what: "curated: a class gets the class instance stub and a members table",
    decls: [mk("C", "class", { members: [field("C.op", { isDirect: true })] })],
  },
  {
    what: "curated: attributes, with the escaping and the trailing newline",
    decls: [mk("f", "theorem", { attrs: ["simp", "instance <priority>"] })],
  },
  {
    what: "curated: a name that needs escaping everywhere it appears",
    decls: [mk("A<B&C", "definition", { doc: "doc" })],
  },
  {
    what: "curated: an unsafe noncomputable abbrev with implicit binders",
    decls: [
      mk("f", "definition", {
        modifiers: ["unsafe", "noncomputable", "abbrev"],
        binders: ["{α : Type}", "(x : α)"],
        implicits: [true, false],
        binderCode: [[], []],
        type: "α",
        typeCode: [],
      }),
    ],
  },
  {
    what: "curated: an empty docstring renders nothing at all",
    decls: [mk("f", "theorem", { doc: "" })],
  },
  {
    what: "curated: a private declaration is skipped by moduleDeclNames",
    decls: [
      mk("f", "theorem", { doc: "`g` is next door", line: 3, index: 1 }),
      mk("_private.Pkg.M.0.g", "theorem", { line: 1, index: 0 }),
      mk("Pkg.M.g", "theorem", { line: 5, index: 2 }),
    ],
  },
];

// --------------------------------------------------------------------- output

type OutDecl = {
  what: string;
  root: string;
  sourceUrl: string;
  module: Any;
  at: number;
  known: Record<string, string>;
  lidx: string;
  irModules: string[];
  header: string;
  html: string;
};

const sortedWorld = (world: World): World => ({
  known: Object.fromEntries(Object.entries(world.known).sort(([a], [b]) => (a < b ? -1 : 1))),
  lidxEntries: Object.fromEntries(
    Object.entries(world.lidxEntries).sort(([a], [b]) => (a < b ? -1 : 1)),
  ),
  irModules: [...world.irModules].sort(),
  lidxModules: [...world.lidxModules].sort(),
});

const curatedBranches: Sink = {};
const curatedOut: OutDecl[] = CURATED.map((c) => {
  const moduleName = c.module ?? "Pkg.M";
  const module = {
    schemaVersion: 4,
    module: moduleName,
    imports: c.imports ?? [],
    moduleDocs: [],
    tactics: [],
    declarations: c.decls,
  };
  const at = c.at ?? 0;
  const decl = c.decls[at];
  const root = prototype.pageRoot(moduleName);
  const sourceUrl = `${sourceUrlBase}/${moduleName.split(".").join("/")}.lean`;
  // A run puts every declaration of the module into `known` after the
  // dependency slices, so the hand-written worlds do too -- otherwise
  // `nameToLink`'s last branch resolves a name it cannot then place, which is
  // the `known.get(n)!` the prototype writes and this port `expect`s.
  const world = sortedWorld({
    known: {
      ...(c.known ?? {}),
      ...Object.fromEntries(c.decls.map((d: Any) => [d.name, moduleName])),
    },
    lidxEntries: c.lidxEntries ?? {},
    irModules: [moduleName, ...(c.irModules ?? [])],
    lidxModules: c.lidxModules ?? [],
  });
  zeroCounters();
  prototype.sinkPageStats();
  const { header, html } = renderInWorld(module, decl, root, sourceUrl, world);
  const branches = counterDelta();
  for (const [k, n] of Object.entries(derivedDeclBranches(decl, header, html))) bump(branches, k, n);
  for (const [k, n] of Object.entries(branches)) bump(curatedBranches, k, n);
  return {
    what: c.what,
    root,
    sourceUrl,
    module,
    at,
    known: world.known,
    lidx: lidxText(world),
    irModules: world.irModules,
    header,
    html,
  };
});

const sample: OutDecl[] = [];
let reductionFailures = 0;
let siblingsKept = 0;
for (const i of selectedDecls) {
  const c = declCases[i];
  const world = sortedWorld(declWorlds.get(i)!);
  // Try without the siblings first, and keep them only when they turn out to
  // matter. Either way the chosen form is proved to reproduce the bytes.
  let module = reduceModule(c.mod, c.decl, false);
  let again = renderInWorld(module, c.decl, c.root, c.sourceUrl, world);
  if (again.html !== c.html || again.header !== c.header) {
    siblingsKept++;
    module = reduceModule(c.mod, c.decl, true);
    again = renderInWorld(module, c.decl, c.root, c.sourceUrl, world);
  }
  if (again.html !== c.html || again.header !== c.header) {
    reductionFailures++;
    console.error(`reduction changed the output of ${JSON.stringify(c.what)}`);
    continue;
  }
  sample.push({
    what: c.what,
    root: c.root,
    sourceUrl: c.sourceUrl,
    module,
    at: module.declarations.findIndex((d: Any) => d.name === c.decl.name),
    known: world.known,
    lidx: lidxText(world),
    irModules: world.irModules,
    header: c.header,
    html: c.html,
  });
}
if (reductionFailures > 0) {
  console.error(`${reductionFailures} cases could not be reduced; the tracing is incomplete`);
  Deno.exit(6);
}
console.error(
  `${siblingsKept} of ${sample.length} sampled declarations need their module's other ` +
    `declarations (containedNames / the moduleDeclNames scan)`,
);

/**
 * Hand-written frames.
 *
 * Every module of the target package imports something and has at least one
 * page entry, so the empty branches of `internalNavHtml` are unreachable from
 * real data; and no module name of it needs escaping.
 */
const CURATED_FRAMES: { what: string; module: string; imports: string[]; members: string[] }[] = [
  {
    what: "curated: a module with no imports and no page entries",
    module: "Pkg",
    imports: [],
    members: [],
  },
  {
    what: "curated: duplicate imports are dropped, the first occurrence kept",
    module: "Pkg.A.B",
    imports: ["Init", "Mathlib.Order", "Init", "Mathlib.Order", "Zzz", "Init.Core"],
    members: ["Pkg.A.B.f"],
  },
  {
    what: "curated: a module name and a member name that need escaping",
    module: "Pkg.A<B",
    imports: ["A&B"],
    members: ["x\"y", "«α».z"],
  },
  {
    what: "curated: five components deep",
    module: "A.B.C.D.E",
    imports: ["A"],
    members: ["A.B.C.D.E.f"],
  },
];

const curatedFrames = CURATED_FRAMES.map((f) => {
  const root = prototype.pageRoot(f.module);
  const sourceUrl = `${sourceUrlBase}/${f.module.split(".").join("/")}.lean`;
  zeroCounters();
  const out = {
    what: f.what,
    module: f.module,
    root,
    sourceUrl,
    imports: f.imports,
    memberNames: f.members,
    head: prototype.headHtml(f.module, root),
    header: prototype.pageHeaderHtml(f.module, root),
    nav: prototype.internalNavHtml(f.module, root, sourceUrl, f.imports, f.members),
  };
  const branches = counterDelta();
  if (f.imports.length === 0) bump(branches, "navWithoutImports");
  if (f.members.length === 0) bump(branches, "navWithoutMembers");
  for (const [k, n] of Object.entries(branches)) bump(curatedBranches, k, n);
  return out;
});

const frameSample = [
  ...curatedFrames,
  ...selectedFrames.map((i) => {
    const f = frameCases[i];
    return {
      what: f.module,
      module: f.module,
      root: f.root,
      sourceUrl: f.sourceUrl,
      imports: f.imports,
      memberNames: f.memberNames,
      head: f.head,
      header: f.header,
      nav: f.nav,
    };
  }),
];

const declFeatureUniverse = new Set<string>();
for (const fs of declCover.featuresOf) for (const f of fs) declFeatureUniverse.add(f);
const declFeaturesCovered = new Set<string>();
for (const i of selectedDecls) for (const f of declCover.featuresOf[i]) declFeaturesCovered.add(f);
const missing = [...declFeatureUniverse].filter((f) => !declFeaturesCovered.has(f));
if (missing.length > 0) {
  console.error(`uncovered declaration features: ${missing.join(" ")}`);
  Deno.exit(7);
}

const provenance = {
  generatedBy: "crates/lean-doc-render/tests/oracle/gen-page-parts-expected.ts",
  oracle: "experiments/stage7d/render.ts declHeader / declHtml / headHtml / pageHeaderHtml / " +
    "internalNavHtml, sliced out of the frozen prototype, with the real IR and the real .lidx",
  renderTsRanges: RANGES,
  ir,
  linkIndex: linkIndexPath,
  sourceUrl: sourceUrlBase,
  /** `--full` denominators. */
  irModules: irModules.length,
  irDeclarations: headers.length,
  pageDeclarations: declCases.length,
  suppressedDeclarations: suppressed.size,
  knownEntries: fullKnownEntries,
  linkIndexEntries: fullLinkIndexEntries,
  knownModules: fullKnownModuleCount,
  branchTotals,
  curatedBranches,
  neverFires: neverFires.sort(),
  deno: Deno.version.deno,
};

if (full) {
  await Deno.writeTextFile(
    full,
    JSON.stringify({
      ...provenance,
      headers,
      decls: declCases.map((c) => ({ what: c.what, header: c.header, html: c.html })),
      frames: frameCases.map((f) => ({
        module: f.module,
        memberNames: f.memberNames,
        head: f.head,
        header: f.header,
        nav: f.nav,
      })),
    }) + "\n",
  );
  console.error(
    `${headers.length} headers + ${declCases.length} declarations + ` +
      `${frameCases.length} frames -> ${full}`,
  );
}

const fixture = JSON.stringify({
  ...provenance,
  sampleFeatures: [...declFeaturesCovered].sort(),
  cases: [...curatedOut, ...sample],
  frames: frameSample,
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
    `${curatedOut.length + sample.length} declarations + ${frameSample.length} frames -> ` +
      `${FIXTURE.pathname} (${new TextEncoder().encode(fixture).length} B)`,
  );
}
