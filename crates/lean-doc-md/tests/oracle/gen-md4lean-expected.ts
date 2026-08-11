#!/usr/bin/env -S deno run --allow-read --allow-write --allow-run --allow-env
// gen-md4lean-expected.ts -- produce the expected trees for `tests/md4lean.rs`
// by running *MD4Lean itself*.
//
// WHY THIS FILE EXISTS
// --------------------
// doc-gen4 renders whatever `MD4Lean.parse` returns. `crates/lean-doc-md`
// claims to build the same tree by driving the same md4c with the same flags
// and the same callback logic. An expected value written by reading
// `MD4Lean/wrapper/wrapper.c` would prove nothing -- the reading and the port
// would share whatever mistake was made. So the expected trees come from Lean:
// `dump-ast.lean` runs `MD4Lean.parse` in the measurement target's own
// environment and prints the tree in the encoding `tests/md4lean.rs` rebuilds.
//
// THE CORPUS IS REAL DATA
// -----------------------
// Every docstring in the target package's IR (declarations, module docs,
// structure fields, tactics), deduplicated. Hand-written cases are prepended,
// not substituted: they exist to reach the corners the package happens not to
// contain (wiki links, underline, images with titles, ...), and they are
// answered by MD4Lean too.
//
// SOME INPUTS CRASH MD4LEAN
// -------------------------
// A NUL inside a fenced code block segfaults it, and a GFM table with a header
// and no body rows aborts it (see `CRASHERS`). The runner below finds them by
// resuming after each crash, records them separately, and keeps them out of
// the expected set -- a crash is not an oracle.
//
// npm/node are broken in this environment; this must run under deno.
//
// usage:
//   deno run --allow-read --allow-write --allow-run --allow-env \
//     crates/lean-doc-md/tests/oracle/gen-md4lean-expected.ts
//   ... --full /tmp/md4lean-full.json   also write every case, not just the
//                                       committed sample
//   ... --check                         fail if the committed file is stale

const FIXTURE = new URL("../data/md4lean-expected.json", import.meta.url);
const DUMPER = new URL("dump-ast.lean", import.meta.url);

const DEFAULT_TARGET = "/Users/haruka/dev/lean-projects";
const DEFAULT_IR = "/private/tmp/lean-doc-relay/w7h/base-ir";

/** How many cases the committed fixture aims for. See `selectCases`. */
const FIXTURE_TARGET = 500;

// ------------------------------------------------------------ hand-written

/**
 * Corners the package's own docstrings do not reach. Each is answered by
 * MD4Lean like every other case; none of them is an expectation written here.
 */
const CURATED: [string, string][] = [
  ["empty", ""],
  ["blank lines only", "\n\n \n\t\n"],
  ["paragraph", "just a paragraph\n"],
  ["two paragraphs", "one\n\ntwo\n"],
  ["soft break", "one\ntwo\n"],
  ["hard break, two spaces", "one  \ntwo\n"],
  ["hard break, backslash", "one\\\ntwo\n"],
  ["crlf", "one\r\ntwo\r\n\r\nthree\r\n"],
  ["emphasis", "*a* _b_ **c** __d__ ***e***\n"],
  ["nested emphasis", "*a **b** c*\n"],
  ["strikethrough", "~~gone~~ and ~one~\n"],
  ["underline is off", "_not underline_\n"],
  ["code span", "`x` ``a ` b`` `` ` ``\n"],
  ["code span across lines", "`a\nb`\n"],
  ["inline math", "$x^2$\n"],
  ["display math", "$$\\sum_i x_i$$\n"],
  ["math with markdown inside", "$a_*b*_c$\n"],
  ["entity, named", "&nbsp; &amp; &notanentity;\n"],
  ["entity, numeric", "&#65; &#x1F600;\n"],
  ["entity in a code span", "`&amp;`\n"],
  ["autolink, angle", "<http://example.org/a?b=c>\n"],
  ["autolink, email angle", "<user@example.org>\n"],
  ["autolink, permissive url", "see http://example.org/x for more\n"],
  ["autolink, permissive www", "see www.example.org for more\n"],
  ["autolink, permissive email", "write to user@example.org now\n"],
  ["inline link", "[text](http://a/b)\n"],
  ["inline link with title", "[text](http://a/b 'the title')\n"],
  ["link with an entity in the title", '[t](http://a "x &quot; y")\n'],
  ["link with empty destination", "[t]()\n"],
  ["reference link", "[t][ref]\n\n[ref]: http://a/b 'title'\n"],
  ["collapsed reference link", "[ref][]\n\n[ref]: http://a/b\n"],
  ["shortcut reference link", "[ref]\n\n[ref]: http://a/b\n"],
  ["image", "![alt](src.png)\n"],
  ["image with title", "![alt](src.png 'title')\n"],
  ["image with markup in the alt", "![a *b* `c`](src.png)\n"],
  ["nested image in a link", "[![alt](s.png)](http://a)\n"],
  ["heading, atx", "# h1\n\n## h2\n\n### h3\n\n#### h4\n\n##### h5\n\n###### h6\n"],
  ["heading, setext", "title\n=====\n\nsub\n---\n"],
  ["heading with markup", "# a *b* `c`\n"],
  ["thematic break", "---\n\n***\n\n___\n"],
  ["blockquote", "> quoted\n"],
  ["blockquote, nested", "> a\n>\n> > b\n"],
  ["blockquote holding a list", "> - a\n> - b\n"],
  ["bullet list, dash", "- a\n- b\n"],
  ["bullet list, plus", "+ a\n+ b\n"],
  ["bullet list, star", "* a\n* b\n"],
  ["bullet list, loose", "- a\n\n- b\n"],
  ["ordered list", "1. a\n2. b\n"],
  ["ordered list starting at 3", "3. a\n4. b\n"],
  ["ordered list, paren delimiter", "1) a\n2) b\n"],
  ["nested list", "- a\n  - b\n    - c\n"],
  ["list item with two paragraphs", "- a\n\n  b\n"],
  ["list item with text then a code block", "- blah\n  ```lean\n  def f := 1\n  ```\n"],
  ["list item with a span then a block", "- *a*\n\n  > q\n"],
  ["task list", "- [x] done\n- [X] also done\n- [ ] not done\n"],
  ["task list, ordered", "1. [ ] a\n"],
  ["fenced code, backtick", "```\nplain\n```\n"],
  ["fenced code, tilde", "~~~\nplain\n~~~\n"],
  ["fenced code with a language", "```lean\ndef f := 1\n```\n"],
  ["fenced code with an info string", "```lean showFrom=2 -- note\nx\n```\n"],
  ["fenced code with an entity in the info", "```a&amp;b\nx\n```\n"],
  ["indented code", "    indented\n    lines\n"],
  ["empty fenced code", "```\n```\n"],
  ["fenced code holding backticks", "````\n```\n````\n"],
  ["raw html block is off", "<div>\nx\n</div>\n"],
  ["raw html span is off", "a <b>c</b> d\n"],
  ["table", "| a | b |\n|---|---|\n| 1 | 2 |\n"],
  ["table with alignment", "| l | c | r |\n|:--|:-:|--:|\n| 1 | 2 | 3 |\n"],
  ["table with markup in cells", "| a | b |\n|---|---|\n| *x* | `y` |\n"],
  ["table with a ragged row", "| a | b |\n|---|---|\n| 1 |\n"],
  ["wiki links are off", "[[target|label]]\n"],
  ["non-bmp scalars", "\u{1D49C} and \u{1F600} in *text*\n"],
  ["combining marks", "e\u0301 and \u00e9\n"],
  ["backslash escapes", "\\*not em\\* \\\\ \\` \\[\n"],
  ["lean names", "`Nat.succ` and `Finset.sum_le_sum` and `_private.Foo.0.bar`\n"],
  ["deeply nested", "> - a\n>   1. b\n>      - [x] c\n"],
  ["tabs", "-\ta\n\t- b\n"],
  ["unclosed emphasis", "*a\n"],
  ["lone brackets", "[ ] ( ) ![ ]\n"],
  // These two survive MD4Lean; the two in CRASHERS do not.
  ["nul in text", "a\u0000b\n"],
  ["nul in a code span", "`a\u0000b`\n"],
];

// `MD_FLAG_*`, transcribed here only to build the dialect cases below. The
// authority is `vendor/md4c/md4c.h`; `tests/abi.rs` is what checks it.
const F = {
  COLLAPSEWHITESPACE: 0x0001,
  PERMISSIVEATXHEADERS: 0x0002,
  PERMISSIVEURLAUTOLINKS: 0x0004,
  PERMISSIVEEMAILAUTOLINKS: 0x0008,
  NOINDENTEDCODEBLOCKS: 0x0010,
  NOHTMLBLOCKS: 0x0020,
  NOHTMLSPANS: 0x0040,
  TABLES: 0x0100,
  STRIKETHROUGH: 0x0200,
  PERMISSIVEWWWAUTOLINKS: 0x0400,
  TASKLISTS: 0x0800,
  LATEXMATHSPANS: 0x1000,
  WIKILINKS: 0x2000,
  UNDERLINE: 0x4000,
  HARD_SOFT_BREAKS: 0x8000,
};
const DIALECT_GITHUB = F.PERMISSIVEURLAUTOLINKS | F.PERMISSIVEEMAILAUTOLINKS |
  F.PERMISSIVEWWWAUTOLINKS | F.TABLES | F.STRIKETHROUGH | F.TASKLISTS;
/** What doc-gen4 uses: `DocGen4/Output/DocString.lean:393`. */
const DOCSTRING_FLAGS = DIALECT_GITHUB | F.LATEXMATHSPANS | F.NOHTMLBLOCKS | F.NOHTMLSPANS;

/**
 * Cases parsed with something other than the docstring dialect.
 *
 * Three constructors of the AST -- `html`, `u`, `wikiLink` -- cannot occur
 * under `DOCSTRING_FLAGS`, so without these the code that builds them would
 * never run and would never be checked against MD4Lean. Turning the flag on
 * for a handful of inputs keeps the oracle rather than moving those branches
 * to hand-written expectations.
 */
const DIALECTS: [string, number, string][] = [
  ["underline on", DOCSTRING_FLAGS | F.UNDERLINE, "_underlined_ and *em*\n"],
  ["wiki links on", DOCSTRING_FLAGS | F.WIKILINKS, "[[target|label]] and [[bare]]\n"],
  ["raw html on, block", DIALECT_GITHUB | F.LATEXMATHSPANS, "<div>\nx\n</div>\n"],
  ["plain commonmark", 0, "~~a~~ www.x.org\n\n| a |\n|---|\n| 1 |\n"],
  ["hard soft breaks", DOCSTRING_FLAGS | F.HARD_SOFT_BREAKS, "one\ntwo\n"],
  ["collapse whitespace", DOCSTRING_FLAGS | F.COLLAPSEWHITESPACE, "a   \t  b\n"],
  ["permissive atx headers", DOCSTRING_FLAGS | F.PERMISSIVEATXHEADERS, "###head\n"],
  ["no indented code blocks", DOCSTRING_FLAGS | F.NOINDENTEDCODEBLOCKS, "    x\n"],
];

/**
 * The inputs that make MD4Lean die. They are recorded, never expected;
 * `tests/md4lean.rs` asserts only that this crate survives them.
 *
 * The first two are reachable from the docstring dialect. The third is not --
 * inline raw HTML puts a bare `String` where `Block.p` expects an
 * `Array Text` -- and is listed because it is the same class of bug and
 * because this crate does have to produce something for that shape.
 */
const CRASHERS: [string, number, string, string][] = [
  ["nul in a fenced code block", DOCSTRING_FLAGS, "```\na\u0000b\n```\n", "SIGSEGV"],
  ["table with a header and no body", DOCSTRING_FLAGS, "| a | b |\n|---|---|\n", "SIGABRT"],
  [
    "raw html span, with MD_FLAG_NOHTMLSPANS off",
    DIALECT_GITHUB | F.LATEXMATHSPANS,
    "a <b>c</b> d\n",
    "exit 1",
  ],
];

// ------------------------------------------------------------------- corpus

type Case = { what: string; md: string; flags: number };

async function readJson(path: string): Promise<any> {
  return JSON.parse(await Deno.readTextFile(path));
}

/** Every docstring in the IR, deduplicated, in a deterministic order. */
async function corpusFromIr(ir: string): Promise<Case[]> {
  const index = await readJson(`${ir}/index.json`);
  const seen = new Set<string>();
  const cases: Case[] = [];
  const add = (what: string, md: string | null | undefined) => {
    if (typeof md !== "string" || seen.has(md)) return;
    seen.add(md);
    cases.push({ what, md, flags: DOCSTRING_FLAGS });
  };

  for (const entry of index.modules) {
    const module = await readJson(`${ir}/${entry.file}`);
    for (const [i, doc] of module.moduleDocs.entries()) {
      add(`${module.module} module doc ${i}`, doc.text);
    }
    for (const decl of module.declarations) {
      add(`${decl.name}`, decl.doc);
      for (const member of decl.members ?? []) {
        add(`${decl.name}.${member.name}`, member.doc);
      }
    }
    for (const tactic of module.tactics ?? []) {
      add(`tactic ${tactic.userName}`, tactic.docString);
    }
  }
  return cases;
}

// ---------------------------------------------------------------- the oracle

const DYLIB = Deno.build.os === "darwin" ? "dylib" : "so";
const MD4LEAN_LIBS = [
  `.lake/packages/MD4Lean/.lake/build/lib/libleanmd4c.${DYLIB}`,
  `.lake/packages/MD4Lean/.lake/build/lib/libMD4Lean_MD4Lean.${DYLIB}`,
];

/**
 * Runs `dump-ast.lean` over `cases`, resuming past any input that kills it.
 *
 * `lake env` has to run inside the target package (that is what supplies
 * Lean, Mathlib and the built MD4Lean); nothing is written there -- both file
 * arguments are in `work`.
 */
async function runOracle(
  target: string,
  work: string,
  cases: Case[],
): Promise<(unknown | "crashed")[]> {
  const answers: (unknown | "crashed")[] = [];
  let from = 0;
  let round = 0;

  while (from < cases.length) {
    const inPath = `${work}/corpus-${round}.jsonl`;
    const outPath = `${work}/ast-${round}.jsonl`;
    await Deno.writeTextFile(
      inPath,
      cases.slice(from).map((c) => JSON.stringify([c.flags, c.md])).join("\n") + "\n",
    );

    const command = new Deno.Command("lake", {
      args: [
        "env",
        "lean",
        ...MD4LEAN_LIBS.map((lib) => `--load-dynlib=${lib}`),
        "--run",
        DUMPER.pathname,
        inPath,
        outPath,
      ],
      cwd: target,
      stdout: "inherit",
      stderr: "inherit",
    });
    const { code, signal } = await command.output();

    let produced: string[] = [];
    try {
      const text = await Deno.readTextFile(outPath);
      produced = text.split("\n").filter((line) => line !== "");
    } catch {
      produced = [];
    }
    for (const line of produced) answers.push(JSON.parse(line));

    if (code === 0) {
      if (produced.length !== cases.length - from) {
        throw new Error(
          `the dumper exited 0 with ${produced.length} of ${cases.length - from} answers`,
        );
      }
      break;
    }

    const victim = from + produced.length;
    if (victim >= cases.length) {
      throw new Error(`the dumper failed (${code}/${signal}) with no input left to blame`);
    }
    console.error(
      `MD4Lean died (${signal ?? code}) on ${JSON.stringify(cases[victim].what)}; skipping it`,
    );
    answers.push("crashed");
    from = victim + 1;
    round++;
  }
  return answers;
}

// ---------------------------------------------------------------- selection

/**
 * What shape a tree has, as a string: which constructors appear anywhere in
 * it, plus how deep it goes. Two docstrings with the same signature exercise
 * the same branches of the builder, so keeping one of each is what makes a
 * sample worth as much as the corpus for structural coverage.
 *
 * The walk is type-directed rather than "recurse into every array": verbatim
 * content is an array of strings, and a code block whose first line happens to
 * read `p` would otherwise register as a paragraph.
 */
function signature(ast: unknown): string {
  const tags = new Set<string>();
  let deepest = 0;

  const note = (node: unknown, depth: number): unknown[] => {
    deepest = Math.max(deepest, depth);
    if (!Array.isArray(node) || typeof node[0] !== "string") {
      tags.add("?");
      return [];
    }
    tags.add(`${node[0]}/${node.length - 1}`);
    return node;
  };
  const walkAttrs = (list: unknown, depth: number) => {
    for (const attr of (list as unknown[]) ?? []) note(attr, depth);
  };
  const walkTexts = (list: unknown, depth: number) => {
    for (const raw of (list as unknown[]) ?? []) {
      const node = note(raw, depth);
      switch (node[0]) {
        case "em":
        case "strong":
        case "u":
        case "del":
          walkTexts(node[1], depth + 1);
          break;
        case "a":
          walkAttrs(node[1], depth + 1);
          walkAttrs(node[2], depth + 1);
          walkTexts(node[4], depth + 1);
          break;
        case "img":
          walkAttrs(node[1], depth + 1);
          walkAttrs(node[2], depth + 1);
          walkTexts(node[3], depth + 1);
          break;
        case "wikiLink":
          walkAttrs(node[1], depth + 1);
          walkTexts(node[2], depth + 1);
          break;
        default:
          break;
      }
    }
  };
  const walkLis = (list: unknown, depth: number) => {
    for (const li of (list as unknown[]) ?? []) {
      const item = li as unknown[];
      tags.add(item?.[0] === true ? "li/task" : "li");
      walkBlocks(item?.[3], depth + 1);
    }
  };
  const walkBlocks = (list: unknown, depth: number) => {
    for (const raw of (list as unknown[]) ?? []) {
      const node = note(raw, depth);
      switch (node[0]) {
        case "p":
        case "html":
          walkTexts(node[1], depth + 1);
          break;
        case "ul":
          walkLis(node[3], depth + 1);
          break;
        case "ol":
          walkLis(node[4], depth + 1);
          break;
        case "header":
          walkTexts(node[2], depth + 1);
          break;
        case "code":
          walkAttrs(node[1], depth + 1);
          walkAttrs(node[2], depth + 1);
          break;
        case "blockquote":
          walkBlocks(node[1], depth + 1);
          break;
        case "table":
          for (const cell of (node[1] as unknown[]) ?? []) walkTexts(cell, depth + 1);
          for (const row of (node[2] as unknown[]) ?? []) {
            for (const cell of (row as unknown[]) ?? []) walkTexts(cell, depth + 1);
          }
          break;
        default:
          break;
      }
    }
  };

  walkBlocks(ast, 0);
  return `${[...tags].sort().join(",")}|${Math.min(deepest, 12)}`;
}

/** Inputs whose *bytes* are the interesting part, whatever tree they made. */
function textFeatures(md: string): string[] {
  const features: string[] = [];
  if (/[\u{10000}-\u{10FFFF}]/u.test(md)) features.push("astral");
  // deno-lint-ignore no-control-regex
  if (/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/.test(md)) features.push("control");
  if (md.includes("\r")) features.push("cr");
  if (md.includes("\t")) features.push("tab");
  if (/&[A-Za-z#]/.test(md)) features.push("entity-ish");
  if (md.length > 4000) features.push("long");
  return features;
}

/**
 * Which cases the committed fixture keeps.
 *
 * The corpus has 1,300-odd distinct signatures; committing one docstring per
 * signature would be a multi-megabyte file whose extra megabytes are near
 * duplicates. So the fixture keeps, in this order:
 *
 * 1. every hand-written case;
 * 2. a greedy cover of the *constructors* -- at each step the docstring that
 *    brings the most tags nothing already chosen has, so every branch of the
 *    builder is exercised by the committed file alone;
 * 3. the first docstring showing each byte-level feature (astral scalars,
 *    controls, CR, tabs, ...);
 * 4. an even stride over the rest, up to `FIXTURE_TARGET`, so the fixture is
 *    not made only of oddities.
 *
 * The whole corpus is still checked -- see `--full` and the env var
 * `LEAN_DOC_MD4LEAN_FULL` in `tests/md4lean.rs`. This selection decides what
 * a machine without the target package can check, not what was checked.
 */
function selectCases(
  cases: Case[],
  answers: (unknown | "crashed")[],
  curatedCount: number,
): number[] {
  const chosen = new Set<number>();
  for (let i = 0; i < curatedCount; i++) chosen.add(i);

  const usable: number[] = [];
  for (let i = curatedCount; i < cases.length; i++) {
    if (answers[i] !== "crashed") usable.push(i);
  }
  const tagsOf = new Map<number, Set<string>>();
  for (const i of usable) {
    tagsOf.set(i, new Set(signature(answers[i]).split("|")[0].split(",")));
  }

  const covered = new Set<string>();
  for (let i = 0; i < curatedCount; i++) {
    if (answers[i] === "crashed") continue;
    for (const tag of signature(answers[i]).split("|")[0].split(",")) covered.add(tag);
  }
  for (;;) {
    let best = -1;
    let bestGain = 0;
    for (const i of usable) {
      if (chosen.has(i)) continue;
      let gain = 0;
      for (const tag of tagsOf.get(i)!) if (!covered.has(tag)) gain++;
      if (gain > bestGain) {
        best = i;
        bestGain = gain;
      }
    }
    if (best < 0) break;
    chosen.add(best);
    for (const tag of tagsOf.get(best)!) covered.add(tag);
  }

  const seenFeature = new Set<string>();
  for (const i of usable) {
    for (const feature of textFeatures(cases[i].md)) {
      if (!seenFeature.has(feature)) {
        seenFeature.add(feature);
        chosen.add(i);
      }
    }
  }

  const rest = usable.filter((i) => !chosen.has(i));
  const want = Math.max(0, FIXTURE_TARGET - chosen.size);
  if (want > 0 && rest.length > 0) {
    const stride = Math.max(1, Math.floor(rest.length / want));
    for (let i = 0; i < rest.length; i += stride) chosen.add(rest[i]);
  }
  return [...chosen].sort((a, b) => a - b);
}

// -------------------------------------------------------------------- main

const args = Deno.args;
const flag = (name: string, fallback: string | null = null) => {
  const at = args.indexOf(name);
  return at >= 0 && at + 1 < args.length ? args[at + 1] : fallback;
};
const target = flag("--target", DEFAULT_TARGET)!;
const ir = flag("--ir", DEFAULT_IR)!;
const full = flag("--full");
const check = args.includes("--check");

const work = await Deno.makeTempDir({ prefix: "md4lean-oracle-" });

const real = await corpusFromIr(ir);
const curated: Case[] = [
  ...CURATED.map(([what, md]) => ({ what: `curated: ${what}`, md, flags: DOCSTRING_FLAGS })),
  ...DIALECTS.map(([what, flags, md]) => ({ what: `dialect: ${what}`, md, flags })),
  ...CRASHERS.map(([what, flags, md]) => ({ what: `crasher: ${what}`, md, flags })),
];
const cases = [...curated, ...real];
console.error(`${real.length} distinct docstrings + ${curated.length} hand-written`);

const answers = await runOracle(target, work, cases);
if (answers.length !== cases.length) {
  throw new Error(`${answers.length} answers for ${cases.length} cases`);
}

const crashed: { what: string; md: string }[] = [];
const usable: number[] = [];
for (let i = 0; i < cases.length; i++) {
  if (answers[i] === "crashed") crashed.push(cases[i]);
  else usable.push(i);
}

const manifest = await readJson(`${target}/lake-manifest.json`);
const md4lean = (manifest.packages ?? []).find((p: any) => p.name === "MD4Lean");
const provenance = {
  generatedBy: "crates/lean-doc-md/tests/oracle/gen-md4lean-expected.ts",
  oracle: "MD4Lean.parse, run under lake env lean in the measurement target",
  target,
  leanToolchain: (await Deno.readTextFile(`${target}/lean-toolchain`)).trim(),
  md4leanRev: md4lean?.rev ?? null,
  irPath: ir,
  irDocstrings: real.length,
  flags: DOCSTRING_FLAGS,
  deno: Deno.version.deno,
};

const buildFixture = (indices: number[]) => ({
  ...provenance,
  cases: indices.map((i) => ({
    what: cases[i].what,
    flags: cases[i].flags,
    md: cases[i].md,
    ast: answers[i],
  })),
  crashesMd4lean: CRASHERS.map(([what, flags, md, how]) => ({ what, flags, md, how })),
});

if (full) {
  await Deno.writeTextFile(full, JSON.stringify(buildFixture(usable), null, 0) + "\n");
  console.error(`${usable.length} cases -> ${full}`);
}

const selected = selectCases(cases, answers, curated.length).filter((i) =>
  answers[i] !== "crashed"
);
const fixture = JSON.stringify(buildFixture(selected)) + "\n";

if (check) {
  const committed = await Deno.readTextFile(FIXTURE);
  if (committed !== fixture) {
    console.error(`${FIXTURE.pathname} is not what this script produces`);
    Deno.exit(1);
  }
  console.error(`${FIXTURE.pathname} is current (${selected.length} cases)`);
} else {
  await Deno.writeTextFile(FIXTURE, fixture);
  console.error(
    `${selected.length} of ${cases.length} cases -> ${FIXTURE.pathname}` +
      ` (${crashed.length} crashed MD4Lean and were dropped)`,
  );
}
await Deno.remove(work, { recursive: true });
