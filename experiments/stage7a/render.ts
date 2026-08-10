#!/usr/bin/env -S deno run --allow-read --allow-write
//
// Stage 7a: rebuild doc-gen4's HTML from the **schema-3** IR without starting
// Lean. Copied from `experiments/stage4c/render.ts` (which reads schema 2) and
// changed in exactly one place: `applyWsWidths` replaces `applyWsHeuristic`, so
// the `splitWhitespaces` whitespace rewrite is reproduced from the widths the
// IR now carries instead of being guessed from the text. Stage 4c is left as it
// was, because its 63.587% and its timings are the committed schema-2 baseline.
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
//   --ir      schema-3 IR root (experiments/stage7a/run.sh --write-ir --tagged-code)
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
//   doc-gen4 renders docstrings with MD4Lean, i.e. md4c, a C CommonMark
//   implementation, and auto-links names inside `<code>`. Reproducing CommonMark
//   byte for byte in Deno with no external libraries is out of scope (plan §6
//   pre-decision #5: no network, so no library either). What is here is a
//   deliberately naive block/inline renderer. **No agreement is claimed for it**
//   -- `coverage.ts` counts docstring bytes as reproduced only when they are
//   byte-identical, and they mostly are not.
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

type Member = { label: string; name: string; text: string; code: Span[] };

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
const FLATTEN_PROBE = !argv.includes("--no-flatten-probe");
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
 * `nameToLink?` (DocString.lean:39-80), approximated. The IR's stand-in for
 * `name2ModIdx` is `known` (declarations + deps + every resolved ref), which is
 * a subset of doc-gen4's whole environment; increment 1 measured the gap at
 * 293 / 5,044 docstring anchors (`stage4-html-inventory.txt` §F).
 */
function nameToLink(
  s: string,
  root: string,
  moduleDeclNames: string[],
  knownModules: Set<string>,
): string | null {
  if (s.endsWith(".lean") && s.includes("/")) return root + s.slice(0, -5) + ".html";
  if (!isNameLit(s)) return null;
  if (known.has(s) && !s.startsWith(PRIVATE_PREFIX)) {
    return moduleLink(root, known.get(s)!) + "#" + s;
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

/**
 * `Lean.Syntax.decodeNameLit ("`" ++ s)`, approximated: dot-separated
 * components, each either `«…»` or an identifier that does not start with a
 * digit. Anything else is not a name and gets no link.
 */
function isNameLit(s: string): boolean {
  if (s.length === 0) return false;
  for (const part of s.split(".")) {
    if (part.length === 0) return false;
    if (part.startsWith("«") && part.endsWith("»")) continue;
    if (/^[0-9]/.test(part)) return false;
    if (/[\s()\[\]{},;"'`\\]/.test(part)) return false;
  }
  return true;
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
 * A **naive** stand-in for `docStringToHtml`. doc-gen4 runs md4c (CommonMark +
 * GitHub dialect + LaTeX math spans, HTML disabled); this handles paragraphs,
 * ATX headings, fenced code, thematic breaks, bullet/ordered lists,
 * blockquotes, and the inline forms `` `code` ``, `[t](u)`, `**strong**`,
 * `*em*`, `$math$`.
 *
 * **It is not CommonMark and no agreement is claimed for it.** It exists so
 * that a generated page has a docstring where doc-gen4 has one, and so that the
 * coverage accounting has something to compare; `coverage.ts` counts these
 * bytes as reproduced only when they are byte-identical.
 */
/** The two top-level entry points into the docstring renderer, timed.
 *  `renderDocString` recurses (blockquotes), so the timer goes here and not
 *  inside it. Like every render timer this one under-counts by whatever rope
 *  flattening it defers -- see "THE ROPE TRAP". */
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
  const lines = (md + "\n\n").split("\n");
  const out: string[] = [];
  let i = 0;
  const blank = (s: string) => s.trim() === "";
  while (i < lines.length) {
    const line = lines[i];
    if (blank(line)) {
      i++;
      continue;
    }
    const fence = /^(\s*)(```|~~~)(.*)$/.exec(line);
    if (fence) {
      const lang = fence[3].trim();
      const body: string[] = [];
      i++;
      while (i < lines.length && !new RegExp(`^\\s*${fence[2]}`).test(lines[i])) body.push(lines[i++]);
      if (i < lines.length) i++;
      const content = body.length > 0 ? body.join("\n") + "\n" : "";
      const attrs = lang !== "" ? ` class="language-${escapeHtml(lang)}"` : "";
      const inner = lang === "" || lang === "lean"
        ? autoLinkInline(content, ctx.root, ctx.moduleDeclNames, ctx.knownModules)
        : escapeHtml(content);
      out.push(`<pre><code${attrs}>${inner}</code></pre>`);
      continue;
    }
    const head = /^(#{1,6})\s+(.*?)\s*#*\s*$/.exec(line);
    if (head) {
      const level = head[1].length;
      const id = headingId(stripInline(head[2]));
      out.push(
        `<h${level} id="${escapeHtml(id)}" class="markdown-heading">${renderInline(head[2], ctx)} ` +
          `<a class="hover-link" href="${escapeHtml("#" + id)}">#</a></h${level}>`,
      );
      i++;
      continue;
    }
    if (/^\s{0,3}([-*_])(\s*\1){2,}\s*$/.test(line)) {
      out.push("<hr>\n");
      i++;
      continue;
    }
    if (/^\s*>/.test(line)) {
      const body: string[] = [];
      while (i < lines.length && !blank(lines[i])) body.push(lines[i++].replace(/^\s*>\s?/, ""));
      out.push(`<blockquote>${renderDocString(body.join("\n"), ctx)}</blockquote>`);
      continue;
    }
    const bullet = /^(\s*)([-*+]|\d+[.)])\s+/.exec(line);
    if (bullet) {
      const ordered = /\d/.test(bullet[2]);
      const items: string[][] = [];
      while (i < lines.length && !blank(lines[i])) {
        const m = /^(\s*)([-*+]|\d+[.)])\s+(.*)$/.exec(lines[i]);
        if (m) items.push([m[3]]);
        else if (items.length > 0) items[items.length - 1].push(lines[i].trim());
        i++;
      }
      const lis = items.map((it) => `<li>${renderInline(it.join("\n"), ctx)}</li>`).join("");
      out.push(ordered ? `<ol>${lis}</ol>` : `<ul>${lis}</ul>`);
      continue;
    }
    const para: string[] = [];
    // A bullet list, a heading, a block quote and a fence all interrupt a
    // paragraph in CommonMark; an ordered list only does when it starts at 1,
    // which is not distinguished here.
    while (
      i < lines.length && !blank(lines[i]) &&
      !/^(#{1,6}\s|\s*>|\s*```|\s*[-*+]\s)/.test(lines[i])
    ) {
      para.push(lines[i++]);
    }
    if (para.length === 0) {
      // The line that stopped the loop was the first one; take it verbatim so
      // the loop cannot spin.
      para.push(lines[i++]);
    }
    out.push(`<p>${renderInline(trimBlockLines(para), ctx)}</p>`);
  }
  return out.join("");
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
    if (c === "*" && s[i + 1] === "*") {
      const close = s.indexOf("**", i + 2);
      if (close > i + 2) {
        push();
        out += `<strong>${renderInline(s.slice(i + 2, close), ctx)}</strong>`;
        i = close + 2;
        continue;
      }
    }
    if (c === "*") {
      const close = s.indexOf("*", i + 1);
      if (close > i + 1) {
        push();
        out += `<em>${renderInline(s.slice(i + 1, close), ctx)}</em>`;
        i = close + 1;
        continue;
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
 * `structureToHtml` + `fieldToHtml` (Structure.lean). Two things doc-gen4 prints
 * here are **not in the IR** and are therefore missing from the output:
 *   * `f.args` -- the binders of the field's own signature (stage4b README,
 *     "What is tagged, and what is not": 940 constant occurrences with no
 *     position, all of them member binders, and the binder *text* is not stored
 *     either);
 *   * `f.doc` -- the field docstring (`div.structure_field_doc`).
 * `f.isDirect` is missing too, so every field is rendered as a direct one.
 */
function structureHtml(d: Decl, root: string, refs: Map<string, string>, r: Renderer): string {
  const ctor = d.members.find((m) => m.label === "ctor");
  const fields = d.members.filter((m) => m.label === "field");
  pageStats.memberTables++;
  const lis = fields
    .map((f) => {
      pageStats.memberFields++;
      const short = f.name.split(".").pop()!;
      const body = r.fragment(f.text, f.code ?? [], root, refs);
      return `<li id="${escapeHtml(f.name)}" class="structure_field">` +
        `<div class="structure_field_info">${escapeHtml(short)} : ${body.html}</div></li>`;
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
  module: string,
  root: string,
  moduleSourceUrl: string,
  header: string,
  ctx: DocCtx,
  r: Renderer,
): string {
  const refs = new Map<string, string>();
  for (const [m, n] of d.refs) refs.set(n, m);

  pageStats.ghLinks++;
  const gh = `<div class="gh_link"><a href="${
    escapeHtml(`${moduleSourceUrl}#L${d.line}-L${d.endLine}`)
  }">source</a></div>`;

  // `div.attributes` -- doc-gen4 prints `@[...]` here. The IR has no attributes
  // (stage4b README, "out of scope"), so nothing is emitted: 89 occurrences on
  // this target are simply missing.

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
      body = structureHtml(d, root, refs, r);
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
    `${gh}${header}${doc}${body}${extra}</div></div>`;
}

/** `moduleToHtml` + `baseHtmlGenerator`. */
function pageHtml(mod: ModuleFile, headers: Map<string, string>, r: Renderer): string {
  const module = mod.module;
  const root = pageRoot(module);
  const moduleSourceUrl = `${SOURCE_URL}/${module.split(".").join("/")}.lean`;
  const ctx: DocCtx = {
    root,
    moduleDeclNames: mod.declarations.map((d) => d.name),
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
    main += declHtml(d, module, root, moduleSourceUrl, headers.get(d.name)!, ctx, r);
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
if (index.schemaVersion < 3) {
  console.error(
    `schemaVersion ${index.schemaVersion}: need a schema-3 IR ` +
      `(experiments/stage7a/run.sh ... --tagged-code). Schema 2 has no splitWhitespaces widths.`,
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
const accounted = T.preMain + T.readIr + T.indexBuild + T.renderHeaders + T.renderPage +
  T.flatten + T.write;
const timingBlock = [
  `## phases (seconds, in-process; \`total\` is timeOrigin -> here)`,
  ``,
  `preMain (module init; see TIMERS) ${secs(T.preMain)}`,
  `read IR (readTextFile + parse)   ${secs(T.readIr)}`,
  `index build (name map + sets)    ${secs(T.indexBuild)}`,
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
      seconds: Object.fromEntries(
        Object.entries(T).map(([k, v]) => [k, Number((v / 1000).toFixed(6))]),
      ),
      secondsAccounted: Number((accounted / 1000).toFixed(6)),
    }) + "\n",
  );
}
