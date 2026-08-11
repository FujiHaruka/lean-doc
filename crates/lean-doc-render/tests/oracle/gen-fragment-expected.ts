#!/usr/bin/env -S deno run --allow-read --allow-write
// gen-fragment-expected.ts -- render every span-carrying fragment of the target
// package's IR with the *prototype's* code renderer, so that
// `tests/fragment.rs` compares the Rust port against the prototype's own bytes
// rather than against a reading of it.
//
// WHY THE PROTOTYPE IS THE ORACLE HERE
// ------------------------------------
// M1-c could use doc-gen4 itself: `docStringToHtml` is reachable with an empty
// `AnalyzerResult`. This step is not reachable that way. `renderedCodeToHtmlAux`
// takes a `CodeWithInfos` -- doc-gen4's *pre-flattening* tree -- and the IR
// carries the flattened span list instead, so there is no input that could be
// handed to both. What the IR does have is the prototype that consumes it, and
// that prototype is the one scoring 439/439 (plan §1).
//
// HOW IT GETS AT render.ts WITHOUT IMPORTING IT
// ---------------------------------------------
// The same way the other oracles in this tree do: render.ts is a script whose
// top level parses argv and exits, and it is frozen, so the definitions are
// sliced out by line range, assembled into a module and imported through a
// `data:` URL. Each range declares what its first line must start with, so a
// range that has drifted onto other code is a hard failure rather than a
// silently different expectation.
//
// WHAT MAKES THE COMMITTED SAMPLE SELF-CONTAINED
// ----------------------------------------------
// `known` is 5,296 entries and a declaration's `refs` can be hundreds. Both are
// handed to the prototype as tracing maps, so the exact set of names each
// fragment looks up is recorded; the fixture carries the answers for those
// names and nothing else. Every reduced case is then re-rendered and required
// to produce the same bytes, so a reduction that lost something fails here
// rather than weakening a test later.
//
// npm/node are broken in this environment; this must run under deno.
//
// usage:
//   deno run --allow-read --allow-write \
//     crates/lean-doc-render/tests/oracle/gen-fragment-expected.ts
//   ... --full /tmp/fragment-full.json   also write every fragment's expected
//                                        HTML (no maps: the Rust side rebuilds
//                                        them from the same IR)
//   ... --check                          fail if the committed file is stale
//   ... --ir DIR                         point at another corpus

const FIXTURE = new URL("../data/fragment-expected.json", import.meta.url);
const RENDER_TS = new URL("../../../../experiments/stage7d/render.ts", import.meta.url);

const DEFAULT_IR = "/private/tmp/lean-doc-relay/w7h/base-ir";

/** Roughly how many real fragments the committed fixture aims for. */
const FIXTURE_TARGET = 200;

/**
 * Above this many UTF-16 units a fragment is not taken for variety alone.
 *
 * The whole corpus is reachable with `--full` (`tests/fragment.rs`), so what is
 * committed should be the variety, not the volume. Coverage still overrides
 * this -- a branch reached by one long signature and nothing else is kept.
 */
const COMPACT_UNITS = 200;

// ---------------------------------------------------- slicing render.ts

const renderLines = (await Deno.readTextFile(RENDER_TS)).split("\n");

/** Lines `from..to` of render.ts, 1-based and inclusive. */
const slice = (from: number, to: number) => renderLines.slice(from - 1, to).join("\n");

/**
 * Line ranges in render.ts. `head` is text the first line must start with.
 *
 * This is the whole of M1-d1's scope: everything below is transcription of
 * these ranges and nothing else.
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
  cssKind: { from: 832, to: 843, head: "function cssKind(" },
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

/**
 * The module assembled out of those slices.
 *
 * `sink` is the counter block `applyWsWidths` and `Renderer` bump
 * (`render.ts:445-533`). It reaches no byte -- the Rust port drops it -- but it
 * is exactly a report of which branch each walk took, so it is kept here and
 * read as a per-case delta to drive the fixture's coverage selection.
 *
 * `known` and each declaration's `refs` are tracing maps, which is what lets
 * the committed fixture carry a reduced map that is provably enough.
 */
const MODULE_SOURCE = [
  "// deno-lint-ignore-file no-explicit-any",
  "type Span = any[];",
  "export class TracingMap extends Map<string, string> {",
  "  readonly seen = new Set<string>();",
  "  override get(k: string) { this.seen.add(k); return super.get(k); }",
  "  override has(k: string) { this.seen.add(k); return super.has(k); }",
  "}",
  "export const sink = {",
  "  anchorsConst: 0, anchorsSort: 0, anchorsModuleFallback: 0,",
  "  constSpansSuppressedByNesting: 0, constSpansUnlinkable: 0,",
  "  constSpansViaParent: 0, constSpansPrivate: 0, constSpansNameNotInRefs: 0,",
  "  wsWidthChars: 0, wsWidthFragments: 0,",
  "};",
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
  at("cssKind"),
  "export { Renderer, breakWithin, buildTree, cssKind, findLinkableParent, kindDescription,",
  "  moduleFromPrivatePrefix, moduleLink, pageRoot, privateToUserName };",
].join("\n\n");

type Span = (number | string)[];
type Rendered = { html: string; hasAnchor: boolean };
type Sink = Record<string, number>;

const prototype = await import(
  `data:text/typescript;charset=utf-8,${encodeURIComponent(MODULE_SOURCE)}`
) as {
  Renderer: new (known: Map<string, string>) => {
    fragment(text: string, spans: Span[], root: string, refs: Map<string, string>): Rendered;
  };
  TracingMap: new () => Map<string, string> & { seen: Set<string> };
  sink: Sink;
  breakWithin(name: string): string;
  cssKind(kind: string): string;
  kindDescription(kind: string, modifiers: string[]): string;
  moduleFromPrivatePrefix(name: string): string | null;
  privateToUserName(name: string): string;
  pageRoot(module: string): string;
};

// -------------------------------------------------------------------- inputs

const args = Deno.args;
const flag = (name: string, fallback: string | null = null) => {
  const i = args.indexOf(name);
  return i >= 0 && i + 1 < args.length ? args[i + 1] : fallback;
};
const ir = flag("--ir", DEFAULT_IR)!;
const full = flag("--full");
const check = args.includes("--check");

// deno-lint-ignore no-explicit-any
const readJson = async (path: string): Promise<any> => JSON.parse(await Deno.readTextFile(path));

const index = await readJson(`${ir}/index.json`);

const known = new prototype.TracingMap();

/**
 * `render.ts:2008-2036`, in the order it does it: dependency slices first, then
 * every declaration of this package (overwriting), then every reference the
 * extractor resolved (filling gaps only).
 *
 * Getting this wrong shows up as *fewer links*, one at a time, which is the
 * failure mode a weak test passes (plan §5, pitfall 6).
 */
for (const dep of index.dependencyMaps) {
  const map = await readJson(`${ir}/${dep.file}`);
  for (const [n, m] of Object.entries(map.declarations as Record<string, string>)) known.set(n, m);
}
// deno-lint-ignore no-explicit-any
const irModules: any[] = [];
for (const entry of index.modules) {
  const mod = await readJson(`${ir}/${entry.file}`);
  irModules.push(mod);
  for (const d of mod.declarations) {
    known.set(d.name, mod.module);
    for (const [m, n] of d.refs) if (!known.has(n)) known.set(n, m);
  }
}
// The three lines above ask `known.has` for every reference, which the tracing
// map records. Per-case tracing starts from a clean sheet.
known.seen.clear();

const renderer = new prototype.Renderer(known);

// -------------------------------------------------------------------- corpus

/**
 * One span-carrying fragment: the five text/span pairs the IR has.
 *
 * The order below is the order `tests/fragment.rs` walks the same IR in, and
 * the `--full` comparison is positional, so the two have to stay in step. Each
 * case also carries a `what` that names it, and the Rust side rebuilds the same
 * string and asserts it.
 */
type Case = {
  what: string;
  module: string;
  root: string;
  text: string;
  spans: Span[];
  refs: Map<string, string> & { seen: Set<string> };
};

const cases: Case[] = [];
for (const mod of irModules) {
  const root = prototype.pageRoot(mod.module);
  for (const d of mod.declarations) {
    // A declaration's members and equations resolve against the *declaration's*
    // references (`render.ts:1698`, `1794-1798`); they have none of their own.
    const refs = new prototype.TracingMap();
    for (const [m, n] of d.refs) refs.set(n, m);
    const add = (what: string, text: unknown, spans: unknown) => {
      if (typeof text !== "string") return;
      cases.push({
        what: `${mod.module} ${d.name} ${what}`,
        module: mod.module,
        root,
        text,
        spans: (spans ?? []) as Span[],
        refs,
      });
    };
    for (let i = 0; i < d.binders.length; i++) add(`binder ${i}`, d.binders[i], d.binderCode?.[i]);
    add("type", d.type, d.typeCode);
    for (let i = 0; i < (d.equations?.length ?? 0); i++) {
      add(`equation ${i}`, d.equations[i], d.equationCode?.[i]);
    }
    for (let j = 0; j < d.members.length; j++) {
      const f = d.members[j];
      add(`member ${j} ${f.label} ${f.name}`, f.text, f.code);
      for (let k = 0; k < (f.binders?.length ?? 0); k++) {
        add(`member ${j} ${f.label} ${f.name} binder ${k}`, f.binders[k], f.binderCode?.[k]);
      }
    }
  }
}

// ------------------------------------------------------------------ rendering

const COUNTERS = Object.keys(prototype.sink);
const zeroSink = () => {
  for (const k of COUNTERS) prototype.sink[k] = 0;
};

type Traced = { known: string[]; refs: string[] };

const html: string[] = [];
const hasAnchor: boolean[] = [];
const branches: string[][] = [];
const traced: Traced[] = [];
/**
 * How often each branch fires over the whole corpus.
 *
 * Recorded because **three of them never fire** on the target package, which is
 * the kind of thing a "the corpus covers everything" assumption gets wrong: a
 * mutation to a branch with a zero here cannot be caught by real data at all,
 * only by the curated cases. `tests/fragment.rs` pins these numbers.
 */
const branchTotals: Sink = Object.fromEntries(COUNTERS.map((k) => [k, 0]));

for (const c of cases) {
  zeroSink();
  known.seen.clear();
  c.refs.seen.clear();
  const out = renderer.fragment(c.text, c.spans, c.root, c.refs);
  html.push(out.html);
  hasAnchor.push(out.hasAnchor);
  branches.push(COUNTERS.filter((k) => prototype.sink[k] > 0));
  for (const k of COUNTERS) branchTotals[k] += prototype.sink[k];
  traced.push({ known: [...known.seen].sort(), refs: [...c.refs.seen].sort() });
}

// ------------------------------------------------------------------- features

/**
 * Which span is inside which, under a given nesting rule.
 *
 * Written out here rather than taken from `buildTree` on purpose: it is used to
 * *select* the sample, and one of the things the sample has to contain is a
 * fragment where the two rules disagree. `buildTree` pops while
 * `start >= top.stop`, so two spans that merely touch are siblings; with `>`
 * the second would become a child of the first and the walk would close its
 * wrapper in the wrong place.
 */
function parentsUnder(spans: Span[], strict: boolean): number[] {
  const out: number[] = [];
  const stack: number[] = [];
  for (let i = 0; i < spans.length; i++) {
    while (stack.length > 0) {
      const top = stack[stack.length - 1];
      const past = strict
        ? (spans[i][0] as number) > (spans[top][1] as number)
        : (spans[i][0] as number) >= (spans[top][1] as number);
      if (past) stack.pop();
      else break;
    }
    out.push(stack.length > 0 ? stack[stack.length - 1] : -1);
    stack.push(i);
  }
  return out;
}

const depthsOf = (parents: number[]) => {
  const depth: number[] = [];
  for (let i = 0; i < parents.length; i++) {
    depth.push(parents[i] < 0 ? 1 : depth[parents[i]] + 1);
  }
  return depth;
};

/** What a case exercises: the branches it took, plus the shapes of its input. */
function features(i: number): Set<string> {
  const c = cases[i];
  const out = new Set<string>(branches[i]);
  const parents = parentsUnder(c.spans, false);
  const depth = depthsOf(parents);
  const kinds = new Set(c.spans.map((s) => s[2] as number));
  for (const k of kinds) out.add(`kind${k}`);
  if (c.spans.length === 0) out.add("no-spans");
  if (c.text.length === 0) out.add("empty-text");
  if (parents.some((p) => p >= 0)) out.add("nested");
  if (depth.some((d) => d >= 3)) out.add("depth3");
  if (parentsUnder(c.spans, true).join() !== parents.join()) out.add("touching-siblings");
  // A `kind 2` span that produced no anchor is one whose subtree already had
  // one -- the suppression `Base.lean:374-381` performs and no counter records.
  if (kinds.has(2) && !branches[i].includes("anchorsSort")) out.add("sort-suppressed");
  if (/[\u{10000}-\u{10FFFF}]/u.test(c.text)) out.add("astral-text");
  if (/[&<>"]/.test(c.text)) out.add("escapable-text");
  if (c.spans.some((s) => typeof s[3] === "string" && (s[3] as string).startsWith("_private."))) {
    out.add("private-span-name");
  }
  if (c.spans.some((s) => s.length > 4 && ((s[4] as number) > 0 || (s[5] as number) > 0))) {
    out.add("ws-widths");
  }
  if (hasAnchor[i]) out.add("anchored");
  out.add(`root:${c.root}`);
  return out;
}

const featuresOf = cases.map((_, i) => features(i));

/**
 * Which cases the committed fixture keeps: a greedy cover of the features
 * above, then the cases with the most anchors (where a subtly wrong resolver
 * has the most chances to show it), then an even stride over the rest so that
 * fragments which link *nothing* are represented too.
 */
function selectCases(): number[] {
  const chosen = new Set<number>();
  const covered = new Set<string>();
  for (;;) {
    let best = -1;
    let bestGain = 0;
    for (let i = 0; i < cases.length; i++) {
      if (chosen.has(i)) continue;
      let gain = 0;
      for (const f of featuresOf[i]) if (!covered.has(f)) gain++;
      // Ties go to the shorter fragment: same branches, fewer committed bytes.
      if (
        gain > bestGain ||
        (gain === bestGain && gain > 0 && cases[i].text.length < cases[best].text.length)
      ) {
        best = i;
        bestGain = gain;
      }
    }
    if (best < 0) break;
    chosen.add(best);
    for (const f of featuresOf[best]) covered.add(f);
  }
  const compact = (i: number) => cases[i].text.length <= COMPACT_UNITS;
  const anchors = (i: number) => (html[i].match(/<a href="/g) ?? []).length;
  const byAnchors = cases
    .map((_, i) => i)
    .filter((i) => !chosen.has(i) && compact(i) && anchors(i) > 0)
    .sort((a, b) => anchors(b) - anchors(a));
  for (const i of byAnchors.slice(0, Math.floor(FIXTURE_TARGET * 0.5))) chosen.add(i);

  const rest = cases.map((_, i) => i).filter((i) => !chosen.has(i) && compact(i));
  const want = Math.max(0, FIXTURE_TARGET - chosen.size);
  if (want > 0 && rest.length > 0) {
    const stride = Math.max(1, Math.floor(rest.length / want));
    for (let i = 0; i < rest.length; i += stride) chosen.add(rest[i]);
  }
  return [...chosen].sort((a, b) => a - b);
}

const selected = selectCases();

// ------------------------------------------------------------------ reduction

type World = { known: Record<string, string>; refs: Record<string, string> };

/** A case's world reduced to the names it actually looked up. */
function reduce(i: number): World {
  const world: World = { known: {}, refs: {} };
  for (const name of traced[i].known) {
    const owner = known.get(name);
    if (owner !== undefined) world.known[name] = owner;
  }
  for (const name of traced[i].refs) {
    const owner = cases[i].refs.get(name);
    if (owner !== undefined) world.refs[name] = owner;
  }
  return world;
}

/**
 * Renders inside a world, the way the Rust fixture test will: nothing outside
 * the world can answer.
 */
function renderInWorld(text: string, spans: Span[], root: string, world: World): Rendered {
  const w = new Map(Object.entries(world.known));
  const refs = new Map(Object.entries(world.refs));
  return new prototype.Renderer(w).fragment(text, spans, root, refs);
}

// --------------------------------------------------------------------- curated

/**
 * Hand-written cases with hand-written worlds.
 *
 * The corpus is real data, so it covers what the target package happens to
 * contain. These cover what it does not: an anchor suppressed by a nested one
 * in both directions, a link target that needs escaping, a private name whose
 * user name still finds a parent, the numeric and `_`-prefixed components
 * `findLinkableParent` skips, spans that only touch, and a span kind the
 * extractor does not currently emit.
 */
type Curated = { what: string; root: string; text: string; spans: Span[] } & Partial<World>;

const CURATED: Curated[] = [
  {
    what: "curated: an unlinkable constant is a plain span",
    root: "./",
    text: "Nowhere",
    spans: [[0, 7, 1, "Nowhere"]],
  },
  {
    what: "curated: a sort around a linked constant renders no anchor of its own",
    root: "./",
    text: "Nat",
    spans: [[0, 3, 2], [0, 3, 1, "Nat"]],
    known: { "Nat": "Init.Prelude" },
  },
  {
    what: "curated: a constant around a sort renders no anchor of its own",
    root: "./",
    text: "Type",
    spans: [[0, 4, 1, "Foo.f"], [0, 4, 2]],
    known: { "Foo.f": "Pkg.A" },
  },
  {
    what: "curated: an unlinkable constant around a sort still wraps it",
    root: "./",
    text: "Type",
    spans: [[0, 4, 1, "Nowhere"], [0, 4, 2]],
  },
  {
    what: "curated: two sorts side by side",
    root: ".././",
    text: "Type Prop",
    spans: [[0, 4, 2], [5, 9, 2]],
  },
  {
    what: "curated: touching spans are siblings, not parent and child",
    root: "./",
    text: "abcdef",
    spans: [[0, 3, 0], [3, 6, 0]],
  },
  {
    what: "curated: touching spans inside a third",
    root: "./",
    text: "abcdef",
    spans: [[0, 6, 0], [0, 3, 1, "Foo.a"], [3, 6, 1, "Foo.b"]],
    known: { "Foo.a": "Pkg.A", "Foo.b": "Pkg.A" },
  },
  {
    what: "curated: the link target is escaped",
    root: "./",
    text: "f",
    spans: [[0, 1, 1, "f"]],
    known: { "f": "A&B<C>\"D" },
  },
  {
    what: "curated: the fragment text is escaped",
    root: "./",
    text: "a < b & c > d \" e",
    spans: [[2, 3, 0]],
  },
  {
    what: "curated: the apostrophe is not escaped",
    root: "./",
    text: "f' g'",
    spans: [[0, 2, 1, "Foo.f'"]],
    known: { "Foo.f'": "Pkg.A" },
  },
  {
    what: "curated: a reference outranks the global map",
    root: "./",
    text: "x",
    spans: [[0, 1, 1, "Nat.succ"]],
    known: { "Nat.succ": "Stale.Module" },
    refs: { "Nat.succ": "Init.Prelude" },
  },
  {
    what: "curated: a name only the global map has",
    root: "./",
    text: "x",
    spans: [[0, 1, 1, "Nat.succ"]],
    known: { "Nat.succ": "Init.Prelude" },
  },
  {
    what: "curated: findLinkableParent skips numeric and underscored components",
    root: "./",
    text: "abc",
    spans: [[0, 1, 1, "Foo.bar._eq_1"], [1, 2, 1, "Foo.bar.42"], [2, 3, 1, "Foo._x.y"]],
    known: { "Foo.bar": "Pkg.A", "Foo": "Pkg.A" },
  },
  {
    what: "curated: a prefix with no dot left is never the answer",
    root: "./",
    text: "x",
    spans: [[0, 1, 1, "Foo.gone"]],
    known: { "Foo": "Pkg.A" },
  },
  {
    what: "curated: a private name falls back to its module",
    root: "../.././",
    text: "xy",
    spans: [[0, 1, 1, "_private.Init.Prelude.0.Foo"], [1, 2, 1, "_private.A.B.12.f"]],
  },
  {
    what: "curated: a private name is not looked up directly",
    root: "./",
    text: "x",
    spans: [[0, 1, 1, "_private.Pkg.A.0.f"]],
    known: { "_private.Pkg.A.0.f": "Pkg.Wrong" },
  },
  {
    what: "curated: a private name whose user name finds a parent",
    root: "./",
    text: "x",
    spans: [[0, 1, 1, "_private.Pkg.A.0.f.g.h"]],
    known: { "f.g": "Pkg.Owner" },
  },
  {
    what: "curated: a private name of no recognisable shape",
    root: "./",
    text: "x",
    spans: [[0, 1, 1, "_private.Pkg.A.f"]],
  },
  {
    what: "curated: the whitespace widths are replayed before the walk",
    root: "./",
    text: "a\n+\tb",
    spans: [[2, 3, 1, "HAdd.hAdd", 1, 1]],
    known: { "HAdd.hAdd": "Init.Prelude" },
  },
  {
    what: "curated: offsets are UTF-16 code units",
    root: "./",
    text: "\u{1d4e7} y \u{1d49c}",
    spans: [[3, 4, 1, "X"], [5, 7, 1, "Y"]],
    known: { "X": "Pkg.A", "Y": "Pkg.A" },
  },
  {
    what: "curated: a span kind the extractor does not emit",
    root: "./",
    text: "abc",
    spans: [[0, 3, 7]],
  },
  {
    what: "curated: no spans at all",
    root: "./",
    text: "a < b",
    spans: [],
  },
  {
    what: "curated: an empty fragment",
    root: "./",
    text: "",
    spans: [],
  },
  {
    what: "curated: three levels of nesting, anchored at the deepest",
    root: ".././",
    text: "(f x)",
    spans: [[0, 5, 0], [1, 4, 0], [1, 2, 1, "Foo.f"]],
    known: { "Foo.f": "Pkg.A" },
  },
];

// --------------------------------------------------------------------- output

type OutCase = {
  what: string;
  root: string;
  known: Record<string, string>;
  refs: Record<string, string>;
  text: string;
  spans: Span[];
  html: string;
  hasAnchor: boolean;
};

const sortedWorld = (world: World): World => ({
  known: Object.fromEntries(Object.entries(world.known).sort(([a], [b]) => (a < b ? -1 : 1))),
  refs: Object.fromEntries(Object.entries(world.refs).sort(([a], [b]) => (a < b ? -1 : 1))),
});

const curatedBranches = new Set<string>();
const curatedOut: OutCase[] = CURATED.map((c) => {
  const world = sortedWorld({ known: c.known ?? {}, refs: c.refs ?? {} });
  zeroSink();
  const out = renderInWorld(c.text, c.spans, c.root, world);
  for (const k of COUNTERS) if (prototype.sink[k] > 0) curatedBranches.add(k);
  return { what: c.what, root: c.root, ...world, text: c.text, spans: c.spans, ...out };
});

const sample: OutCase[] = [];
let reductionFailures = 0;
for (const i of selected) {
  const c = cases[i];
  const world = sortedWorld(reduce(i));
  const again = renderInWorld(c.text, c.spans, c.root, world);
  if (again.html !== html[i] || again.hasAnchor !== hasAnchor[i]) {
    reductionFailures++;
    console.error(`reduction changed the output of ${JSON.stringify(c.what)}`);
    continue;
  }
  sample.push({
    what: c.what,
    root: c.root,
    ...world,
    text: c.text,
    spans: c.spans,
    html: html[i],
    hasAnchor: hasAnchor[i],
  });
}
if (reductionFailures > 0) {
  console.error(`${reductionFailures} cases could not be reduced; the tracing is incomplete`);
  Deno.exit(6);
}

/**
 * The three functions that are per *declaration* rather than per fragment.
 *
 * Every distinct input the corpus has, which is small: `kindDescription` and
 * `cssKind` are keyed by `(kind, modifiers)` and `kind`, and `breakWithin` by
 * the name. The names are deduplicated and then sampled, because there are
 * thousands of them and they differ only in their components.
 */
const kindInputs = new Map<string, { kind: string; modifiers: string[] }>();
const names = new Set<string>();
const privateNames = new Set<string>();
let declCount = 0;
for (const mod of irModules) {
  for (const d of mod.declarations) {
    declCount++;
    kindInputs.set(`${d.kind} ${[...d.modifiers].sort().join(" ")}`, {
      kind: d.kind,
      modifiers: d.modifiers,
    });
    names.add(d.name);
    if (d.name.startsWith("_private.")) privateNames.add(d.name);
    for (const f of d.members) names.add(f.name);
  }
}
for (const c of cases) {
  for (const s of c.spans) {
    if (typeof s[3] === "string" && s[3].startsWith("_private.")) privateNames.add(s[3]);
  }
}

const kinds = [...kindInputs.values()].map((k) => ({
  ...k,
  description: prototype.kindDescription(k.kind, k.modifiers),
  css: prototype.cssKind(k.kind),
}));

const allNames = [...names].sort();
/** An even stride over the names, plus every name with a non-ASCII component. */
const nameSample = (() => {
  const chosen = new Set<string>();
  for (const n of allNames) if (!/^[\x20-\x7e]*$/.test(n)) chosen.add(n);
  const stride = Math.max(1, Math.floor(allNames.length / 120));
  for (let i = 0; i < allNames.length; i += stride) chosen.add(allNames[i]);
  return [...chosen].sort();
})();
const breaks = nameSample.map((name) => ({ name, html: prototype.breakWithin(name) }));

const privates = [...privateNames].sort().map((name) => ({
  name,
  userName: prototype.privateToUserName(name),
  module: prototype.moduleFromPrivatePrefix(name),
}));

const roots = new Set(cases.map((c) => c.root));
const anchorCount = html.reduce((n, h) => n + (h.match(/<a href="/g) ?? []).length, 0);
const coveredFeatures = new Set<string>();
for (const i of selected) for (const f of featuresOf[i]) coveredFeatures.add(f);
const allFeatures = new Set<string>();
for (const fs of featuresOf) for (const f of fs) allFeatures.add(f);

console.error(
  `${cases.length} fragments over ${irModules.length} modules / ${declCount} declarations; ` +
    `${anchorCount} anchors; ${roots.size} distinct roots (${[...roots].sort().join(" ")})`,
);
console.error(
  `features ${coveredFeatures.size}/${allFeatures.size} covered by the sample; ` +
    `sample ${sample.length} real + ${curatedOut.length} curated`,
);
const missing = [...allFeatures].filter((f) => !coveredFeatures.has(f));
if (missing.length > 0) {
  console.error(`uncovered features: ${missing.join(" ")}`);
  Deno.exit(7);
}

const provenance = {
  generatedBy: "crates/lean-doc-render/tests/oracle/gen-fragment-expected.ts",
  oracle: "experiments/stage7d/render.ts Renderer.fragment, sliced out of the frozen prototype, " +
    "with the real IR",
  renderTsRanges: RANGES,
  ir,
  /** The `--full` corpus: every span-carrying fragment in the IR. */
  irFragments: cases.length,
  irModules: irModules.length,
  irDeclarations: declCount,
  irNames: allNames.length,
  knownEntries: known.size,
  anchors: anchorCount,
  anchoredFragments: hasAnchor.filter(Boolean).length,
  roots: [...roots].sort(),
  features: [...allFeatures].sort(),
  branchTotals,
  curatedBranches: [...curatedBranches].sort(),
  deno: Deno.version.deno,
};

if (full) {
  // No maps and no text: the Rust side rebuilds them by walking the same IR in
  // the same order, which is the half of the port the reduced fixture cannot
  // exercise. `what` is what the two sides agree on.
  await Deno.writeTextFile(
    full,
    JSON.stringify({
      ...provenance,
      cases: cases.map((c, i) => ({ what: c.what, html: html[i], hasAnchor: hasAnchor[i] })),
      kinds,
      breaks: allNames.map((name) => ({ name, html: prototype.breakWithin(name) })),
      privates,
    }) + "\n",
  );
  console.error(`${cases.length} fragments -> ${full}`);
}

const fixture = JSON.stringify({
  ...provenance,
  sampleFeatures: [...coveredFeatures].sort(),
  cases: [...curatedOut, ...sample],
  kinds,
  breaks,
  privates,
}) + "\n";

if (check) {
  const committed = await Deno.readTextFile(FIXTURE);
  if (committed !== fixture) {
    console.error(`${FIXTURE.pathname} is not what this script produces`);
    Deno.exit(1);
  }
  console.error(`${FIXTURE.pathname} is current (${curatedOut.length + sample.length} cases)`);
} else {
  await Deno.writeTextFile(FIXTURE, fixture);
  console.error(
    `${curatedOut.length + sample.length} cases -> ${FIXTURE.pathname} ` +
      `(${new TextEncoder().encode(fixture).length} B)`,
  );
}
