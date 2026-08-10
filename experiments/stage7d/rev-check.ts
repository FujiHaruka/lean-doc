#!/usr/bin/env -S deno run --allow-read --allow-write --allow-env
// rev-check.ts -- read the *content* of every region `coverage.ts` still counts
// as unreproduced, and check that the label it puts on them ("設定値 (rev)") is
// what the bytes actually say.
//
// Why this exists: `coverage.ts` assigns causes **by rule**, and stage 7b showed
// a rule can outlive its premise (every `members` mismatch was labelled "member
// data missing"; both remaining ones were autolink). So the last label standing
// gets read by hand once, here.
//
// The check: strip the 40-hex revision from both sides of every mismatching
// region; if what is left is identical, the difference really is only the
// revision. Also counts which revisions doc-gen4's own tree used.
//
// usage: rev-check.ts --pages <dir> [--doc-root <dir>] [--report <path>]

const argv = Deno.args.slice();
const opt = (name: string, dflt = "") => {
  const i = argv.indexOf(name);
  return i >= 0 ? argv[i + 1] : dflt;
};
const PAGES = opt("--pages");
const DOC_ROOT = opt(
  "--doc-root",
  "/Users/haruka/dev/lean-projects/.lake/build/doc/InformationTheory",
);
const REPORT = opt("--report");
if (!PAGES) {
  console.error("usage: rev-check.ts --pages <dir> [--doc-root <dir>] [--report <path>]");
  Deno.exit(2);
}

async function* walk(dir: string): AsyncGenerator<string> {
  for await (const e of Deno.readDir(dir)) {
    const p = `${dir}/${e.name}`;
    if (e.isDirectory) yield* walk(p);
    else if (e.isFile && e.name.endsWith(".html")) yield p;
  }
}

/** Every `href` whose path carries a `/blob/<40 hex>/`. */
const HREF = /href="([^"]*\/blob\/([0-9a-f]{40})\/[^"]*)"/g;

const revsTheirs = new Map<string, number>();
const revsMine = new Map<string, number>();
let hrefsCompared = 0;
let hrefsRevOnly = 0;
let hrefsOther = 0;
const otherExamples: string[] = [];
let pagesRevOnly = 0;
let pagesIdentical = 0;
let pagesOther = 0;
let pages = 0;
/** Pages whose difference is a permutation of the declaration order -- which
 *  `coverage.ts` cannot see, because it keys every region on the declaration
 *  name and not on its position. */
const orderOnly: string[] = [];
const otherPages: string[] = [];

for await (const path of walk(DOC_ROOT)) {
  const rel = path.slice(DOC_ROOT.length + 1);
  const module = "InformationTheory." + rel.slice(0, -".html".length).split("/").join(".");
  const theirs = await Deno.readTextFile(path);
  let mine: string;
  try {
    mine = await Deno.readTextFile(`${PAGES}/${module.split(".").join("/")}.html`);
  } catch {
    continue;
  }
  pages++;
  for (const m of theirs.matchAll(HREF)) revsTheirs.set(m[2], (revsTheirs.get(m[2]) ?? 0) + 1);
  for (const m of mine.matchAll(HREF)) revsMine.set(m[2], (revsMine.get(m[2]) ?? 0) + 1);

  const theirHrefs = [...theirs.matchAll(HREF)].map((m) => m[1]);
  const myHrefs = [...mine.matchAll(HREF)].map((m) => m[1]);
  const n = Math.min(theirHrefs.length, myHrefs.length);
  for (let i = 0; i < n; i++) {
    hrefsCompared++;
    if (theirHrefs[i] === myHrefs[i]) continue;
    const blank = (s: string) => s.replace(/\/blob\/[0-9a-f]{40}\//, "/blob/REV/");
    if (blank(theirHrefs[i]) === blank(myHrefs[i])) hrefsRevOnly++;
    else {
      hrefsOther++;
      if (otherExamples.length < 10) {
        otherExamples.push(`${module}\n  mine  : ${myHrefs[i]}\n  theirs: ${theirHrefs[i]}`);
      }
    }
  }
  if (theirHrefs.length !== myHrefs.length) {
    hrefsOther++;
    if (otherExamples.length < 10) {
      otherExamples.push(`${module}: href count ${myHrefs.length} != ${theirHrefs.length}`);
    }
  }

  // Whole page: identical after blanking the revision on both sides?
  const blankAll = (s: string) => s.replace(/\/blob\/[0-9a-f]{40}\//g, "/blob/REV/");
  if (mine === theirs) pagesIdentical++;
  else if (blankAll(mine) === blankAll(theirs)) pagesRevOnly++;
  else {
    pagesOther++;
    const ids = (s: string) =>
      [...s.matchAll(/<div class="decl[^"]*" id="([^"]*)"/g)].map((x) => x[1]);
    const a = ids(mine);
    const b = ids(theirs);
    const perm = a.length === b.length &&
      JSON.stringify([...a].sort()) === JSON.stringify([...b].sort()) &&
      JSON.stringify(a) !== JSON.stringify(b);
    let k = 0;
    while (k < a.length && a[k] === b[k]) k++;
    if (perm) orderOnly.push(`${module}: ${a.length} 宣言、最初にずれるのは #${k} (mine ${a[k]} / theirs ${b[k]})`);
    else otherPages.push(module);
  }
}

const lines: string[] = [];
lines.push(`# rev-check — 残った未再現領域のラベルを中身で確認 (診断)`);
lines.push(``);
lines.push(`pages(mine)  ${PAGES}`);
lines.push(`doc-root     ${DOC_ROOT}`);
lines.push(`date         ${new Date().toISOString()}`);
lines.push(``);
lines.push(`## ページ`);
lines.push(``);
lines.push(`| | |`);
lines.push(`|---|---:|`);
lines.push(`| 比較したページ | ${pages} |`);
lines.push(`| byte 完全一致 | ${pagesIdentical} |`);
lines.push(`| **rev を伏せると byte 完全一致** | **${pagesRevOnly}** |`);
lines.push(`| それ以外の差が残るページ | ${pagesOther} |`);
lines.push(`| … うち宣言の**並び順**だけが違う (coverage.ts には見えない) | ${orderOnly.length} |`);
lines.push(`| … それ以外 | ${otherPages.length} |`);
lines.push(``);
if (orderOnly.length > 0) {
  lines.push(
    `\`coverage.ts\` は領域を宣言名で対応付けるので、**同じ集合の並べ替えは一致と数える**。` +
      `再現率には出ないが byte は違う:`,
  );
  lines.push(``);
  for (const o of orderOnly) lines.push(`* ${o}`);
  lines.push(``);
}
if (otherPages.length > 0) {
  lines.push(`並び順で説明できない差の残るページ: ${otherPages.join(", ")}`);
  lines.push(``);
}
lines.push(`## \`/blob/<40 hex>/\` を含む href`);
lines.push(``);
lines.push(`| | |`);
lines.push(`|---|---:|`);
lines.push(`| 比較した href | ${hrefsCompared} |`);
lines.push(`| 一致しなかったもののうち rev だけが違う | ${hrefsRevOnly} |`);
lines.push(`| それ以外 | ${hrefsOther} |`);
lines.push(``);
lines.push(`## doc-gen4 自身の参照ツリーが持つ revision`);
lines.push(``);
lines.push(`| revision | doc-gen4 側の href 数 | 生成側 |`);
lines.push(`|---|---:|---:|`);
for (const [rev, n] of [...revsTheirs].sort((a, b) => b[1] - a[1])) {
  lines.push(`| \`${rev}\` | ${n} | ${revsMine.get(rev) ?? 0} |`);
}
lines.push(``);
lines.push(
  `**\`--source-url\` は 1 本しか渡せないので、2 つの revision を同時には出せない。**`,
);
if (otherExamples.length > 0) {
  lines.push(``);
  lines.push(`## rev 以外の差 (あってはいけない)`);
  lines.push(``);
  for (const e of otherExamples) lines.push(e);
}
const out = lines.join("\n") + "\n";
console.log(out);
if (REPORT) await Deno.writeTextFile(REPORT, out);
