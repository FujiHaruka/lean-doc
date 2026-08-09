#!/usr/bin/env -S deno run --allow-read --allow-write
//
// Stage 4 increment 3, scoring half: score `render.ts`'s `div.decl_header`
// against doc-gen4's own output.
//
// usage:
//   compare.ts --mine <render.jsonl> --truth <decl-header-truth.jsonl>
//              [--report <path.txt>] [--diffs <path.jsonl>] [--max-diffs N]
//
// WHAT IS COMPARED
//   Per declaration, the **ordered sequence** of `(start, text, href)` for every
//   `<a>` inside `div.decl_header`, plus the header's plaintext and
//   `span.decl_kind`. `start` is the anchor's offset in the plaintext in UTF-16
//   code units, i.e. a JavaScript string index. Nothing here is a set or a
//   multiset comparison: increment 1 scored multisets and got a precision of
//   100% that was really an upper bound with the positions thrown away.
//
// INDEPENDENCE
//   The plaintext/anchor extraction below is written from scratch against the
//   HTML this repository generates. The ground truth was extracted by
//   benchmarks/tools/decl-header-truth.py with Python's html.parser plus a
//   second regex path. No code is shared between the two, so a bug in one does
//   not cancel a bug in the other. The definitions they implement are the same
//   four rules stated in that file's header comment.

type Anchor = { start: number; text: string; href: string | null; cls: string | null };
type Rec = {
  module: string;
  name: string;
  kind_text: string;
  header_text: string;
  impl_arg_count: number;
  anchors: Anchor[];
};

// ---------------------------------------------------------------- CLI

const argv = Deno.args.slice();
const opt = (n: string, d = "") => {
  const i = argv.indexOf(n);
  return i >= 0 ? argv[i + 1] : d;
};
const MINE = opt("--mine");
const TRUTH = opt("--truth");
const REPORT = opt("--report");
const DIFFS = opt("--diffs");
const MAX_DIFFS = Number(opt("--max-diffs", "200"));
/** Small-loop mode: score only the modules the generator was asked to emit, so
 *  a `--only <Module>` run is not drowned by 3,476 "missing" declarations. The
 *  full run must never use this -- the denominator has to stay 3,477. */
const MODULES_FROM_MINE = argv.includes("--modules-from-mine");
if (!MINE || !TRUTH) {
  console.error("usage: compare.ts --mine <jsonl> --truth <jsonl> [--report <txt>] [--diffs <jsonl>]");
  Deno.exit(2);
}

// ---------------------------------------------------------------- HTML parse

const ENTITY: Record<string, string> = {
  amp: "&",
  lt: "<",
  gt: ">",
  quot: '"',
  apos: "'",
  nbsp: " ",
};

function unescapeHtml(s: string): string {
  if (!s.includes("&")) return s;
  return s.replace(/&(#x?[0-9a-fA-F]+|[a-zA-Z]+);/g, (m, body: string) => {
    if (body[0] === "#") {
      const code = body[1] === "x" || body[1] === "X"
        ? parseInt(body.slice(2), 16)
        : parseInt(body.slice(1), 10);
      return Number.isFinite(code) ? String.fromCodePoint(code) : m;
    }
    const v = ENTITY[body];
    return v === undefined ? m : v;
  });
}

type Tag = { name: string; close: boolean; selfClose: boolean; attrs: Record<string, string> };

/** Minimal tag scanner. The generated markup has no comments, CDATA, scripts or
 *  unquoted attributes, so a tag is `<` `/`? name (attr)* `/`? `>`. */
function parseTag(src: string, lt: number, gt: number): Tag {
  let i = lt + 1;
  let close = false;
  if (src[i] === "/") {
    close = true;
    i++;
  }
  let j = i;
  while (j < gt && !/[\s/>]/.test(src[j])) j++;
  const name = src.slice(i, j).toLowerCase();
  const attrs: Record<string, string> = {};
  let selfClose = false;
  const rest = src.slice(j, gt);
  if (rest.trimEnd().endsWith("/")) selfClose = true;
  const re = /([a-zA-Z_:][-a-zA-Z0-9_:.]*)\s*=\s*"([^"]*)"/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(rest)) !== null) attrs[m[1].toLowerCase()] = unescapeHtml(m[2]);
  return { name, close, selfClose, attrs };
}

type Extracted = {
  header_text: string;
  kind_text: string;
  impl_arg_count: number;
  anchors: Anchor[];
  problems: string[];
};

/**
 * Walk the `div.decl_header` markup and produce exactly the four things the
 * ground-truth file carries: the plaintext (descendant text nodes, entity
 * references decoded, whitespace kept), the ordered anchors with their UTF-16
 * offsets, `span.decl_kind`, and the `span.impl_arg` count.
 */
function extract(html: string): Extracted {
  const problems: string[] = [];
  let text = "";
  let kind = "";
  let implArgs = 0;
  const anchors: Anchor[] = [];
  const openAnchors: { rec: Anchor; buf: string[] }[] = [];
  const stack: string[] = [];
  let kindDepth: number | null = null;
  const kindBuf: string[] = [];

  let i = 0;
  while (i < html.length) {
    const lt = html.indexOf("<", i);
    if (lt < 0) {
      const t = unescapeHtml(html.slice(i));
      text += t;
      for (const a of openAnchors) a.buf.push(t);
      if (kindDepth !== null) kindBuf.push(t);
      break;
    }
    if (lt > i) {
      const t = unescapeHtml(html.slice(i, lt));
      text += t;
      for (const a of openAnchors) a.buf.push(t);
      if (kindDepth !== null) kindBuf.push(t);
    }
    const gt = html.indexOf(">", lt);
    if (gt < 0) {
      problems.push("unterminated tag");
      break;
    }
    const tag = parseTag(html, lt, gt);
    if (!tag.close && !tag.selfClose) {
      stack.push(tag.name);
      const depth = stack.length;
      const cls = tag.attrs["class"] ?? null;
      if (tag.name === "span" && cls === "decl_kind" && kindDepth === null) kindDepth = depth;
      if (tag.name === "span" && cls === "impl_arg") implArgs++;
      if (tag.name === "a") {
        if (openAnchors.length > 0) problems.push("nested anchor");
        const rec: Anchor = {
          start: text.length,
          text: "",
          href: tag.attrs["href"] ?? null,
          cls,
        };
        anchors.push(rec);
        openAnchors.push({ rec, buf: [] });
      }
    } else if (tag.close) {
      const depth = stack.length;
      if (stack.length === 0 || stack[stack.length - 1] !== tag.name) {
        problems.push(`unbalanced </${tag.name}>`);
      } else {
        stack.pop();
      }
      if (tag.name === "a") {
        const a = openAnchors.pop();
        if (a) a.rec.text = a.buf.join("");
        else problems.push("stray </a>");
      }
      if (kindDepth !== null && depth === kindDepth) {
        kind = kindBuf.join("");
        kindDepth = null;
      }
    }
    i = gt + 1;
  }
  if (openAnchors.length > 0) problems.push("unclosed anchor");
  if (stack.length > 0) problems.push(`unclosed elements: ${stack.join(",")}`);
  return { header_text: text, kind_text: kind, impl_arg_count: implArgs, anchors, problems };
}

// ---------------------------------------------------------------- alignment

/**
 * Longest common subsequence over anchor *texts*, so that a pair of anchors can
 * be judged on position and href even when one side has an extra or missing
 * entry. Text is the weakest of the three fields to key on (an href error or an
 * offset shift must not break the alignment) while still being ordered.
 */
function align<T extends { text: string }>(a: T[], b: T[]): [number | null, number | null][] {
  const n = a.length, m = b.length;
  const dp: number[][] = Array.from({ length: n + 1 }, () => new Array(m + 1).fill(0));
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      dp[i][j] = a[i].text === b[j].text
        ? dp[i + 1][j + 1] + 1
        : Math.max(dp[i + 1][j], dp[i][j + 1]);
    }
  }
  const out: [number | null, number | null][] = [];
  let i = 0, j = 0;
  while (i < n && j < m) {
    if (a[i].text === b[j].text) {
      out.push([i, j]);
      i++;
      j++;
    } else if (dp[i + 1][j] >= dp[i][j + 1]) {
      out.push([i, null]);
      i++;
    } else {
      out.push([null, j]);
      j++;
    }
  }
  while (i < n) out.push([i++, null]);
  while (j < m) out.push([null, j++]);
  return out;
}

// ---------------------------------------------------------------- load

async function readJsonl<T>(path: string): Promise<T[]> {
  const text = await Deno.readTextFile(path);
  const out: T[] = [];
  for (const line of text.split("\n")) {
    if (line.length === 0) continue;
    out.push(JSON.parse(line));
  }
  return out;
}

let truth = await readJsonl<Rec>(TRUTH);
const mineRaw = await readJsonl<{ module: string; name: string; html: string }>(MINE);

if (MODULES_FROM_MINE) {
  const mods = new Set(mineRaw.map((r) => r.module));
  truth = truth.filter((t) => mods.has(t.module));
}

const truthByKey = new Map<string, Rec>();
let truthDupes = 0;
for (const t of truth) {
  const k = `${t.module} ${t.name}`;
  if (truthByKey.has(k)) truthDupes++;
  truthByKey.set(k, t);
}
const mineByKey = new Map<string, Extracted>();
const mineProblems: string[] = [];
let mineDupes = 0;
for (const r of mineRaw) {
  const k = `${r.module} ${r.name}`;
  if (mineByKey.has(k)) mineDupes++;
  const e = extract(r.html);
  if (e.problems.length > 0 && mineProblems.length < 20) {
    mineProblems.push(`${r.name}: ${e.problems.join("; ")}`);
  }
  mineByKey.set(k, e);
}

// ---------------------------------------------------------------- score

type DiffRec = {
  module: string;
  name: string;
  kindOk: boolean;
  textCmp: string;
  anchorsMine: number;
  anchorsTruth: number;
  issues: { kind: string; mine: Anchor | null; truth: Anchor | null }[];
};

/** How the two plaintexts differ, if they do. */
function compareText(mine: string, truth: string): string {
  if (mine === truth) return "equal";
  if (mine.length !== truth.length) return "length";
  let wsOnly = true;
  for (let i = 0; i < mine.length; i++) {
    if (mine[i] === truth[i]) continue;
    if (!/\s/.test(mine[i]) || !/\s/.test(truth[i])) {
      wsOnly = false;
      break;
    }
  }
  return wsOnly ? "whitespace" : "chars";
}

const c = {
  truthDecls: truth.length,
  matchedDecls: 0,
  missingFromMine: 0,
  extraInMine: 0,
  declExact: 0,
  declTextExact: 0,
  declTextWhitespace: 0,
  declTextOther: 0,
  declKindExact: 0,
  declImplArgExact: 0,
  anchorsTruth: 0,
  anchorsMine: 0,
  anchorExact: 0,
  anchorHrefOnly: 0, // start+text agree, href differs
  anchorStartOnly: 0, // text+href agree, start differs
  anchorBoth: 0, // text agrees, start and href both differ
  anchorMissing: 0, // in truth, not in mine
  anchorSpurious: 0, // in mine, not in truth
  wsCharsDiffering: 0,
  wsRunsDiffering: 0,
};

/** `mine char -> truth char` tally over the whitespace-only differences. */
const wsPairs = new Map<string, number>();

const diffs: DiffRec[] = [];
const causeCounter = new Map<string, number>();
const bump = (k: string, n = 1) => causeCounter.set(k, (causeCounter.get(k) ?? 0) + n);

for (const [k, t] of truthByKey) {
  const m = mineByKey.get(k);
  if (!m) {
    c.missingFromMine++;
    continue;
  }
  c.matchedDecls++;
  const textCmp = compareText(m.header_text, t.header_text);
  if (textCmp === "equal") c.declTextExact++;
  else if (textCmp === "whitespace") {
    c.declTextWhitespace++;
    let prev = -2;
    for (let i = 0; i < m.header_text.length; i++) {
      if (m.header_text[i] === t.header_text[i]) continue;
      c.wsCharsDiffering++;
      if (i !== prev + 1) c.wsRunsDiffering++;
      prev = i;
      const k = `${JSON.stringify(m.header_text[i])} -> ${JSON.stringify(t.header_text[i])}`;
      wsPairs.set(k, (wsPairs.get(k) ?? 0) + 1);
    }
  } else c.declTextOther++;
  const kindOk = m.kind_text === t.kind_text;
  if (kindOk) c.declKindExact++;
  if (m.impl_arg_count === t.impl_arg_count) c.declImplArgExact++;

  c.anchorsTruth += t.anchors.length;
  c.anchorsMine += m.anchors.length;

  const pairs = align(m.anchors, t.anchors);
  const issues: DiffRec["issues"] = [];
  let allExact = m.anchors.length === t.anchors.length;
  for (const [mi, ti] of pairs) {
    if (mi !== null && ti !== null) {
      const a = m.anchors[mi], b = t.anchors[ti];
      const sameStart = a.start === b.start;
      const sameHref = a.href === b.href;
      if (sameStart && sameHref) {
        c.anchorExact++;
      } else if (sameStart) {
        c.anchorHrefOnly++;
        allExact = false;
        issues.push({ kind: "href", mine: a, truth: b });
      } else if (sameHref) {
        c.anchorStartOnly++;
        allExact = false;
        issues.push({ kind: "start", mine: a, truth: b });
      } else {
        c.anchorBoth++;
        allExact = false;
        issues.push({ kind: "start+href", mine: a, truth: b });
      }
    } else if (ti !== null) {
      c.anchorMissing++;
      allExact = false;
      issues.push({ kind: "missing", mine: null, truth: t.anchors[ti] });
    } else if (mi !== null) {
      c.anchorSpurious++;
      allExact = false;
      issues.push({ kind: "spurious", mine: m.anchors[mi], truth: null });
    }
  }
  if (allExact) c.declExact++;
  if (!allExact || !kindOk || textCmp !== "equal") {
    const rec: DiffRec = {
      module: t.module,
      name: t.name,
      kindOk,
      textCmp,
      anchorsMine: m.anchors.length,
      anchorsTruth: t.anchors.length,
      issues,
    };
    if (diffs.length < MAX_DIFFS) diffs.push(rec);
    if (!kindOk) bump("kind_text mismatch");
    if (textCmp === "whitespace") bump("plaintext: whitespace rebuilt (splitWhitespaces)");
    else if (textCmp === "length") bump("plaintext: different length");
    else if (textCmp === "chars") bump("plaintext: different characters");
    for (const is of issues) bump(`anchor: ${is.kind}`);
  }
}
c.extraInMine = mineByKey.size - c.matchedDecls;

// ---------------------------------------------------------------- report

const n = (x: number) => x.toLocaleString("en-US");
const pct = (x: number, d: number) => (d === 0 ? "-" : `${((100 * x) / d).toFixed(3)}%`);
const out: string[] = [];
const say = (s = "") => out.push(s);

say("# compare — IR 由来の decl_header と doc-gen4 の正解の突き合わせ (実測)");
say();
say(`mine   ${MINE}`);
say(`truth  ${TRUTH}`);
say(`date   ${new Date().toISOString().replace(/\.\d+Z$/, "Z")}`);
say(`deno   ${Deno.version.deno} / V8 ${Deno.version.v8}`);
say();
say("すべて **順序を保った列** の比較。集合・多重集合の比較は一切していない。");
say();
say("## 母数");
say();
say("| | |");
say("|---|---:|");
say(`| 正解の宣言 (348 モジュール分) | ${n(c.truthDecls)} |`);
say(`| うち生成側にも存在した宣言 | ${n(c.matchedDecls)} |`);
say(`| 正解にあって生成側に無い宣言 | ${n(c.missingFromMine)} |`);
say(`| 生成側にあって正解に無い宣言 (HTML の無い 84 モジュール分を含む) | ${n(c.extraInMine)} |`);
say(`| 正解のアンカー (break_within 込み) | ${n(c.anchorsTruth)} |`);
say(`| 生成側のアンカー | ${n(c.anchorsMine)} |`);
say();
say("## 指標");
say();
say("| 指標 | 一致 | 母数 | 率 |");
say("|---|---:|---:|---:|");
say(`| 宣言単位の完全一致 ((位置,テキスト,href) の列) | ${n(c.declExact)} | ${n(c.truthDecls)} | ${pct(c.declExact, c.truthDecls)} |`);
say(`| アンカー単位の一致 (三つ組) | ${n(c.anchorExact)} | ${n(c.anchorsTruth)} | ${pct(c.anchorExact, c.anchorsTruth)} |`);
say(`| … href だけ違う (位置とテキストは一致) | ${n(c.anchorHrefOnly)} | ${n(c.anchorsTruth)} | ${pct(c.anchorHrefOnly, c.anchorsTruth)} |`);
say(`| … 位置だけずれ (テキストと href は一致) | ${n(c.anchorStartOnly)} | ${n(c.anchorsTruth)} | ${pct(c.anchorStartOnly, c.anchorsTruth)} |`);
say(`| … 位置も href も違う (テキストのみ一致) | ${n(c.anchorBoth)} | ${n(c.anchorsTruth)} | ${pct(c.anchorBoth, c.anchorsTruth)} |`);
say(`| … 正解にあって生成側に無い | ${n(c.anchorMissing)} | ${n(c.anchorsTruth)} | ${pct(c.anchorMissing, c.anchorsTruth)} |`);
say(`| … 生成側にしかない | ${n(c.anchorSpurious)} | ${n(c.anchorsMine)} | ${pct(c.anchorSpurious, c.anchorsMine)} |`);
say(`| 平文の完全一致 | ${n(c.declTextExact)} | ${n(c.truthDecls)} | ${pct(c.declTextExact, c.truthDecls)} |`);
say(`| … 空白の作り直しのみ (長さ同じ、空白同士が違う) | ${n(c.declTextWhitespace)} | ${n(c.truthDecls)} | ${pct(c.declTextWhitespace, c.truthDecls)} |`);
say(`| … それ以外の平文差 | ${n(c.declTextOther)} | ${n(c.truthDecls)} | ${pct(c.declTextOther, c.truthDecls)} |`);
say(`| decl_kind の一致 | ${n(c.declKindExact)} | ${n(c.truthDecls)} | ${pct(c.declKindExact, c.truthDecls)} |`);
say(`| impl_arg の個数の一致 | ${n(c.declImplArgExact)} | ${n(c.truthDecls)} | ${pct(c.declImplArgExact, c.truthDecls)} |`);
say();
say("## 差分の内訳 (件数、丸めない)");
say();
if (causeCounter.size === 0) say("差分なし。");
else {
  say("| 事象 | 件数 |");
  say("|---|---:|");
  for (const [k, v] of [...causeCounter].sort((a, b) => b[1] - a[1])) {
    say(`| ${k} | ${n(v)} |`);
  }
}
say();
if (c.declTextWhitespace > 0) {
  say("## 平文の空白差の内訳 (実測)");
  say();
  say("| | |");
  say("|---|---:|");
  say(`| 差のある宣言 | ${n(c.declTextWhitespace)} |`);
  say(`| 差のある文字 | ${n(c.wsCharsDiffering)} |`);
  say(`| 差のある連続区間 | ${n(c.wsRunsDiffering)} |`);
  say(`| 平文の総長 (正解、UTF-16 code units) | ${n(truth.reduce((a, t) => a + t.header_text.length, 0))} |`);
  say();
  say("| 生成側 -> 正解側 | 件数 |");
  say("|---|---:|");
  for (const [k, v] of [...wsPairs].sort((a, b) => b[1] - a[1])) say(`| ${k} | ${n(v)} |`);
  say();
}
say("## 生成 HTML の自己検査");
say();
say(`| 生成側でパースに問題のあった宣言 (先頭 20 件のみ表示) | ${mineProblems.length} |`);
for (const p of mineProblems) say(`  - ${p}`);
if (truthDupes || mineDupes) say(`重複キー: truth ${truthDupes} / mine ${mineDupes}`);
say();

// -------------------------------------------------------------- self-test
//
// A scorer that reports 100% is worth exactly as much as the evidence that it
// can report anything else. `--self-test` injects four faults into the
// generator's own markup and requires each to be counted in the right bucket.

if (argv.includes("--self-test")) {
  type Mut = { name: string; apply: (h: string) => string | null; expect: (d: DiffRec) => boolean };
  const muts: Mut[] = [
    {
      name: "href を 1 個壊す",
      apply: (h) => {
        const i = h.lastIndexOf('<a href="');
        return i < 0 ? null : h.slice(0, i + 9) + "ZZZ/" + h.slice(i + 9);
      },
      expect: (d) => d.issues.some((x) => x.kind === "href"),
    },
    {
      name: "平文に空白を 1 文字入れて以降の位置をずらす",
      apply: (h) => {
        const i = h.indexOf('<div class="decl_type">');
        return i < 0 ? null : h.slice(0, i + 23) + " " + h.slice(i + 23);
      },
      expect: (d) => d.issues.some((x) => x.kind === "start" || x.kind === "start+href"),
    },
    {
      name: "アンカーを 1 個消す (中身は残す)",
      apply: (h) => {
        const i = h.lastIndexOf("<a href=");
        if (i < 0) return null;
        const gt = h.indexOf(">", i);
        const close = h.indexOf("</a>", gt);
        if (gt < 0 || close < 0) return null;
        return h.slice(0, i) + h.slice(gt + 1, close) + h.slice(close + 4);
      },
      expect: (d) => d.issues.some((x) => x.kind === "missing"),
    },
    {
      name: "decl_kind を書き換える",
      apply: (h) => h.replace(/<span class="decl_kind">[^<]*<\/span>/, '<span class="decl_kind">zzz</span>'),
      expect: (d) => !d.kindOk,
    },
  ];

  const detected = new Array(muts.length).fill(0);
  const attempted = new Array(muts.length).fill(0);
  let sampled = 0;
  for (const r of mineRaw) {
    const t = truthByKey.get(`${r.module} ${r.name}`);
    if (!t || t.anchors.length < 3) continue;
    if (sampled >= 300) break;
    sampled++;
    for (let k = 0; k < muts.length; k++) {
      const h = muts[k].apply(r.html);
      if (h === null || h === r.html) continue;
      attempted[k]++;
      const m2 = extract(h);
      const pairs2 = align(m2.anchors, t.anchors);
      const issues: DiffRec["issues"] = [];
      for (const [mi, ti] of pairs2) {
        if (mi !== null && ti !== null) {
          const a = m2.anchors[mi], b = t.anchors[ti];
          if (a.start === b.start && a.href === b.href) continue;
          if (a.start === b.start) issues.push({ kind: "href", mine: a, truth: b });
          else if (a.href === b.href) issues.push({ kind: "start", mine: a, truth: b });
          else issues.push({ kind: "start+href", mine: a, truth: b });
        } else if (ti !== null) issues.push({ kind: "missing", mine: null, truth: t.anchors[ti] });
        else if (mi !== null) issues.push({ kind: "spurious", mine: m2.anchors[mi!], truth: null });
      }
      const d: DiffRec = {
        module: r.module,
        name: r.name,
        kindOk: m2.kind_text === t.kind_text,
        textCmp: compareText(m2.header_text, t.header_text),
        anchorsMine: m2.anchors.length,
        anchorsTruth: t.anchors.length,
        issues,
      };
      if (muts[k].expect(d)) detected[k]++;
    }
  }
  say("## スコアラの自己検査 (--self-test、実測)");
  say();
  say("生成した HTML にわざと欠陥を入れ、スコアラがそれを所定の欄で数えるかを見る。");
  say("100% という数字は、スコアラが 100% 以外を出せることの証拠と一緒でなければ意味がない。");
  say();
  say("| 注入した欠陥 | 検出 | 試行 |");
  say("|---|---:|---:|");
  for (let k = 0; k < muts.length; k++) {
    say(`| ${muts[k].name} | ${n(detected[k])} | ${n(attempted[k])} |`);
  }
  say();
  const allOk = muts.every((_, k) => attempted[k] > 0 && detected[k] === attempted[k]);
  say(`self-test: ${allOk ? "PASS (全欠陥を全試行で検出)" : "FAIL — スコアラが見落としている"}`);
  say();
}

const text = out.join("\n") + "\n";
console.log(text);
if (REPORT) await Deno.writeTextFile(REPORT, text);
if (DIFFS) {
  await Deno.writeTextFile(DIFFS, diffs.map((d) => JSON.stringify(d)).join("\n") + "\n");
}
