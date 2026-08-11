#!/usr/bin/env -S deno run --allow-read --allow-write --allow-run --allow-env
// gen-ts-docstring-expected.ts -- render the same corpus with the *prototype's*
// docstring renderer, and record where it disagrees with doc-gen4.
//
// WHY THIS FILE EXISTS
// --------------------
// The inner loop of the migration is "byte diff against the TypeScript
// prototype" (plan §1). M1-c is where that loosens: the prototype hand-wrote a
// 594-line CommonMark subset, and this crate links the real md4c, so a
// disagreement no longer means the port is wrong. Plan §5 fixes the judgement:
// whichever side matches doc-gen4 is right, and md4c is not to be bent towards
// the subset.
//
// So this script does not produce an expectation for the Rust code. It produces
// the *list of inputs where the prototype and doc-gen4 differ*, which is the
// list of places M1-d will see page bytes move. `tests/ts_docstring.rs` asserts
// that on each of them the Rust output is doc-gen4's and not the prototype's --
// i.e. that every difference is the subset's limit, and none is a regression.
//
// HOW IT GETS AT render.ts WITHOUT IMPORTING IT
// ---------------------------------------------
// The same way `crates/lean-doc-render/tests/gen-ts-expected.ts` does: render.ts
// is a script whose top level parses argv and exits, and it is frozen, so its
// function definitions are sliced out by line range, assembled into a module and
// imported through a `data:` URL. `checkRanges` fails loudly if a range stops
// naming the function it claims to name.
//
// npm/node are broken in this environment; this must run under deno.
//
// usage:
//   deno run --allow-read --allow-write crates/lean-doc-md/tests/oracle/gen-ts-docstring-expected.ts
//   ... --full /tmp/docgen4-full.json   measure over the whole corpus, not just
//                                       the committed sample
//   ... --check                         verify the committed file

const FIXTURE = new URL("../data/ts-docstring-expected.json", import.meta.url);
const DOCGEN4 = new URL("../data/docgen4-expected.json", import.meta.url);
const RENDER_TS = new URL("../../../../experiments/stage7d/render.ts", import.meta.url);

// ---------------------------------------------------- slicing render.ts

const renderLines = (await Deno.readTextFile(RENDER_TS)).split("\n");

/** Lines `from..to` of render.ts, 1-based and inclusive. */
const slice = (from: number, to: number) => renderLines.slice(from - 1, to).join("\n");

/**
 * Line ranges in render.ts. `head` is text the first line must start with, so
 * that a range which has drifted onto other code is a failure rather than a
 * silently different expectation.
 *
 * `docstrings` is the whole of the prototype's docstring renderer in one piece:
 * `nameToLink` through `renderInline`, which is `render.ts:925-1672` minus its
 * leading comment. It is taken whole rather than function by function because
 * everything in it is mutually referential.
 */
const RANGES = {
  escapeHtml: { from: 331, to: 341, head: "function escapeHtml(" },
  moduleLink: { from: 385, to: 387, head: "function moduleLink(" },
  privatePrefix: { from: 415, to: 415, head: "const PRIVATE_PREFIX =" },
  docstrings: { from: 932, to: 1672, head: "function nameToLink(" },
} as const;

for (const [what, { from, to, head }] of Object.entries(RANGES)) {
  const text = slice(from, to);
  if (!text.startsWith(head)) {
    console.error(
      `render.ts:${from}-${to} no longer starts with ${JSON.stringify(head)} (${what}).\n` +
        `Found:\n${text.split("\n")[0]}`,
    );
    Deno.exit(3);
  }
}

const at = (k: keyof typeof RANGES) => slice(RANGES[k].from, RANGES[k].to);

/**
 * The module assembled out of those slices, plus the four names the sliced code
 * reads from render.ts's surroundings:
 *
 *   `known` / `linkIndex`  the name-to-module maps. Empty here, which is what
 *                          makes the prototype resolve nothing -- the same
 *                          condition the doc-gen4 oracle runs under.
 *   `abl`                  the CommonMark ablation switches, all off; that is
 *                          the production configuration (render.ts:304-312).
 *   `pageStats` / `T`      counters and timers, not behaviour.
 */
const MODULE_SOURCE = [
  "const known = new Map<string, string>();",
  "const linkIndex = new Map<string, string>();",
  "const abl = (_name: string) => false;",
  "const pageStats = { autolinkAttempts: 0, autolinkResolved: 0 };",
  "const T = { docstring: 0 };",
  at("privatePrefix"),
  at("escapeHtml"),
  at("moduleLink"),
  at("docstrings"),
  "export { renderDocString };",
].join("\n\n");

const prototype = await import(
  `data:text/typescript;charset=utf-8,${encodeURIComponent(MODULE_SOURCE)}`
) as {
  renderDocString(
    md: string,
    ctx: { root: string; moduleDeclNames: string[]; knownModules: Set<string> },
  ): string;
};

const renderTs = (md: string, root: string) =>
  prototype.renderDocString(md, { root, moduleDeclNames: [], knownModules: new Set() });

// -------------------------------------------------------------------- main

type Case = { what: string; root: string; md: string; html: string };

const args = Deno.args;
const flag = (name: string, fallback: string | null = null) => {
  const at = args.indexOf(name);
  return at >= 0 && at + 1 < args.length ? args[at + 1] : fallback;
};
const full = flag("--full");
const check = args.includes("--check");

const committed: { cases: Case[] } = JSON.parse(await Deno.readTextFile(DOCGEN4));

/** How the prototype and doc-gen4 compare over a set of cases. */
function measure(cases: Case[]) {
  const differ: { what: string; root: string; ts: string }[] = [];
  for (const c of cases) {
    const ts = renderTs(c.md, c.root);
    if (ts !== c.html) differ.push({ what: c.what, root: c.root, ts });
  }
  return { total: cases.length, differ };
}

/**
 * Over the whole corpus, when the doc-gen4 generator's `--full` file is on this
 * machine. The committed fixture records the number so that a claim about it can
 * be traced, and the sample carries the examples.
 */
const handWritten = (what: string) => what.startsWith("curated: ") || what.startsWith("html: ");

let corpus:
  | { total: number; differed: number; realTotal: number; realDiffered: number; realWhat: string[] }
  | null = null;
if (full) {
  const all: { cases: Case[] } = JSON.parse(await Deno.readTextFile(full));
  const m = measure(all.cases);
  const realWhat = m.differ.filter((d) => !handWritten(d.what)).map((d) => d.what);
  corpus = {
    total: m.total,
    differed: m.differ.length,
    realTotal: all.cases.filter((c) => !handWritten(c.what)).length,
    realDiffered: realWhat.length,
    realWhat,
  };
  console.error(
    `whole corpus: ${m.differ.length} of ${m.total} differ from doc-gen4` +
      ` (${realWhat.length} of ${corpus.realTotal} real docstrings)`,
  );
  for (const what of realWhat) console.error(`  real: ${what}`);
}

const sample = measure(committed.cases);
console.error(`committed sample: ${sample.differ.length} of ${sample.total} differ from doc-gen4`);

const fixture = JSON.stringify({
  generatedBy: "crates/lean-doc-md/tests/oracle/gen-ts-docstring-expected.ts",
  oracle: "experiments/stage7d/render.ts renderDocString, sliced out of the frozen prototype",
  renderTsRanges: RANGES,
  against: "tests/data/docgen4-expected.json",
  sampleCases: sample.total,
  corpus,
  // Only the disagreements: where the prototype and doc-gen4 agree, the test
  // that the Rust output is doc-gen4's already says it is the prototype's too.
  cases: sample.differ,
}) + "\n";

if (check) {
  const onDisk = await Deno.readTextFile(FIXTURE);
  if (onDisk !== fixture) {
    console.error(`${FIXTURE.pathname} is not what this script produces`);
    Deno.exit(1);
  }
  console.error(`${FIXTURE.pathname} is current (${sample.differ.length} disagreements)`);
} else {
  await Deno.writeTextFile(FIXTURE, fixture);
  console.error(`${sample.differ.length} disagreements -> ${FIXTURE.pathname}`);
}
