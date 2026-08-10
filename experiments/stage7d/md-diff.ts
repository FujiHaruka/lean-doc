#!/usr/bin/env -S deno run --allow-read --allow-write --allow-env
// md-diff.ts -- the CommonMark iteration loop: for every prose region that is
// not byte-identical, print the **markdown source** next to both renderings.
//
// A diagnostic, not an oracle. The segmentation below is a copy of the one in
// `experiments/stage4c/coverage.ts` (which is never modified); the scores in the
// summary always come from that program, never from this one.
//
// usage: md-diff.ts --pages <dir> --ir <dir> [--doc-root <dir>] [--report <path>]
//                   [--context N] [--only <Module>]

const argv = Deno.args.slice();
const opt = (name: string, dflt = "") => {
  const i = argv.indexOf(name);
  return i >= 0 ? argv[i + 1] : dflt;
};
const PAGES = opt("--pages");
const IR = opt("--ir");
const DOC_ROOT = opt(
  "--doc-root",
  "/Users/haruka/dev/lean-projects/.lake/build/doc/InformationTheory",
);
const REPORT = opt("--report");
const CONTEXT = Number(opt("--context", "160"));
const ONLY = opt("--only");
const FULL = argv.includes("--full");
if (!PAGES || !IR) {
  console.error("usage: md-diff.ts --pages <dir> --ir <dir> [--report <path>] [--only <Module>]");
  Deno.exit(2);
}

const enc = new TextEncoder();
const u8 = (s: string) => enc.encode(s).length;

// ------------------------------------------------ tag scanner (copy of stage4c)

const VOID = new Set(["br", "hr", "img", "wbr"]);
type Tag = { close: boolean; name: string; start: number; end: number; text: string };
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
    return { close: m[1] === "/", name: m[2].toLowerCase(), start: lt, end: gt + 1, text: html.slice(lt, gt + 1) };
  }
}
function isVoidTag(html: string, t: Tag): boolean {
  if (VOID.has(t.name)) return true;
  if (t.name === "input") return !html.startsWith("</input>", t.end);
  return false;
}
function elementEnd(html: string, pos: number): number {
  const open = nextTag(html, pos)!;
  if (open.start !== pos) throw new Error(`no tag at ${pos}`);
  if (isVoidTag(html, open)) return open.end;
  let depth = 0;
  let i = pos;
  for (;;) {
    const t = nextTag(html, i);
    if (t === null) throw new Error(`unclosed <${open.name}> at ${pos}`);
    if (t.close) {
      depth--;
      if (depth === 0) return t.end;
    } else if (!isVoidTag(html, t)) depth++;
    i = t.end;
  }
}
type Child = { kind: "el" | "text"; start: number; end: number; tag: string; attrs: string };
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

type Seg = { region: string; key: string; start: number; end: number };
function segmentPage(html: string): Seg[] {
  const segs: Seg[] = [];
  const push = (region: string, key: string, a: number, b: number) => {
    if (b > a) segs.push({ region, key, start: a, end: b });
  };
  const htmlOpen = nextTag(html, 0)!;
  const headEnd = elementEnd(html, htmlOpen.end);
  push("frame", "frame:html-open", 0, htmlOpen.end);
  push("head", "head", htmlOpen.end, headEnd);
  const bodyOpen = nextTag(html, headEnd)!;
  const inputEnd = elementEnd(html, bodyOpen.end);
  push("frame", "frame:body-open", headEnd, inputEnd);
  const headerEnd = elementEnd(html, inputEnd);
  push("header", "header", inputEnd, headerEnd);
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
      const href = /href="#([^"]*)"/.exec(html.slice(c.start, c.end));
      push("nav_links", `nav_links:${href ? href[1] : c.start}`, c.start, c.end);
    }
  }
  const mainOpen = nextTag(html, navEnd)!;
  const mainEnd = elementEnd(html, navEnd);
  const mainCloseStart = html.lastIndexOf("</main>", mainEnd);
  let mainContent = mainOpen.end;
  if (html[mainContent] === "\n") mainContent++;
  push("frame", "frame:main-open", navEnd, mainContent);
  push("frame", "frame:tail", mainCloseStart, html.length);
  let modDocs = 0;
  for (const c of children(html, mainContent, mainCloseStart)) {
    if (c.kind === "text") throw new Error(`unexpected text in <main>`);
    const cls = attr(c.attrs, "class") ?? "";
    if (cls === "mod_doc") {
      push("mod_doc", `mod_doc:${modDocs++}`, c.start, c.end);
      continue;
    }
    if (cls !== "decl" && cls !== "decl sorried") throw new Error(`unexpected <main> child class ${cls}`);
    segmentDecl(html, c, push);
  }
  segs.sort((a, b) => a.start - b.start);
  let p = 0;
  for (const s of segs) {
    if (s.start !== p) throw new Error(`segmentation gap/overlap at ${p} != ${s.start} (${s.key})`);
    p = s.end;
  }
  if (p !== html.length) throw new Error(`segmentation stops at ${p}, file is ${html.length}`);
  return segs;
}
function segmentDecl(html: string, decl: Child, push: (r: string, k: string, a: number, b: number) => void) {
  const name = attr(decl.attrs, "id")!;
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
      if (docStart < 0) docStart = c.start;
      docEnd = c.end;
      continue;
    }
    flushDoc();
    let end = c.end;
    if (region === "attributes" && html[end] === "\n") {
      end++;
      const nx = kids[ki + 1];
      if (nx && nx.kind === "text" && nx.start === c.end) {
        if (nx.end === end) ki++;
        else nx.start = end;
      }
    }
    push(region, `${region}:${name}`, c.start, end);
  }
  flushDoc();
}

// ------------------------------------------------------------- markdown source

type Member = { label: string; name: string; doc?: string | null };
type Decl = { name: string; doc?: string; members: Member[] };
type ModuleFile = { module: string; declarations: Decl[]; moduleDocs: { text: string }[] };

const index = JSON.parse(await Deno.readTextFile(`${IR}/index.json`)) as {
  modules: { module: string; file: string }[];
};
/** module -> (region key -> markdown). Member docstrings land in `members`, not
 *  in a `docstring` region, so they are keyed on the owning declaration. */
const sources = new Map<string, Map<string, string[]>>();
for (const e of index.modules) {
  const mod = JSON.parse(await Deno.readTextFile(`${IR}/${e.file}`)) as ModuleFile;
  const m = new Map<string, string[]>();
  (mod.moduleDocs ?? []).forEach((md, i) => m.set(`mod_doc:${i}`, [md.text]));
  for (const d of mod.declarations) {
    if (d.doc !== undefined) m.set(`docstring:${d.name}`, [d.doc]);
    const memberDocs = (d.members ?? []).filter((f) => f.doc).map((f) => `[${f.name}] ${f.doc}`);
    if (memberDocs.length > 0) m.set(`members:${d.name}`, memberDocs);
  }
  sources.set(mod.module, m);
}

// ------------------------------------------------------------------- the diff

async function* walk(dir: string): AsyncGenerator<string> {
  for await (const e of Deno.readDir(dir)) {
    const p = `${dir}/${e.name}`;
    if (e.isDirectory) yield* walk(p);
    else if (e.isFile && e.name.endsWith(".html")) yield p;
  }
}

const PROSE = new Set(["docstring", "mod_doc", "members"]);
type Case = { module: string; key: string; bytes: number; mine: string; theirs: string; md: string[] };
const cases: Case[] = [];

for await (const path of walk(DOC_ROOT)) {
  const rel = path.slice(DOC_ROOT.length + 1);
  const module = "InformationTheory." + rel.slice(0, -".html".length).split("/").join(".");
  if (ONLY && module !== ONLY) continue;
  const theirs = await Deno.readTextFile(path);
  let mine: string;
  try {
    mine = await Deno.readTextFile(`${PAGES}/${module.split(".").join("/")}.html`);
  } catch {
    continue;
  }
  const theirSegs = segmentPage(theirs);
  const mineByKey = new Map(segmentPage(mine).map((s) => [s.key, mine.slice(s.start, s.end)]));
  for (const s of theirSegs) {
    if (!PROSE.has(s.region)) continue;
    const text = theirs.slice(s.start, s.end);
    const m = mineByKey.get(s.key);
    if (m === undefined || m === text) continue;
    cases.push({
      module,
      key: s.key,
      bytes: u8(text),
      mine: m,
      theirs: text,
      md: sources.get(module)?.get(s.key) ?? ["(source not found in the IR)"],
    });
  }
}

cases.sort((a, b) => b.bytes - a.bytes);
const lines: string[] = [];
lines.push(`# md-diff — byte 一致しない prose 領域 (診断。採点は coverage.ts)`);
lines.push(``);
lines.push(`pages ${PAGES}`);
lines.push(`ir    ${IR}`);
lines.push(`date  ${new Date().toISOString()}`);
lines.push(``);
lines.push(`領域 ${cases.length} / バイト ${cases.reduce((a, c) => a + c.bytes, 0).toLocaleString("en-US")}`);
lines.push(``);
for (const c of cases) {
  let i = 0;
  while (i < c.mine.length && i < c.theirs.length && c.mine[i] === c.theirs[i]) i++;
  lines.push(`### ${c.module} ${c.key} (${c.bytes} B, first diff @${i})`);
  lines.push(`--- markdown ---`);
  for (const md of c.md) lines.push(JSON.stringify(md));
  if (FULL) {
    lines.push(`--- mine ---`);
    lines.push(JSON.stringify(c.mine));
    lines.push(`--- theirs ---`);
    lines.push(JSON.stringify(c.theirs));
  } else {
    lines.push(`mine : ${JSON.stringify(c.mine.slice(Math.max(0, i - 40), i + CONTEXT))}`);
    lines.push(`their: ${JSON.stringify(c.theirs.slice(Math.max(0, i - 40), i + CONTEXT))}`);
  }
  lines.push(``);
}
const out = lines.join("\n");
if (REPORT) await Deno.writeTextFile(REPORT, out);
else console.log(out);
console.error(`${cases.length} regions, ${cases.reduce((a, c) => a + c.bytes, 0)} bytes`);
