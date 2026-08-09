#!/usr/bin/env -S deno run --allow-read
/**
 * Round-trip check for the stage-4b tagged IR: slice every fragment's plain text
 * with the span list that is stored next to it and verify that what comes out is
 * what the span claims to be.
 *
 * This is the *consumer-side* half of the offset contract. The extractor asserts
 * that the spans cover the fragment (`RefSink.collect` throws when the walked
 * width differs from the printed width); this asserts that a `String`-slicing
 * runtime — the one the offsets were chosen for — reproduces the tokens.
 *
 * Offsets are UTF-16 code units, which is exactly what `String.prototype.slice`
 * indexes, so there is no conversion step here on purpose: if the unit were wrong
 * this file would be full of `Array.from` and it would be the wrong file.
 *
 * usage: check-spans.ts --ir <dir> [--samples N]
 */

type Span = [number, number, number] | [number, number, 1, string];

interface Member {
  label: string;
  name: string;
  text: string;
  code?: Span[];
}

interface Decl {
  name: string;
  kind: string;
  index?: number;
  binders: string[];
  binderCode?: Span[][];
  type: string;
  typeCode?: Span[];
  equations: string[];
  equationCode?: Span[][];
  members: Member[];
  modifiers?: string[];
  line: number;
  endLine?: number;
}

interface ModuleFile {
  schemaVersion: number;
  module: string;
  declarations: Decl[];
}

const args = Deno.args;
const irDir = args[args.indexOf("--ir") + 1];
const sampleTarget = args.includes("--samples")
  ? Number(args[args.indexOf("--samples") + 1])
  : 8;
if (!irDir || irDir.startsWith("--")) {
  console.error("usage: check-spans.ts --ir <dir> [--samples N]");
  Deno.exit(2);
}

const errors: string[] = [];
const samples: string[] = [];
let fragments = 0;
let taggedFragments = 0;
let spans = 0;
const byKind = [0, 0, 0];
let nonAsciiFragments = 0;
let astralFragments = 0;
let astralSpans = 0;
let constSpans = 0;
let constNonAscii = 0;
let constTailMatch = 0;
let constTailAscii = 0;
let sortTexts = new Map<string, number>();
let maxDepth = 0;
let nestedConstInConst = 0;
let modules = 0;
let decls = 0;

/** Code points, as opposed to the UTF-16 units the spans are in. */
function codePoints(s: string): number {
  let n = 0;
  for (const _ of s) n++;
  return n;
}

function fail(where: string, msg: string) {
  if (errors.length < 40) errors.push(`${where}: ${msg}`);
  else if (errors.length === 40) errors.push("… (more suppressed)");
}

function checkFragment(where: string, text: string, code: Span[] | undefined) {
  fragments++;
  if (code === undefined) return;
  taggedFragments++;
  const utf16 = text.length;
  const cps = codePoints(text);
  if (cps !== utf16) astralFragments++;
  // deno-lint-ignore no-control-regex
  if (/[^\x00-\x7F]/.test(text)) nonAsciiFragments++;

  // A stack replay is the nesting rule from the README: pre-order, parent first,
  // outer first at equal offsets. If the list really is a flattened tree this
  // never fails, and the tree is recoverable from the list alone.
  const stack: Span[] = [];
  let prevStart = -1;
  for (const sp of code) {
    spans++;
    const [start, stop, kind] = sp;
    byKind[kind]++;
    if (!(0 <= start && start <= stop && stop <= utf16)) {
      fail(where, `span ${JSON.stringify(sp)} out of bounds for width ${utf16}`);
      continue;
    }
    if (start < prevStart) fail(where, `span ${JSON.stringify(sp)} starts before its predecessor`);
    prevStart = start;
    while (stack.length > 0 && start >= stack[stack.length - 1][1]) stack.pop();
    if (stack.length > 0 && stop > stack[stack.length - 1][1]) {
      fail(where, `span ${JSON.stringify(sp)} crosses its parent ${JSON.stringify(stack[stack.length - 1])}`);
    }
    if (kind === 1 && stack.some((p) => p[2] === 1)) nestedConstInConst++;
    stack.push(sp);
    if (stack.length > maxDepth) maxDepth = stack.length;

    const slice = text.slice(start, stop);
    if (codePoints(slice) !== stop - start) astralSpans++;
    if (kind === 1) {
      constSpans++;
      const name = (sp as [number, number, 1, string])[3];
      if (typeof name !== "string" || name.length === 0) {
        fail(where, `const span ${JSON.stringify(sp)} has no name`);
      }
      // doc-gen4's `splitWhitespaces` keeps the whitespace out of the anchor.
      if (slice !== slice.trim()) {
        fail(where, `const span ${JSON.stringify(sp)} has untrimmed whitespace: ${JSON.stringify(slice)}`);
      }
      // deno-lint-ignore no-control-regex
      if (/[^\x00-\x7F]/.test(slice)) {
        constNonAscii++;
        if (samples.length < sampleTarget) {
          samples.push(`${JSON.stringify(slice)} -> ${name}   in ${JSON.stringify(text.slice(0, 70))}`);
        }
      } else if (/^[A-Za-z_][A-Za-z0-9_.'!?]*$/.test(slice)) {
        constTailAscii++;
        if (slice.split(".").pop() === name.split(".").pop()) constTailMatch++;
      }
    } else if (kind === 2) {
      sortTexts.set(slice, (sortTexts.get(slice) ?? 0) + 1);
    }
  }
}

for (const entry of Deno.readDirSync(`${irDir}/modules`)) {
  if (!entry.isFile || !entry.name.endsWith(".json")) continue;
  modules++;
  const m: ModuleFile = JSON.parse(Deno.readTextFileSync(`${irDir}/modules/${entry.name}`));
  for (const d of m.declarations) {
    decls++;
    const w = `${m.module}#${d.name}`;
    if (d.binderCode !== undefined && d.binderCode.length !== d.binders.length) {
      fail(w, `binderCode has ${d.binderCode.length} entries for ${d.binders.length} binders`);
    }
    if (d.equationCode !== undefined && d.equationCode.length !== d.equations.length) {
      fail(w, `equationCode has ${d.equationCode.length} entries for ${d.equations.length} equations`);
    }
    d.binders.forEach((b, i) => checkFragment(`${w}.binder[${i}]`, b, d.binderCode?.[i]));
    checkFragment(`${w}.type`, d.type, d.typeCode);
    d.equations.forEach((e, i) => checkFragment(`${w}.equation[${i}]`, e, d.equationCode?.[i]));
    d.members.forEach((mem, i) => checkFragment(`${w}.member[${i}]`, mem.text, mem.code));
  }
}

const pct = (a: number, b: number) => b === 0 ? "-" : (100 * a / b).toFixed(2) + "%";
console.log(`ir                 ${irDir}`);
console.log(`modules            ${modules}`);
console.log(`declarations       ${decls}`);
console.log(`fragments          ${fragments} (${taggedFragments} carrying a span list)`);
console.log(`spans              ${spans} — ${byKind[1]} const, ${byKind[2]} sort, ${byKind[0]} other`);
console.log(`max nesting depth  ${maxDepth}`);
console.log(`const inside const ${nestedConstInConst}  (doc-gen4 drops the outer anchor for these)`);
console.log(`sort span texts    ${JSON.stringify([...sortTexts.entries()].sort((a, b) => b[1] - a[1]))}`);
console.log(`fragments with non-ASCII   ${nonAsciiFragments} (${pct(nonAsciiFragments, fragments)})`);
console.log(`fragments with non-BMP     ${astralFragments}   <- where UTF-16 units != code points`);
console.log(`spans whose slice is non-BMP ${astralSpans}`);
console.log(`const spans        ${constSpans}; non-ASCII slice ${constNonAscii} (${pct(constNonAscii, constSpans)})`);
console.log(`ASCII identifier slices whose last component matches the name: ${constTailMatch}/${constTailAscii} (${pct(constTailMatch, constTailAscii)})`);
if (samples.length > 0) {
  console.log(`\nnon-ASCII const samples (slice -> constant):`);
  for (const s of samples) console.log(`  ${s}`);
}
if (errors.length > 0) {
  console.log(`\nFAILURES (${errors.length}):`);
  for (const e of errors) console.log(`  ${e}`);
  Deno.exit(1);
}
console.log(`\nOK — every span is in bounds, properly nested and trimmed.`);
