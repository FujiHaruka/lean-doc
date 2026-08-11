#!/usr/bin/env -S deno run --allow-read --allow-write --allow-run --allow-env
// gen-docgen4-expected.ts -- produce the expected HTML for `tests/docgen4.rs`
// by running *doc-gen4 itself*.
//
// WHY THIS FILE EXISTS
// --------------------
// `src/html.rs` is a transcription of `DocGen4/Output/DocString.lean`. Expected
// values derived from reading that file would prove nothing -- the reading and
// the port would share whatever mistake was made. So they come from doc-gen4:
// `dump-html.lean` calls `docStringToHtml` in the measurement target's own
// environment and prints the bytes.
//
// THE CORPUS IS REAL DATA
// -----------------------
// Every docstring in the target package's IR, deduplicated, plus the
// hand-written cases from `gen-md4lean-expected.ts` (which exist to reach the
// corners the package does not contain). Every case is answered by doc-gen4.
//
// THE ROOT IS VARIED
// ------------------
// `getRoot` is prepended to every relative link and to the `find/?pattern=`
// fallback, so it is part of the bytes. Cases cycle through depths 0, 1 and 2
// so that a port which ignored the root, or applied it twice, could not pass.
//
// SOME INPUTS CRASH IT
// --------------------
// The two that kill `MD4Lean.parse` kill this too (a NUL inside a fenced code
// block; a GFM table with a header and no body rows). The runner finds them by
// resuming after each crash and records them separately -- a crash is not an
// oracle.
//
// npm/node are broken in this environment; this must run under deno.
//
// usage:
//   deno run --allow-read --allow-write --allow-run --allow-env \
//     crates/lean-doc-md/tests/oracle/gen-docgen4-expected.ts
//   ... --full /tmp/docgen4-full.json   also write every case, not just the
//                                       committed sample
//   ... --check                         fail if the committed file is stale

const FIXTURE = new URL("../data/docgen4-expected.json", import.meta.url);
const DUMPER = new URL("dump-html.lean", import.meta.url);
const CURATED_FROM = new URL("gen-md4lean-expected.ts", import.meta.url);

const DEFAULT_TARGET = "/Users/haruka/dev/lean-projects";
const DEFAULT_IR = "/private/tmp/lean-doc-relay/w7h/base-ir";

/** How many cases the committed fixture aims for. See `selectCases`. */
const FIXTURE_TARGET = 320;

// ---------------------------------------------------------- the corpus

type Case = { what: string; depth: number; md: string };

/**
 * The hand-written inputs, taken from `gen-md4lean-expected.ts` rather than
 * copied: one list, so a corner added for the parser is also rendered.
 *
 * Its `CURATED` is a plain array literal, so it is sliced out of the source and
 * evaluated. The dialect-varying and crashing lists are not reused: this
 * oracle's flags are whatever `docStringToHtml` hardcodes, and the crashers are
 * appended below with their own labels.
 */
async function curatedCases(): Promise<[string, string][]> {
  const source = await Deno.readTextFile(CURATED_FROM);
  const from = source.indexOf("const CURATED: [string, string][] = [");
  const to = source.indexOf("\n];", from);
  if (from < 0 || to < 0) {
    console.error(`${CURATED_FROM.pathname} no longer declares CURATED as an array literal`);
    Deno.exit(3);
  }
  const literal = source.slice(source.indexOf("[", from + 30), to + 2);
  const module = await import(
    `data:text/typescript;charset=utf-8,${
      encodeURIComponent(`export const CURATED: [string, string][] = ${literal};`)
    }`
  ) as { CURATED: [string, string][] };
  return module.CURATED;
}

/**
 * Corners that only exist once the tree is rendered, so the parser's corpus has
 * no reason to contain them. Each is still answered by doc-gen4.
 */
const HTML_CURATED: [string, string][] = [
  ["heading id, punctuation runs", "# a..b -- c, d!\n"],
  ["heading id, leading and trailing punctuation", "# ...a...\n"],
  ["heading id, only punctuation", "# ---\n\n# .\n"],
  ["heading id, markup inside", "# a `b.c` *d* [e](f)\n"],
  ["heading id, non-ascii", "# ∑ over ℕ and 𝒜\n"],
  ["heading id, entity", "# a &amp; b\n"],
  ["heading levels", "# a\n\n## a\n\n### a\n\n#### a\n\n##### a\n\n###### a\n"],
  ["escapes in text", 'a < b & c > d " e \' f\n'],
  ["escapes in a code span", "`a < b & c`\n"],
  ["escapes in a link destination", '[t](a"b&c)\n'],
  ["escapes in a link title", '[t](a "b & <c>")\n'],
  ["escapes in an image", '![a & b](s"rc.png "t&t")\n'],
  ["escapes in a heading id", "# a & b < c\n"],
  ["escapes in math", "$a < b$\n\n$$a & b$$\n"],
  ["escapes in a code fence language", "```a&b\nx\n```\n"],
  ["link, root-relative", "[t](x/y.html)\n"],
  ["link, anchor", "[t](#frag)\n"],
  ["link, http", "[t](http://a/b)\n"],
  ["link, httpfoo is left alone too", "[t](httpfoo/bar)\n"],
  ["link, name search", "[t](##Nat.succ)\n"],
  ["link, name search with punctuation", "[t](##a b&c)\n"],
  ["link, empty destination", "[t]()\n"],
  ["nested link and code", "[`Nat.succ`](x)\n"],
  ["ordered list, start 1 has no attribute", "1. a\n"],
  ["ordered list, start 5", "5. a\n6. b\n"],
  ["ordered list, start 0", "0. a\n"],
  ["tight list with two blocks", "- a\n  - b\n"],
  ["loose list with two blocks", "- a\n\n  b\n"],
  ["list item holding a quote", "- > q\n"],
  ["task list, mixed", "- [x] a\n- [X] b\n- [ ] c\n"],
  ["task list, loose", "- [x] a\n\n- [ ] b\n"],
  ["table, full", "| a | b |\n|:-:|--:|\n| 1 | 2 |\n| 3 | 4 |\n"],
  ["table, markup in cells", "| `a` | *b* |\n|---|---|\n| [c](d) | $e$ |\n"],
  ["hard break", "a  \nb\n"],
  ["soft break", "a\nb\n"],
  ["thematic break between paragraphs", "a\n\n---\n\nb\n"],
  ["blockquote holding everything", "> # h\n>\n> - a\n>\n> ```\n> c\n> ```\n"],
  ["code block, lean is auto-linked", "```lean\nNat.succ x\n```\n"],
  ["code block, no language is auto-linked", "```\nNat.succ x\n```\n"],
  ["code block, other language is not", "```python\nNat.succ x\n```\n"],
  ["code block, info string beyond the language", "```lean showFrom=2\nx\n```\n"],
  ["code span with several words", "`Nat.succ x y`\n"],
  ["code span with tabs and newlines", "`a\tb`\n"],
  ["image without a title", "![alt](s.png)\n"],
  ["image with markup in the alt", "![a *b* `c` &amp;](s.png)\n"],
  ["nul in text", "a\u0000b\n"],
  ["empty", ""],
  ["only a trailing newline", "\n"],
];

/**
 * Real docstrings the sample must keep whatever the coverage search picks.
 *
 * The one entry is the *only* docstring in the package on which the frozen
 * prototype and doc-gen4 disagree 【実測: 1 of 4,858; see
 * `gen-ts-docstring-expected.ts`】. Without it the committed fixture would
 * exercise that disagreement only through hand-written cases, and
 * `tests/ts_docstring.rs` would have nothing real to stand on.
 */
const MUST_INCLUDE = new Set([
  "InformationTheory.Shannon.TimeBandLimiting.Count module doc 1",
]);

async function readJson(path: string): Promise<any> {
  return JSON.parse(await Deno.readTextFile(path));
}

/** Every docstring in the IR, deduplicated, in a deterministic order. */
async function corpusFromIr(ir: string): Promise<{ what: string; md: string }[]> {
  const index = await readJson(`${ir}/index.json`);
  const seen = new Set<string>();
  const cases: { what: string; md: string }[] = [];
  const add = (what: string, md: string | null | undefined) => {
    if (typeof md !== "string" || seen.has(md)) return;
    seen.add(md);
    cases.push({ what, md });
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

/** The two inputs that kill the Lean side, kept out of the expected set. */
const CRASHERS: [string, string][] = [
  ["nul in a fenced code block", "```\na\u0000b\n```\n"],
  ["table with a header and no body", "| a | b |\n|---|---|\n"],
];

// ---------------------------------------------------------------- the oracle

const DYLIB = Deno.build.os === "darwin" ? "dylib" : "so";
/**
 * The interpreter needs a native implementation for every `@[extern]` the
 * script reaches. MD4Lean's are the parser; UnicodeBasic's is the general
 * category lookup, and without it every heading containing a non-ASCII
 * character dies (`getGC` answers ASCII from a literal table and calls out for
 * the rest) -- which looks exactly like a crashing corpus.
 */
const LIBS = [
  `.lake/packages/MD4Lean/.lake/build/lib/libleanmd4c.${DYLIB}`,
  `.lake/packages/MD4Lean/.lake/build/lib/libMD4Lean_MD4Lean.${DYLIB}`,
  `.lake/packages/UnicodeBasic/.lake/build/lib/libUnicodeBasic_UnicodeBasic.${DYLIB}`,
];

/**
 * Runs `dump-html.lean` over `cases`, resuming past any input that kills it.
 *
 * `lake env` has to run inside the target package (that is what supplies Lean,
 * doc-gen4 and the built MD4Lean); nothing is written there -- both file
 * arguments are in `work`.
 */
async function runOracle(
  target: string,
  work: string,
  cases: Case[],
): Promise<(string | "crashed")[]> {
  const answers: (string | "crashed")[] = [];
  let from = 0;
  let round = 0;

  while (from < cases.length) {
    const inPath = `${work}/corpus-${round}.jsonl`;
    const outPath = `${work}/html-${round}.jsonl`;
    await Deno.writeTextFile(
      inPath,
      cases.slice(from).map((c) => JSON.stringify([c.depth, c.md])).join("\n") + "\n",
    );

    const command = new Deno.Command("lake", {
      args: [
        "env",
        "lean",
        ...LIBS.map((lib) => `--load-dynlib=${lib}`),
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
      `doc-gen4 died (${signal ?? code}) on ${JSON.stringify(cases[victim].what)}; skipping it`,
    );
    answers.push("crashed");
    from = victim + 1;
    round++;
  }
  return answers;
}

// ---------------------------------------------------------------- selection

/**
 * What shape a rendered docstring has: which tags, which attributes and which
 * of the handful of one-off constructs it contains. Two docstrings with the
 * same signature exercise the same branches of the renderer, so keeping one of
 * each is what makes a sample worth as much as the corpus.
 *
 * Unlike the parser's oracle this reads the *output*, which is the thing being
 * checked -- there is no tree here to walk.
 */
function signature(html: string): string {
  const features = new Set<string>();
  for (const m of html.matchAll(/<\/?([a-z0-9-]+)/g)) features.add(`<${m[1]}`);
  for (const m of html.matchAll(/ ([a-z-]+)="/g)) features.add(`@${m[1]}`);
  if (html.includes('class="language-')) features.add("lang");
  if (html.includes("checked=")) features.add("checked");
  if (html.includes("<br>\n")) features.add("br");
  if (html.includes("$$")) features.add("mathdisplay");
  else if (html.includes("$")) features.add("math");
  if (html.includes("�")) features.add("nullchar");
  if (/&(?!amp;|lt;|gt;|quot;)[a-zA-Z#]/.test(html)) features.add("raw-entity");
  if (html.includes("find/?pattern=")) features.add("find");
  if (html.includes('start="')) features.add("start");
  return [...features].sort().join(",");
}

/** Inputs whose *bytes* are the interesting part, whatever HTML they made. */
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
 * Which cases the committed fixture keeps: every hand-written one, then a
 * greedy cover of the output features, then the first case showing each
 * byte-level feature, then an even stride over the rest. The whole corpus is
 * still checked -- see `--full` and `LEAN_DOC_DOCGEN4_FULL` in
 * `tests/docgen4.rs`.
 */
function selectCases(
  cases: Case[],
  answers: (string | "crashed")[],
  curatedCount: number,
): number[] {
  const chosen = new Set<number>();
  for (let i = 0; i < curatedCount; i++) {
    if (answers[i] !== "crashed") chosen.add(i);
  }

  const usable: number[] = [];
  const wanted = new Set(MUST_INCLUDE);
  for (let i = curatedCount; i < cases.length; i++) {
    if (answers[i] === "crashed") continue;
    usable.push(i);
    if (wanted.delete(cases[i].what)) chosen.add(i);
  }
  if (wanted.size > 0) {
    console.error(`MUST_INCLUDE names nothing in the corpus: ${[...wanted].join(", ")}`);
    Deno.exit(5);
  }
  const featuresOf = new Map<number, Set<string>>();
  for (const i of usable) {
    featuresOf.set(i, new Set(signature(answers[i] as string).split(",")));
  }

  const covered = new Set<string>();
  for (const i of chosen) {
    for (const f of signature(answers[i] as string).split(",")) covered.add(f);
  }
  for (;;) {
    let best = -1;
    let bestGain = 0;
    for (const i of usable) {
      if (chosen.has(i)) continue;
      let gain = 0;
      for (const f of featuresOf.get(i)!) if (!covered.has(f)) gain++;
      if (gain > bestGain) {
        best = i;
        bestGain = gain;
      }
    }
    if (best < 0) break;
    chosen.add(best);
    for (const f of featuresOf.get(best)!) covered.add(f);
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

const work = await Deno.makeTempDir({ prefix: "docgen4-oracle-" });

/** `getRoot` for depth 0, 1, 2: `"./"`, `"../"`, `"../.././"`. */
const DEPTHS = [0, 1, 2];

const curated = [
  ...(await curatedCases()).map(([what, md]) => ({ what: `curated: ${what}`, md })),
  ...HTML_CURATED.map(([what, md]) => ({ what: `html: ${what}`, md })),
  ...CRASHERS.map(([what, md]) => ({ what: `crasher: ${what}`, md })),
];
const real = await corpusFromIr(ir);
const cases: Case[] = [...curated, ...real].map((c, i) => ({
  what: c.what,
  depth: DEPTHS[i % DEPTHS.length],
  md: c.md,
}));
console.error(`${real.length} distinct docstrings + ${curated.length} hand-written`);

const answers = await runOracle(target, work, cases);
if (answers.length !== cases.length) {
  throw new Error(`${answers.length} answers for ${cases.length} cases`);
}

const usable: number[] = [];
let crashed = 0;
for (let i = 0; i < cases.length; i++) {
  if (answers[i] === "crashed") crashed++;
  else usable.push(i);
}

const manifest = await readJson(`${target}/lake-manifest.json`);
// Lake writes the name with guillemets because of the hyphen.
const docGen4 = (manifest.packages ?? []).find((p: any) =>
  p.name === "doc-gen4" || p.name === "«doc-gen4»"
);

/**
 * The doc-gen4 checkout in the target carries the benchmark instrumentation
 * patch, so "the rev" alone would not say whether the oracle ran modified code.
 * These two files are the oracle; if either is dirty the answers below are not
 * doc-gen4's.
 */
const oracleFiles = ["DocGen4/Output/DocString.lean", "DocGen4/Output/ToHtmlFormat.lean"];
const dirty = await new Deno.Command("git", {
  args: ["diff", "--quiet", "--", ...oracleFiles],
  cwd: `${target}/.lake/packages/doc-gen4`,
  stdout: "null",
  stderr: "null",
}).output();
if (dirty.code !== 0) {
  console.error(`${oracleFiles.join(" and ")} have local changes; the oracle is not doc-gen4`);
  Deno.exit(4);
}
const provenance = {
  generatedBy: "crates/lean-doc-md/tests/oracle/gen-docgen4-expected.ts",
  oracle: "DocGen4.Output.docStringToHtml, run under lake env lean in the measurement target",
  target,
  leanToolchain: (await Deno.readTextFile(`${target}/lean-toolchain`)).trim(),
  docGen4Rev: docGen4?.rev ?? null,
  docGen4OracleFilesClean: oracleFiles,
  irPath: ir,
  irDocstrings: real.length,
  roots: DEPTHS.map((d) => "../".repeat(d) + "./"),
  deno: Deno.version.deno,
};

const buildFixture = (indices: number[]) => ({
  ...provenance,
  cases: indices.map((i) => ({
    what: cases[i].what,
    root: "../".repeat(cases[i].depth) + "./",
    md: cases[i].md,
    html: answers[i],
  })),
  crashesDocGen4: CRASHERS.map(([what, md]) => ({ what, md })),
});

if (full) {
  await Deno.writeTextFile(full, JSON.stringify(buildFixture(usable), null, 0) + "\n");
  console.error(`${usable.length} cases -> ${full}`);
}

const selected = selectCases(cases, answers, curated.length);
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
      ` (${crashed} crashed doc-gen4 and were dropped)`,
  );
}
await Deno.remove(work, { recursive: true });
