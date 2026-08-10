#!/usr/bin/env -S deno run --allow-read --allow-write --allow-env
//
// Stage 4 increment 4: **byte accounting** of the generated module pages against
// doc-gen4's own pages, region by region.
//
// The question this answers is not "does it look right" but "of the bytes
// doc-gen4 writes, which ones can be produced from the IR, and which ones
// cannot, and why". A region counts as reproduced only when it is **byte
// identical**; emitting plausible HTML in the right place counts for nothing.
//
// usage:
//   coverage.ts --pages <dir> [--doc-root <dir>] [--report <path.txt>]
//               [--diffs <path.txt>] [--max-diffs N]
//
//   --pages     the generator's output tree (render.ts --pages)
//   --doc-root  doc-gen4's HTML, default $TARGET_REPO (else
//               /Users/haruka/dev/lean-projects) + .lake/build/doc/InformationTheory
//
// The doc tree is opened read-only; the measurement target is never written to.
//
// SEGMENTATION
//   Every page is cut into non-overlapping regions that **tile the file** -- the
//   tool asserts that the segment byte lengths sum to the file size, so a region
//   cannot be quietly dropped from the denominator. The cut points come from
//   doc-gen4's own structure (`Output/Template.lean`, `Output/Module.lean`):
//
//     frame          <html>, <body>, the nav-toggle input, <main>, the trailing
//                    <nav class="nav"> and the closing tags
//     head           <head> ... </head>
//     header         <header> ... </header>
//     nav_top        the "return to top" <p> inside nav.internal_nav
//     nav_gh         the "source" <p> inside nav.internal_nav (configuration)
//     nav_imports    div.imports (the import list + the imported-by stub)
//     nav_links      the per-declaration jump list
//     nav_frame      the <nav class="internal_nav"> tags themselves
//     mod_doc        div.mod_doc -- a module docstring (CommonMark)
//     decl_frame     div.decl + div.<kind> open/close tags
//     gh_link        div.gh_link
//     attributes     div.attributes
//     decl_header    div.decl_header   (increment 3's territory)
//     docstring      the declaration docstring (CommonMark)
//     members        ul.structure_fields / ul.structure_ext / ul.constructors
//     equations      details > ul.equations
//     instances      details.instances-for-list / details.instances
//
//   Both sides go through the same segmenter. That is fine here because the
//   comparison is a byte comparison: a shared misreading of the markup can move
//   a byte from one region to another, but it cannot turn a difference into a
//   match. The page-level byte-identity number below is computed without the
//   segmenter at all, as a cross-check on that.

const argv = Deno.args.slice();
const opt = (n: string, d = "") => {
  const i = argv.indexOf(n);
  return i >= 0 ? argv[i + 1] : d;
};
const PAGES = opt("--pages");
const TARGET = Deno.env.get("TARGET_REPO") ?? "/Users/haruka/dev/lean-projects";
const DOC_ROOT = opt("--doc-root", `${TARGET}/.lake/build/doc/InformationTheory`);
const REPORT = opt("--report");
const DIFFS = opt("--diffs");
const MAX_DIFFS = Number(opt("--max-diffs", "5")); // per region
if (!PAGES) {
  console.error("usage: coverage.ts --pages <dir> [--doc-root <dir>] [--report <txt>] [--diffs <txt>]");
  Deno.exit(2);
}

const enc = new TextEncoder();
const u8 = (s: string) => enc.encode(s).length;

// ---------------------------------------------------------------- tag scanner

/** Tags doc-gen4 emits unclosed. All of them come from `Html.raw` in DocString.lean. */
const VOID = new Set(["br", "hr", "img", "wbr"]);

type Tag = { close: boolean; name: string; start: number; end: number; text: string };

/** The next tag at or after `i`. Attribute values are `Html.escape`d, so no `>` inside. */
function nextTag(html: string, i: number): Tag | null {
  for (;;) {
    const lt = html.indexOf("<", i);
    if (lt < 0) return null;
    const m = /^<(\/?)([a-zA-Z][a-zA-Z0-9-]*)/.exec(html.slice(lt, lt + 40));
    if (!m) {
      i = lt + 1;
      continue;
    }
    const gt = html.indexOf(">", lt);
    if (gt < 0) return null;
    return {
      close: m[1] === "/",
      name: m[2].toLowerCase(),
      start: lt,
      end: gt + 1,
      text: html.slice(lt, gt + 1),
    };
  }
}

/**
 * `input` is the one ambiguous tag: doc-gen4's own two are written
 * `<input …></input>`, and the one a markdown task list produces is raw and
 * unclosed. Decide by looking at what follows.
 */
function isVoidTag(html: string, t: Tag): boolean {
  if (VOID.has(t.name)) return true;
  if (t.name === "input") return !html.startsWith("</input>", t.end);
  return false;
}

/** End offset (exclusive) of the element whose open tag starts at `pos`. */
function elementEnd(html: string, pos: number): number {
  const open = nextTag(html, pos)!;
  if (open.start !== pos) throw new Error(`no tag at ${pos}: ${JSON.stringify(html.slice(pos, pos + 40))}`);
  if (isVoidTag(html, open)) return open.end;
  let depth = 0;
  let i = pos;
  for (;;) {
    const t = nextTag(html, i);
    if (t === null) throw new Error(`unclosed <${open.name}> at ${pos}`);
    if (t.close) {
      depth--;
      if (depth === 0) return t.end;
    } else if (!isVoidTag(html, t)) {
      depth++;
    }
    i = t.end;
  }
}

type Child = { kind: "el" | "text"; start: number; end: number; tag: string; attrs: string };

/** The top-level children of the range `[start, end)`. */
function children(html: string, start: number, end: number): Child[] {
  const out: Child[] = [];
  let i = start;
  while (i < end) {
    const t = nextTag(html, i);
    if (t === null || t.start >= end) {
      out.push({ kind: "text", start: i, end, tag: "", attrs: "" });
      break;
    }
    if (t.start > i) out.push({ kind: "text", start: i, end: t.start, tag: "", attrs: "" });
    const e = elementEnd(html, t.start);
    out.push({ kind: "el", start: t.start, end: e, tag: t.name, attrs: t.text });
    i = e;
  }
  return out.filter((c) => c.end > c.start);
}

const attr = (openTag: string, name: string): string | null => {
  const m = new RegExp(`\\s${name}="([^"]*)"`).exec(openTag);
  return m ? m[1] : null;
};

// ---------------------------------------------------------------- segmentation

type Seg = { region: string; key: string; start: number; end: number };

function segmentPage(html: string): Seg[] {
  const segs: Seg[] = [];
  const push = (region: string, key: string, a: number, b: number) => {
    if (b > a) segs.push({ region, key, start: a, end: b });
  };

  // <html lang="en">
  const htmlOpen = nextTag(html, 0)!;
  // <head> ... </head>
  const headEnd = elementEnd(html, htmlOpen.end);
  push("frame", "frame:html-open", 0, htmlOpen.end);
  push("head", "head", htmlOpen.end, headEnd);

  // <body> <input id="nav_toggle" …></input>
  const bodyOpen = nextTag(html, headEnd)!;
  const inputEnd = elementEnd(html, bodyOpen.end);
  push("frame", "frame:body-open", headEnd, inputEnd);

  // <header> … </header>
  const headerEnd = elementEnd(html, inputEnd);
  push("header", "header", inputEnd, headerEnd);

  // <nav class="internal_nav"> … </nav>
  const navOpen = nextTag(html, headerEnd)!;
  const navEnd = elementEnd(html, headerEnd);
  const navCloseStart = html.lastIndexOf("</nav>", navEnd);
  push("nav_frame", "nav_frame:open", headerEnd, navOpen.end);
  push("nav_frame", "nav_frame:close", navCloseStart, navEnd);
  let navSeen = 0;
  for (const c of children(html, navOpen.end, navCloseStart)) {
    if (c.kind !== "el") throw new Error("unexpected text in internal_nav");
    if (c.tag === "p") {
      navSeen++;
      push(navSeen === 1 ? "nav_top" : "nav_gh", navSeen === 1 ? "nav_top" : "nav_gh", c.start, c.end);
    } else if (c.tag === "div" && attr(c.attrs, "class") === "imports") {
      push("nav_imports", "nav_imports", c.start, c.end);
    } else {
      // `declarationToNavLink`: the target name is the key, so the two sides
      // line up even when one of them has the wrong number of entries.
      const href = /href="#([^"]*)"/.exec(html.slice(c.start, c.end));
      push("nav_links", `nav_links:${href ? href[1] : c.start}`, c.start, c.end);
    }
  }

  // <main> … </main>
  const mainOpen = nextTag(html, navEnd)!;
  const mainEnd = elementEnd(html, navEnd);
  const mainCloseStart = html.lastIndexOf("</main>", mainEnd);
  // `Html.element "main" false` prints "<main>\n" -- the newline is frame.
  let mainContent = mainOpen.end;
  if (html[mainContent] === "\n") mainContent++;
  push("frame", "frame:main-open", navEnd, mainContent);
  push("frame", "frame:tail", mainCloseStart, html.length);

  let modDocs = 0;
  for (const c of children(html, mainContent, mainCloseStart)) {
    if (c.kind === "text") throw new Error(`unexpected text in <main>: ${JSON.stringify(html.slice(c.start, c.end))}`);
    const cls = attr(c.attrs, "class") ?? "";
    if (cls === "mod_doc") {
      push("mod_doc", `mod_doc:${modDocs++}`, c.start, c.end);
      continue;
    }
    if (cls !== "decl" && cls !== "decl sorried") throw new Error(`unexpected <main> child class ${cls}`);
    segmentDecl(html, c, push);
  }

  segs.sort((a, b) => a.start - b.start);
  // The tiling check: no gap, no overlap, and the last segment ends at EOF.
  let p = 0;
  for (const s of segs) {
    if (s.start !== p) throw new Error(`segmentation gap/overlap at ${p} != ${s.start} (${s.key})`);
    p = s.end;
  }
  if (p !== html.length) throw new Error(`segmentation stops at ${p}, file is ${html.length}`);
  return segs;
}

function segmentDecl(
  html: string,
  decl: Child,
  push: (region: string, key: string, a: number, b: number) => void,
) {
  const name = attr(decl.attrs, "id")!;
  // `div.decl` wraps exactly one `div.<kind>`, and both close at the very end.
  const inner = nextTag(html, decl.start + decl.attrs.length)!;
  const innerCloseStart = html.lastIndexOf("</div>", html.lastIndexOf("</div>", decl.end) - 1);
  push("decl_frame", `decl_open:${name}`, decl.start, inner.end);
  push("decl_frame", `decl_close:${name}`, innerCloseStart, decl.end);

  let docStart = -1;
  let docEnd = -1;
  const flushDoc = () => {
    if (docStart >= 0) push("docstring", `docstring:${name}`, docStart, docEnd);
    docStart = -1;
  };
  const kids = children(html, inner.end, innerCloseStart);
  for (let ki = 0; ki < kids.length; ki++) {
    const c = kids[ki];
    const cls = c.kind === "el" ? (attr(c.attrs, "class") ?? "") : "";
    const id = c.kind === "el" ? (attr(c.attrs, "id") ?? "") : "";
    let region: string | null = null;
    if (c.kind === "el" && c.tag === "div" && cls === "gh_link") region = "gh_link";
    else if (c.kind === "el" && c.tag === "div" && cls === "attributes") region = "attributes";
    else if (c.kind === "el" && c.tag === "div" && cls === "decl_header") region = "decl_header";
    else if (
      c.kind === "el" && c.tag === "ul" &&
      (cls === "structure_fields" || cls === "structure_ext" || cls === "constructors")
    ) region = "members";
    else if (c.kind === "el" && c.tag === "details" && html.startsWith("<summary>Equations</summary>", c.start + c.attrs.length)) {
      region = "equations";
    } else if (
      c.kind === "el" && c.tag === "details" &&
      (cls === "instances-for-list" || cls === "instances" || id.startsWith("instances-for-list-"))
    ) region = "instances";

    if (region === null) {
      // Not a marker doc-gen4 puts after the header, so it is docstring output.
      if (docStart < 0) docStart = c.start;
      docEnd = c.end;
      continue;
    }
    flushDoc();
    // `div.attributes` is the one `Html.element … false` at this level, so it
    // prints a trailing "\n" that `children` sees as a separate text node.
    let end = c.end;
    if (region === "attributes" && html[end] === "\n") {
      end++;
      const nx = kids[ki + 1];
      if (nx && nx.kind === "text" && nx.start === c.end) {
        if (nx.end === end) ki++; // the text node was exactly that newline
        else nx.start = end; // it had more after it
      }
    }
    push(region, `${region}:${name}`, c.start, end);
  }
  flushDoc();
}

// ---------------------------------------------------------------- corpus

async function* walk(dir: string): AsyncGenerator<string> {
  for await (const e of Deno.readDir(dir)) {
    const p = `${dir}/${e.name}`;
    if (e.isDirectory) yield* walk(p);
    else if (e.name.endsWith(".html")) yield p;
  }
}

const theirPaths: string[] = [];
for await (const p of walk(DOC_ROOT)) theirPaths.push(p);
theirPaths.sort();

let minePages = 0;
for await (const _ of walk(PAGES)) minePages++;

// ---------------------------------------------------------------- accounting

type Row = {
  theirBytes: number;
  theirCount: number;
  mineBytes: number;
  mineCount: number;
  matchedBytes: number;
  matchedCount: number;
  missingCount: number; // key present in doc-gen4's page, absent from ours
  extraCount: number; // key present in ours only
  extraBytes: number;
  // How the mismatching regions differ.
  diffWsOnly: number; // same length, every difference is whitespace-vs-whitespace
  diffLength: number; // different length
  diffOther: number; // same length, a non-whitespace difference
};
const rows = new Map<string, Row>();
const row = (r: string): Row => {
  let x = rows.get(r);
  if (!x) {
    x = {
      theirBytes: 0,
      theirCount: 0,
      mineBytes: 0,
      mineCount: 0,
      matchedBytes: 0,
      matchedCount: 0,
      missingCount: 0,
      extraCount: 0,
      extraBytes: 0,
      diffWsOnly: 0,
      diffLength: 0,
      diffOther: 0,
    };
    rows.set(r, x);
  }
  return x;
};

/** Same classification `bytes.ts` uses, so the two tools can be read together. */
function classify(mine: string, theirs: string): "ws" | "len" | "other" {
  if (mine.length !== theirs.length) return "len";
  for (let i = 0; i < mine.length; i++) {
    if (mine[i] === theirs[i]) continue;
    if (!/\s/.test(mine[i]) || !/\s/.test(theirs[i])) return "other";
  }
  return "ws";
}

// The four numbers `benchmarks/results/stage4-html-inventory.txt` §C already
// measured, recomputed here from scratch as a check on the segmenter.
const inv = { total: 0, chrome: 0, head: 0, header: 0, internalNav: 0, declHeader: 0, ulEquations: 0 };

let pagesIdentical = 0;
let pagesIdenticalIgnoringProse = 0;
let pagesIdenticalIgnoringRev = 0;
let pagesMissing = 0;
let pagesUnsegmentable = 0;
const segmentErrors: string[] = [];
const examples: string[] = [];
const exampleCount = new Map<string, number>();
let ghRevOnly = 0;
let ghOther = 0;
let ghTotal = 0;
/** Fields doc-gen4 prints with binders the IR does not store. */
let fieldsWithBinders = 0;
let fieldsWithDoc = 0;
let fieldsTotal = 0;

/** Regions that carry CommonMark output; see "docstring" in the report. */
const PROSE = new Set(["docstring", "mod_doc"]);

/**
 * Every unmatched byte is attributed to exactly one cause, by rule -- not by
 * hand, and not per case. The rules are applied in this order:
 *   1. a `gh_link` / `nav_gh` whose only difference is the 40-hex revision
 *      -> configuration; the IR never had it (and doc-gen4's own tree carries
 *      two different revisions, see §D)
 *   2. same length, whitespace-vs-whitespace only -> the `splitWhitespaces`
 *      gap that stage4b's schema 2 cannot close
 *   3. a docstring / module docstring that becomes identical once every `<a>`
 *      is stripped from both sides -> the autolink index; doc-gen4 resolves the
 *      name against the whole environment and the IR only carries what the
 *      signatures referred to (`stage4-html-inventory.txt` §F)
 *   3b. any other docstring / module docstring -> CommonMark not implemented
 *   4. nothing emitted at all -> the IR has no such field
 *   5. a member table -> the IR has the field types but not the field binders
 *      or the field docstrings
 *   6. anything else -> unclassified, i.e. a bug to look at
 */
const CAUSES = [
  "設定値 (rev) — IR に無い",
  "splitWhitespaces — IR schema 2 の既知欠落",
  "docstring の autolink 索引が IR に無い",
  "CommonMark 未実装",
  "IR に情報が無い (領域ごと欠落)",
  "IR に情報が無い (メンバの binder / docstring)",
  "未分類",
] as const;
const causeBytes = new Map<string, number>();
const causeCount = new Map<string, number>();
const addCause = (c: string, bytes: number) => {
  causeBytes.set(c, (causeBytes.get(c) ?? 0) + bytes);
  causeCount.set(c, (causeCount.get(c) ?? 0) + 1);
};
/** Characters that actually differ, for the mismatches where the length is unchanged. */
let diffCharsWs = 0;

// ---- fine-grained accounting added for the stage-7b breakdown -------------
const fine = new Map<string, { bytes: number; count: number }>();
const fineAdd = (cause: string, region: string, bytes: number) => {
  const k = `${cause} ${region}`;
  const x = fine.get(k) ?? { bytes: 0, count: 0 };
  x.bytes += bytes;
  x.count++;
  fine.set(k, x);
};
let proseAnchorsMine = 0, proseAnchorsTheir = 0;
let autolinkAnchorsMine = 0, autolinkAnchorsTheir = 0, autolinkRegions = 0;
let autolinkTheirMore = 0, autolinkMineMore = 0, autolinkSameCount = 0;
let autolinkTagBytesTheir = 0;
const autolinkTargets = new Map<string, number>();
const autolinkMissingNames = new Set<string>();
const cmRegions: { module: string; key: string; bytes: number; mine: string; their: string }[] = [];
const MISSING = "領域ごと欠落(missing key)";
const EXTRA = "生成側にしか無い領域(extra key)";
/** cause set per page -> how many pages, how many bytes of miss */
const pageCauseSets = new Map<string, { pages: number; bytes: number }>();
let pageCauses = new Set<string>();
let pageMissBytes = 0;

for (const path of theirPaths) {
  pageCauses = new Set<string>();
  pageMissBytes = 0;
  const rel = path.slice(DOC_ROOT.length + 1);
  const module = "InformationTheory." + rel.slice(0, -".html".length).split("/").join(".");
  const theirs = await Deno.readTextFile(path);
  inv.total += u8(theirs);

  let mine: string | null = null;
  try {
    mine = await Deno.readTextFile(`${PAGES}/${module.split(".").join("/")}.html`);
  } catch {
    mine = null;
  }
  if (mine === null) pagesMissing++;
  if (mine !== null && mine === theirs) pagesIdentical++;

  let theirSegs: Seg[];
  try {
    theirSegs = segmentPage(theirs);
  } catch (e) {
    pagesUnsegmentable++;
    segmentErrors.push(`${module}: doc-gen4 side: ${(e as Error).message}`);
    continue;
  }
  let mineSegs: Seg[] = [];
  if (mine !== null) {
    try {
      mineSegs = segmentPage(mine);
    } catch (e) {
      segmentErrors.push(`${module}: generated side: ${(e as Error).message}`);
      mineSegs = [];
    }
  }

  // §C cross-check, computed the way html-inventory.py computes it.
  {
    const headStart = theirs.indexOf("<head>");
    const headStop = theirs.indexOf("</head>") + "</head>".length;
    const hdrStart = theirs.indexOf("<header>");
    const hdrStop = theirs.indexOf("</header>") + "</header>".length;
    const navStart = theirs.indexOf('<nav class="internal_nav">');
    const navStop = elementEnd(theirs, navStart);
    inv.head += u8(theirs.slice(headStart, headStop));
    inv.header += u8(theirs.slice(hdrStart, hdrStop));
    inv.internalNav += u8(theirs.slice(navStart, navStop));
    inv.chrome += u8(theirs.slice(0, navStop));
    for (const s of theirSegs) {
      if (s.region === "decl_header") inv.declHeader += u8(theirs.slice(s.start, s.end));
      if (s.region === "equations") {
        const ul = theirs.indexOf('<ul class="equations">', s.start);
        inv.ulEquations += u8(theirs.slice(ul, elementEnd(theirs, ul)));
      }
      if (s.region === "members") {
        const seg = theirs.slice(s.start, s.end);
        fieldsTotal += (seg.match(/<div class="structure_field_info">/g) ?? []).length;
        fieldsWithBinders += (seg.match(/<span class="decl_args">/g) ?? []).length;
        fieldsWithDoc += (seg.match(/<div class="structure_field_doc">/g) ?? []).length;
      }
    }
  }

  const mineByKey = new Map(mineSegs.map((s) => [s.key, mine!.slice(s.start, s.end)]));
  const usedKeys = new Set<string>();
  let proseOnlyMismatch = mine !== null;
  let revOnlyMismatch = mine !== null;
  for (const s of theirSegs) {
    const text = theirs.slice(s.start, s.end);
    const r = row(s.region);
    r.theirBytes += u8(text);
    r.theirCount++;
    const m = mineByKey.get(s.key);
    if (m === undefined) {
      r.missingCount++;
      addCause(CAUSES[4], u8(text));
      fineAdd(MISSING, s.region, u8(text));
      pageCauses.add(MISSING + ":" + s.region);
      pageMissBytes += u8(text);
      revOnlyMismatch = false;
      if (!PROSE.has(s.region)) proseOnlyMismatch = false;
      continue;
    }
    usedKeys.add(s.key);
    if (m === text) {
      r.matchedBytes += u8(text);
      r.matchedCount++;
      continue;
    }
    if (!PROSE.has(s.region)) proseOnlyMismatch = false;
    const cl = classify(m, text);
    if (cl === "ws") r.diffWsOnly++;
    else if (cl === "len") r.diffLength++;
    else r.diffOther++;
    const revless = (x: string) => x.replace(/\/blob\/[0-9a-f]{40}\//, "/blob/REV/");
    const revOnly = (s.region === "gh_link" || s.region === "nav_gh") && revless(m) === revless(text);
    if (s.region === "gh_link") {
      ghTotal++;
      if (revOnly) ghRevOnly++;
      else ghOther++;
    }
    // Prose is checked before the whitespace rule: a whitespace difference in a
    // docstring is this renderer's CommonMark approximation, not the IR's
    // `splitWhitespaces` gap, and folding the two together would inflate the
    // gap the schema-3 change is supposed to close.
    const noAnchors = (x: string) => x.replace(/<a [^>]*>|<\/a>/g, "");
    const cause = revOnly
      ? CAUSES[0]
      : PROSE.has(s.region)
      ? (noAnchors(m) === noAnchors(text) ? CAUSES[2] : CAUSES[3])
      : cl === "ws"
      ? CAUSES[1]
      : s.region === "members"
      ? CAUSES[5]
      : CAUSES[6];
    addCause(cause, u8(text));
    fineAdd(cause, s.region, u8(text));
    pageCauses.add(cause);
    pageMissBytes += u8(text);
    if (PROSE.has(s.region)) {
      const aMine = (m.match(/<a [^>]*>/g) ?? []).length;
      const aTheir = (text.match(/<a [^>]*>/g) ?? []).length;
      proseAnchorsMine += aMine;
      proseAnchorsTheir += aTheir;
      if (cause === CAUSES[2]) {
        autolinkAnchorsMine += aMine;
        autolinkAnchorsTheir += aTheir;
        autolinkRegions++;
        if (aTheir > aMine) autolinkTheirMore++;
        else if (aMine > aTheir) autolinkMineMore++;
        else autolinkSameCount++;
        // bytes of the anchor tags themselves, both sides
        for (const t of text.match(/<a [^>]*>|<\/a>/g) ?? []) autolinkTagBytesTheir += u8(t);
        // which link targets do we fail to emit?
        const mineHrefs = new Set((m.match(/<a href="[^"]*"/g) ?? []));
        for (const a of text.match(/<a href="[^"]*"/g) ?? []) {
          if (mineHrefs.has(a)) continue;
          const href = a.slice('<a href="'.length, -1);
          const frag = href.includes("#") ? href.slice(href.indexOf("#") + 1) : "";
          const path = href.split("#")[0].replace(/^(\.\.?\/)+/, "").replace(/\.html$/, "");
          const top = path.split("/")[0] || "(same page)";
          autolinkTargets.set(top, (autolinkTargets.get(top) ?? 0) + 1);
          if (frag) autolinkMissingNames.add(frag);
        }
      } else {
        cmRegions.push({
          module,
          key: s.key,
          bytes: u8(text),
          mine: m,
          their: text,
        });
      }
    }
    if (!revOnly) revOnlyMismatch = false;
    if (cause === CAUSES[1]) {
      for (let i = 0; i < m.length; i++) if (m[i] !== text[i]) diffCharsWs++;
    }
    // Sampled per region, not globally: 497 `gh_link` diffs would otherwise
    // fill the file and hide the one `members` diff.
    const seen = exampleCount.get(s.region) ?? 0;
    if (seen < MAX_DIFFS) {
      exampleCount.set(s.region, seen + 1);
      let i = 0;
      while (i < m.length && i < text.length && m[i] === text[i]) i++;
      examples.push(
        `${module} ${s.key}\n  first difference at char ${i}\n` +
          `  mine : ${JSON.stringify(m.slice(Math.max(0, i - 60), i + 60))}\n` +
          `  their: ${JSON.stringify(text.slice(Math.max(0, i - 60), i + 60))}`,
      );
    }
  }
  for (const s of mineSegs) {
    const text = mine!.slice(s.start, s.end);
    const r = row(s.region);
    r.mineBytes += u8(text);
    r.mineCount++;
    if (!usedKeys.has(s.key) && !theirSegs.some((t) => t.key === s.key)) {
      r.extraCount++;
      r.extraBytes += u8(text);
      fineAdd(EXTRA, s.region, u8(text));
      pageCauses.add(EXTRA + ":" + s.region);
      revOnlyMismatch = false;
      if (!PROSE.has(s.region)) proseOnlyMismatch = false;
    }
  }
  if (proseOnlyMismatch) pagesIdenticalIgnoringProse++;
  if (revOnlyMismatch) pagesIdenticalIgnoringRev++;
  const key = [...pageCauses].sort().join(" + ") || "(byte 一致)";
  const pc = pageCauseSets.get(key) ?? { pages: 0, bytes: 0 };
  pc.pages++;
  pc.bytes += pageMissBytes;
  pageCauseSets.set(key, pc);
}

// ---------------------------------------------------------------- report

const n = (x: number) => x.toLocaleString("en-US");
const pct = (a: number, b: number) => (b === 0 ? "—" : `${((100 * a) / b).toFixed(1)}%`);
const out: string[] = [];
const say = (s = "") => out.push(s);

const ORDER = [
  "decl_header",
  "nav_links",
  "docstring",
  "mod_doc",
  "head",
  "equations",
  "decl_frame",
  "nav_imports",
  "gh_link",
  "instances",
  "header",
  "members",
  "frame",
  "nav_frame",
  "nav_top",
  "nav_gh",
  "attributes",
];
const known = new Set(ORDER);
for (const k of rows.keys()) if (!known.has(k)) ORDER.push(k);

let totTheir = 0, totMatched = 0, totMine = 0;
for (const [, r] of rows) {
  totTheir += r.theirBytes;
  totMatched += r.matchedBytes;
  totMine += r.mineBytes;
}

say("# coverage — モジュールページ全体の領域別バイト会計 (実測)");
say();
say(`pages(mine)  ${PAGES}`);
say(`doc-root     ${DOC_ROOT}`);
say(`date         ${new Date().toISOString().replace(/\.\d+Z$/, "Z")}`);
say(`deno         ${Deno.version.deno} / V8 ${Deno.version.v8}`);
say();
say("母数は **doc-gen4 がディスクに出した 348 ページ**。生成側は IR の 432 モジュール分を");
say(`書いているが (${n(minePages)} ファイル)、doc-gen4 側に無い 84 ページは採点していない。`);
say();
say("## A. 領域別 (母数 = doc-gen4 の 348 ページ = " + n(inv.total) + " バイト)");
say();
say("「再現」= **その領域が byte 完全一致した**もの。それらしい HTML を出しただけのものは 0 と数える。");
say();
say("| 領域 | doc-gen4 のバイト | 比 | 生成側のバイト | 再現できたバイト | 再現率 | 領域数 | 一致 | 生成側に無い | 生成側にしか無い |");
say("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|");
for (const k of ORDER) {
  const r = rows.get(k);
  if (!r) continue;
  say(
    `| \`${k}\` | ${n(r.theirBytes)} | ${pct(r.theirBytes, inv.total)} | ${n(r.mineBytes)} | ` +
      `${n(r.matchedBytes)} | ${pct(r.matchedBytes, r.theirBytes)} | ${n(r.theirCount)} | ` +
      `${n(r.matchedCount)} | ${n(r.missingCount)} | ${n(r.extraCount)} |`,
  );
}
say(
  `| **合計** | **${n(totTheir)}** | 100.0% | **${n(totMine)}** | **${n(totMatched)}** | ` +
    `**${pct(totMatched, totTheir)}** | | | | |`,
);
say();
say("### 一致しなかった領域の内訳");
say();
say("`bytes.ts` と同じ 3 分類。「空白のみ」は長さが同じで違いが空白同士だけのもの —");
say("stage4b README の `splitWhitespaces` の欠落がこれに当たる。");
say();
say("| 領域 | 不一致 | 空白のみ (長さ同じ) | 長さが違う | 長さ同じ・空白以外 |");
say("|---|---:|---:|---:|---:|");
for (const k of ORDER) {
  const r = rows.get(k);
  if (!r) continue;
  const d = r.diffWsOnly + r.diffLength + r.diffOther;
  if (d === 0 && r.missingCount === 0) continue;
  say(`| \`${k}\` | ${n(d)} | ${n(r.diffWsOnly)} | ${n(r.diffLength)} | ${n(r.diffOther)} |`);
}
say();
say(`領域に切ったバイトの合計 ${n(totTheir)} と 348 ページの実バイト ${n(inv.total)} は` +
  `${totTheir === inv.total ? "一致する (領域は隙間なくページを覆っている)" : "**食い違っている — 切り出しが壊れている**"}。`);
say();

say("### 再現できなかったバイトの原因 (規則で機械的に振り分け、手作業の割り当てなし)");
say();
say("| 原因 | バイト | 母数比 | 領域数 |");
say("|---|---:|---:|---:|");
let causeTotal = 0;
for (const c of CAUSES) {
  const b = causeBytes.get(c) ?? 0;
  causeTotal += b;
  if (b === 0 && (causeCount.get(c) ?? 0) === 0) continue;
  say(`| ${c} | ${n(b)} | ${pct(b, inv.total)} | ${n(causeCount.get(c) ?? 0)} |`);
}
say(`| **合計 (= 母数 − 再現できたバイト)** | **${n(causeTotal)}** | ${pct(causeTotal, inv.total)} | |`);
say();
say(
  `再現できたバイト ${n(totMatched)} + 再現できなかったバイト ${n(causeTotal)} = ${n(totMatched + causeTotal)}` +
    `${totMatched + causeTotal === inv.total ? " = 母数" : " ≠ 母数 — **会計が合っていない**"}。`,
);
say();
say("**バイトの帰属は全か無かであることに注意。** 1 文字違えば領域まるごと未再現に数える。");
say(
  `長さが変わらない不一致で実際に食い違っている文字は全領域あわせて **${n(diffCharsWs)}** 文字しかなく、` +
    "その領域の合計バイトは " + n(causeBytes.get(CAUSES[1]) ?? 0) + " バイトある。",
);
say("この 2 つの数の差が、次の担当が「あと何を直せばページが byte 一致するか」を読むときの罠。");
say();

say("## B. 既存実測値 (stage4-html-inventory.txt §C) の再現");
say();
say("同じ 348 ページを別実装で切り直して、前回の数字が出るかを見る。出なければこちらの切り出しが疑わしい。");
say();
say("| | 前回 (§C) | 今回 | |");
say("|---|---:|---:|---|");
const chk = (label: string, was: number, now: number) =>
  say(`| ${label} | ${n(was)} | ${n(now)} | ${was === now ? "一致" : "**不一致**"} |`);
chk("348 ページ合計", 22028728, inv.total);
chk("`div.decl_header`", 15661530, inv.declHeader);
chk("`ul.equations`", 497903, inv.ulEquations);
chk("chrome (head+header+internal_nav の前置き)", 2065396, inv.chrome);
chk("… うち `head`", 428608, inv.head);
chk("… うち `header`", 166883, inv.header);
chk("… うち `nav.internal_nav`", 1445893, inv.internalNav);
say();

say("## C. ページ単位");
say();
say("| | |");
say("|---|---:|");
say(`| doc-gen4 のページ | ${n(theirPaths.length)} |`);
say(`| 生成側に同名のページがあった | ${n(theirPaths.length - pagesMissing)} |`);
say(`| **byte 完全一致したページ** | **${n(pagesIdentical)}** |`);
say(`| 差が docstring / mod_doc にしか無いページ (完全一致を含む) | ${n(pagesIdenticalIgnoringProse)} |`);
say(`| 差が gh_link/nav_gh の rev にしか無いページ (完全一致を含む) | ${n(pagesIdenticalIgnoringRev)} |`);
say(`| 領域に切れなかったページ | ${n(pagesUnsegmentable)} |`);
say();
say("下の 2 行は完全一致ページを含む上位集合で、互いに排他でもない。**「あと 1 つ直せば");
say("一致するページ数」ではない** — その領域はいま一致していない。差がどこに集中しているかを");
say("次の担当が読むための内訳として出している。");
say();

say("## D. `gh_link` の不一致の内訳");
say();
say("| | |");
say("|---|---:|");
say(`| 不一致 | ${n(ghTotal)} |`);
say(`| … rev (40 桁 hex) だけが違う | ${n(ghRevOnly)} |`);
say(`| … それ以外 | ${n(ghOther)} |`);
say();

say("## E. 構造体メンバ表の中身 (doc-gen4 側の実測)");
say();
say("| | |");
say("|---|---:|");
say(`| \`div.structure_field_info\` | ${n(fieldsTotal)} |`);
say(`| その中の \`span.decl_args\` (メンバ署名の binder。IR に無い) | ${n(fieldsWithBinders)} |`);
say(`| \`div.structure_field_doc\` (IR に無い) | ${n(fieldsWithDoc)} |`);
say();

if (segmentErrors.length > 0) {
  say("## 切り出しに失敗したページ");
  say();
  for (const e of segmentErrors.slice(0, 20)) say(`* ${e}`);
  say();
}

// ---------------------------------------------------------------- fine-grained
say("## X1. 原因 × 領域 (母数 = " + n(inv.total) + " B)");
say();
say("| 原因 | 領域 | バイト | 母数比 | 領域数 |");
say("|---|---|---:|---:|---:|");
for (const [k, v] of [...fine.entries()].sort((a, b) => b[1].bytes - a[1].bytes)) {
  const i = k.lastIndexOf(" ");
  say(`| ${k.slice(0, i)} | \`${k.slice(i + 1)}\` | ${n(v.bytes)} | ${pct(v.bytes, inv.total)} | ${n(v.count)} |`);
}
say();

say("## X2. ページごとの「残っている原因の組」");
say();
say("そのページを byte 一致させるために**全部**直す必要がある原因の集合。");
say();
say("| 原因の組 | ページ | 未再現バイト |");
say("|---|---:|---:|");
for (const [k, v] of [...pageCauseSets.entries()].sort((a, b) => b[1].pages - a[1].pages)) {
  say(`| ${k} | ${n(v.pages)} | ${n(v.bytes)} |`);
}
say();

say("## X3. autolink 索引 (CAUSES[2]) の中身");
say();
say("| | |");
say("|---|---:|");
say(`| 領域数 | ${n(autolinkRegions)} |`);
say(`| doc-gen4 側の \`<a …>\` 総数 | ${n(autolinkAnchorsTheir)} |`);
say(`| 生成側の \`<a …>\` 総数 | ${n(autolinkAnchorsMine)} |`);
say(`| doc-gen4 のほうがアンカーが多い領域 | ${n(autolinkTheirMore)} |`);
say(`| 生成側のほうが多い領域 | ${n(autolinkMineMore)} |`);
say(`| 数が同じ (href だけ違う) 領域 | ${n(autolinkSameCount)} |`);
say(`| doc-gen4 側のアンカータグ自体のバイト | ${n(autolinkTagBytesTheir)} |`);
say(`| (参考) prose 全体のアンカー数 mine / theirs | ${n(proseAnchorsMine)} / ${n(proseAnchorsTheir)} |`);
say();
say("生成側に出せていないリンクの**あて先**の内訳:");
say();
say("| あて先のトップレベル | アンカー数 |");
say("|---|---:|");
for (const [k, v] of [...autolinkTargets.entries()].sort((a, b) => b[1] - a[1])) say(`| \`${k}\` | ${n(v)} |`);
say(`\n出せていない名前の種類: **${n(autolinkMissingNames.size)}**。`);
say();

say("## X4. CommonMark 未実装 (CAUSES[3]) の 1 件ずつ");
say();
say("| module | key | バイト | 最初の差の位置 | 長さ mine/theirs |");
say("|---|---|---:|---:|---|");
for (const c of cmRegions.sort((a, b) => b.bytes - a.bytes)) {
  let i = 0;
  while (i < c.mine.length && i < c.their.length && c.mine[i] === c.their[i]) i++;
  say(`| ${c.module} | ${c.key} | ${n(c.bytes)} | ${n(i)} | ${n(c.mine.length)}/${n(c.their.length)} |`);
}
say();
{
  const dump: string[] = [];
  for (const c of cmRegions) {
    let i = 0;
    while (i < c.mine.length && i < c.their.length && c.mine[i] === c.their[i]) i++;
    dump.push(
      `### ${c.module} ${c.key} (${c.bytes} B, first diff @${i})\n` +
        `mine : ${JSON.stringify(c.mine.slice(Math.max(0, i - 120), i + 200))}\n` +
        `their: ${JSON.stringify(c.their.slice(Math.max(0, i - 120), i + 200))}\n`,
    );
  }
  await Deno.writeTextFile(opt("--cm-dump", "/dev/null"), dump.join("\n"));
}
{
  const c = (s: string, re: RegExp) => (s.match(re) ?? []).length;
  const FEAT: [string, RegExp][] = [
    ["indented code block (`<pre><code>`)", /<pre><code>/g],
    ["loose list item (`<li><p>`)", /<li><p>/g],
    ["nested list (`<li>…<ul>/<ol>`)", /<(ul|ol)>/g],
    ["blockquote", /<blockquote>/g],
    ["heading", /<h[1-6][ >]/g],
    ["table", /<table>/g],
    ["em/strong", /<(em|strong)>/g],
    ["inline code (`<code>`)", /<code>/g],
    ["hard break (`<br>`)", /<br\/?>/g],
  ];
  const bucket = new Map<string, { n: number; bytes: number }>();
  for (const r of cmRegions) {
    let label = "その他 (上記の素性は一致、中身が違う)";
    for (const [name, re] of FEAT) {
      if (c(r.mine, re) !== c(r.their, re)) {
        label = `${name}: ${c(r.mine, re)} → ${c(r.their, re)}`.replace(/: \d+ → \d+/, "");
        break;
      }
    }
    const b = bucket.get(label) ?? { n: 0, bytes: 0 };
    b.n++;
    b.bytes += r.bytes;
    bucket.set(label, b);
  }
  say("### X4b. CommonMark 47 件を構文別に (最初に個数が食い違った素性で分類)");
  say();
  say("| 構文 | 領域 | バイト | 母数比 |");
  say("|---|---:|---:|---:|");
  for (const [k, v] of [...bucket.entries()].sort((a, b) => b[1].bytes - a[1].bytes)) {
    say(`| ${k} | ${n(v.n)} | ${n(v.bytes)} | ${pct(v.bytes, inv.total)} |`);
  }
  say();
}
const cmTotal = cmRegions.reduce((a, b) => a + b.bytes, 0);
say(`CommonMark 47 件の合計 ${n(cmTotal)} B。`);
say();

const text = out.join("\n") + "\n";
console.log(text);
if (REPORT) await Deno.writeTextFile(REPORT, text);
if (DIFFS) await Deno.writeTextFile(DIFFS, examples.join("\n\n") + "\n");
