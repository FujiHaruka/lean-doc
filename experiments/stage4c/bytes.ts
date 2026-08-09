#!/usr/bin/env -S deno run --allow-read --allow-write
//
// Stage 4 increment 3, third check: byte-for-byte diff of `render.ts`'s markup
// against the `div.decl_header` substrings of doc-gen4's own pages.
//
// `compare.ts` scores the *extracted* anchor sequences, so a bug shared between
// its extractor and the ground truth's extractor could hide behind a 100%. This
// tool compares the raw markup instead: no extraction, no normalisation, no
// tolerance. It is the check that makes the compare.ts number believable, and
// it is also the one that sees differences compare.ts is designed to ignore
// (attribute order, `class` values, elements with no text).
//
// usage:
//   bytes.ts --mine <render.jsonl> --doc-root <dir> [--report <path.txt>]
//            [--diffs <path.txt>] [--max-diffs N]
//
//   --doc-root  doc-gen4's HTML, default $TARGET_REPO (else
//               /Users/haruka/dev/lean-projects) + .lake/build/doc/InformationTheory
//
// The doc tree is opened read-only; the measurement target is never written to.

const argv = Deno.args.slice();
const opt = (n: string, d = "") => {
  const i = argv.indexOf(n);
  return i >= 0 ? argv[i + 1] : d;
};
const MINE = opt("--mine");
const TARGET = Deno.env.get("TARGET_REPO") ?? "/Users/haruka/dev/lean-projects";
const DOC_ROOT = opt("--doc-root", `${TARGET}/.lake/build/doc/InformationTheory`);
const REPORT = opt("--report");
const DIFFS = opt("--diffs");
const MAX_DIFFS = Number(opt("--max-diffs", "40"));
if (!MINE) {
  console.error("usage: bytes.ts --mine <jsonl> [--doc-root <dir>] [--report <txt>] [--diffs <txt>]");
  Deno.exit(2);
}

/** End offset (exclusive) of the element opening at `start`, by tag depth. */
function blockEnd(text: string, start: number, tag: string): number {
  const re = new RegExp(`</?${tag}\\b`, "g");
  re.lastIndex = start;
  let depth = 0;
  for (;;) {
    const m = re.exec(text);
    if (!m) return text.length;
    if (text.startsWith("</", m.index)) {
      depth--;
      if (depth === 0) return text.indexOf(">", m.index) + 1;
    } else {
      depth++;
    }
  }
}

async function* walk(dir: string): AsyncGenerator<string> {
  for await (const e of Deno.readDir(dir)) {
    const p = `${dir}/${e.name}`;
    if (e.isDirectory) yield* walk(p);
    else if (e.name.endsWith(".html")) yield p;
  }
}

/** `<root>/Fano/Core.html` -> `InformationTheory.Fano.Core`. */
function pageModule(root: string, path: string): string {
  const rel = path.slice(root.length + 1, -".html".length);
  const base = root.split("/").filter((x) => x.length > 0).pop()!;
  return base + "." + rel.split("/").join(".");
}

const DECL_RE = /<div class="decl(?: sorried)?" id="([^"]*)">/g;

const theirs = new Map<string, string>();
let pages = 0;
const roots: string[] = [];
for await (const p of walk(DOC_ROOT)) roots.push(p);
roots.sort();
for (const path of roots) {
  pages++;
  const text = await Deno.readTextFile(path);
  const module = pageModule(DOC_ROOT, path);
  DECL_RE.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = DECL_RE.exec(text)) !== null) {
    const end = blockEnd(text, m.index, "div");
    const seg = text.slice(m.index, end);
    const hi = seg.indexOf('<div class="decl_header">');
    if (hi < 0) continue;
    const hj = blockEnd(seg, hi, "div");
    theirs.set(`${module} ${m[1]}`, seg.slice(hi, hj));
    DECL_RE.lastIndex = end;
  }
}

const mine = new Map<string, string>();
for (const line of (await Deno.readTextFile(MINE)).split("\n")) {
  if (!line) continue;
  const r = JSON.parse(line) as { module: string; name: string; html: string };
  mine.set(`${r.module} ${r.name}`, r.html);
}

let identical = 0;
let differ = 0;
let missing = 0;
let wsOnly = 0;
let lenDiffer = 0;
const examples: string[] = [];
for (const [k, t] of theirs) {
  const m = mine.get(k);
  if (m === undefined) {
    missing++;
    continue;
  }
  if (m === t) {
    identical++;
    continue;
  }
  differ++;
  if (m.length === t.length) {
    let ws = true;
    for (let i = 0; i < m.length; i++) {
      if (m[i] === t[i]) continue;
      if (!/\s/.test(m[i]) || !/\s/.test(t[i])) {
        ws = false;
        break;
      }
    }
    if (ws) wsOnly++;
  } else {
    lenDiffer++;
  }
  if (examples.length < MAX_DIFFS) {
    let i = 0;
    while (i < m.length && i < t.length && m[i] === t[i]) i++;
    examples.push(
      `${k}\n  first difference at char ${i}\n` +
        `  mine : ${JSON.stringify(m.slice(Math.max(0, i - 40), i + 40))}\n` +
        `  their: ${JSON.stringify(t.slice(Math.max(0, i - 40), i + 40))}`,
    );
  }
}

const n = (x: number) => x.toLocaleString("en-US");
const out: string[] = [];
const say = (s = "") => out.push(s);
say("# bytes — 生成した decl_header マークアップと doc-gen4 のページの byte 比較 (実測)");
say();
say(`mine      ${MINE}`);
say(`doc-root  ${DOC_ROOT}`);
say(`date      ${new Date().toISOString().replace(/\.\d+Z$/, "Z")}`);
say(`deno      ${Deno.version.deno} / V8 ${Deno.version.v8}`);
say();
say("| | |");
say("|---|---:|");
say(`| 読んだ HTML ページ | ${n(pages)} |`);
say(`| ページ側の div.decl_header | ${n(theirs.size)} |`);
say(`| 生成側にも同じ宣言があった | ${n(theirs.size - missing)} |`);
say(`| **byte 完全一致** | **${n(identical)}** |`);
say(`| 不一致 | ${n(differ)} |`);
say(`| … 長さは同じで空白文字だけが違う | ${n(wsOnly)} |`);
say(`| … 長さが違う | ${n(lenDiffer)} |`);
say(`| … 長さは同じだが空白以外も違う | ${n(differ - wsOnly - lenDiffer)} |`);
say(`| 生成側に無かった宣言 | ${n(missing)} |`);
say();
const text = out.join("\n") + "\n";
console.log(text);
if (REPORT) await Deno.writeTextFile(REPORT, text);
if (DIFFS) await Deno.writeTextFile(DIFFS, examples.join("\n\n") + "\n");
