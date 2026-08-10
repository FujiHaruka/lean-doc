#!/usr/bin/env -S deno run --allow-read --allow-write --allow-env
//
// Stage 7b diagnostic — **not** an oracle. `experiments/stage4c/coverage.ts`
// decides the score; this file only answers two questions the score cannot:
//
//   A. Did the member markup land in the right *proportion*? stage4c's README
//      records a regex over `li.structure_field` that matched 4 of 153 blocks
//      because it depended on attribute order, and nothing noticed for a while.
//      So every marker is counted on both sides, per page, and any page where
//      the two counts differ is printed. A count that is right in total but
//      wrong per page is still wrong.
//
//   B. Of the `members` regions that still do not match byte for byte, how many
//      are missing member *data* and how many are missing *links inside a field
//      docstring*? `coverage.ts` attributes every non-matching `members` region
//      to "the IR has no member binders / docstrings", because that rule was
//      written when that was the only possibility. The test applied here is the
//      one `coverage.ts` itself applies to prose: strip every `<a …>` / `</a>`
//      from both sides and compare again. Identical afterwards means the region
//      differs only in autolinks, i.e. it belongs to the autolink-index cause.
//
// usage: member-check.ts --pages <dir> [--doc-root <dir>] [--report <path.txt>]
//
// The doc tree is opened read-only.

const argv = Deno.args.slice();
const opt = (n: string, d = "") => {
  const i = argv.indexOf(n);
  return i >= 0 ? argv[i + 1] : d;
};
const PAGES = opt("--pages");
const TARGET = Deno.env.get("TARGET_REPO") ?? "/Users/haruka/dev/lean-projects";
const DOC_ROOT = opt("--doc-root", `${TARGET}/.lake/build/doc/InformationTheory`);
const REPORT = opt("--report");
if (!PAGES) {
  console.error("usage: member-check.ts --pages <dir> [--doc-root <dir>] [--report <txt>]");
  Deno.exit(2);
}

const enc = new TextEncoder();
const u8 = (s: string) => enc.encode(s).length;

async function* walk(dir: string): AsyncGenerator<string> {
  for await (const e of Deno.readDir(dir)) {
    const p = `${dir}/${e.name}`;
    if (e.isDirectory) yield* walk(p);
    else if (e.name.endsWith(".html")) yield p;
  }
}

/** Every `div.structure_field_info` body. The div contains only spans and
 *  anchors (`renderedCodeToHtml` emits no `div`), so the first `</div>` closes
 *  it -- unlike the `<li>` around it, which may contain a whole docstring. */
function fieldInfos(html: string): string[] {
  const out: string[] = [];
  const open = '<div class="structure_field_info">';
  let i = html.indexOf(open);
  while (i >= 0) {
    const end = html.indexOf("</div>", i);
    out.push(html.slice(i + open.length, end));
    i = html.indexOf(open, end);
  }
  return out;
}

const count = (s: string, needle: string) => s.split(needle).length - 1;

type Marks = {
  info: number;
  args: number;
  doc: number;
  direct: number;
  inherited: number;
  inheritedWithId: number;
};
const marks = (html: string): Marks => ({
  info: count(html, '<div class="structure_field_info">'),
  args: fieldInfos(html).reduce((a, b) => a + count(b, '<span class="decl_args">'), 0),
  doc: count(html, '<div class="structure_field_doc">'),
  // A direct field always carries an id; an inherited one carries the
  // `inherited_field` class and only sometimes an id. Matching the whole open
  // tag rather than the class alone is what keeps the two apart.
  direct: (html.match(/<li id="[^"]*" class="structure_field">/g) ?? []).length,
  inherited: count(html, 'class="structure_field inherited_field"'),
  inheritedWithId: (html.match(/<li id="[^"]*" class="structure_field inherited_field">/g) ?? [])
    .length,
});

const zero = (): Marks => ({ info: 0, args: 0, doc: 0, direct: 0, inherited: 0, inheritedWithId: 0 });
const add = (a: Marks, b: Marks) => {
  a.info += b.info;
  a.args += b.args;
  a.doc += b.doc;
  a.direct += b.direct;
  a.inherited += b.inherited;
  a.inheritedWithId += b.inheritedWithId;
};

const theirPaths: string[] = [];
for await (const p of walk(DOC_ROOT)) theirPaths.push(p);
theirPaths.sort();

const mineTot = zero();
const theirTot = zero();
const mismatchedPages: string[] = [];

// --- B: the member regions that still differ ------------------------------
const noAnchors = (x: string) => x.replace(/<a [^>]*>|<\/a>/g, "");
type Residual = {
  module: string;
  name: string;
  bytes: number;
  anchorsOnly: boolean;
  anchorsMine: number;
  anchorsTheir: number;
};
const residuals: Residual[] = [];

/** `ul.structure_fields` / `ul.structure_ext` / `ul.constructors`, balanced on
 *  `<ul`/`</ul>` because a field docstring can contain a markdown list. */
function memberTables(html: string): Map<string, string> {
  const out = new Map<string, string>();
  const re = /<ul class="(?:structure_fields|structure_ext|constructors)"[^>]*>/g;
  for (let m = re.exec(html); m !== null; m = re.exec(html)) {
    let i = m.index + m[0].length;
    let depth = 1;
    while (depth > 0) {
      const nextOpen = html.indexOf("<ul", i);
      const nextClose = html.indexOf("</ul>", i);
      if (nextClose < 0) throw new Error("unbalanced <ul>");
      if (nextOpen >= 0 && nextOpen < nextClose) {
        depth++;
        i = nextOpen + 3;
      } else {
        depth--;
        i = nextClose + 5;
      }
    }
    // The enclosing declaration's id, i.e. `coverage.ts`'s region key.
    const before = html.lastIndexOf('<div class="decl" id="', m.index);
    const idEnd = html.indexOf('"', before + '<div class="decl" id="'.length);
    const name = html.slice(before + '<div class="decl" id="'.length, idEnd);
    out.set(name, html.slice(m.index, i));
    re.lastIndex = i;
  }
  return out;
}

for (const path of theirPaths) {
  const rel = path.slice(DOC_ROOT.length + 1);
  const module = "InformationTheory." + rel.slice(0, -".html".length).split("/").join(".");
  const theirs = await Deno.readTextFile(path);
  let mine: string;
  try {
    mine = await Deno.readTextFile(`${PAGES}/${module.split(".").join("/")}.html`);
  } catch {
    continue;
  }
  const a = marks(mine);
  const b = marks(theirs);
  add(mineTot, a);
  add(theirTot, b);
  if (JSON.stringify(a) !== JSON.stringify(b)) {
    mismatchedPages.push(`${module}\n    mine : ${JSON.stringify(a)}\n    their: ${JSON.stringify(b)}`);
  }

  const mt = memberTables(mine);
  const tt = memberTables(theirs);
  for (const [name, tText] of tt) {
    const mText = mt.get(name);
    if (mText === undefined || mText === tText) continue;
    residuals.push({
      module,
      name,
      bytes: u8(tText),
      anchorsOnly: noAnchors(mText) === noAnchors(tText),
      anchorsMine: (mText.match(/<a [^>]*>/g) ?? []).length,
      anchorsTheir: (tText.match(/<a [^>]*>/g) ?? []).length,
    });
  }
}

const n = (x: number) => x.toLocaleString("en-US");
const out: string[] = [];
const say = (s = "") => out.push(s);

say("# stage7b — メンバ表マークアップの割合検算と、残った members 領域の帰属 (実測)");
say();
say(`pages(mine)  ${PAGES}`);
say(`doc-root     ${DOC_ROOT}`);
say(`date         ${new Date().toISOString().replace(/\.\d+Z$/, "Z")}`);
say(`deno         ${Deno.version.deno} / V8 ${Deno.version.v8}`);
say();
say("## A. マーカーの個数 (doc-gen4 が出した 348 ページの上だけ、ページごとに突き合わせ)");
say();
say("| マーカー | 生成側 | doc-gen4 | |");
say("|---|---:|---:|---|");
const row = (label: string, a: number, b: number) =>
  say(`| ${label} | ${n(a)} | ${n(b)} | ${a === b ? "一致" : "**不一致**"} |`);
row("`div.structure_field_info`", mineTot.info, theirTot.info);
row("… その中の `span.decl_args`", mineTot.args, theirTot.args);
row("`div.structure_field_doc`", mineTot.doc, theirTot.doc);
row("`li.structure_field` (id 付き = direct)", mineTot.direct, theirTot.direct);
row("`li.structure_field.inherited_field`", mineTot.inherited, theirTot.inherited);
row("… うち id 付き (containedNames が真)", mineTot.inheritedWithId, theirTot.inheritedWithId);
say();
say(
  mismatchedPages.length === 0
    ? "**ページ単位でも全て一致**。合計だけ合っていて配り方が違う、という壊れ方はしていない。"
    : `**${n(mismatchedPages.length)} ページで個数が食い違っている:**`,
);
if (mismatchedPages.length > 0) {
  say();
  for (const p of mismatchedPages.slice(0, 20)) say(`  * ${p}`);
}
say();
say("## B. byte 一致しなかった members 領域の帰属");
say();
say("`coverage.ts` は members 領域の不一致を全部「IR に情報が無い (メンバの binder /");
say("docstring)」に入れる — その規則が書かれた時点ではそれしか原因が無かったため。");
say("ここでは prose と同じ判定 (両側から `<a …>` を剥がして一致するか) を当てて、");
say("**メンバのデータ欠落**と**autolink 索引の欠落**を分ける。");
say();
say("| module | 宣言 | doc-gen4 のバイト | `<a>` 数 生成側/doc-gen4 | アンカーを剥がすと一致 |");
say("|---|---|---:|---|---|");
for (const r of residuals.sort((a, b) => b.bytes - a.bytes)) {
  say(
    `| ${r.module} | \`${r.name}\` | ${n(r.bytes)} | ${r.anchorsMine}/${r.anchorsTheir} | ` +
      `${r.anchorsOnly ? "する → autolink 索引" : "**しない → メンバのデータ**"} |`,
  );
}
say();
const anchorBytes = residuals.filter((r) => r.anchorsOnly).reduce((a, b) => a + b.bytes, 0);
const dataBytes = residuals.filter((r) => !r.anchorsOnly).reduce((a, b) => a + b.bytes, 0);
say(`autolink 索引に帰する: **${n(anchorBytes)} B** / メンバのデータに帰する: **${n(dataBytes)} B**。`);
say();

const text = out.join("\n") + "\n";
console.log(text);
if (REPORT) await Deno.writeTextFile(REPORT, text);
