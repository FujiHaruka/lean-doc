// stage 7e (second half): compare what the Lean *parser alone* can say about a
// module against the IR produced by the semantic extractor.
//
// The IR is the independent source of truth here. Nothing in this file reads a
// number that the parser probe computed about itself; it only reads the names,
// binder text and type text the probe emitted, and diffs them against the IR.
//
// usage: deno run --allow-read analyze-decls.ts <decls.jsonl> <ir-dir> [--dump <prefix>]

const [declsPath, irDir, ...rest] = Deno.args;
let dumpPrefix: string | null = null;
for (let i = 0; i < rest.length; i++) if (rest[i] === "--dump") dumpPrefix = rest[i + 1];

type SynDecl = {
  mod: string; name: string; asWritten: string; kind: string;
  line: number; col: number; binders: string; type: string;
  hasType: boolean; underIn: boolean; private?: boolean;
};
type IrDecl = {
  name: string; kind: string; binders: string[]; type: string;
  implicits: boolean[]; modifiers: string[]; members: unknown[];
  line: number; col: number;
};

// ---------------------------------------------------------------- load

const synAll: SynDecl[] = [];
for (const line of (await Deno.readTextFile(declsPath)).split("\n")) {
  if (line.trim()) synAll.push(JSON.parse(line) as SynDecl);
}
const nPrivate = synAll.filter((d) => d.private).length;
// doc-gen4 does not emit private declarations, so they are dropped before the
// diff; the count is reported so the drop is visible rather than silent.
const synPublic = synAll.filter((d) => !d.private);

const syn = new Map<string, SynDecl[]>();
for (const d of synPublic) {
  let a = syn.get(d.mod);
  if (!a) syn.set(d.mod, a = []);
  a.push(d);
}

const ir = new Map<string, IrDecl[]>();
const irNameToMod = new Map<string, string>();
for await (const e of Deno.readDir(irDir)) {
  if (!e.name.endsWith(".json")) continue;
  const j = JSON.parse(await Deno.readTextFile(`${irDir}/${e.name}`));
  ir.set(j.module, j.declarations as IrDecl[]);
  for (const d of j.declarations as IrDecl[]) irNameToMod.set(d.name, j.module);
}

// ---------------------------------------------------------------- helpers

const ws = (s: string) => s.replace(/\s+/g, " ").trim();
/** Drops namespace prefixes from dotted identifiers: the IR pretty-prints
 * `MeasureTheory.Measure` where the source, inside `open MeasureTheory`, wrote
 * `Measure`. Nothing but `open` resolution can bridge that, so this normalises
 * it away to size the effect. */
const unqual = (s: string) =>
  s.replace(/(?<![\p{L}\p{N}_'.])(?:[\p{L}_][\p{L}\p{N}_']*\.)+(?=[\p{L}_])/gu, "");

function splitBinders(raw: string): string[] {
  const out: string[] = [];
  const open = "([{⦃⟨", close = ")]}⦄⟩";
  let depth = 0, cur = "";
  for (const ch of raw) {
    if (open.includes(ch)) {
      if (depth === 0 && cur.trim()) { out.push(cur.trim()); cur = ""; }
      depth++; cur += ch;
    } else if (close.includes(ch)) {
      depth--; cur += ch;
      if (depth === 0) { out.push(cur.trim()); cur = ""; }
    } else if (depth === 0 && /\s/.test(ch)) {
      if (cur.trim()) { out.push(cur.trim()); cur = ""; }
    } else cur += ch;
  }
  if (cur.trim()) out.push(cur.trim());
  return out;
}

function missKind(d: IrDecl, structNames: Set<string>, fields: Set<string>): string {
  const last = d.name.split(".").pop()!;
  if (d.kind === "constructor") {
    return structNames.has(d.name.slice(0, -(last.length + 1)))
      ? "auto: structure constructor (.mk)" : "auto: constructor";
  }
  if (fields.has(d.name)) return "auto: structure field projection (in `structFields`, walker does not descend)";
  if (/^«/.test(last)) return "auto: notation-generated decl (`«term...»`)";
  if (d.kind === "instance" && /^inst/.test(last)) return "auto: anonymous instance (name synthesised by elaborator)";
  if (/^(rec|recOn|casesOn|injEq|noConfusion|noConfusionType|below|brecOn|sizeOf_spec|toCtorIdx|induct)$/.test(last)) return "auto: kernel-generated";
  if (/^eq_\d+$/.test(last) || last === "eq_def") return "auto: equation lemma";
  return "MISSED (should have been visible in the syntax)";
}

// ---------------------------------------------------------------- (B) names

let nMatch = 0, nSynOnly = 0, nIrOnly = 0, nIr = 0, nSyn = 0, nSynOnlyElsewhere = 0;
const missBuckets = new Map<string, number>();
const missSamples = new Map<string, string[]>();
const fpBuckets = new Map<string, number>();
const fpSamples = new Map<string, string[]>();
const perModule: Record<string, number | string>[] = [];
const matched: { s: SynDecl; i: IrDecl }[] = [];
const bump = (b: Map<string, number>, s: Map<string, string[]>, k: string, v: string) => {
  b.set(k, (b.get(k) ?? 0) + 1);
  const a = s.get(k) ?? [];
  if (a.length < 8 && v) { a.push(v); s.set(k, a); }
};

for (const [mod, irDecls] of ir) {
  const synDecls = syn.get(mod) ?? [];
  const synByName = new Map<string, SynDecl>();
  for (const s of synDecls) if (!synByName.has(s.name)) synByName.set(s.name, s);
  const structNames = new Set(irDecls.filter((d) => d.kind === "structure").map((d) => d.name));
  const irNames = new Set(irDecls.map((d) => d.name));
  // Structure fields show up twice in the IR: inside the structure's `members`
  // and again as their own top-level projection declaration.
  const fields = new Set<string>();
  for (const d of irDecls) {
    for (const mm of (d.members ?? []) as { label?: string; name?: string }[]) {
      if (mm.label === "field" && mm.name) fields.add(mm.name);
    }
  }

  let m = 0, io = 0, so = 0;
  for (const d of irDecls) {
    const s = synByName.get(d.name);
    if (s) { m++; matched.push({ s, i: d }); }
    else { io++; bump(missBuckets, missSamples, missKind(d, structNames, fields), `${d.kind} ${d.name}`); }
  }
  for (const s of synDecls) {
    if (irNames.has(s.name)) continue;
    so++;
    const elsewhere = irNameToMod.get(s.name);
    if (elsewhere) { nSynOnlyElsewhere++; bump(fpBuckets, fpSamples, "name exists in the IR but in another module", `${s.name}\n        syntax says ${mod}, IR says ${elsewhere}`); }
    else bump(fpBuckets, fpSamples, "name absent from the IR entirely", `${mod}  ${s.kind} ${s.name}`);
  }
  nMatch += m; nIrOnly += io; nSynOnly += so;
  nIr += irDecls.length; nSyn += synDecls.length;
  perModule.push({ mod, ir: irDecls.length, syn: synDecls.length, match: m, irOnly: io, synOnly: so });
}

console.log("=== (B) declaration names: syntax tree vs IR");
console.log(`syntax declarations (raw)  ${synAll.length}`);
console.log(`  of which \`private\`       ${nPrivate}   (doc-gen4 does not document these; dropped before the diff)`);
console.log(`syntax declarations (kept) ${nSyn}`);
console.log(`IR declarations            ${nIr}`);
console.log(`matched (same full name in the same module)`);
console.log(`                           ${nMatch}  = ${(100 * nMatch / nIr).toFixed(1)}% of IR, ${(100 * nMatch / nSyn).toFixed(1)}% of syntax`);
console.log(`IR only (missed)           ${nIrOnly}  (${(100 * nIrOnly / nIr).toFixed(1)}% of IR)`);
console.log(`syntax only (false pos.)   ${nSynOnly}  (${(100 * nSynOnly / nSyn).toFixed(1)}% of syntax), of which ${nSynOnlyElsewhere} exist in another module's IR`);
console.log("");
console.log("missed, by category:");
for (const [k, v] of [...missBuckets].sort((a, b) => b[1] - a[1])) {
  console.log(`  ${String(v).padStart(5)}  (${(100 * v / nIr).toFixed(2)}% of IR)  ${k}`);
  for (const s of missSamples.get(k) ?? []) console.log(`           ${s}`);
}
console.log("");
console.log("false positives, by category:");
for (const [k, v] of [...fpBuckets].sort((a, b) => b[1] - a[1])) {
  console.log(`  ${String(v).padStart(5)}  ${k}`);
  for (const s of fpSamples.get(k) ?? []) console.log(`           ${s}`);
}

const recalls = perModule.filter((p) => (p.ir as number) > 0)
  .map((p) => (p.match as number) / (p.ir as number)).sort((a, b) => a - b);
const q = (f: number) => recalls[Math.min(recalls.length - 1, Math.floor(f * recalls.length))];
console.log("");
console.log(`per-module recall  min ${(100 * recalls[0]).toFixed(1)}%  p10 ${(100 * q(0.10)).toFixed(1)}%  p50 ${(100 * q(0.50)).toFixed(1)}%  p90 ${(100 * q(0.90)).toFixed(1)}%`);
const perfect = perModule.filter((p) => (p.ir as number) > 0 && p.irOnly === 0 && p.synOnly === 0).length;
console.log(`modules with an exactly matching declaration set: ${perfect} / ${perModule.length}`);

// ---------------------------------------------------------------- (C) signatures

// "Signature" is defined as <binders> " : " <type>. The IR side is `.binders`
// (an array of pretty-printed binder groups) joined with single spaces plus
// `.type` (the pretty-printed result type). Nothing from `typeCode` /
// `binderCode` (hyperlink spans) or `modifiers` is used. The syntax side is the
// verbatim source text of the same two regions of the declaration.
//
// This is a naive text comparison, NOT the acceptance scoring of
// experiments/stage4c/coverage.ts, which is left untouched.
let exact = 0, wsEq = 0, unqEq = 0, sufEq = 0;
let bExact = 0, bWs = 0, bUnq = 0, tExact = 0, tWs = 0, tUnq = 0;
let noType = 0, irExtraBinders = 0, irExtraBinderCount = 0, synBinderCount = 0, irBinderCount = 0;
const sigB = new Map<string, number>(), sigS = new Map<string, string[]>();

for (const { s, i } of matched) {
  const irB = i.binders ?? [], irT = i.type ?? "";
  const synB = splitBinders(s.binders), synT = s.type;
  const irSig = [...irB, ":", irT].join(" ");
  const synSig = [...synB, ":", synT].join(" ");

  if (irSig === synSig) exact++;
  const w1 = ws(irSig) === ws(synSig); if (w1) wsEq++;
  const w2 = ws(unqual(irSig)) === ws(unqual(synSig)); if (w2) unqEq++;

  if (irB.join(" ") === synB.join(" ")) bExact++;
  if (ws(irB.join(" ")) === ws(synB.join(" "))) bWs++;
  if (ws(unqual(irB.join(" "))) === ws(unqual(synB.join(" ")))) bUnq++;
  if (irT === synT) tExact++;
  if (ws(irT) === ws(synT)) tWs++;
  const tUnqEq = ws(unqual(irT)) === ws(unqual(synT));
  if (tUnqEq) tUnq++;

  // Is the source's binder list a suffix of the IR's (after unqualifying)?
  // That is the `variable` / auto-bound-implicit case: the IR shows binders the
  // file never wrote at the declaration.
  const irW = irB.map((x) => ws(unqual(x))), synW = synB.map((x) => ws(unqual(x)));
  const isSuffix = synW.length <= irW.length &&
    synW.every((x, k) => irW[irW.length - synW.length + k] === x);
  if (tUnqEq && isSuffix) sufEq++;
  irBinderCount += irW.length; synBinderCount += synW.length;
  if (isSuffix && irW.length > synW.length) { irExtraBinders++; irExtraBinderCount += irW.length - synW.length; }

  if (irSig === synSig) { bump(sigB, sigS, "byte-exact", ""); continue; }
  if (w1) { bump(sigB, sigS, "whitespace / line-wrapping only", i.name); continue; }
  if (!s.hasType) { noType++; bump(sigB, sigS, "type omitted at the definition site (`def f := ...`)", `${i.name}  | IR type: ${ws(irT).slice(0, 70)}`); continue; }
  if (w2) { bump(sigB, sigS, "name qualification only (IR prints `A.b`, source wrote `b` under `open A`)", `${i.name}\n        ir : ${ws(irT).slice(0, 80)}\n        syn: ${ws(synT).slice(0, 80)}`); continue; }
  if (tUnqEq && isSuffix) { bump(sigB, sigS, "IR adds leading binders the file never wrote (`variable` / auto-bound) + qualification", `${i.name}\n        ir : ${irW.join(" ").slice(0, 100)}\n        syn: ${synW.join(" ").slice(0, 100)}`); continue; }
  if (tUnqEq) { bump(sigB, sigS, "binders differ beyond qualification (type agrees)", `${i.name}\n        ir : ${irW.join(" ").slice(0, 100)}\n        syn: ${synW.join(" ").slice(0, 100)}`); continue; }
  if (isSuffix) { bump(sigB, sigS, "type differs beyond qualification (binder prefix explains the rest)", `${i.name}\n        ir : ${ws(unqual(irT)).slice(0, 100)}\n        syn: ${ws(unqual(synT)).slice(0, 100)}`); continue; }
  bump(sigB, sigS, "both binders and type differ beyond qualification", `${i.name}\n        ir : ${irW.join(" ").slice(0, 70)} : ${ws(unqual(irT)).slice(0, 70)}\n        syn: ${synW.join(" ").slice(0, 70)} : ${ws(unqual(synT)).slice(0, 70)}`);
}

const M = matched.length;
const pc = (n: number) => `${n}  (${(100 * n / M).toFixed(1)}%)`;
console.log("");
console.log("=== (C) signature: verbatim source text vs IR pretty-print");
console.log(`signature := <binders> " : " <type>`);
console.log(`  IR side     = .binders joined by " " + " : " + .type   (not typeCode/binderCode/modifiers)`);
console.log(`  syntax side = verbatim source of the binder region and of the term after ":"`);
console.log(`  naive text comparison; NOT the stage4c coverage.ts acceptance scoring`);
console.log(`matched declarations       ${M}`);
console.log(`whole signature, byte-exact                          ${pc(exact)}`);
console.log(`  + whitespace/line-wrap normalised                  ${pc(wsEq)}`);
console.log(`  + namespace qualification normalised away          ${pc(unqEq)}`);
console.log(`  + IR-only leading binders ignored (type must match) ${pc(sufEq)}`);
console.log(`binders byte-exact ${pc(bExact)} | ws ${pc(bWs)} | unqual ${pc(bUnq)}`);
console.log(`type    byte-exact ${pc(tExact)} | ws ${pc(tWs)} | unqual ${pc(tUnq)}`);
console.log(`type omitted at the definition site  ${pc(noType)}`);
console.log(`binder groups: IR ${irBinderCount}, source ${synBinderCount}  (source writes ${(100 * synBinderCount / irBinderCount).toFixed(1)}% of them)`);
console.log(`declarations where the IR's binder list strictly extends the source's (a `+"`variable`"+` block the file never repeats): ${pc(irExtraBinders)}, ${irExtraBinderCount} binder groups invisible in the source`);
console.log("");
console.log("how the signatures deviate (each declaration counted once, first matching cause):");
for (const [k, v] of [...sigB].sort((a, b) => b[1] - a[1])) {
  console.log(`  ${String(v).padStart(5)}  (${(100 * v / M).toFixed(1)}%)  ${k}`);
  for (const s of sigS.get(k) ?? []) if (s) console.log(`        ${s}`);
}

if (dumpPrefix) {
  await Deno.writeTextFile(`${dumpPrefix}-permodule.jsonl`, perModule.map((p) => JSON.stringify(p)).join("\n") + "\n");
}
