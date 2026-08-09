#!/usr/bin/env -S deno run --allow-read --allow-write
//
// Stage 4 increment 3: rebuild doc-gen4's `div.decl_header` from the schema-2 IR
// **without starting Lean**.
//
// This is the generator half of the acceptance test; `compare.ts` is the scoring
// half. The two are deliberately separate programs: this one never reads the
// ground truth (`decl-header-truth.jsonl`), it only reads the IR and the
// specification transcribed from doc-gen4's own source.
//
// usage:
//   render.ts --ir <dir> --out <path.jsonl> [--only <Module>] [--stats <path>]
//             [--ws-heuristic]
//
//   --ir      schema-2 IR root (experiments/stage4b/run.sh --write-ir --tagged-code)
//   --out     JSONL, one rendered declaration per line: {module, name, html}
//   --only    render just this module (repeatable), for the small loop
//   --stats   also write the counter block here
//   --ws-heuristic  guess back doc-gen4's `splitWhitespaces` whitespace rewrite
//             (see "The splitWhitespaces gap" below). **Off by default**: the
//             default run is the faithful one, and the honest number is the one
//             it produces. This flag exists to measure whether the information
//             the IR drops is recoverable, not to improve the score.
//
// THE splitWhitespaces GAP
//   `renderTagged` (RenderedCode.lean:249-256) pushes a `.const` tag's leading
//   and trailing whitespace outside the anchor via `splitWhitespaces`
//   (Base.lean:290-297), which **rebuilds it as spaces** (`"".pushn ' ' n`). So
//   a token whose tag text was `" ≤\n  "` renders as `" ≤   "`: same character
//   count, so no offset moves, but different characters. The IR stores the
//   pretty printer's original text plus the *narrowed* span, so the tag's full
//   extent -- the only thing that says which whitespace was inside the tag -- is
//   not recoverable from it.
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
// The one part of `Html.toStringAux` that matters for plaintext offsets is the
// `flatten` (second) argument of `Html.element`: JSX literals build it `true`
// (no newlines emitted), and the two `Html.element ... false` calls in the path
// -- `span.decl_kind` and `argToHtml`'s `span.decl_args` -- emit `\n` after the
// open tag (unless the only child is text/raw) and after the close tag. Those
// newlines are part of the header's plaintext, so they are part of every anchor
// offset after them.

type Span = [number, number, number] | [number, number, 1, string];

type Member = { label: string; name: string; text: string; code: Span[] };

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
  index: number;
  members: Member[];
  refs: [string, string][];
};

type ModuleFile = { module: string; declarations: Decl[]; schemaVersion: number };

type IndexEntry = { module: string; file: string };
type DepEntry = { file: string };
type Index = {
  schemaVersion: number;
  generator: string;
  modules: IndexEntry[];
  dependencyMaps: DepEntry[];
};

// ---------------------------------------------------------------- CLI

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
const ONLY = opts("--only");
const STATS = opt("--stats");
if (!IR || !OUT) {
  console.error(
    "usage: render.ts --ir <dir> --out <path.jsonl> [--only <Module>]... [--stats <path>]",
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
  /** Characters `--ws-heuristic` rewrote (0 when the flag is off). */
  wsHeuristicChars: 0,
  /** Fragments it touched. */
  wsHeuristicFragments: 0,
};

const WS_HEURISTIC = argv.includes("--ws-heuristic");

/**
 * The `--ws-heuristic` guess: every whitespace run touching a childless `kind 1`
 * span becomes spaces. It is a guess because the run may be only partly inside
 * the tag -- the IR cannot say -- so it can rewrite whitespace doc-gen4 left
 * alone. Whether it does is what the flag is for measuring.
 */
function applyWsHeuristic(text: string, roots: Node[]): string {
  // One entry per UTF-16 code unit, because that is the unit the spans index.
  // Splitting a surrogate pair is harmless here: neither half matches `\s`, and
  // `join("")` puts them back together unchanged.
  const units: string[] = new Array(text.length);
  for (let i = 0; i < text.length; i++) units[i] = text[i];
  let changed = 0;
  const isWs = (i: number) => i >= 0 && i < units.length && /\s/.test(units[i]);
  const visit = (n: Node) => {
    if (n.kind === 1 && n.children.length === 0) {
      for (let i = n.start - 1; isWs(i); i--) {
        if (units[i] !== " ") {
          units[i] = " ";
          changed++;
        }
      }
      for (let i = n.stop; isWs(i); i++) {
        if (units[i] !== " ") {
          units[i] = " ";
          changed++;
        }
      }
    }
    for (const c of n.children) visit(c);
  };
  for (const r of roots) visit(r);
  if (changed === 0) return text;
  stats.wsHeuristicChars += changed;
  stats.wsHeuristicFragments++;
  return units.join("");
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
    const t = WS_HEURISTIC ? applyWsHeuristic(text, roots) : text;
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
      stats.anchorsSort++;
      return {
        html: `<a href="${escapeHtml(root + "foundational_types.html")}">${inner.html}</a>`,
        hasAnchor: true,
      };
    }
    // kind 1: `.const name`, Base.lean:337-373.
    const link = this.constLink(n.name, root, refs);
    if (link === null) {
      stats.constSpansUnlinkable++;
      return { html: `<span class="fn">${inner.html}</span>`, hasAnchor: inner.hasAnchor };
    }
    if (inner.hasAnchor) {
      stats.constSpansSuppressedByNesting++;
      return { html: inner.html, hasAnchor: true };
    }
    stats.anchorsConst++;
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
        if (!refs.has(name)) stats.constSpansNameNotInRefs++;
        return moduleLink(root, mod) + "#" + name;
      }
    } else {
      stats.constSpansPrivate++;
    }
    // Step 1: auxiliary name removal.
    const search = isPrivate ? privateToUserName(name) : name;
    const parent = findLinkableParent(this.known, search);
    if (parent !== null) {
      stats.constSpansViaParent++;
      return moduleLink(root, this.known.get(parent)!) + "#" + parent;
    }
    // Step 2: module link for a private name.
    if (isPrivate) {
      const mod = moduleFromPrivatePrefix(name);
      if (mod !== null) {
        stats.anchorsModuleFallback++;
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

// ---------------------------------------------------------------- main

const index: Index = JSON.parse(await Deno.readTextFile(`${IR}/index.json`));
if (index.schemaVersion < 2) {
  console.error(`schemaVersion ${index.schemaVersion}: need a --tagged-code IR (schema 2)`);
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
  const map = JSON.parse(await Deno.readTextFile(`${IR}/${dep.file}`));
  for (const [n, m] of Object.entries(map.declarations as Record<string, string>)) known.set(n, m);
}

const modules: ModuleFile[] = [];
for (const entry of index.modules) {
  const mod: ModuleFile = JSON.parse(await Deno.readTextFile(`${IR}/${entry.file}`));
  modules.push(mod);
  stats.modulesRead++;
  for (const d of mod.declarations) {
    stats.declarationsInIr++;
    known.set(d.name, mod.module);
    for (const [m, n] of d.refs) if (!known.has(n)) known.set(n, m);
  }
}

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
  for (const d of decls) {
    stats.declarationsRendered++;
    lines.push(
      JSON.stringify({ module: mod.module, name: d.name, html: declHeader(d, mod.module, renderer) }),
    );
  }
}
await Deno.writeTextFile(OUT, lines.join("\n") + "\n");

const report = [
  `# render — decl_header rebuilt from the IR (no Lean)`,
  ``,
  `ir              ${IR}`,
  `schemaVersion   ${index.schemaVersion}`,
  `generator       ${index.generator}`,
  `out             ${OUT}`,
  `only            ${ONLY.length > 0 ? ONLY.join(", ") : "(all modules)"}`,
  ``,
  ...Object.entries(stats).map(([k, v]) => `${k.padEnd(32)} ${v.toLocaleString("en-US")}`),
  ``,
].join("\n");
console.log(report);
if (STATS) await Deno.writeTextFile(STATS, report);
