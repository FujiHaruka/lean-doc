#!/usr/bin/env -S deno run --allow-read --allow-write
//
// Stage 7c: rebuild doc-gen4's HTML from the **schema-4** IR without starting
// Lean. Copied from `experiments/stage7b/render.ts` (same IR, same extractor)
// and changed only on the generating side, in the two places stage 7b left open:
//
//   * the **autolink index**. `nameToLink?` resolves a docstring token against
//     doc-gen4's whole environment (`name2ModIdx`); the IR carries only what the
//     signatures referred to. `--link-index` supplies the missing map, derived
//     from a doc-gen4 site's `declarations/declaration-data.bmp` by
//     `build-link-index.ts` (see that file for what is assumed and what is not).
//   * **CommonMark**. `renderDocString` was a placeholder that claimed no
//     agreement. It is now a block parser with the five constructs the 47
//     mismatching regions of stage 7b needed: indented code blocks, list
//     tightness (`<li><p>`), indent-based nesting, lazy continuation, and the
//     rule that an ordered list interrupts a paragraph only when it starts at 1.
//
// Stage 7a / 7b are left as they were: their 96.506% / 97.099% and their timings
// are the committed baselines.
//
// Increment 3 (`--out`) produces `div.decl_header` only. Increment 4 (`--pages`)
// produces the whole module page. Both are the generator half of an acceptance
// test; `compare.ts` / `bytes.ts` / `coverage.ts` are the scoring halves. The
// halves are deliberately separate programs: this one never reads doc-gen4's
// output, it only reads the IR and the specification transcribed from doc-gen4's
// own source.
//
// usage:
//   render.ts --ir <dir> [--out <path.jsonl>] [--pages <dir>]
//             [--source-url <base>] [--only <Module>] [--stats <path>]
//
//   --ir      schema-4 IR root (experiments/stage7b/run.sh --write-ir --tagged-code)
//   --out     JSONL, one rendered declaration per line: {module, name, html}
//             (increment 3's output; unchanged)
//   --pages   write one full `<Module>/<path>.html` per IR module under this
//             directory (increment 4)
//   --source-url  the repository/revision prefix `gh_link` needs, e.g.
//             `https://github.com/owner/repo/blob/<rev>`. **This is
//             configuration, not IR**: doc-gen4 gets it from lake + git, the
//             extractor never saw it. Required by `--pages`.
//   --only    render just this module (repeatable), for the small loop
//   --stats   also write the counter block here
//   --limit N render only the first N modules of the index (increment 4's
//             timing task, for the linearity check: a timer that does not move
//             with the input is a timer that returns 0). Reading is limited
//             too, so every phase scales. Links still resolve, because `known`
//             is filled from `refs` as well as from the module list.
//   --timings <path>  write the phase timers as JSON. The timers themselves are
//             always on -- they are ~1,300 `performance.now()` calls, which is
//             microseconds -- so a timed run and an untimed run take the same
//             code path and produce the same bytes.
//   --no-flatten-probe  turn off the rope-flattening probe (see TIMERS below).
//   --link-index <path>  the `.lidx` written by `build-link-index.ts`: the
//             dependency closure's name -> module map, which is what
//             `nameToLink?` needs and the IR does not have. Optional; without it
//             the renderer resolves only what `deps/` and the signatures'
//             `refs` cover, which is stage 7b's behaviour.
//
//   Stage 4c's `--ws-heuristic` is **gone**, not renamed: it existed to measure
//   how much of the schema-2 gap was guessable, and guessing is what schema 3
//   removes. There is no flag here that trades faithfulness for score.
//
// The `stats` counters below are split in two on purpose: `stats` counts only
// what the `decl_header` path does, so a `--pages` run leaves increment 3's
// numbers untouched, and `pageStats` counts everything the page adds.
//
// TIMERS (increment 4, second half -- the judgement-point-2 timing)
//   Five phases, all inside this process:
//     preMain   `performance.timeOrigin` -> the first line of this module.
//               **This is NOT Deno's start-up**: measured against an empty
//               script it comes out at 0.002 s while the empty script's wall
//               clock is 0.027 s, so `timeOrigin` is set well after `exec`.
//               The start-up floor is the empty script's wall clock, measured
//               from outside (`time-render.sh --empty`); this timer only says
//               how much of it is this file's own module initialisation.
//     readIr    `Deno.readTextFile` + `JSON.parse` of index.json, deps/*.json
//               and modules/*.json. Comparable with `benchmarks/tools/read-ir.ts`
//               (schema-2 median 0.100 s) -- same files, same parser, a little
//               less walking.
//     indexBuild  the name map, the suppressed set and the module set.
//     render    `declHeader` for every declaration plus `pageHtml` per module.
//     write     `Deno.mkdir` + `Deno.writeTextFile` per module page.
//
//   THE ROPE TRAP. `pageHtml` builds its result with `+=` and a final
//   concatenation, so V8 hands back a **cons string**: the characters have not
//   been copied anywhere yet. `Deno.writeTextFile` is what forces the copy, so
//   without a probe the flattening cost would be billed to `write` and `render`
//   would look faster than it is. `flattenProbe` does an `indexOf` on the page,
//   which requires flat contents, and is timed on its own line. This is the same
//   class of mistake as the 0 us timers in stage 4 increment 2 -- a consumer of
//   the value sitting outside the timer -- so it is measured, not assumed:
//   `--no-flatten-probe` moves the cost to `write` and leaves the total alone.
//
// THE splitWhitespaces GAP -- closed by schema 3
//   `renderTagged` (RenderedCode.lean:249-256) pushes a `.const` tag's leading
//   and trailing whitespace outside the anchor via `splitWhitespaces`
//   (Base.lean:281-288), which **rebuilds it as spaces** (`"".pushn ' ' n`). So
//   a token whose tag text was `" ≤\n  "` renders as `" ≤   "`: same character
//   count, so no offset moves, but different characters. Schema 2 stored the
//   pretty printer's original text plus the *narrowed* span, so the tag's full
//   extent -- the only thing that says which whitespace was inside the tag --
//   was not recoverable from it. Schema 3 stores the two widths the extractor
//   already computed and threw away, and `applyWsWidths` replays the rewrite.
//
// SPECIFICATION (transcribed from doc-gen4 v4.31.0, read-only):
//   DocGen4/Output/Module.lean       docInfoHeader, structureInfoHeader
//   DocGen4/Output/Arg.lean          argToHtml
//   DocGen4/Output/Base.lean         renderedCodeToHtmlAux, declNameToLink,
//                                    moduleNameToLink, breakWithin,
//                                    declNameToHtmlBreakWithinLink,
//                                    findLinkableParent
//   DocGen4/Output/ToHtmlFormat.lean Html.toStringAux, Html.escape
//   DocGen4/Process/DocInfo.lean     getKindDescription, ofConstant (render flag)
//   DocGen4/Output.lean:150          depthToRoot = module.components.dropLast.length
//
// PAGE SPECIFICATION (increment 4, same tree, read-only):
//   DocGen4/Output/Template.lean     baseHtmlGenerator (head / header / frame)
//   DocGen4/Output/Base.lean         baseHtmlHeadDeclarations, moduleNameToLink,
//                                    getSourceUrl, breakWithin
//   DocGen4/Output/Module.lean       moduleToHtml, docInfoToHtml, internalNav,
//                                    importsHtml, declarationToNavLink, modDocToHtml
//   DocGen4/Output/SourceLinker.lean mkGithubSourceLinker (`#L{a}-L{b}`)
//   DocGen4/Output/Definition.lean   equationsToHtml
//   DocGen4/Output/Structure.lean    structureToHtml, fieldToHtml
//   DocGen4/Output/Inductive.lean    instancesForToHtml
//   DocGen4/Output/Class.lean        classInstancesToHtml
//   DocGen4/Output/DocString.lean    docStringToHtml (CommonMark -- NOT reproduced,
//                                    see "Docstrings" below)
//   DocGen4/Process/Analyze.lean:158 module members = modDocs ++ docInfos, sorted
//                                    by declaration-range start
//   DocGen4/Process/Base.lean:119    equationLimit = 200
//   DocGen4/DB.lean:162/289          imports are de-duplicated by the DB;
//                                    an equation whose *text length* (code
//                                    points) is >= 200 is stored as NULL and
//                                    flips `equationsWereOmitted`
//
// DOCSTRINGS
//   doc-gen4 renders docstrings with MD4Lean = md4c (C), with the flag set
//   `MD_DIALECT_GITHUB ||| MD_FLAG_LATEXMATHSPANS ||| MD_FLAG_NOHTML`
//   (`Output/DocString.lean:393`) -- GitHub dialect, `$math$` spans, raw HTML
//   disabled. Reproducing md4c in full is not the goal and is not claimed here;
//   what is implemented is the subset this target exercises, driven by the
//   regions `coverage.ts` scored as mismatching. Anything outside that subset
//   (tables, setext headings, reference links, hard breaks, entities) is still
//   approximate or absent, and `coverage.ts` will say so the moment a target
//   uses it -- a docstring counts as reproduced only when it is byte-identical.
//
//   No external dependency is used. This repository has no TS manifest and
//   `docs/plans/three-axes.md` pre-decision 5 forbids network access; the npm /
//   jsr CommonMark implementations do not agree with md4c byte for byte anyway.
//
// The one part of `Html.toStringAux` that matters for plaintext offsets is the
// `flatten` (second) argument of `Html.element`: JSX literals build it `true`
// (no newlines emitted), and the two `Html.element ... false` calls in the path
// -- `span.decl_kind` and `argToHtml`'s `span.decl_args` -- emit `\n` after the
// open tag (unless the only child is text/raw) and after the close tag. Those
// newlines are part of the header's plaintext, so they are part of every anchor
// offset after them.

/**
 * Schema 3. The two extra numbers on a `kind 1` span are the `splitWhitespaces`
 * widths: `front` code units immediately before `start`, `back` immediately
 * after `stop`, both of which doc-gen4 re-emits as plain spaces. They are
 * written only when at least one is non-zero, so the 4-element form still
 * occurs and means `front = back = 0`.
 */
type Span =
  | [number, number, number]
  | [number, number, 1, string]
  | [number, number, 1, string, number, number];

/**
 * Schema 4 adds five keys to a `label: "field"` member: the binders of the
 * field's own signature (`binders` / `implicits` / `binderCode`, printed by
 * `argToHtml` inside `div.structure_field_info`), the field docstring (`doc`),
 * and its origin (`isDirect`, which selects the whole other branch of
 * `fieldToHtml`). `ctor` and `parent` members do not carry them.
 */
type Member = {
  label: string;
  name: string;
  text: string;
  code: Span[];
  binders?: string[];
  implicits?: boolean[];
  binderCode?: Span[][];
  doc?: string | null;
  isDirect?: boolean;
};

type ModuleDoc = { line: number; col: number; text: string };

type Decl = {
  name: string;
  kind: string;
  modifiers: string[];
  binders: string[];
  implicits: boolean[];
  binderCode: Span[][];
  type: string;
  typeCode: Span[];
  line: number;
  col: number;
  endLine: number;
  endCol: number;
  index: number;
  members: Member[];
  refs: [string, string][];
  doc?: string;
  equations: string[];
  equationCode: Span[][];
  /** Schema 4. Absent when the declaration has none -- see `declToIrJson`. */
  attrs?: string[];
  /** Schema 4, instances only. Not printed on a module page: the browser fills
   *  the instance lists from `declaration-data.bmp`. Read here only so that
   *  "the IR carries what doc-gen4 computes" can be checked. */
  instClass?: string;
  instTypes?: string[];
};

type ModuleFile = {
  module: string;
  declarations: Decl[];
  schemaVersion: number;
  imports: string[];
  moduleDocs: ModuleDoc[];
};

/** `bytes` is the writer's `String.utf8ByteSize`; read only for the timing
 *  report's throughput line, never to size a buffer. */
type IndexEntry = { module: string; file: string; bytes: number };
type DepEntry = { file: string; bytes: number };
type Index = {
  schemaVersion: number;
  generator: string;
  modules: IndexEntry[];
  dependencyMaps: DepEntry[];
  /** Present only when the extractor was run with an ablation flag. Such an IR
   *  is deliberately incomplete and must not be rendered. */
  ablations?: string[];
};

// ---------------------------------------------------------------- CLI

/** Time from `performance.timeOrigin` to here. Deno sets `timeOrigin` partway
 *  through its own initialisation, **not** at `exec`, so this is the tail of
 *  start-up plus this module's initialisation -- not the start-up floor. The
 *  floor is measured from outside, by timing an empty script. */
const T_PRE_MAIN = performance.now();

/** Phase accumulators, in milliseconds. See "TIMERS" at the top. */
const T = {
  preMain: T_PRE_MAIN,
  readIr: 0,
  indexBuild: 0,
  renderHeaders: 0,
  renderPage: 0,
  flatten: 0,
  write: 0,
  total: 0,
  /** A slice of `renderPage`, not a phase of its own: how much of the page
   *  render is the docstring renderer. Recorded because the docstring path is
   *  the biggest **unimplemented** piece (CommonMark + the autolink index), so
   *  any statement about how far the measured time could grow needs its size. */
  docstring: 0,
  /** Reading and indexing `--link-index`. A phase of its own, not a slice: it is
   *  work the stage-7b renderer did not do at all, and §8's "how do we ship the
   *  dependency map" needs its price separated from everything else. */
  linkIndex: 0,
};

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
const OUT = opt("--out");
const PAGES = opt("--pages");
const SOURCE_URL = opt("--source-url").replace(/\/+$/, "");
const ONLY = opts("--only");
const STATS = opt("--stats");
const LIMIT = opt("--limit") ? Number(opt("--limit")) : 0;
const TIMINGS = opt("--timings");
const LINK_INDEX = opt("--link-index");
const FLATTEN_PROBE = !argv.includes("--no-flatten-probe");

/**
 * CommonMark ablations. Each one removes exactly one rule of the block parser so
 * that its share of the 47 regions stage 7b could not reproduce can be measured
 * the way stage 7b measured its three extraction additions -- by taking the
 * thing away and re-scoring, not by reading the code.
 *
 * A run with any of these produces **deliberately wrong pages** and must never
 * be the source of a headline number; `--stats` prints the list so a report can
 * be checked. They are listed in `stage7c-commonmark-ablation.txt`.
 */
const MD_ABLATIONS = new Set(
  argv.filter((a) =>
    a === "--md-no-indented-code" || a === "--md-no-loose-list" ||
    a === "--md-no-nested-list" || a === "--md-no-lazy" ||
    a === "--md-lazy-over-markers" || a === "--md-ol-interrupts" ||
    a === "--md-no-underscore-emph"
  ).map((a) => a.slice("--md-".length)),
);
const abl = (name: string) => MD_ABLATIONS.has(name);
if (!IR || (!OUT && !PAGES)) {
  console.error(
    "usage: render.ts --ir <dir> [--out <path.jsonl>] [--pages <dir> --source-url <base>]" +
      " [--only <Module>]... [--stats <path>]",
  );
  Deno.exit(2);
}
if (PAGES && !SOURCE_URL) {
  console.error(
    "--pages needs --source-url <https://host/owner/repo/blob/REV>: the source URL is\n" +
      "configuration that doc-gen4 reads from lake + git. It is not in the IR.",
  );
  Deno.exit(2);
}

// ---------------------------------------------------------------- HTML

/** `Html.escape` (ToHtmlFormat.lean): & < > " and nothing else. */
function escapeHtml(s: string): string {
  let out = "";
  for (const c of s) {
    if (c === "&") out += "&amp;";
    else if (c === "<") out += "&lt;";
    else if (c === ">") out += "&gt;";
    else if (c === '"') out += "&quot;";
    else out += c;
  }
  return out;
}

// ---------------------------------------------------------------- kind

/**
 * `getKindDescription` (Process/DocInfo.lean:211-247) recomposed from the IR's
 * `kind` + `modifiers`, per experiments/stage4b/README.md "Kind modifiers".
 */
function kindDescription(kind: string, modifiers: string[]): string {
  const has = (m: string) => modifiers.includes(m);
  switch (kind) {
    case "definition": {
      const parts: string[] = [];
      if (has("unsafe")) parts.push("unsafe");
      if (has("noncomputable")) parts.push("noncomputable");
      parts.push(has("abbrev") ? "abbrev" : "def");
      return parts.join(" ");
    }
    case "instance": {
      const parts: string[] = [];
      if (has("unsafe")) parts.push("unsafe");
      if (has("noncomputable")) parts.push("noncomputable");
      parts.push("instance");
      return parts.join(" ");
    }
    case "axiom":
      return has("unsafe") ? "unsafe axiom" : "axiom";
    case "opaque":
      if (has("partial")) return "partial def";
      if (has("unsafe")) return "unsafe opaque";
      return "opaque";
    case "inductive":
      return has("unsafe") ? "unsafe inductive" : "inductive";
    case "class_inductive":
      return "class inductive";
    default:
      // structure / class / theorem / constructor
      return kind;
  }
}

// ---------------------------------------------------------------- links

/** `moduleNameToLink`: getRoot ++ components joined by "/" ++ ".html". */
function moduleLink(root: string, module: string): string {
  return root + module.split(".").join("/") + ".html";
}

/** `getRoot`: "../" repeated depthToRoot, then "./". */
function pageRoot(module: string): string {
  const depth = module.split(".").length - 1;
  return "../".repeat(depth) + "./";
}

/**
 * `findLinkableParent` (Base.lean): strip trailing components that start with
 * "_" or are numeric, and return the first prefix that is a known declaration.
 * The IR has no `Name` structure, only the printed string, so `.num` components
 * are recognised by being all digits -- which is what `Name.toString` prints
 * them as. Counted separately in the stats so a wrong guess here is visible.
 */
function findLinkableParent(known: Map<string, string>, name: string): string | null {
  let cur = name;
  for (;;) {
    const dot = cur.lastIndexOf(".");
    if (dot < 0) return null;
    const last = cur.slice(dot + 1);
    const isNum = last.length > 0 && /^[0-9]+$/.test(last);
    if (!isNum && !last.startsWith("_") && known.has(cur)) return cur;
    cur = cur.slice(0, dot);
    if (cur === "") return null;
  }
}

const PRIVATE_PREFIX = "_private.";

// ---------------------------------------------------------------- span tree

type Node = { start: number; stop: number; kind: number; name: string; children: Node[] };

/**
 * Rebuild the tree from the flat pre-order span list
 * (experiments/stage4b/README.md, "Nesting"): pop while `start >= top.stop`,
 * then the new span is a child of whatever is left on the stack.
 */
function buildTree(spans: Span[]): Node[] {
  const roots: Node[] = [];
  const stack: Node[] = [];
  for (const s of spans) {
    const node: Node = {
      start: s[0],
      stop: s[1],
      kind: s[2],
      name: s.length > 3 ? (s[3] as string) : "",
      children: [],
    };
    while (stack.length > 0 && node.start >= stack[stack.length - 1].stop) stack.pop();
    if (stack.length > 0) stack[stack.length - 1].children.push(node);
    else roots.push(node);
    stack.push(node);
  }
  return roots;
}

// ---------------------------------------------------------------- counters

/**
 * The counters every `renderedCodeToHtmlAux` walk bumps. There are two
 * instances: `stats` (the `decl_header` path, increment 3's numbers) and
 * `pageStats` (everything increment 4 adds). `sink` selects which one is live,
 * so that adding `--pages` cannot move increment 3's numbers.
 */
type FragCounters = {
  anchorsConst: number;
  anchorsSort: number;
  anchorsModuleFallback: number;
  constSpansSuppressedByNesting: number;
  constSpansUnlinkable: number;
  constSpansViaParent: number;
  constSpansPrivate: number;
  constSpansNameNotInRefs: number;
  wsWidthChars: number;
  wsWidthFragments: number;
};

const stats = {
  modulesRead: 0,
  modulesRendered: 0,
  declarationsInIr: 0,
  declarationsSuppressed: 0, // members of another declaration (render := false)
  declarationsRendered: 0,
  anchorsConst: 0,
  anchorsSort: 0,
  anchorsModuleFallback: 0,
  anchorsBreakWithin: 0,
  constSpansSuppressedByNesting: 0,
  constSpansUnlinkable: 0,
  constSpansViaParent: 0,
  constSpansPrivate: 0,
  constSpansNameNotInRefs: 0,
  implArgs: 0,
  extendsRendered: 0,
  /** Code units `applyWsWidths` actually changed, i.e. non-space whitespace
   *  inside a schema-3 `front`/`back` run. Runs that were already spaces are
   *  rewritten to the same bytes and are not counted. */
  wsWidthChars: 0,
  /** Fragments in which at least one code unit changed. */
  wsWidthFragments: 0,
};

/** Increment 4 only. Zero unless `--pages` is given. */
const pageStats = {
  pagesWritten: 0,
  /** UTF-16 code units handed to `writeTextFile`, NOT bytes on disk. */
  pageCodeUnits: 0,
  moduleDocs: 0,
  navLinks: 0,
  importListItems: 0,
  importDuplicatesDropped: 0,
  ghLinks: 0,
  docstringsRendered: 0,
  docstringChars: 0,
  equationBlocks: 0,
  equationItems: 0,
  equationsOmittedNotices: 0,
  equationsDroppedOverLimit: 0,
  memberTables: 0,
  memberFields: 0,
  /** Schema 4 (stage 7b). */
  memberFieldArgs: 0,
  memberFieldDocs: 0,
  memberFieldsInherited: 0,
  memberFieldsInheritedWithId: 0,
  attributeBlocks: 0,
  attributeItems: 0,
  instancesForStubs: 0,
  classInstancesStubs: 0,
  autolinkAttempts: 0,
  autolinkResolved: 0,
  // FragCounters, for the non-header fragments (members, equations).
  anchorsConst: 0,
  anchorsSort: 0,
  anchorsModuleFallback: 0,
  constSpansSuppressedByNesting: 0,
  constSpansUnlinkable: 0,
  constSpansViaParent: 0,
  constSpansPrivate: 0,
  constSpansNameNotInRefs: 0,
  wsWidthChars: 0,
  wsWidthFragments: 0,
};

let sink: FragCounters = stats;

/**
 * `splitWhitespaces` (Base.lean:281-288) in reverse, from the schema-3 widths.
 *
 * doc-gen4 replaces a `.const` tag's leading and trailing whitespace with a run
 * of **plain spaces of the same length** (`"".pushn ' ' n`), so `" =\n  "` comes
 * out as `" =   "`: no offset moves, but `\n` and `\t` become `' '`. Schema 2
 * kept only the narrowed span, which is why the tag's real extent -- and with it
 * the identity of the whitespace that was inside it -- could not be recovered;
 * stage 4c's `--ws-heuristic` guessed the extent from whole whitespace runs and
 * rewrote whitespace doc-gen4 had left alone (実測: 407 fixed, 100 broken).
 *
 * Schema 3 carries the two widths, so this is no longer a guess: exactly the
 * units `[start - front, start)` and `[stop, stop + back)` become spaces, and
 * nothing else is touched.
 *
 * The ranges are disjoint by construction (each lies inside its own tag's
 * extent, and sibling tags do not overlap in the printed text). An overlap or an
 * out-of-range width means the IR disagrees with its own text, so this throws
 * rather than quietly producing plausible bytes.
 */
function applyWsWidths(text: string, spans: Span[]): string {
  let ranges: [number, number][] | null = null;
  for (const s of spans) {
    if (s.length < 6) continue;
    const front = s[4] as number;
    const back = s[5] as number;
    if (front > 0) (ranges ??= []).push([s[0] - front, s[0]]);
    if (back > 0) (ranges ??= []).push([s[1], s[1] + back]);
  }
  if (ranges === null) return text;
  ranges.sort((a, b) => a[0] - b[0]);

  // Offsets are UTF-16 code units and so are JavaScript string indices, so this
  // indexes `text` directly; `[...text]` (code points) would be the wrong unit.
  let changed = 0;
  let end = 0;
  for (const [lo, hi] of ranges) {
    if (lo < end || hi > text.length) {
      throw new Error(
        `schema-3 whitespace width out of range: [${lo},${hi}) in a ${text.length}-unit ` +
          `fragment (previous run ended at ${end})`,
      );
    }
    for (let i = lo; i < hi; i++) if (text[i] !== " ") changed++;
    end = hi;
  }
  if (changed === 0) return text;

  let out = "";
  let pos = 0;
  for (const [lo, hi] of ranges) {
    out += text.slice(pos, lo) + " ".repeat(hi - lo);
    pos = hi;
  }
  out += text.slice(pos);
  sink.wsWidthChars += changed;
  sink.wsWidthFragments++;
  return out;
}

// ---------------------------------------------------------------- renderer

type Rendered = { html: string; hasAnchor: boolean };

class Renderer {
  constructor(
    /** name -> defining module, global (deps + every module's own declarations + every ref). */
    readonly known: Map<string, string>,
  ) {}

  /** `renderedCodeToHtmlAux` over one fragment. */
  fragment(text: string, spans: Span[], root: string, refs: Map<string, string>): Rendered {
    const roots = buildTree(spans);
    const t = applyWsWidths(text, spans);
    return this.range(t, 0, t.length, roots, root, refs);
  }

  private range(
    text: string,
    lo: number,
    hi: number,
    children: Node[],
    root: string,
    refs: Map<string, string>,
  ): Rendered {
    let html = "";
    let hasAnchor = false;
    let pos = lo;
    for (const c of children) {
      if (c.start > pos) html += escapeHtml(text.slice(pos, c.start));
      const r = this.node(text, c, root, refs);
      html += r.html;
      hasAnchor = hasAnchor || r.hasAnchor;
      pos = c.stop;
    }
    if (hi > pos) html += escapeHtml(text.slice(pos, hi));
    return { html, hasAnchor };
  }

  private node(text: string, n: Node, root: string, refs: Map<string, string>): Rendered {
    const inner = this.range(text, n.start, n.stop, n.children, root, refs);
    if (n.kind === 0) {
      return { html: `<span class="fn">${inner.html}</span>`, hasAnchor: inner.hasAnchor };
    }
    if (n.kind === 2) {
      // Base.lean:374-381 -- no `fn` wrapper, and a nested anchor is suppressed.
      if (inner.hasAnchor) return { html: inner.html, hasAnchor: true };
      sink.anchorsSort++;
      return {
        html: `<a href="${escapeHtml(root + "foundational_types.html")}">${inner.html}</a>`,
        hasAnchor: true,
      };
    }
    // kind 1: `.const name`, Base.lean:337-373.
    const link = this.constLink(n.name, root, refs);
    if (link === null) {
      sink.constSpansUnlinkable++;
      return { html: `<span class="fn">${inner.html}</span>`, hasAnchor: inner.hasAnchor };
    }
    if (inner.hasAnchor) {
      sink.constSpansSuppressedByNesting++;
      return { html: inner.html, hasAnchor: true };
    }
    sink.anchorsConst++;
    return { html: `<a href="${escapeHtml(link)}">${inner.html}</a>`, hasAnchor: true };
  }

  /**
   * `renderedCodeToHtmlAux`'s `.const` resolution, in order:
   *   direct hit in name2ModIdx and not a private name  -> declaration link
   *   findLinkableParent                                -> declaration link
   *   private prefix                                    -> module link
   *   otherwise                                         -> null (span.fn)
   *
   * The IR's stand-in for `name2ModIdx` is the declaration's own `refs` (the
   * extractor resolved every constant it tagged with `env.getModuleIdxFor?`,
   * which *is* `const2ModIdx`, the map doc-gen4 uses), backed by the global map
   * for the parent-stripping fallback.
   */
  private constLink(name: string, root: string, refs: Map<string, string>): string | null {
    const isPrivate = name.startsWith(PRIVATE_PREFIX);
    if (!isPrivate) {
      const mod = refs.get(name) ?? this.known.get(name);
      if (mod !== undefined) {
        if (!refs.has(name)) sink.constSpansNameNotInRefs++;
        return moduleLink(root, mod) + "#" + name;
      }
    } else {
      sink.constSpansPrivate++;
    }
    // Step 1: auxiliary name removal.
    const search = isPrivate ? privateToUserName(name) : name;
    const parent = findLinkableParent(this.known, search);
    if (parent !== null) {
      sink.constSpansViaParent++;
      return moduleLink(root, this.known.get(parent)!) + "#" + parent;
    }
    // Step 2: module link for a private name.
    if (isPrivate) {
      const mod = moduleFromPrivatePrefix(name);
      if (mod !== null) {
        sink.anchorsModuleFallback++;
        return moduleLink(root, mod);
      }
    }
    return null;
  }
}

/** `Lean.privateToUserName?`: `_private.<Module>.<n>.<rest>` -> `<rest>`. */
function privateToUserName(name: string): string {
  const m = /^_private\.(.*?)\.\d+\.(.*)$/.exec(name);
  return m ? m[2] : name;
}

/** `moduleFromPrivatePrefix`: `_private.Init.Prelude.0.Foo` -> `Init.Prelude`. */
function moduleFromPrivatePrefix(name: string): string | null {
  const m = /^_private\.(.*?)\.\d+\./.exec(name);
  return m ? m[1] : null;
}

// ---------------------------------------------------------------- header

/** `breakWithin` (Base.lean): split on ".", each part in `span.name`. */
function breakWithin(name: string): string {
  return name
    .split(".")
    .map((s) => `<span class="name">${escapeHtml(s)}</span>`)
    .join(".");
}

function declHeader(d: Decl, module: string, r: Renderer): string {
  const root = pageRoot(module);
  const refs = new Map<string, string>();
  for (const [m, n] of d.refs) refs.set(n, m);

  let out = `<div class="decl_header">`;

  // `Html.element "span" false #[text kind]` -> "<span ...>kind</span>\n"
  out += `<span class="decl_kind">${escapeHtml(kindDescription(d.kind, d.modifiers))}</span>\n`;

  // JSX (inline): <span class="decl_name">{a.break_within}</span>
  const selfLink = moduleLink(root, module) + "#" + d.name;
  stats.anchorsBreakWithin++;
  out += `<span class="decl_name"><a class="break_within" href="${escapeHtml(selfLink)}">` +
    `${breakWithin(d.name)}</a></span>`;

  // argToHtml, one per binder.
  for (let i = 0; i < d.binders.length; i++) {
    const body = r.fragment(d.binders[i], d.binderCode[i] ?? [], root, refs);
    // `Html.element "span" false attrs #[child]` -> "<span ...>\n{child}</span>\n"
    const arg = `<span class="decl_args">\n<span class="fn">${body.html}</span></span>\n`;
    if (d.implicits[i]) {
      stats.implArgs++;
      out += `<span class="impl_arg">${arg}</span>`;
    } else {
      out += arg;
    }
  }

  // structureInfoHeader, structure/class only.
  if (d.kind === "structure" || d.kind === "class") {
    const parents = d.members.filter((m) => m.label === "parent");
    if (parents.length > 0) {
      stats.extendsRendered++;
      out += `<span class="decl_extends">extends</span> `;
      out += parents
        .map((p) => {
          const body = r.fragment(p.text, p.code ?? [], root, refs);
          return `<span id="${escapeHtml(p.name)}">${body.html}</span>`;
        })
        .join(", ");
    }
  }

  // `Html.element "span" true #[text " :"]` (inline)
  out += `<span class="decl_args"> :</span>`;

  const ty = r.fragment(d.type, d.typeCode ?? [], root, refs);
  out += `<div class="decl_type">${ty.html}</div>`;
  out += `</div>`;
  return out;
}

// ---------------------------------------------------------------- page frame

/**
 * `String.quote` (Lean core), which is what the two `<script>` constants in
 * `baseHtmlGenerator` go through. Only the escapes `Char.quoteCore` produces.
 */
function leanQuote(s: string): string {
  let out = '"';
  for (const c of s) {
    if (c === "\n") out += "\\n";
    else if (c === "\t") out += "\\t";
    else if (c === "\\") out += "\\\\";
    else if (c === '"') out += '\\"';
    else if (c.codePointAt(0)! <= 31 || c.codePointAt(0)! === 127) {
      out += "\\x" + c.codePointAt(0)!.toString(16).padStart(2, "0");
    } else out += c;
  }
  return out + '"';
}

/** Lean's `String.lt`: `List.lt` over the characters, i.e. code points. */
function stringLt(a: string, b: string): boolean {
  const ca = [...a], cb = [...b];
  for (let i = 0; i < ca.length && i < cb.length; i++) {
    const x = ca[i].codePointAt(0)!, y = cb[i].codePointAt(0)!;
    if (x !== y) return x < y;
  }
  return ca.length < cb.length;
}

/**
 * `Lean.Name.lt`, which `importsHtml` sorts with. It compares the *parents*
 * first and only then the last component, so a name with fewer components sorts
 * before one with more regardless of the strings (`.anonymous` is smaller than
 * everything). Module names here are all `.str`, so the `.num` cases of
 * `Name.lt` are not transcribed.
 */
function nameLt(a: string[], b: string[]): boolean {
  if (a.length === 0) return b.length !== 0;
  if (b.length === 0) return false;
  const pa = a.slice(0, -1), pb = b.slice(0, -1);
  if (nameLt(pa, pb)) return true;
  if (pa.length === pb.length && pa.every((x, i) => x === pb[i])) {
    return stringLt(a[a.length - 1], b[b.length - 1]);
  }
  return false;
}

/**
 * `DocInfo.getKind` (Process/DocInfo.lean:48-58) -- the CSS class of the inner
 * `div`, which is *not* `span.decl_kind`. The IR's `kind` is the extractor's
 * own vocabulary (stage4b), so this is a two-step mapping.
 */
function cssKind(kind: string): string {
  switch (kind) {
    case "definition":
      return "def";
    case "class_inductive":
      return "class";
    case "constructor":
      return "ctor";
    default:
      return kind; // axiom / theorem / opaque / instance / inductive / structure / class
  }
}

/** `baseHtmlGenerator`'s `<head>` (Template.lean + Base.lean baseHtmlHeadDeclarations). */
function headHtml(module: string, root: string): string {
  const a = (s: string) => escapeHtml(s);
  return `<head><meta charset="UTF-8"></meta>` +
    `<meta name="viewport" content="width=device-width, initial-scale=1"></meta>` +
    `<link rel="stylesheet" href="${a(root + "style.css")}"></link>` +
    `<link rel="icon" href="${a(root + "favicon.svg")}"></link>` +
    `<link rel="mask-icon" href="${a(root + "favicon.svg")}" color="#000000"></link>` +
    `<link rel="prefetch" href="${a(root + "/declarations/declaration-data.bmp")}" as="image"></link>` +
    `<title>${escapeHtml(module)}</title>` +
    `<script defer="true" src="${a(root + "mathjax-config.js")}"></script>` +
    `<script defer="true" src="https://cdnjs.cloudflare.com/polyfill/v3/polyfill.min.js?features=es6"></script>` +
    `<script defer="true" src="https://cdn.jsdelivr.net/npm/mathjax@3/es5/tex-mml-chtml.js"></script>` +
    `<script>const SITE_ROOT=${leanQuote(root)};</script>` +
    `<script>const MODULE_NAME=${leanQuote(module)};</script>` +
    `<script type="module" src="${a(root + "jump-src.js")}"></script>` +
    `<script type="module" src="${a(root + "search.js")}"></script>` +
    `<script type="module" src="${a(root + "expand-nav.js")}"></script>` +
    `<script type="module" src="${a(root + "how-about.js")}"></script>` +
    `<script type="module" src="${a(root + "instances.js")}"></script>` +
    `<script type="module" src="${a(root + "importedBy.js")}"></script></head>`;
}

/**
 * `baseHtmlGenerator`'s `<header>`. The whitespace written inside the JSX tags
 * (`<span> {x} </span>`) never reaches the output -- Lean's tokenizer eats it --
 * so there is none here either.
 */
function pageHeaderHtml(module: string, root: string): string {
  return `<header><h1><label for="nav_toggle"></label><span>Documentation</span></h1>` +
    `<h2 class="header_filename break_within">${breakWithin(module)}</h2>` +
    `<form id="search_form"><input type="text" name="q" autocomplete="off"></input>&#32;` +
    `<button id="search_button" onclick="${
      escapeHtml(`javascript: form.action='${root}search.html';`)
    }">Search</button></form></header>`;
}

/** `internalNav` (Module.lean:158-176). */
function internalNavHtml(
  module: string,
  root: string,
  moduleSourceUrl: string,
  imports: string[],
  memberNames: string[],
): string {
  // `importsHtml`: `(← getImports moduleName).qsort Name.lt`. The list itself
  // comes back from the DB, which inserts with `INSERT OR IGNORE` (DB.lean:162),
  // so duplicates -- the module system produces them -- are dropped.
  const seen = new Set<string>();
  const uniq: string[] = [];
  for (const i of imports) {
    if (seen.has(i)) {
      pageStats.importDuplicatesDropped++;
      continue;
    }
    seen.add(i);
    uniq.push(i);
  }
  uniq.sort((x, y) => (nameLt(x.split("."), y.split(".")) ? -1 : nameLt(y.split("."), x.split(".")) ? 1 : 0));
  pageStats.importListItems += uniq.length;
  const importLis = uniq
    .map((i) => `<li><a href="${escapeHtml(moduleLink(root, i))}">${escapeHtml(i)}</a></li>`)
    .join("");
  pageStats.navLinks += memberNames.length;
  const navLinks = memberNames
    .map((n) =>
      `<div class="nav_link"><a class="break_within" href="${escapeHtml("#" + n)}">` +
      `${breakWithin(n)}</a></div>`
    )
    .join("");
  return `<nav class="internal_nav"><p><a href="#top">return to top</a></p>` +
    `<p class="gh_nav_link"><a href="${escapeHtml(moduleSourceUrl)}">source</a></p>` +
    `<div class="imports"><details><summary>Imports</summary><ul>${importLis}</ul></details>` +
    `<details><summary>Imported by</summary>` +
    `<ul id="${escapeHtml("imported-by-" + module)}" class="imported-by-list"></ul>` +
    `</details></div>${navLinks}</nav>`;
}

// ---------------------------------------------------------------- docstrings

/**
 * `nameToLink?` (DocString.lean:39-80). doc-gen4's `name2ModIdx` is the whole
 * environment; the IR's stand-in is `known` (declarations + deps + every
 * resolved ref) **plus** `linkIndex` (`--link-index`, the dependency closure's
 * map). Without the second one the gap is 297 anchors / 191 targets on this
 * target (`stage7c-autolink-*.txt`).
 */
function nameToLink(
  s: string,
  root: string,
  moduleDeclNames: string[],
  knownModules: Set<string>,
): string | null {
  if (s.endsWith(".lean") && s.includes("/")) return root + s.slice(0, -5) + ".html";
  if (!isNameLit(s)) return null;
  if (!s.startsWith(PRIVATE_PREFIX)) {
    const mod = known.get(s) ?? linkIndex.get(s);
    if (mod !== undefined) return moduleLink(root, mod) + "#" + s;
  }
  if (knownModules.has(s)) return moduleLink(root, s);
  // "find similar name in the same module": the first declaration whose
  // component suffix matches.
  const want = s.split(".").reverse();
  for (const n of moduleDeclNames) {
    const have = n.split(".").reverse();
    const k = Math.min(want.length, have.length);
    let ok = true;
    for (let i = 0; i < k; i++) if (want[i] !== have[i]) ok = false;
    if (ok) return moduleLink(root, known.get(n)!) + "#" + n;
  }
  return null;
}

/** `Char.isAlpha` / `isDigit` are ASCII-only in Lean core. */
const isAlphaCp = (c: number) =>
  (c >= 0x41 && c <= 0x5a) || (c >= 0x61 && c <= 0x7a);
const isDigitCp = (c: number) => c >= 0x30 && c <= 0x39;

/** `Lean.isLetterLike` (Init/Meta/Defs.lean:101-109). */
function isLetterLike(c: number): boolean {
  return (c >= 0x3b1 && c <= 0x3c9 && c !== 0x3bb) ||
    (c >= 0x391 && c <= 0x3a9 && c !== 0x3a0 && c !== 0x3a3) ||
    (c >= 0x3ca && c <= 0x3fb) ||
    (c >= 0x1f00 && c <= 0x1ffe) ||
    (c >= 0x2100 && c <= 0x214f) ||
    (c >= 0x1d49c && c <= 0x1d59f) ||
    (c >= 0x00c0 && c <= 0x00ff && c !== 0x00d7 && c !== 0x00f7) ||
    (c >= 0x0100 && c <= 0x017f);
}

/** `Lean.isSubScriptAlnum` (Init/Meta/Defs.lean:114-118). */
const isSubScriptAlnum = (c: number) =>
  (c >= 0x2080 && c <= 0x2089) || (c >= 0x2090 && c <= 0x209c) ||
  (c >= 0x1d62 && c <= 0x1d6a) || c === 0x2c7c;

const isIdFirst = (c: number) => isAlphaCp(c) || c === 0x5f || isLetterLike(c);
const isIdRest = (c: number) =>
  isAlphaCp(c) || isDigitCp(c) || c === 0x5f || c === 0x27 /* ' */ ||
  c === 0x21 /* ! */ || c === 0x3f /* ? */ || isLetterLike(c) || isSubScriptAlnum(c);

/**
 * `Lean.Syntax.decodeNameLit ("`" ++ s)` = `splitNameLitAux`
 * (Init/Meta/Defs.lean:1180-1203) followed by "not anonymous". A component is
 * `«…»`, an identifier (`isIdFirst` then `isIdRest`*), or a run of digits, and
 * after each component the rest must be empty or start with `.`.
 *
 * Stage 7b rejected any component containing `'`, `!` or `?`, all three of which
 * are `isIdRest`; that alone accounted for 9 of the anchors doc-gen4 emitted and
 * this renderer did not (`Foo.bar'`).
 */
function isNameLit(s: string): boolean {
  const n = s.length;
  let i = 0;
  for (;;) {
    if (i >= n) return false; // empty component -> `[]` -> anonymous
    const c = s.codePointAt(i)!;
    if (c === 0x00ab /* « */) {
      const j = s.indexOf("»", i + 1);
      if (j < 0) return false;
      i = j + 1;
    } else if (isIdFirst(c)) {
      i += c > 0xffff ? 2 : 1;
      while (i < n) {
        const d = s.codePointAt(i)!;
        if (!isIdRest(d)) break;
        i += d > 0xffff ? 2 : 1;
      }
    } else if (isDigitCp(c)) {
      while (i < n && isDigitCp(s.charCodeAt(i))) i++;
    } else return false;
    if (i >= n) return true;
    if (s.charCodeAt(i) === 0x2e /* . */) {
      i++;
      continue;
    }
    return false;
  }
}

/** `autoLinkInline` (DocString.lean:175-197): split on Unicode Z|C, keep separators. */
function autoLinkInline(
  text: string,
  root: string,
  moduleDeclNames: string[],
  knownModules: Set<string>,
): string {
  const parts = text.split(/(\p{Z}|\p{C})/u).filter((p) => p !== undefined);
  let out = "";
  for (const part of parts) {
    if (part === "") continue;
    pageStats.autolinkAttempts++;
    const link = nameToLink(part, root, moduleDeclNames, knownModules);
    if (link !== null) {
      pageStats.autolinkResolved++;
      out += `<a href="${escapeHtml(link)}">${escapeHtml(part)}</a>`;
      continue;
    }
    const dot = part.lastIndexOf(".");
    const head = dot >= 0 ? part.slice(0, dot + 1) : "";
    const tail = dot >= 0 ? part.slice(dot + 1) : part;
    const link2 = head === "" ? null : nameToLink(tail, root, moduleDeclNames, knownModules);
    if (link2 !== null) {
      pageStats.autolinkResolved++;
      out += escapeHtml(head) + `<a href="${escapeHtml(link2)}">${escapeHtml(tail)}</a>`;
    } else {
      out += escapeHtml(part);
    }
  }
  return out;
}

/** `mdGetHeadingId`: drop Unicode P|Z|C runs, join what is left with "-". */
function headingId(text: string): string {
  return text.split(/[\p{P}\p{Z}\p{C}]/u).filter((s) => s.length > 0).join("-");
}

/** `extendLink` (DocString.lean:90-103). */
function extendLink(
  s: string,
  root: string,
  moduleDeclNames: string[],
  knownModules: Set<string>,
): string {
  if (s.startsWith("##")) {
    const link = nameToLink(s.slice(2), root, moduleDeclNames, knownModules);
    return link ?? `${root}find/?pattern=${s.slice(2)}#doc`;
  }
  if (s.startsWith("#")) return s;
  if (s.startsWith("http")) return s;
  return root + s;
}

type DocCtx = { root: string; moduleDeclNames: string[]; knownModules: Set<string> };

/**
 * ------------------------------------------------------------------ CommonMark
 *
 * `docStringToHtml` runs md4c with `MD_DIALECT_GITHUB ||| MD_FLAG_LATEXMATHSPANS
 * ||| MD_FLAG_NOHTML`. What follows is a block parser for the CommonMark subset
 * this target uses, plus `renderBlock` / `renderLi` transcribed from
 * `Output/DocString.lean:287-363`. **This is not a complete md4c.** What is here:
 *
 *   ATX headings, setext headings, thematic breaks, fenced code, indented code,
 *   block quotes, bullet and ordered lists (tight/loose, nested, lazy
 *   continuation), paragraphs.
 *
 * What is deliberately absent, because this corpus contains none of it (checked
 * against doc-gen4's own 348 pages: zero `<table>`, `<del>`, `<br>`,
 * `<input type=checkbox>`): GFM tables, strikethrough, task lists, hard breaks,
 * reference links and images. Permissive autolinks are also absent. If a target
 * ever uses one, `coverage.ts` will fail that region -- it is not silently
 * approximated.
 */

type MdBlock =
  | { t: "p"; lines: string[] }
  | { t: "h"; level: number; text: string }
  | { t: "hr" }
  | { t: "code"; lang: string; content: string }
  | { t: "list"; ordered: boolean; start: number; tight: boolean; items: MdBlock[][] }
  | { t: "quote"; blocks: MdBlock[] };

const isBlankLine = (s: string) => /^[ \t]*$/.test(s);

/** Leading whitespace measured in **columns** (a tab advances to the next
 *  multiple of 4), which is how CommonMark counts block indentation. */
function indentCols(s: string): number {
  let n = 0;
  for (let i = 0; i < s.length; i++) {
    const c = s[i];
    if (c === " ") n++;
    else if (c === "\t") n += 4 - (n % 4);
    else return n;
  }
  return n;
}

/** Drop up to `n` columns of leading whitespace; a tab straddling the boundary
 *  becomes the spaces that stick out past it. */
function stripCols(line: string, n: number): string {
  let col = 0;
  let i = 0;
  while (i < line.length && col < n) {
    const c = line[i];
    if (c === " ") {
      col++;
      i++;
    } else if (c === "\t") {
      const adv = 4 - (col % 4);
      i++;
      if (col + adv > n) return " ".repeat(col + adv - n) + line.slice(i);
      col += adv;
    } else break;
  }
  return line.slice(i);
}

const THEMATIC = /^ {0,3}([-*_])(?:[ \t]*\1){2,}[ \t]*$/;
const ATX = /^ {0,3}(#{1,6})(?:([ \t].*)|)$/;
const FENCE = /^( {0,3})(`{3,}|~{3,})(.*)$/;
const SETEXT = /^ {0,3}(=+|-+)[ \t]*$/;

type ItemStart = {
  ordered: boolean;
  num: number;
  delim: string;
  contentIndent: number;
  empty: boolean;
  /** The item's first line with the marker and the content indent removed. */
  firstLine: string;
};

/** A list-item marker at the start of `line` (CommonMark "list items" §5.2). */
function itemStart(line: string): ItemStart | null {
  const m = /^([ \t]*)(?:([-+*])|([0-9]{1,9})([.)]))(.*)$/.exec(line);
  if (m === null) return null;
  const markerCol = indentCols(m[1]);
  if (markerCol > 3) return null;
  const ordered = m[2] === undefined;
  const markerLen = ordered ? m[3].length + 1 : 1;
  const rest = m[5];
  if (rest !== "" && rest[0] !== " " && rest[0] !== "\t") return null;
  const empty = isBlankLine(rest);
  // Spaces after the marker, in columns, capped: 5 or more means the item's
  // first block is indented code, and the content indent is marker + 1.
  let spaces = 0;
  if (!empty) {
    let col = markerCol + markerLen;
    for (let i = 0; i < rest.length; i++) {
      if (rest[i] === " ") {
        spaces++;
        col++;
      } else if (rest[i] === "\t") {
        const adv = 4 - (col % 4);
        spaces += adv;
        col += adv;
      } else break;
    }
  }
  const contentIndent = empty || spaces > 4
    ? markerCol + markerLen + 1
    : markerCol + markerLen + spaces;
  return {
    ordered,
    num: ordered ? Number(m[3]) : 0,
    delim: ordered ? m[4] : m[2],
    contentIndent,
    empty,
    firstLine: stripCols(rest, contentIndent - markerCol - markerLen),
  };
}

/** Can `line` end an open paragraph? (CommonMark: which blocks may interrupt.) */
function interruptsParagraph(line: string): boolean {
  if (isBlankLine(line)) return true;
  if (indentCols(line) >= 4) return false;
  if (ATX.test(line)) return true;
  if (FENCE.test(line)) return true;
  if (THEMATIC.test(line)) return true;
  if (/^ {0,3}>/.test(line)) return true;
  const it = itemStart(line);
  if (it !== null) {
    if (it.empty) return false; // an empty list item cannot interrupt
    if (it.ordered && it.num !== 1 && !abl("ol-interrupts")) return false; // only `1.` may interrupt
    return true;
  }
  return false;
}

/** Is `lines[k]` a blank line that separates two blocks (rather than one inside
 *  a code block)? Used only for list tightness. */
function interiorBlockBlank(lines: string[], k: number): boolean {
  let before = -1;
  for (let i = k - 1; i >= 0; i--) {
    if (!isBlankLine(lines[i])) {
      before = i;
      break;
    }
  }
  let after = -1;
  for (let i = k + 1; i < lines.length; i++) {
    if (!isBlankLine(lines[i])) {
      after = i;
      break;
    }
  }
  if (before < 0 || after < 0) return false;
  // Inside a fenced code block?
  let fence: string | null = null;
  for (let i = 0; i <= k; i++) {
    const f = FENCE.exec(lines[i]);
    if (f === null) continue;
    if (fence === null) fence = f[2][0];
    else if (f[2][0] === fence && f[3].trim() === "") fence = null;
  }
  if (fence !== null) return false;
  // Inside one indented code block?
  if (indentCols(lines[before]) >= 4 && indentCols(lines[after]) >= 4) return false;
  return true;
}

function parseBlocks(lines: string[], inItem = false): MdBlock[] {
  const out: MdBlock[] = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    if (isBlankLine(line)) {
      i++;
      continue;
    }
    const ind = indentCols(line);

    // fenced code
    const fence = ind <= 3 ? FENCE.exec(line) : null;
    if (fence !== null && !(fence[2][0] === "`" && fence[3].includes("`"))) {
      const openIndent = indentCols(fence[1]);
      const ch = fence[2][0];
      const len = fence[2].length;
      const body: string[] = [];
      i++;
      while (i < lines.length) {
        const c = FENCE.exec(lines[i]);
        if (
          c !== null && c[2][0] === ch && c[2].length >= len && c[3].trim() === "" &&
          indentCols(c[1]) <= 3
        ) {
          i++;
          break;
        }
        body.push(stripCols(lines[i], openIndent));
        i++;
      }
      // md4c's `lang` is the first word of the info string.
      const lang = fence[3].trim().split(/[ \t]/)[0] ?? "";
      out.push({
        t: "code",
        lang,
        content: body.length > 0 ? body.join("\n") + "\n" : "",
      });
      continue;
    }

    // ATX heading
    const atx = ind <= 3 ? ATX.exec(line) : null;
    if (atx !== null) {
      let text = (atx[2] ?? "").replace(/^[ \t]+/, "").replace(/[ \t]+$/, "");
      const closing = /^(.*?)[ \t]#+$/.exec(text);
      if (closing !== null) text = closing[1].replace(/[ \t]+$/, "");
      else if (/^#+$/.test(text)) text = "";
      out.push({ t: "h", level: atx[1].length, text });
      i++;
      continue;
    }

    // thematic break
    if (ind <= 3 && THEMATIC.test(line)) {
      out.push({ t: "hr" });
      i++;
      continue;
    }

    // block quote
    if (ind <= 3 && /^ {0,3}>/.test(line)) {
      const body: string[] = [];
      while (i < lines.length) {
        const m = /^ {0,3}>[ \t]?/.exec(lines[i]);
        if (m !== null) {
          body.push(lines[i].slice(m[0].length));
          i++;
          continue;
        }
        if (isBlankLine(lines[i])) break;
        // lazy continuation of the quote's last paragraph
        if (
          body.length > 0 && !isBlankLine(body[body.length - 1]) &&
          !interruptsParagraph(lines[i])
        ) {
          body.push(lines[i]);
          i++;
          continue;
        }
        break;
      }
      out.push({ t: "quote", blocks: parseBlocks(body) });
      continue;
    }

    // list
    const start = ind <= 3 && !(inItem && abl("no-nested-list")) ? itemStart(line) : null;
    if (start !== null) {
      const r = parseList(lines, i, start);
      out.push(r.block);
      i = r.next;
      continue;
    }

    // indented code block (cannot interrupt a paragraph, so this is reached only
    // when no paragraph is open)
    if (ind >= 4 && !abl("no-indented-code")) {
      const body: string[] = [];
      let lastContent = -1;
      while (i < lines.length) {
        if (isBlankLine(lines[i])) {
          body.push("");
          i++;
          continue;
        }
        if (indentCols(lines[i]) < 4) break;
        body.push(stripCols(lines[i], 4));
        lastContent = body.length - 1;
        i++;
      }
      body.length = lastContent + 1; // trailing blank lines are not part of it
      out.push({ t: "code", lang: "", content: body.join("\n") + "\n" });
      continue;
    }

    // paragraph, with setext underlines and lazy continuation
    const para: string[] = [line];
    i++;
    let setext = 0;
    while (i < lines.length) {
      const l = lines[i];
      const sx = indentCols(l) <= 3 ? SETEXT.exec(l) : null;
      if (sx !== null && !(sx[1][0] === "-" && THEMATIC.test(l) && para.length === 0)) {
        setext = sx[1][0] === "=" ? 1 : 2;
        i++;
        break;
      }
      if (interruptsParagraph(l)) break;
      para.push(l);
      i++;
    }
    if (setext > 0) out.push({ t: "h", level: setext, text: trimBlockLines(para) });
    else out.push({ t: "p", lines: para });
  }
  return out;
}

/**
 * One list, starting at `lines[i]`. A list ends at the first line that is
 * neither a continuation of the current item nor a marker of the same type
 * (`isTight` per CommonMark: blank line between items, or a blank line between
 * two blocks inside one item).
 */
function parseList(
  lines: string[],
  i: number,
  first: ItemStart,
): { block: MdBlock; next: number } {
  const ordered = first.ordered;
  const delim = first.delim;
  const items: string[][] = [];
  let loose = false;
  while (i < lines.length) {
    const m = indentCols(lines[i]) <= 3 ? itemStart(lines[i]) : null;
    if (m === null || m.ordered !== ordered || m.delim !== delim) break;
    const item: string[] = [m.firstLine];
    i++;
    let pendingBlanks = 0;
    let fenceDepth = 0;
    while (i < lines.length) {
      const l = lines[i];
      if (isBlankLine(l)) {
        pendingBlanks++;
        i++;
        continue;
      }
      if (indentCols(l) >= m.contentIndent) {
        for (let k = 0; k < pendingBlanks; k++) item.push("");
        pendingBlanks = 0;
        const stripped = stripCols(l, m.contentIndent);
        const f = FENCE.exec(stripped);
        if (f !== null) fenceDepth = fenceDepth === 0 ? 1 : 0;
        item.push(stripped);
        i++;
        continue;
      }
      // Lazy continuation: only a paragraph continuation line, and only when no
      // blank line intervened.
      //
      // Any list marker ends the item here, including `2.` and `3)`. The
      // "an ordered list may interrupt a paragraph only when it starts at 1"
      // rule does **not** apply at this point: cmark passes
      // `interrupts_paragraph = (container->type == PARAGRAPH)`, and a line that
      // failed the item's content indent has already closed the item, so the
      // container is the list, not the paragraph. The rule does apply one level
      // down, inside `parseBlocks` -- which is why `Wiley,\n2006. Theorem` stays
      // one paragraph while `1. …\n2. …` becomes two items.
      if (
        pendingBlanks === 0 && fenceDepth === 0 && !isBlankLine(item[item.length - 1]) &&
        (itemStart(l) === null || abl("lazy-over-markers")) && !interruptsParagraph(l) &&
        !abl("no-lazy")
      ) {
        item.push(l);
        i++;
        continue;
      }
      break;
    }
    items.push(item);
    if (item.some((_, k) => isBlankLine(item[k]) && interiorBlockBlank(item, k))) loose = true;
    if (pendingBlanks > 0 && i < lines.length) {
      const nx = indentCols(lines[i]) <= 3 ? itemStart(lines[i]) : null;
      if (nx !== null && nx.ordered === ordered && nx.delim === delim) loose = true;
    }
  }
  return {
    block: {
      t: "list",
      ordered,
      start: ordered ? first.num : 1,
      tight: !loose || abl("no-loose-list"),
      items: items.map((it) => parseBlocks(it, true)),
    },
    next: i,
  };
}

/** The two top-level entry points into the docstring renderer, timed.
 *  `renderDocString` recurses, so the timer goes here and not inside it. Like
 *  every render timer this one under-counts by whatever rope flattening it
 *  defers -- see "THE ROPE TRAP". */
function renderDocStringTimed(md: string, ctx: DocCtx): string {
  const t = performance.now();
  const html = renderDocString(md, ctx);
  T.docstring += performance.now() - t;
  return html;
}

function renderDocString(md: string, ctx: DocCtx): string {
  // doc-gen4 parses `docString ++ refsMarkdown`, and `refsMarkdown` is "\n\n"
  // plus one line per bibliography key found. This target has no bibliography
  // entries in any docstring, so it is just the "\n\n".
  return renderMdBlocks(parseBlocks((md + "\n\n").split("\n")), ctx, false);
}

/** `renderBlock` (DocString.lean:287-350). `tight` reaches only `.p`. */
function renderMdBlocks(bs: MdBlock[], ctx: DocCtx, tight: boolean): string {
  let out = "";
  for (const b of bs) {
    switch (b.t) {
      case "p": {
        const inner = renderInline(trimBlockLines(b.lines), ctx);
        out += tight ? inner : `<p>${inner}</p>`;
        break;
      }
      case "h": {
        const id = headingId(stripInline(b.text));
        out += `<h${b.level} id="${escapeHtml(id)}" class="markdown-heading">` +
          `${renderInline(b.text, ctx)} ` +
          `<a class="hover-link" href="${escapeHtml("#" + id)}">#</a></h${b.level}>`;
        break;
      }
      case "hr":
        out += "<hr>\n";
        break;
      case "code": {
        const attrs = b.lang !== "" ? ` class="language-${escapeHtml(b.lang)}"` : "";
        // `isLeanCode`: an empty or `lean` info string is auto-linked.
        const inner = b.lang === "" || b.lang === "lean"
          ? autoLinkInline(b.content, ctx.root, ctx.moduleDeclNames, ctx.knownModules)
          : escapeHtml(b.content);
        out += `<pre><code${attrs}>${inner}</code></pre>`;
        break;
      }
      case "list": {
        let lis = "";
        for (const it of b.items) lis += `<li>${renderMdBlocks(it, ctx, b.tight)}</li>`;
        out += b.ordered
          ? (b.start !== 1 ? `<ol start="${b.start}">${lis}</ol>` : `<ol>${lis}</ol>`)
          : `<ul>${lis}</ul>`;
        break;
      }
      case "quote":
        out += `<blockquote>${renderMdBlocks(b.blocks, ctx, false)}</blockquote>`;
        break;
    }
  }
  return out;
}

/**
 * CommonMark strips leading and trailing whitespace from every line of a
 * paragraph (two or more trailing spaces would be a hard break; that case is
 * not handled here).
 */
function trimBlockLines(lines: string[]): string {
  return lines.map((l) => l.trim()).join("\n");
}

/** Plain text of an inline run, for `mdGetHeadingId`. */
function stripInline(s: string): string {
  return s
    .replace(/`([^`]*)`/g, "$1")
    .replace(/\*\*([^*]*)\*\*/g, "$1")
    .replace(/\*([^*]*)\*/g, "$1")
    .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1");
}

/**
 * md4c's three-way classification of what sits next to an emphasis run
 * (`md4c.c:3049-3086`): 0 = whitespace or line edge, 1 = punctuation, 2 = other.
 * "Punctuation" is the Unicode `P` **and** `S` categories -- md4c's `PUNCT_MAP`
 * is generated from exactly those two, which is why `∑` (Sm) counts and
 * `∑_a 1_` is emphasis rather than an intra-word underscore.
 */
const MD_PUNCT = /[\p{P}\p{S}]/u;
const MD_SPACE = /[\s\p{Z}]/u;
function delimLevel(cp: string | undefined): number {
  if (cp === undefined) return 0; // line edge
  if (MD_SPACE.test(cp)) return 0;
  if (MD_PUNCT.test(cp)) return 1;
  return 2;
}

/** `[opener, closer]` for the delimiter run `s[start..end)` of character `ch`. */
function delimRun(s: string, start: number, end: number, ch: string): [boolean, boolean] {
  let left = delimLevel(start > 0 ? s[start - 1] : undefined);
  let right = delimLevel(end < s.length ? s[end] : undefined);
  // "Intra-word underscore doesn't have special meaning."
  if (ch === "_" && left === 2 && right === 2) {
    left = 0;
    right = 0;
  }
  return [right > 0 && right >= left, left > 0 && left >= right];
}

/** The next `ch` run at or after `from` that can close, skipping code spans. */
function findDelimClose(s: string, from: number, ch: string): number {
  let j = from;
  while (j < s.length) {
    const c = s[j];
    if (c === "`") {
      let k = 1;
      while (s[j + k] === "`") k++;
      const close = s.indexOf("`".repeat(k), j + k);
      j = close >= 0 ? close + k : j + k;
      continue;
    }
    if (c === ch) {
      let m = 1;
      while (s[j + m] === ch) m++;
      if (delimRun(s, j, j + m, ch)[1]) return j;
      j += m;
      continue;
    }
    j++;
  }
  return -1;
}

function renderInline(s: string, ctx: DocCtx): string {
  let out = "";
  let i = 0;
  const flush = (t: string) => escapeHtml(t);
  let plain = "";
  const push = () => {
    if (plain !== "") {
      out += flush(plain);
      plain = "";
    }
  };
  while (i < s.length) {
    const c = s[i];
    if (c === "`") {
      let n = 1;
      while (s[i + n] === "`") n++;
      const close = s.indexOf("`".repeat(n), i + n);
      if (close >= 0) {
        push();
        // CommonMark: line endings inside a code span become spaces, and then
        // one leading and one trailing space are stripped if both are present.
        let body = s.slice(i + n, close).replace(/\r?\n/g, " ");
        if (body.length > 1 && body.startsWith(" ") && body.endsWith(" ") && body.trim() !== "") {
          body = body.slice(1, -1);
        }
        out += `<code>${autoLinkInline(body, ctx.root, ctx.moduleDeclNames, ctx.knownModules)}</code>`;
        i = close + n;
        continue;
      }
    }
    if (c === "$") {
      const close = s.indexOf("$", i + 1);
      if (close > i + 1 && !s.slice(i + 1, close).includes("\n")) {
        push();
        out += `$${escapeHtml(s.slice(i + 1, close))}$`;
        i = close + 1;
        continue;
      }
    }
    // Emphasis. Both delimiters go through md4c's flanking test: without it
    // every `snake_case_name` in prose would emphasise, and `a * b * c` would
    // become `a <em> b </em> c`.
    if (c === "*" || (c === "_" && !abl("no-underscore-emph"))) {
      let n = 1;
      while (s[i + n] === c) n++;
      if (delimRun(s, i, i + n, c)[0]) {
        const close = findDelimClose(s, i + n, c);
        if (close >= 0) {
          let m = 1;
          while (s[close + m] === c) m++;
          const w = Math.min(n, m, 2);
          push();
          const inner = renderInline(s.slice(i + w, close), ctx);
          out += w === 2 ? `<strong>${inner}</strong>` : `<em>${inner}</em>`;
          i = close + w;
          continue;
        }
      }
    }
    if (c === "[") {
      const m = /^\[([^\]]*)\]\(([^)\s]*)(?:\s+"([^"]*)")?\)/.exec(s.slice(i));
      if (m) {
        push();
        const href = extendLink(m[2], ctx.root, ctx.moduleDeclNames, ctx.knownModules);
        const title = m[3] ? ` title="${escapeHtml(m[3])}"` : "";
        out += `<a href="${escapeHtml(href)}"${title}>${renderInline(m[1], ctx)}</a>`;
        i += m[0].length;
        continue;
      }
    }
    plain += c;
    i++;
  }
  push();
  return out;
}

// ---------------------------------------------------------------- declaration

/** `equationsToHtml` (Definition.lean) + the DB's `equationLimit` filter. */
function equationsHtml(d: Decl, root: string, refs: Map<string, string>, r: Renderer): string {
  // `RenderedCode.textLength` counts Lean `Char`s, i.e. code points -- not
  // UTF-16 units and not bytes.
  const keep: number[] = [];
  let omitted = false;
  for (let i = 0; i < d.equations.length; i++) {
    if ([...d.equations[i]].length < 200) keep.push(i);
    else {
      omitted = true;
      pageStats.equationsDroppedOverLimit++;
    }
  }
  if (keep.length === 0 && !omitted) return "";
  pageStats.equationBlocks++;
  let lis = "";
  if (omitted) {
    pageStats.equationsOmittedNotices++;
    lis += `<li class="equation">One or more equations did not get rendered due to their size.</li>`;
  }
  for (const i of keep) {
    pageStats.equationItems++;
    const body = r.fragment(d.equations[i], d.equationCode[i] ?? [], root, refs);
    lis += `<li class="equation">${body.html}</li>`;
  }
  return `<details><summary>Equations</summary><ul class="equations">${lis}</ul></details>`;
}

/** `instancesForToHtml` (Inductive.lean:12-17): a stub the browser fills in. */
function instancesForHtml(name: string): string {
  pageStats.instancesForStubs++;
  return `<details id="${escapeHtml("instances-for-list-" + name)}" class="instances-for-list">` +
    `<summary>Instances For</summary><ul class="instances-for-enum"></ul></details>`;
}

/** `classInstancesToHtml` (Class.lean:11-16): also a stub. */
function classInstancesHtml(name: string): string {
  pageStats.classInstancesStubs++;
  return `<details class="instances"><summary>Instances</summary>` +
    `<ul id="${escapeHtml("instances-list-" + name)}" class="instances-list"></ul></details>`;
}

/**
 * `declNameToLink` (Base.lean:231-234). doc-gen4 indexes `name2ModIdx` with `!`,
 * i.e. it panics on a name it cannot place; the IR's stand-in is the same two
 * maps `constLink` uses. Throwing rather than emitting a plausible `href` is
 * deliberate: a wrong link is a wrong byte either way, and a silent one costs a
 * debugging round to find.
 */
function declNameToLink(
  name: string,
  root: string,
  refs: Map<string, string>,
  r: Renderer,
): string {
  const mod = refs.get(name) ?? r.known.get(name);
  if (mod === undefined) {
    throw new Error(`declNameToLink: no defining module for ${name} (doc-gen4 would panic here)`);
  }
  return moduleLink(root, mod) + "#" + name;
}

/**
 * The `containedNames` query (`DB/Read.lean:177-185`), which decides whether an
 * inherited field's `<li>` gets an `id`: the names in the same module, other
 * than the declaration itself, whose declaration range is **inside** the
 * declaration's range. In the IR every declaration carries `(line, col)` and
 * `(endLine, endCol)`, including the ones that are not rendered, which is the
 * same population as the DB's `name_info` rows for that module.
 *
 * Computed lazily, per structure that actually has an inherited field. doc-gen4
 * runs the query for every module it renders; that is its cost of having put
 * the data in SQLite, not work this side has to reproduce.
 */
function containedNames(mod: ModuleFile, parent: Decl): Set<string> {
  const out = new Set<string>();
  for (const c of mod.declarations) {
    if (c.name === parent.name) continue;
    const startsInside = c.line > parent.line ||
      (c.line === parent.line && c.col >= parent.col);
    const endsInside = c.endLine < parent.endLine ||
      (c.endLine === parent.endLine && c.endCol <= parent.endCol);
    if (startsInside && endsInside) out.add(c.name);
  }
  return out;
}

/**
 * `structureToHtml` + `fieldToHtml` (Structure.lean). Schema 4 carries the three
 * things stage 7a had to leave out -- the field's own binders, its docstring and
 * its origin -- so both branches of `fieldToHtml` are reproduced here.
 *
 * The two branches differ in more than a CSS class, and doc-gen4 writes their
 * attributes in different orders (`id` then `class` for a direct field,
 * `class` alone or `id` then `class` for an inherited one). Byte order of
 * attributes is byte identity, so each branch is written out rather than
 * parameterised.
 */
function structureHtml(
  d: Decl,
  mod: ModuleFile,
  root: string,
  refs: Map<string, string>,
  r: Renderer,
  ctx: DocCtx,
): string {
  const ctor = d.members.find((m) => m.label === "ctor");
  const fields = d.members.filter((m) => m.label === "field");
  pageStats.memberTables++;
  let contained: Set<string> | null = null;
  const lis = fields
    .map((f) => {
      pageStats.memberFields++;
      const short = f.name.split(".").pop()!;
      // `argToHtml`, exactly as in the declaration header.
      let args = "";
      for (let i = 0; i < (f.binders?.length ?? 0); i++) {
        pageStats.memberFieldArgs++;
        const b = r.fragment(f.binders![i], f.binderCode?.[i] ?? [], root, refs);
        const arg = `<span class="decl_args">\n<span class="fn">${b.html}</span></span>\n`;
        args += f.implicits?.[i] ? `<span class="impl_arg">${arg}</span>` : arg;
      }
      const body = r.fragment(f.text, f.code ?? [], root, refs);
      if (f.isDirect === false) {
        pageStats.memberFieldsInherited++;
        const inner = `<div class="structure_field_info">` +
          `<a href="${escapeHtml(declNameToLink(f.name, root, refs, r))}">${escapeHtml(short)}</a>` +
          `${args} : ${body.html}</div>`;
        contained ??= containedNames(mod, d);
        const projName = d.name + "." + short;
        if (contained.has(projName)) {
          pageStats.memberFieldsInheritedWithId++;
          return `<li id="${escapeHtml(projName)}" class="structure_field inherited_field">` +
            `${inner}</li>`;
        }
        return `<li class="structure_field inherited_field">${inner}</li>`;
      }
      let doc = "";
      if (f.doc) {
        pageStats.memberFieldDocs++;
        doc = `<div class="structure_field_doc">${renderDocStringTimed(f.doc, ctx)}</div>`;
      }
      return `<li id="${escapeHtml(f.name)}" class="structure_field">` +
        `<div class="structure_field_info">${escapeHtml(short)}${args} : ${body.html}</div>` +
        `${doc}</li>`;
    })
    .join("");
  const ctorName = ctor ? ctor.name : d.name + ".mk";
  if (ctorName.split(".").pop() === "mk") {
    return `<ul class="structure_fields" id="${escapeHtml(ctorName)}">${lis}</ul>`;
  }
  const short = ctorName.split(".").pop()!;
  return `<ul class="structure_ext"><li id="${escapeHtml(ctorName)}" class="structure_ext_ctor">` +
    `${escapeHtml(short + " ")} :: (</li><ul class="structure_ext_fields">${lis}</ul>` +
    `<li class="structure_ext_ctor">)</li></ul>`;
}

/** `docInfoToHtml` (Module.lean:67-112). */
function declHtml(
  d: Decl,
  mod: ModuleFile,
  root: string,
  moduleSourceUrl: string,
  header: string,
  ctx: DocCtx,
  r: Renderer,
): string {
  const module = mod.module;
  const refs = new Map<string, string>();
  for (const [m, n] of d.refs) refs.set(n, m);

  pageStats.ghLinks++;
  const gh = `<div class="gh_link"><a href="${
    escapeHtml(`${moduleSourceUrl}#L${d.line}-L${d.endLine}`)
  }">source</a></div>`;

  // `div.attributes` (Module.lean:88-94). `Html.element "div" false … #[text s]`
  // is the one non-flattened element at this level, so `Html.toStringAux` prints
  // it as `<div …>escape s</div>\n` -- the trailing newline is part of the
  // element and therefore part of this region (coverage.ts absorbs it on both
  // sides; it once got counted twice, on 42 pages).
  let attrs = "";
  if (d.attrs && d.attrs.length > 0) {
    pageStats.attributeBlocks++;
    pageStats.attributeItems += d.attrs.length;
    attrs = `<div class="attributes">${escapeHtml("@[" + d.attrs.join(", ") + "]")}</div>\n`;
  }

  let doc = "";
  if (d.doc) {
    pageStats.docstringsRendered++;
    pageStats.docstringChars += d.doc.length;
    doc = renderDocStringTimed(d.doc, ctx);
  }

  let body = "";
  let extra = "";
  switch (d.kind) {
    case "structure":
    case "class":
      body = structureHtml(d, mod, root, refs, r, ctx);
      extra = d.kind === "class" ? classInstancesHtml(d.name) : instancesForHtml(d.name);
      break;
    case "definition":
      extra = equationsHtml(d, root, refs, r) + instancesForHtml(d.name);
      break;
    case "instance":
      extra = equationsHtml(d, root, refs, r);
      break;
    case "inductive":
      extra = instancesForHtml(d.name);
      break;
    case "class_inductive":
      extra = classInstancesHtml(d.name);
      break;
    default:
      break; // theorem / axiom / opaque / constructor
  }

  return `<div class="decl" id="${escapeHtml(d.name)}"><div class="${escapeHtml(cssKind(d.kind))}">` +
    `${gh}${attrs}${header}${doc}${body}${extra}</div></div>`;
}

/** `moduleToHtml` + `baseHtmlGenerator`. */
function pageHtml(mod: ModuleFile, headers: Map<string, string>, r: Renderer): string {
  const module = mod.module;
  const root = pageRoot(module);
  const moduleSourceUrl = `${SOURCE_URL}/${module.split(".").join("/")}.lean`;
  // `nameToLink?`'s last resort walks `res.moduleInfo[currentName].members`
  // (DocString.lean:66-69): every `DocInfo` of the module -- including the ones
  // that get no page entry, because `filterDocInfo` is not `shouldRender` --
  // minus the private ones, in `members` order, which is the declaration-range
  // sort. Stage 7b passed the IR's own order instead and picked the wrong
  // `sincN` of two candidates on 6 anchors.
  const ctx: DocCtx = {
    root,
    moduleDeclNames: mod.declarations
      .filter((d) => !d.name.startsWith(PRIVATE_PREFIX))
      .slice()
      .sort((a, b) => a.line - b.line || a.col - b.col || a.index - b.index)
      .map((d) => d.name),
    knownModules,
  };

  // `Process.Module.members` is `modDocs` (in file order) followed by every
  // `DocInfo`, then `qsort ModuleMember.order` on the declaration-range start.
  // qsort is not stable, but increment 1 measured a stable sort on (line, col)
  // reproducing the page order of 348/348 pages, so that is what is used, with
  // the modDocs kept ahead of the declarations at an equal position because
  // that is the insertion order.
  type Item = { line: number; col: number; seq: number; html: string; name: string | null };
  const items: Item[] = [];
  let seq = 0;
  for (const md of mod.moduleDocs ?? []) {
    pageStats.moduleDocs++;
    items.push({
      line: md.line,
      col: md.col,
      seq: seq++,
      name: null,
      html: `<div class="mod_doc">${renderDocStringTimed(md.text, ctx)}</div>`,
    });
  }
  const rendered = mod.declarations.filter((d) => !suppressed.has(d.name));
  for (const d of rendered) {
    items.push({
      line: d.line,
      col: d.col,
      seq: seq + d.index,
      name: d.name,
      html: "",
    });
  }
  items.sort((a, b) => a.line - b.line || a.col - b.col || a.seq - b.seq);

  const byName = new Map(rendered.map((d) => [d.name, d]));
  const memberNames: string[] = [];
  let main = "";
  for (const it of items) {
    if (it.name === null) {
      main += it.html;
      continue;
    }
    memberNames.push(it.name);
    const d = byName.get(it.name)!;
    main += declHtml(d, mod, root, moduleSourceUrl, headers.get(d.name)!, ctx, r);
  }

  pageStats.pagesWritten++;
  return `<html lang="en">${headHtml(module, root)}<body>` +
    `<input id="nav_toggle" type="checkbox"></input>` +
    pageHeaderHtml(module, root) +
    internalNavHtml(module, root, moduleSourceUrl, mod.imports ?? [], memberNames) +
    `<main>\n${main}</main>\n` +
    `<nav class="nav"><iframe src="${
      escapeHtml(root + "navbar.html")
    }" class="navframe" frameBorder="0"></iframe></nav></body></html>`;
}

// ---------------------------------------------------------------- main

/** Kept alive so the flatten probe cannot be optimised away. */
let flattenSink = 0;
let irBytes = 0;

const T_MAIN = performance.now();
const index: Index = JSON.parse(await Deno.readTextFile(`${IR}/index.json`));
T.readIr += performance.now() - T_MAIN;
if (index.schemaVersion < 4) {
  console.error(
    `schemaVersion ${index.schemaVersion}: need a schema-4 IR ` +
      `(experiments/stage7b/run.sh ... --tagged-code). Schema 3 has no attributes, ` +
      `no instance index and no member binders / docstrings / origin.`,
  );
  Deno.exit(2);
}
if (index.ablations && index.ablations.length > 0) {
  console.error(
    `this IR was written with ablations [${index.ablations.join(", ")}] and is ` +
      `incomplete on purpose. It is for the stopwatch only; rendering it would ` +
      `produce a page that looks fine and is wrong.`,
  );
  Deno.exit(2);
}

// Global name -> module map. Three sources, in the order doc-gen4's
// `name2ModIdx` would have them:
//   deps/*.json      constants from other packages that this one refers to
//   modules/*.json   every declaration of this package (including the ones that
//                    are not rendered -- projection functions and constructors)
//   refs             every constant the extractor resolved (superset of the two
//                    above for the names that actually get linked)
const known = new Map<string, string>();
for (const dep of index.dependencyMaps) {
  const a = performance.now();
  const map = JSON.parse(await Deno.readTextFile(`${IR}/${dep.file}`));
  const b = performance.now();
  for (const [n, m] of Object.entries(map.declarations as Record<string, string>)) known.set(n, m);
  T.readIr += b - a;
  T.indexBuild += performance.now() - b;
  irBytes += dep.bytes;
}

// `--limit N` cuts the module list, for the timing task's linearity check.
const entries = LIMIT > 0 ? index.modules.slice(0, LIMIT) : index.modules;
const modules: ModuleFile[] = [];
for (const entry of entries) {
  const a = performance.now();
  const mod: ModuleFile = JSON.parse(await Deno.readTextFile(`${IR}/${entry.file}`));
  const b = performance.now();
  modules.push(mod);
  stats.modulesRead++;
  irBytes += entry.bytes;
  for (const d of mod.declarations) {
    stats.declarationsInIr++;
    known.set(d.name, mod.module);
    for (const [m, n] of d.refs) if (!known.has(n)) known.set(n, m);
  }
  T.readIr += b - a;
  T.indexBuild += performance.now() - b;
}
const T_SETS = performance.now();

// `DocInfo.ofConstant` sets `render := false` for projection functions and for
// constructors (Process/DocInfo.lean:176/186/207), i.e. exactly the names that
// show up as another declaration's `members`. That is the IR-side rule for the
// 186 declarations doc-gen4 does not put on a page.
const suppressed = new Set<string>();
for (const mod of modules) {
  for (const d of mod.declarations) {
    for (const m of d.members) suppressed.add(m.name);
  }
}

/** Every module name a link can point at: the package's own plus its dependencies'. */
const knownModules = new Set<string>(modules.map((m) => m.module));
for (const m of known.values()) knownModules.add(m);
T.indexBuild += performance.now() - T_SETS;

// The dependency closure's name -> module map (`--link-index`). doc-gen4 reads
// the same information out of `env.name2ModIdx`, which it has because it holds
// the whole environment; this renderer holds none of it, so the map is an input.
//
// It is kept **separate from `known`** on purpose. `known` also backs the
// signature path (`Renderer.constLink`'s `findLinkableParent` fallback), and that
// path is byte-exact today (3,477/3,477 `decl_header` regions); a map 50x larger
// underneath it would change results that are already right. `nameToLink`
// consults `known` first and this second, so nothing that resolved before can
// start resolving elsewhere.
const linkIndex = new Map<string, string>();
let linkIndexBytes = 0;
if (LINK_INDEX) {
  const a = performance.now();
  const text = await Deno.readTextFile(LINK_INDEX);
  linkIndexBytes = text.length;
  let module = "";
  let pos = 0;
  const n = text.length;
  while (pos < n) {
    let nl = text.indexOf("\n", pos);
    if (nl < 0) nl = n;
    const c = text.charCodeAt(pos);
    if (c === 9 /* tab */) linkIndex.set(text.slice(pos + 1, nl), module);
    else if (c === 64 /* @ */) knownModules.add(text.slice(pos + 1, nl));
    else if (c === 35 /* # */) { /* format marker */ }
    else if (nl > pos) module = text.slice(pos, nl);
    pos = nl + 1;
  }
  T.linkIndex += performance.now() - a;
}

const renderer = new Renderer(known);
const wanted = ONLY.length > 0 ? new Set(ONLY) : null;
const lines: string[] = [];

for (const mod of modules) {
  if (wanted && !wanted.has(mod.module)) continue;
  stats.modulesRendered++;
  stats.declarationsSuppressed += mod.declarations.filter((d) => suppressed.has(d.name)).length;
  // Page order: stable sort on (line, col), ties broken by the in-module index
  // (experiments/stage4b/README.md, "In-module index").
  const decls = mod.declarations
    .filter((d) => !suppressed.has(d.name))
    .slice()
    .sort((a, b) => a.line - b.line || a.col - b.col || a.index - b.index);
  const headers = new Map<string, string>();
  sink = stats;
  const tH0 = performance.now();
  for (const d of decls) {
    stats.declarationsRendered++;
    const html = declHeader(d, mod.module, renderer);
    headers.set(d.name, html);
    if (OUT) lines.push(JSON.stringify({ module: mod.module, name: d.name, html }));
  }
  T.renderHeaders += performance.now() - tH0;
  if (PAGES) {
    // Everything below this line counts into `pageStats`, so that increment 3's
    // counters above are the same whether or not `--pages` was given.
    sink = pageStats;
    const tP0 = performance.now();
    const page = pageHtml(mod, headers, renderer);
    const tP1 = performance.now();
    // The rope trap: `page` is a cons string until something reads it. See
    // "TIMERS" at the top -- this is billed to render, not to write.
    if (FLATTEN_PROBE) flattenSink += page.indexOf(" ") + page.length;
    const tP2 = performance.now();
    const rel = mod.module.split(".").join("/") + ".html";
    const path = `${PAGES}/${rel}`;
    const dir = path.slice(0, path.lastIndexOf("/"));
    await Deno.mkdir(dir, { recursive: true });
    await Deno.writeTextFile(path, page);
    T.renderPage += tP1 - tP0;
    T.flatten += tP2 - tP1;
    T.write += performance.now() - tP2;
    pageStats.pageCodeUnits += page.length;
    sink = stats;
  }
}
const tOut0 = performance.now();
if (OUT) await Deno.writeTextFile(OUT, lines.join("\n") + "\n");
T.write += performance.now() - tOut0;
T.total = performance.now();

const secs = (ms: number) => (ms / 1000).toFixed(4);
// `docstring` is NOT added: it is a slice of `renderPage`, already counted.
const accounted = T.preMain + T.readIr + T.indexBuild + T.linkIndex + T.renderHeaders +
  T.renderPage + T.flatten + T.write;
const timingBlock = [
  `## phases (seconds, in-process; \`total\` is timeOrigin -> here)`,
  ``,
  `preMain (module init; see TIMERS) ${secs(T.preMain)}`,
  `read IR (readTextFile + parse)   ${secs(T.readIr)}`,
  `index build (name map + sets)    ${secs(T.indexBuild)}`,
  `link index (read + map)          ${secs(T.linkIndex)}${
    LINK_INDEX ? "" : "   (not given)"
  }`,
  `render decl_header               ${secs(T.renderHeaders)}`,
  `render page                      ${secs(T.renderPage)}`,
  `flatten probe                    ${secs(T.flatten)}${FLATTEN_PROBE ? "" : "   (disabled)"}`,
  `write (mkdir + writeTextFile)    ${secs(T.write)}`,
  `  of "render page": docstrings   ${secs(T.docstring)}`,
  `--------------------------------`,
  `sum of the above                 ${secs(accounted)}`,
  `total (timeOrigin -> here)       ${secs(T.total)}`,
  `unaccounted                      ${secs(T.total - accounted)}`,
  ``,
  `ir bytes read                    ${irBytes.toLocaleString("en-US")}`,
  `link index code units            ${linkIndexBytes.toLocaleString("en-US")}`,
  `link index entries               ${linkIndex.size.toLocaleString("en-US")}`,
  `flatten sink (keeps the probe alive) ${flattenSink}`,
  ``,
];

const report = [
  `# render — doc-gen4 HTML rebuilt from the IR (no Lean)`,
  ``,
  `ir              ${IR}`,
  `schemaVersion   ${index.schemaVersion}`,
  `generator       ${index.generator}`,
  `out             ${OUT || "(none)"}`,
  `pages           ${PAGES || "(none)"}`,
  `source-url      ${SOURCE_URL || "(none)"}   <- configuration, not IR`,
  `link-index      ${LINK_INDEX || "(none)"}`,
  `md ablations    ${
    MD_ABLATIONS.size > 0
      ? [...MD_ABLATIONS].join(", ") + "   <- PAGES ARE DELIBERATELY WRONG"
      : "(none)"
  }`,
  `only            ${ONLY.length > 0 ? ONLY.join(", ") : "(all modules)"}`,
  ``,
  `## decl_header path (increment 3; unaffected by --pages)`,
  ``,
  ...Object.entries(stats).map(([k, v]) => `${k.padEnd(32)} ${v.toLocaleString("en-US")}`),
  ``,
  ...(PAGES
    ? [
      `## page path (increment 4)`,
      ``,
      ...Object.entries(pageStats).map(([k, v]) => `${k.padEnd(32)} ${v.toLocaleString("en-US")}`),
      ``,
    ]
    : []),
  ...timingBlock,
].join("\n");
console.log(report);
if (STATS) await Deno.writeTextFile(STATS, report);
if (TIMINGS) {
  await Deno.writeTextFile(
    TIMINGS,
    JSON.stringify({
      deno: Deno.version.deno,
      v8: Deno.version.v8,
      ir: IR,
      pages: PAGES || null,
      limit: LIMIT || null,
      flattenProbe: FLATTEN_PROBE,
      modulesRead: stats.modulesRead,
      modulesRendered: stats.modulesRendered,
      declarationsRendered: stats.declarationsRendered,
      pagesWritten: pageStats.pagesWritten,
      pageCodeUnits: pageStats.pageCodeUnits,
      irBytes,
      linkIndexPath: LINK_INDEX || null,
      linkIndexCodeUnits: linkIndexBytes,
      linkIndexEntries: linkIndex.size,
      seconds: Object.fromEntries(
        Object.entries(T).map(([k, v]) => [k, Number((v / 1000).toFixed(6))]),
      ),
      secondsAccounted: Number((accounted / 1000).toFixed(6)),
    }) + "\n",
  );
}
