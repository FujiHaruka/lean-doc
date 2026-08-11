#!/usr/bin/env -S deno run --allow-read --allow-write
// gen-autolink-expected.ts -- render every docstring of the target package with
// the *prototype's* docstring renderer and its *real* name maps, so that
// `tests/autolink.rs` compares the Rust resolver against the prototype's own
// bytes rather than against a reading of it.
//
// WHY THIS FILE EXISTS
// --------------------
// M1-c's first two steps had a stronger oracle available: doc-gen4 itself,
// through `docStringToHtml`. That oracle cannot see this step. doc-gen4 resolves
// names out of the environment it holds, and `dump-html.lean` runs it with an
// empty `AnalyzerResult` -- which is exactly why the 4,987-case comparison could
// be run at all (every lookup misses, matching `NoLinks`). Turning the lookups
// on would mean rebuilding doc-gen4's whole `AnalyzerResult` in Lean.
//
// So the oracle here is the frozen prototype. That is not a downgrade of
// convenience: `render.ts`'s `nameToLink` and doc-gen4's `nameToLink?` are
// *different functions* (the prototype has no environment, so it uses
// `PRIVATE_PREFIX` matching, `known` + `linkIndex`, and `knownModules` where
// doc-gen4 uses `isPrivateName`, `name2ModIdx` and an eliminator fallback), and
// it is the prototype that scores 439/439. It is fed the real dependency map
// (`--link-index`, 8,508,273 B), the real IR, and each docstring's real page
// context.
//
// HOW IT GETS AT render.ts WITHOUT IMPORTING IT
// ---------------------------------------------
// The same way `tests/gen-ts-expected.ts` and the md crate's oracles do:
// render.ts is a script whose top level parses argv and exits, and it is frozen,
// so function definitions are sliced out by line range, assembled into a module
// and imported through a `data:` URL. A range whose first line stopped saying
// what it should is a hard failure, not a different expectation.
//
// WHAT MAKES THE COMMITTED SAMPLE SELF-CONTAINED
// ---------------------------------------------
// The real maps are 8.5 MB and cannot be committed. Instead `known`, `linkIndex`
// and `knownModules` are handed to the prototype as tracing collections, so the
// *exact* set of names each docstring looks up is recorded; the fixture carries
// the answers for those names and nothing else. Every reduced case is then
// re-rendered and required to produce the same bytes as the full run, so a
// reduction that lost something fails here rather than weakening a test later.
//
// npm/node are broken in this environment; this must run under deno.
//
// usage:
//   deno run --allow-read --allow-write \
//     crates/lean-doc-render/tests/oracle/gen-autolink-expected.ts
//   ... --full /tmp/autolink-full.json   also write every case (no maps: the
//                                        Rust side rebuilds them from the IR)
//   ... --check                          fail if the committed file is stale
//   ... --ir DIR --link-index FILE       point at another corpus

const FIXTURE = new URL("../data/autolink-expected.json", import.meta.url);
const RENDER_TS = new URL("../../../../experiments/stage7d/render.ts", import.meta.url);
const MD_TS_FIXTURE = new URL(
  "../../../lean-doc-md/tests/data/ts-docstring-expected.json",
  import.meta.url,
);

const DEFAULT_IR = "/private/tmp/lean-doc-relay/w7h/base-ir";
const DEFAULT_LINK_INDEX = "/private/tmp/lean-doc-relay/w7c/linkindex/link-index.lidx";

/** How many real docstrings the committed fixture aims for. See `selectCases`. */
const FIXTURE_TARGET = 140;

/**
 * Above this many characters a docstring is not taken for its anchors alone.
 *
 * The corpus's most anchor-dense docstrings are also its longest, and a fixture
 * has to stay in the repository: the whole corpus is reachable with `--full`
 * (`tests/autolink.rs`), so what is committed should be the *variety*, not the
 * volume. Coverage still overrides this — a branch reached by one long
 * docstring and nothing else is kept.
 */
const COMPACT_CHARS = 800;

// ---------------------------------------------------- slicing render.ts

const renderLines = (await Deno.readTextFile(RENDER_TS)).split("\n");

/** Lines `from..to` of render.ts, 1-based and inclusive. */
const slice = (from: number, to: number) => renderLines.slice(from - 1, to).join("\n");

/**
 * Line ranges in render.ts. `head` is text the first line must start with, so
 * that a range which has drifted onto other code is a failure rather than a
 * silently different expectation.
 */
const RANGES = {
  escapeHtml: { from: 331, to: 341, head: "function escapeHtml(" },
  links: { from: 385, to: 393, head: "function moduleLink(" },
  privatePrefix: { from: 415, to: 415, head: "const PRIVATE_PREFIX =" },
  docstrings: { from: 932, to: 1672, head: "function nameToLink(" },
  // The one input that is not a whole function: `pageHtml` builds the page's
  // `DocCtx` inline, and its `moduleDeclNames` entry is the list the last branch
  // of `nameToLink` scans. The expression is lifted out below.
  pageCtxDeclNames: { from: 1912, to: 1916, head: "    moduleDeclNames: mod.declarations" },
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
 * `pageHtml`'s `moduleDeclNames` expression, lifted into a function.
 *
 * Taken from render.ts rather than rewritten here because it is an *input* to
 * both sides: a rewrite that was wrong in the same way for the prototype and
 * for the port would make the two agree about the wrong answer. (The reference
 * pages of `tests/ref_pages.rs` are the independent check on this, since they
 * came out of the whole of render.ts.)
 */
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
 * `known`, `linkIndex` and the per-page `knownModules` are the three maps
 * `nameToLink` reads (`render.ts:2001-2085`). They are tracing collections here:
 * every key the prototype asks about is recorded, which is what lets the
 * committed fixture carry a reduced map that is provably enough.
 *
 * `abl` (the CommonMark ablation switches, all off -- the production
 * configuration, `render.ts:304-312`), `pageStats` and `T` are counters and
 * switches, not behaviour.
 */
const MODULE_SOURCE = [
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
  "const pageStats = { autolinkAttempts: 0, autolinkResolved: 0 };",
  "const T = { docstring: 0 };",
  at("privatePrefix"),
  at("escapeHtml"),
  at("links"),
  at("docstrings"),
  `export const moduleDeclNamesOf = (mod: any): string[] => (${declNamesExpr});`,
  "export { renderDocString, moduleLink, pageRoot };",
].join("\n\n");

type Ctx = { root: string; moduleDeclNames: string[]; knownModules: Set<string> };

const prototype = await import(
  `data:text/typescript;charset=utf-8,${encodeURIComponent(MODULE_SOURCE)}`
) as {
  renderDocString(md: string, ctx: Ctx): string;
  moduleLink(root: string, module: string): string;
  pageRoot(module: string): string;
  // deno-lint-ignore no-explicit-any
  moduleDeclNamesOf(mod: any): string[];
  known: Map<string, string>;
  linkIndex: Map<string, string>;
  TracingSet: new (values?: Iterable<string>) => Set<string>;
  traced: Set<string>;
  setTracing(on: boolean): void;
};

// -------------------------------------------------------------------- inputs

const args = Deno.args;
const flag = (name: string, fallback: string | null = null) => {
  const i = args.indexOf(name);
  return i >= 0 && i + 1 < args.length ? args[i + 1] : fallback;
};
const ir = flag("--ir", DEFAULT_IR)!;
const linkIndexPath = flag("--link-index", DEFAULT_LINK_INDEX)!;
const full = flag("--full");
const check = args.includes("--check");

// deno-lint-ignore no-explicit-any
const readJson = async (path: string): Promise<any> => JSON.parse(await Deno.readTextFile(path));

const index = await readJson(`${ir}/index.json`);

/**
 * `render.ts:2001-2053`, in the order it does it: dependency slices first, then
 * every declaration of this package (overwriting), then every reference the
 * extractor resolved (filling gaps only).
 */
for (const dep of index.dependencyMaps) {
  const map = await readJson(`${ir}/${dep.file}`);
  for (const [n, m] of Object.entries(map.declarations as Record<string, string>)) {
    prototype.known.set(n, m);
  }
}
// deno-lint-ignore no-explicit-any
const irModules: any[] = [];
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

console.error(
  `known ${prototype.known.size} / linkIndex ${prototype.linkIndex.size} / ` +
    `knownModules ${fullKnownModules.size} (${irModuleNames.size} from the IR, ` +
    `${lidxModuleNames.size} from the .lidx)`,
);

// -------------------------------------------------------------------- corpus

type Case = { what: string; module: string; root: string; declNames: string[]; md: string };

const declNamesCache = new Map<string, string[]>();
// deno-lint-ignore no-explicit-any
const declNamesOf = (mod: any): string[] => {
  let names = declNamesCache.get(mod.module);
  if (names === undefined) {
    names = prototype.moduleDeclNamesOf(mod);
    declNamesCache.set(mod.module, names);
  }
  return names;
};

/**
 * Every docstring in the IR, deduplicated by text, each carrying the page
 * context of the module it was first seen in.
 *
 * The labels are the ones `crates/lean-doc-md/tests/oracle/gen-docgen4-expected.ts`
 * uses, so that the list of inputs where the prototype and doc-gen4 disagree
 * (recorded there) names cases in this corpus too.
 */
const seen = new Set<string>();
const real: Case[] = [];
for (const mod of irModules) {
  const context = {
    module: mod.module as string,
    root: prototype.pageRoot(mod.module),
    declNames: declNamesOf(mod),
  };
  const add = (what: string, md: unknown) => {
    if (typeof md !== "string" || seen.has(md)) return;
    seen.add(md);
    real.push({ what, md, ...context });
  };
  for (const [i, doc] of mod.moduleDocs.entries()) add(`${mod.module} module doc ${i}`, doc.text);
  for (const decl of mod.declarations) {
    add(`${decl.name}`, decl.doc);
    for (const member of decl.members ?? []) add(`${decl.name}.${member.name}`, member.doc);
  }
  for (const tactic of mod.tactics ?? []) add(`tactic ${tactic.userName}`, tactic.docString);
}
console.error(`${real.length} distinct docstrings`);

// ------------------------------------------------------------------ rendering

const countAnchors = (html: string) => (html.match(/<a href="/g) ?? []).length;

/**
 * Whether an anchor sits inside another one.
 *
 * This is the one place where the prototype and doc-gen4 disagree about
 * *resolution* rather than about CommonMark: `renderText` carries an `inLink`
 * flag and renders a code span inside an `<a>` as plain text
 * (`DocString.lean:264`), while `renderInline` auto-links every code span
 * unconditionally (`render.ts:1622`). With no map the two are
 * indistinguishable, which is why M1-c's earlier doc-gen4 comparison could not
 * see it. The Rust port follows doc-gen4 (plan §5), so cases that show this are
 * recorded rather than compared -- `tests/docgen4_linked.rs` carries the
 * positive expectation.
 *
 * **No docstring of the target package produces one** 【実測: 0 of 4,858】, so
 * this does not move a byte of gate A. It is reached by a hand-written case.
 */
function nestsAnchors(html: string): boolean {
  let depth = 0;
  for (const m of html.matchAll(/<a\s|<\/a>/g)) {
    if (m[0] === "</a>") depth--;
    else if (++depth > 1) return true;
  }
  return false;
}

/** Renders one case with the full maps, recording what it looked up. */
function renderReal(c: Case): { html: string; traced: Set<string> } {
  prototype.traced.clear();
  prototype.setTracing(true);
  const html = prototype.renderDocString(c.md, {
    root: c.root,
    moduleDeclNames: c.declNames,
    knownModules: fullKnownModules,
  });
  prototype.setTracing(false);
  return { html, traced: new Set(prototype.traced) };
}

const rendered = real.map(renderReal);

// ------------------------------------------------------------------ reduction

/** The three sources of a case's world, in the shape the fixture stores them. */
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
 * them from a correct one. So each module name the case asked about is put back
 * into a source that supplies it -- the IR's own names, the `.lidx`'s `@`
 * section, or, when it is reachable only as a value of `known`, a carrier entry
 * whose value it is. A carrier's key contains a space, so it is not a name
 * literal and can never be looked up.
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
 * Renders inside a world, the way the Rust fixture test will: the global maps
 * are emptied first, so nothing outside the world can answer.
 *
 * Every call after this point runs here, which is why the full maps are only
 * needed until [`reduce`] has run.
 */
function renderInWorld(md: string, root: string, declNames: string[], world: World): string {
  prototype.known.clear();
  prototype.linkIndex.clear();
  for (const [n, m] of Object.entries(world.known)) prototype.known.set(n, m);
  for (const [n, m] of Object.entries(world.lidxEntries)) prototype.linkIndex.set(n, m);
  const knownModules = new Set<string>(world.irModules);
  for (const m of prototype.known.values()) knownModules.add(m);
  for (const m of world.lidxModules) knownModules.add(m);
  return prototype.renderDocString(md, { root, moduleDeclNames: declNames, knownModules });
}

// ------------------------------------------------------------------ selection

/** Which resolution branches a case's output shows, read back out of the HTML. */
function features(c: Case, html: string): Set<string> {
  const out = new Set<string>();
  for (const m of html.matchAll(/<a href="([^"]*)">([^<]*)<\/a>/g)) {
    const href = m[1];
    const text = m[2].replaceAll("&lt;", "<").replaceAll("&gt;", ">")
      .replaceAll("&quot;", '"').replaceAll("&amp;", "&");
    if (href.includes("find/?pattern=")) out.add("find-fallback");
    else if (!href.includes("#")) out.add(text.endsWith(".lean") ? "source-path" : "module");
    else {
      const fragment = href.slice(href.indexOf("#") + 1);
      if (fragment !== text) out.add("suffix-scan");
      else if (prototype.known.has(text)) out.add("known");
      else out.add("lidx");
    }
    if (/[\u{10000}-\u{10FFFF}]/u.test(text)) out.add("astral-name");
    if (/['!?]/.test(text)) out.add("punctuated-name");
    if (text.includes("«")) out.add("guillemet-name");
    if (text.includes(".")) out.add("qualified-name");
  }
  if (html.includes("<code><a")) out.add("anchor-opens-code");
  if (/<pre><code[^>]*><a/.test(html)) out.add("anchor-in-block");
  if (c.declNames.length === 0) out.add("no-decl-names");
  return out;
}

/**
 * Which cases the committed fixture keeps: a greedy cover of the branches
 * above, then the cases with the most anchors (where a subtly wrong resolver
 * has the most chances to show it), then an even stride over the rest so that
 * docstrings which resolve *nothing* are represented too.
 */
function selectCases(): number[] {
  const chosen = new Set<number>();
  const covered = new Set<string>();
  const featuresOf = real.map((c, i) => features(c, rendered[i].html));
  for (;;) {
    let best = -1;
    let bestGain = 0;
    for (let i = 0; i < real.length; i++) {
      if (chosen.has(i)) continue;
      let gain = 0;
      for (const f of featuresOf[i]) if (!covered.has(f)) gain++;
      // Ties go to the shorter docstring: same branches, fewer committed bytes.
      if (gain > bestGain || (gain === bestGain && gain > 0 && real[i].md.length < real[best].md.length)) {
        best = i;
        bestGain = gain;
      }
    }
    if (best < 0) break;
    chosen.add(best);
    for (const f of featuresOf[best]) covered.add(f);
  }
  const compact = (i: number) => real[i].md.length <= COMPACT_CHARS;
  const byAnchors = real
    .map((_, i) => i)
    .filter((i) => !chosen.has(i) && compact(i) && countAnchors(rendered[i].html) > 0)
    .sort((a, b) => countAnchors(rendered[b].html) - countAnchors(rendered[a].html));
  for (const i of byAnchors.slice(0, Math.floor(FIXTURE_TARGET * 0.6))) chosen.add(i);

  const rest = real.map((_, i) => i).filter((i) => !chosen.has(i) && compact(i));
  const want = Math.max(0, FIXTURE_TARGET - chosen.size);
  if (want > 0 && rest.length > 0) {
    const stride = Math.max(1, Math.floor(rest.length / want));
    for (let i = 0; i < rest.length; i += stride) chosen.add(rest[i]);
  }
  return [...chosen].sort((a, b) => a - b);
}

const selected = selectCases();
const worlds = new Map<number, World>(selected.map((i) => [i, reduce(rendered[i].traced)]));

// The full maps are emptied by `renderInWorld` below; their sizes are part of
// what the fixture records, so they are read here for the last time.
const fullKnownEntries = prototype.known.size;
const fullLinkIndexEntries = prototype.linkIndex.size;

// --------------------------------------------------------------------- curated

/**
 * Hand-written cases with hand-written worlds.
 *
 * The corpus is real data, so it covers what the target package happens to
 * contain. These cover what it does not: the identifier characters Lean allows
 * and an ASCII notion of "name" does not (`'`, `!`, `?`), the letter-like range
 * **above the BMP**, guillemet and numeric components, private names, the empty
 * piece between two separators, each source of `knownModules` on its own, and
 * the order the four branches are tried in.
 */
type Curated = { what: string; root: string; declNames?: string[]; md: string } & Partial<World>;

const CURATED: Curated[] = [
  {
    what: "curated: apostrophe, bang and question mark are identifier characters",
    root: "./",
    known: { "Foo.bar'": "Pkg.A", "Foo.baz!": "Pkg.A", "Foo.qux?": "Pkg.A" },
    md: "`Foo.bar' Foo.baz! Foo.qux?`\n",
  },
  {
    what: "curated: letter-like above the BMP",
    root: "./",
    known: { "\u{1d49c}": "Pkg.A", "Foo.\u{1d59f}'": "Pkg.A", "Nat.\u{1d4b7}₁": "Pkg.A" },
    md: "`\u{1d49c} Foo.\u{1d59f}' Nat.\u{1d4b7}₁`\n",
  },
  {
    what: "curated: the code points either side of the astral range",
    root: "./",
    known: { "\u{1d49b}": "Pkg.A", "\u{1d5a0}": "Pkg.A", "\u{1d49c}": "Pkg.A" },
    md: "`\u{1d49b} \u{1d5a0} \u{1d49c}`\n",
  },
  {
    what: "curated: greek, letter-like and the three that are not",
    root: "./",
    known: { "α": "Pkg.A", "λ": "Pkg.A", "Π": "Pkg.A", "Σ": "Pkg.A", "Ω": "Pkg.A", "ℕ": "Pkg.A" },
    md: "`α λ Π Σ Ω ℕ`\n",
  },
  {
    what: "curated: subscripts inside an identifier but not starting one",
    root: "./",
    known: { "x₁": "Pkg.A", "₁x": "Pkg.A" },
    md: "`x₁ ₁x`\n",
  },
  {
    what: "curated: guillemet components",
    root: "./",
    known: { "«a b».c": "Pkg.A", "«»": "Pkg.A", "«a": "Pkg.A" },
    md: "`«a b».c «» «a`\n",
  },
  {
    what: "curated: numeric components",
    root: "./",
    known: { "Foo.1": "Pkg.A", "1Foo": "Pkg.A", "1": "Pkg.A" },
    md: "`Foo.1 1Foo 1`\n",
  },
  {
    what: "curated: what is not a name literal",
    root: "./",
    known: { "a.": "Pkg.A", ".a": "Pkg.A", "a..b": "Pkg.A", "a-b": "Pkg.A" },
    md: "`a. .a a..b a-b`\n",
  },
  {
    what: "curated: consecutive separators leave empty pieces",
    root: "./",
    known: { "": "Pkg.A", "Foo.a": "Pkg.A" },
    irModules: [""],
    md: "`  Foo.a \t  `\n",
  },
  {
    what: "curated: a private name is not looked up",
    root: "./",
    known: { "_private.Pkg.A.hidden": "Pkg.A", "Pkg.A.shown": "Pkg.A" },
    md: "`_private.Pkg.A.hidden Pkg.A.shown`\n",
  },
  {
    what: "curated: a private name can still be a module or match this page",
    root: "./",
    known: { "Pkg.A.hidden": "Pkg.A" },
    irModules: ["_private.Pkg.A"],
    declNames: ["Pkg.A.hidden"],
    md: "`_private.Pkg.A _private.Pkg.A.hidden`\n",
  },
  {
    what: "curated: knownModules from the IR's own module names",
    root: "./",
    irModules: ["Pkg.OnlyIr"],
    md: "`Pkg.OnlyIr`\n",
  },
  {
    what: "curated: knownModules from a value in known",
    root: "./",
    known: { "Pkg.thing": "Pkg.OnlyValue" },
    md: "`Pkg.OnlyValue`\n",
  },
  {
    what: "curated: knownModules from the .lidx @ section",
    root: "./",
    lidxModules: ["Pkg.OnlyLidx"],
    md: "`Pkg.OnlyLidx`\n",
  },
  {
    what: "curated: known is consulted before the .lidx",
    root: "./",
    known: { "Foo.both": "Pkg.FromKnown" },
    lidxEntries: { "Foo.both": "Pkg.FromLidx", "Foo.lidxOnly": "Pkg.FromLidx" },
    md: "`Foo.both Foo.lidxOnly`\n",
  },
  {
    what: "curated: a name that is both a declaration and a module takes the declaration",
    root: "./",
    known: { "Pkg.Both": "Pkg.Owner" },
    irModules: ["Pkg.Both"],
    md: "`Pkg.Both`\n",
  },
  {
    what: "curated: the suffix scan takes the first match in page order",
    root: "./",
    known: { "Pkg.A.foo.bar": "Pkg.A", "Pkg.A.baz.bar": "Pkg.A" },
    declNames: ["Pkg.A.foo.bar", "Pkg.A.baz.bar"],
    md: "`bar`\n",
  },
  {
    what: "curated: the suffix scan compares whole components from the end",
    root: "./",
    known: { "Pkg.A.foobar": "Pkg.A" },
    declNames: ["Pkg.A.foobar"],
    md: "`bar foobar A.foobar X.A.foobar`\n",
  },
  {
    what: "curated: the suffix scan is the last resort, not the first",
    root: "./",
    known: { "bar": "Pkg.Elsewhere", "Pkg.A.bar": "Pkg.A" },
    declNames: ["Pkg.A.bar"],
    md: "`bar`\n",
  },
  {
    what: "curated: the fallback to the tail after the last dot",
    root: "./",
    known: { "succ": "Pkg.A" },
    md: "`Nat.succ Nat. .succ`\n",
  },
  {
    what: "curated: a source path is resolved without any map",
    root: "../.././",
    md: "`Foo/Bar.lean Foo.lean Foo/Bar.leanx`\n",
  },
  {
    what: "curated: a name search that resolves and one that does not",
    root: ".././",
    known: { "Pkg.A.here": "Pkg.A" },
    md: "[a](##Pkg.A.here) [b](##Pkg.A.gone)\n",
  },
  {
    what: "curated: a name search resolving to a module and to a source path",
    root: ".././",
    irModules: ["Pkg.A"],
    md: "[a](##Pkg.A) [b](##Foo/Bar.lean)\n",
  },
  {
    what: "curated: auto-linked in a lean code block, and not in another language",
    root: ".././",
    known: { "Pkg.A.f": "Pkg.A" },
    md: "```lean\nPkg.A.f x\n```\n\n```\nPkg.A.f x\n```\n\n```python\nPkg.A.f x\n```\n",
  },
  {
    what: "curated: no anchor inside an anchor",
    root: ".././",
    known: { "Pkg.A.f": "Pkg.A" },
    md: "[`Pkg.A.f`](x) and `Pkg.A.f`\n",
  },
  {
    what: "curated: the link target is escaped",
    root: "./",
    known: { "Pkg.A.f": "Pkg.A&B<C>\"D" },
    md: "`Pkg.A.f`\n",
  },
  {
    what: "curated: the root reaches the anchor, the fallback and a plain link",
    root: "../.././",
    known: { "Pkg.A.f": "Pkg.A" },
    md: "`Pkg.A.f` [x](##Pkg.A.gone) [y](z.html)\n",
  },
  {
    what: "curated: a module name with one component",
    root: "./",
    irModules: ["Pkg"],
    known: { "Pkg.f": "Pkg" },
    md: "`Pkg Pkg.f`\n",
  },
  {
    what: "curated: separators other than the space",
    root: "./",
    known: { "a": "Pkg.A", "b": "Pkg.A", "c": "Pkg.A", "d": "Pkg.A" },
    md: "`a b c‍d`\n",
  },
];

type OutCase = {
  what: string;
  root: string;
  declNames: string[];
  known: Record<string, string>;
  lidx: string;
  irModules: string[];
  md: string;
  html: string;
};

const toOut = (
  what: string,
  root: string,
  declNames: string[],
  world: World,
  md: string,
  html: string,
): OutCase => ({
  what,
  root,
  declNames,
  known: world.known,
  lidx: lidxText(world),
  irModules: world.irModules,
  md,
  html,
});

// Everything below runs inside a reduced world, so the full maps have to have
// been read for the last time above.
const curatedOut: OutCase[] = CURATED.map((c) => {
  const world: World = {
    known: c.known ?? {},
    lidxEntries: c.lidxEntries ?? {},
    irModules: c.irModules ?? [],
    lidxModules: c.lidxModules ?? [],
  };
  const declNames = c.declNames ?? [];
  return toOut(
    c.what,
    c.root,
    declNames,
    world,
    c.md,
    renderInWorld(c.md, c.root, declNames, world),
  );
});

const sample: OutCase[] = [];
let reductionFailures = 0;
for (const i of selected) {
  const c = real[i];
  const world = worlds.get(i)!;
  const again = renderInWorld(c.md, c.root, c.declNames, world);
  if (again !== rendered[i].html) {
    reductionFailures++;
    console.error(`reduction changed the output of ${JSON.stringify(c.what)}`);
    continue;
  }
  sample.push(toOut(c.what, c.root, c.declNames, world, c.md, rendered[i].html));
}
if (reductionFailures > 0) {
  console.error(`${reductionFailures} cases could not be reduced; the tracing is incomplete`);
  Deno.exit(6);
}

// --------------------------------------------------------------------- output

/**
 * The docstrings on which the prototype and doc-gen4 disagree, taken from the
 * md crate's fixture rather than restated. They are the inputs where a byte
 * difference against this oracle would be the *subset's* limit rather than a
 * fault of the port, and the Rust side has to be told which they are.
 */
const mdFixture = await readJson(MD_TS_FIXTURE.pathname);
const prototypeDiffers: string[] = mdFixture.corpus?.realWhat ?? [];

const totalAnchors = rendered.reduce((n, r) => n + countAnchors(r.html), 0);
const casesWithAnchors = rendered.filter((r) => countAnchors(r.html) > 0).length;
const realNesting = real.filter((_, i) => nestsAnchors(rendered[i].html)).map((c) => c.what);
const prototypeNestsAnchors = [
  ...realNesting,
  ...curatedOut.filter((c) => nestsAnchors(c.html)).map((c) => c.what),
];
console.error(
  `prototype nests anchors on ${prototypeNestsAnchors.length} cases ` +
    `(${realNesting.length} of them real docstrings)`,
);
console.error(
  `${totalAnchors} anchors over ${casesWithAnchors} of ${real.length} docstrings; ` +
    `sample ${sample.length} real + ${curatedOut.length} curated`,
);

const provenance = {
  generatedBy: "crates/lean-doc-render/tests/oracle/gen-autolink-expected.ts",
  oracle: "experiments/stage7d/render.ts renderDocString, sliced out of the frozen prototype, " +
    "with the real IR and the real --link-index",
  renderTsRanges: RANGES,
  ir,
  linkIndexPath,
  irDocstrings: real.length,
  knownEntries: fullKnownEntries,
  linkIndexEntries: fullLinkIndexEntries,
  knownModules: fullKnownModules.size,
  irModuleNames: irModuleNames.size,
  lidxModuleNames: lidxModuleNames.size,
  totalAnchors,
  casesWithAnchors,
  prototypeDiffers,
  // The `inLink` divergence; see `nestsAnchors`. Empty over the real corpus.
  prototypeNestsAnchors,
  realDocstringsNestingAnchors: realNesting.length,
  deno: Deno.version.deno,
};

if (full) {
  // No maps: the Rust side rebuilds them from the same IR and `.lidx`, which is
  // the half of the port the reduced fixture cannot exercise.
  await Deno.writeTextFile(
    full,
    JSON.stringify({
      ...provenance,
      cases: real.map((c, i) => ({
        what: c.what,
        module: c.module,
        root: c.root,
        md: c.md,
        html: rendered[i].html,
      })),
    }) + "\n",
  );
  console.error(`${real.length} cases -> ${full}`);
}

const fixture = JSON.stringify({ ...provenance, cases: [...curatedOut, ...sample] }) + "\n";

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
