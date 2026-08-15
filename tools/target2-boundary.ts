#!/usr/bin/env -S deno run --allow-read
// M5-b — what the six boundary values did to the bytes.
//
// Plan §5, §7 and §8 each name an input that "does not occur in the target
// package but can occur in a second one". `tools/make-target2.sh` puts one in
// each of seven modules; this reads the produced site, IR, dependency map and
// whole-package state and says, for each of them, **what came out** — not
// whether it matches a prediction. Several of these have no right answer
// recorded anywhere yet, and inventing one here would be worse than reporting
// the byte.
//
// Deno rather than Rust 【判断, CLAUDE.md】: this is a reading of the product's
// output, so it belongs on the oracle's side of the line, where `coverage.ts`
// already lives. A checker written in the language it checks makes the same
// mistake twice.
//
// usage: deno run --allow-read tools/target2-boundary.ts \
//          --site <dir> --ir <dir> --lidx <file> --state <file>

const args = new Map<string, string>();
for (let i = 0; i < Deno.args.length; i += 2) {
  args.set(Deno.args[i].replace(/^--/, ""), Deno.args[i + 1]);
}
const SITE = args.get("site")!;
const IR = args.get("ir")!;
const LIDX = args.get("lidx")!;
const STATE = args.get("state")!;

const read = (p: string) => Deno.readTextFileSync(p);
const readOr = (p: string, fallback = "") => {
  try {
    return read(p);
  } catch {
    return fallback;
  }
};
const exists = (p: string) => {
  try {
    Deno.statSync(p);
    return true;
  } catch {
    return false;
  }
};

/** Every file under a directory, as a path relative to it. */
function walk(root: string, prefix = ""): string[] {
  const out: string[] = [];
  for (const entry of Deno.readDirSync(root + (prefix ? "/" + prefix : ""))) {
    const rel = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (entry.isDirectory) out.push(...walk(root, rel));
    else out.push(rel);
  }
  return out.sort();
}

/** A code point spelled the way this report spells it. */
const cp = (c: string) => "U+" + c.codePointAt(0)!.toString(16).toUpperCase().padStart(4, "0");

function section(n: string, title: string) {
  console.log("");
  console.log(`── ${n}  ${title}`);
}

const siteFiles = walk(SITE);
const irFiles = walk(IR);
console.log(`site                ${SITE} (${siteFiles.length} files)`);
console.log(`ir                  ${IR} (${irFiles.length} files)`);
console.log(`link index          ${LIDX} (${Deno.statSync(LIDX).size} B)`);

// ---------------------------------------------------------------------------
section("1a", "NUL inside a fenced code block — MD4Lean SIGSEGVs (wrapper.c:558)");
{
  const page = `${SITE}/Alpha/NulCode.html`;
  if (!exists(page)) {
    console.log("  page                MISSING — the renderer did not produce it");
  } else {
    // **Counted over the bytes, never searched for as a source literal** — a
    // NUL written into this file would be invisible, and a space typed by
    // mistake in its place makes the test pass on every page.
    const bytes = Deno.readFileSync(page);
    const html = new TextDecoder().decode(bytes);
    const nul = bytes.filter((b) => b === 0).length;
    const repl = [...html].filter((c) => c.codePointAt(0) === 0xfffd).length;
    console.log(`  page                ${page} (${bytes.length} B)`);
    console.log(`  raw NUL bytes       ${nul}`);
    console.log(`  U+FFFD              ${repl}`);
    const at = html.indexOf("before");
    if (at >= 0) {
      const slice = html.slice(at, at + 12);
      console.log(`  the code block      ${JSON.stringify(slice)}`);
      console.log(`  code points         ${[...slice].map(cp).join(" ")}`);
    }
  }
  const ir = Deno.readFileSync(`${IR}/modules/Alpha.NulCode.json`);
  const irText = new TextDecoder().decode(ir);
  console.log(
    `  IR carries the byte ${irText.includes("\\u0000") ? "yes, as the JSON escape \\u0000" : "no"}` +
      ` (raw NUL bytes in the IR file: ${ir.filter((b) => b === 0).length})`,
  );
}

// ---------------------------------------------------------------------------
section("1b", "a GFM table with no body row — MD4Lean SIGABRTs (wrapper.c:389)");
{
  const page = `${SITE}/Alpha/EmptyTable.html`;
  if (!exists(page)) {
    console.log("  page                MISSING");
  } else {
    const html = read(page);
    const table = html.match(/<table[\s\S]*?<\/table>/);
    console.log(`  page                ${page} (${new TextEncoder().encode(html).length} B)`);
    console.log(`  <table> rendered    ${table ? "yes" : "no"}`);
    if (table) console.log(`  the table           ${table[0].replaceAll("\n", "\\n")}`);
  }
}

// ---------------------------------------------------------------------------
section("2", "declaration names above the BMP (U1: UTF-16 order vs code point order)");
{
  const mapPath = `${SITE}/declarations/name-map.json`;
  const map = JSON.parse(readOr(mapPath, "{}"));
  const names: string[] = Array.isArray(map) ? map : Object.keys(map);
  const astral = names.filter((n) => [...n].some((c) => c.codePointAt(0)! > 0xffff));
  console.log(`  names in the map    ${names.length}`);
  console.log(`  above the BMP       ${astral.length}: ${astral.join(", ")}`);
  const a = names.findIndex((n) => n.includes("\u{1D49C}-z"));
  const b = names.findIndex((n) => n.includes("ﬀ-z"));
  console.log(`  «𝒜-z» at index      ${a}`);
  console.log(`  «ﬀ-z» at index      ${b}`);
  if (a >= 0 && b >= 0) {
    // U+1D49C is D835 DC9C in UTF-16, so its first code unit (D835) is *below*
    // U+FB00; by code point it is above. The two orders therefore disagree, and
    // which one the file is in is decided by plan §7's U1.
    const utf16First = a < b;
    console.log(
      `  order               ${utf16First ? "«𝒜-z» first = UTF-16 code unit order (U1 honoured)" : "«ﬀ-z» first = code point / UTF-8 order (U1 NOT honoured)"}`,
    );
    console.log(
      `  cross-check         JS .sort() puts ${["\u{1D49C}-z", "ﬀ-z"].sort()[0] === "\u{1D49C}-z" ? "«𝒜-z»" : "«ﬀ-z»"} first`,
    );
  }
  // `declaration-data.bmp` is the search index, and its bytes come out of the
  // same sorted array (plan §7, U1).
  const bmp = readOr(`${SITE}/declarations/declaration-data.bmp`);
  const bmpA = bmp.indexOf("\u{1D49C}-z");
  const bmpB = bmp.indexOf("\uFB00-z");
  console.log(
    `  declaration-data    «𝒜-z» at ${bmpA}, «ﬀ-z» at ${bmpB} — ` +
      (bmpA >= 0 && bmpB >= 0 ? (bmpA < bmpB ? "UTF-16 order" : "code point order") : "one is absent"),
  );
  const page = `${SITE}/Alpha/AstralNames.html`;
  console.log(`  page                ${exists(page) ? page : "MISSING"}`);
  if (exists(page)) {
    const html = read(page);
    for (const name of ["\u{1D49C}", "\u{1D49C}-z", "ﬀ-z"]) {
      const id = html.includes(`id="Alpha.AstralNames.${name}"`) ||
        html.includes(`id="Alpha.AstralNames.«${name}»"`);
      console.log(`  anchor for ${name.padEnd(4)}   ${id ? "yes" : "no"}`);
    }
  }
}

// ---------------------------------------------------------------------------
section("3", "a heading containing U+2B96 (UnicodeBasic vs V8 heading-id table)");
{
  const page = `${SITE}/Alpha/HeadingSplit.html`;
  if (!exists(page)) {
    console.log("  page                MISSING");
  } else {
    const html = read(page);
    const heading = html.match(/<h[1-6][^>]*id="[^"]*"[^>]*>[\s\S]*?<\/h[1-6]>/g) ?? [];
    console.log(`  page                ${page}`);
    for (const h of heading) console.log(`  heading             ${h.replaceAll("\n", "\\n")}`);
    console.log(`  raw U+2B96 in page  ${html.includes("⮖") ? "yes" : "no"}`);
    const ids = [...html.matchAll(/id="([^"]*)"/g)].map((m) => m[1]).filter((i) => i.includes("Head"));
    for (const id of ids) {
      console.log(`  id                  ${JSON.stringify(id)}  (${[...id].map(cp).join(" ")})`);
    }
  }
}

// ---------------------------------------------------------------------------
section("4", "a module name that needs «…» (M5-a trap 4)");
{
  const escaped = "Alpha.«Odd-Name»";
  const plain = "Alpha.Odd-Name";
  const irNames = irFiles.filter((f) => f.startsWith("modules/") && f.includes("Odd"));
  console.log(`  IR module file      ${irNames.join(", ") || "NONE"}`);
  const index = JSON.parse(readOr(`${IR}/index.json`, "{}"));
  const inIndex = (index.modules ?? [])
    .map((m: { module: string }) => m.module)
    .filter((m: string) => m.includes("Odd"));
  console.log(`  index.json spells   ${JSON.stringify(inIndex)}`);
  const pages = siteFiles.filter((f) => f.includes("Odd"));
  console.log(`  page                ${pages.join(", ") || "NONE"}`);
  const lidx = read(LIDX);
  console.log(`  .lidx group line    ${lidx.includes(`\n${plain}\n`) ? `"${plain}" (unescaped)` : lidx.includes(`\n${escaped}\n`) ? `"${escaped}" (escaped)` : "NONE"}`);
  console.log(`  .lidx @ line        ${lidx.includes(`\n@${plain}\n`) ? `"@${plain}" (unescaped)` : lidx.includes(`\n@${escaped}\n`) ? `"@${escaped}" (escaped)` : "NONE"}`);
  const nav = readOr(`${SITE}/navbar.html`);
  const hrefs = [...nav.matchAll(/href="([^"]*Odd[^"]*)"/g)].map((m) => m[1]);
  console.log(`  navbar href         ${hrefs.join(", ") || "NONE"}`);
  const map = JSON.parse(readOr(`${SITE}/declarations/name-map.json`, "{}"));
  const owner = (map as Record<string, unknown>)["Alpha.OddName.oddNameConst"];
  console.log(`  name-map owner      ${JSON.stringify(owner)}`);
}

// ---------------------------------------------------------------------------
section("5a", "_private. names (plan 決定 5)");
{
  const ir = readOr(`${IR}/modules/Alpha.Private.json`);
  const irPrivate = (ir.match(/_private\.[A-Za-z0-9_.«»]+/g) ?? []);
  console.log(`  IR names them       ${irPrivate.length} (${[...new Set(irPrivate)].slice(0, 4).join(", ")})`);
  const decls: string[] = JSON.parse(ir || "{}").declarations?.map((d: { name: string }) => d.name) ?? [];
  console.log(`  IR declarations     ${JSON.stringify(decls)}`);
  const page = `${SITE}/Alpha/Private.html`;
  const html = readOr(page);
  console.log(`  page                ${exists(page) ? page : "MISSING"}`);
  console.log(`  page says _private. ${html.includes("_private.") ? "yes" : "no"}`);
  const lidx = read(LIDX);
  console.log(`  .lidx says _private ${lidx.includes("_private.") ? "yes" : "no"}`);
  const map = JSON.parse(readOr(`${SITE}/declarations/name-map.json`, "{}"));
  const names = Object.keys(map);
  console.log(`  name-map _private   ${names.filter((n) => n.startsWith("_private.")).length}`);
  console.log(`  name-map has hidden ${names.includes("Alpha.Private.hidden") ? "yes" : "no"}`);
}

// ---------------------------------------------------------------------------
section("5b", "one declaration name in more than one module's IR");
{
  const owners = new Map<string, string[]>();
  for (const f of irFiles.filter((f) => f.startsWith("modules/"))) {
    const module = f.slice("modules/".length).replace(/\.json$/, "");
    const body = JSON.parse(read(`${IR}/${f}`));
    for (const d of body.declarations ?? []) {
      const list = owners.get(d.name) ?? [];
      list.push(module);
      owners.set(d.name, list);
    }
  }
  const shared = [...owners].filter(([, ms]) => ms.length > 1);
  console.log(`  declarations        ${owners.size}`);
  console.log(`  in >1 module        ${shared.length}`);
  for (const [name, ms] of shared.slice(0, 10)) console.log(`    ${name}  ${ms.join(" / ")}`);
  const step = [...owners.keys()].filter((n) => n.startsWith("Alpha.Basic.step"));
  console.log(`  step* declarations  ${JSON.stringify(step)}`);
}

// ---------------------------------------------------------------------------
section("6", "U+088F inside a code span (V6: a separator for V8, not for UnicodeBasic)");
{
  const state = JSON.parse(readOr(STATE, "{}"));
  const modules = state.modules ?? state.facts ?? {};
  const facts = modules["Beta.TokenSep"];
  const tokens: string[] = facts?.tokens ?? [];
  console.log(`  state              ${STATE}`);
  console.log(`  Beta.TokenSep tokens ${JSON.stringify(tokens)}`);
  const split = tokens.includes("Alpha.Basic.alphaConst");
  const joined = tokens.some((t) => t.includes("࢏"));
  console.log(`  split on U+088F     ${split ? "yes — the union table saw it" : "no"}`);
  console.log(`  a token still holds it ${joined ? "yes" : "no"}`);
  const page = `${SITE}/Beta/TokenSep.html`;
  const html = readOr(page);
  console.log(`  page                ${exists(page) ? page : "MISSING"}`);
  // **The span that holds U+088F, not the page.** The same docstring names
  // `Alpha.Basic.alphaConst` a second time in ordinary prose, and that one *is*
  // linked; looking at the page as a whole would report the control's link as
  // this boundary value's.
  const spans = [...html.matchAll(/<code[^>]*>[\s\S]*?<\/code>/g)].map((m) => m[0]);
  const withSep = spans.filter((s) => s.includes("\u088F"));
  console.log(`  spans holding U+088F ${withSep.length}`);
  for (const s of withSep) {
    console.log(`  the span            ${JSON.stringify(s)}`);
    console.log(
      `  linked              ${s.includes("<a href") ? "yes" : "no — the renderer's UnicodeBasic table does not split here"}`,
    );
  }
  const control = spans.find((s) => !s.includes("\u088F") && s.includes("alphaConst"));
  console.log(`  the control span    ${control ? JSON.stringify(control) : "none"}`);
}

console.log("");
