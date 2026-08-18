// Measures the shipped `search-index.json` and the proposed binary format v2.
//
// Usage:
//   deno run --allow-read benchmarks/tools/search-format-probe.ts <search-index.json>
//   deno run --allow-read --allow-write \
//       benchmarks/tools/search-format-probe.ts <search-index.json> --emit <out.bin>
//   deno run --allow-read --v8-flags=--expose-gc \
//       benchmarks/tools/search-format-probe.ts <search-index.json> --mem <json|json+source|json+lower>
//   deno run --allow-read --v8-flags=--expose-gc \
//       benchmarks/tools/search-format-probe.ts <out.bin> --mem bin
//
// The `--mem` modes must run one per process: V8 heap deltas are only readable
// against a baseline taken before anything is loaded.
//
// Every number this prints is a measurement of the file handed to it. Nothing
// here reads the repository, so it says nothing about a site it was not given.

const encoder = new TextEncoder();
const decoder = new TextDecoder();
const RESTART = 16;
const DOT = 46;

type Decl = [string, number, number];
type Index = { decls: Decl[]; kinds: string[]; modules: { n: string; p: string }[] };

const path = Deno.args[0] ?? "search-index.json";
const memMode = Deno.args.includes("--mem") ? Deno.args[Deno.args.indexOf("--mem") + 1] : null;
const emitTo = Deno.args.includes("--emit") ? Deno.args[Deno.args.indexOf("--emit") + 1] : null;

async function gzipLen(bytes: Uint8Array): Promise<number> {
  // Copied into a freshly-allocated buffer so the stream types line up; the
  // measurement is of `bytes`, and a copy of it compresses to the same length.
  const owned = new Uint8Array(bytes.length);
  owned.set(bytes);
  const source = new ReadableStream<BufferSource>({
    start(controller) { controller.enqueue(owned); controller.close(); },
  });
  const stream = source.pipeThrough(new CompressionStream("gzip"));
  return new Uint8Array(await new Response(stream).arrayBuffer()).length;
}
const median = (xs: number[]) => xs.slice().sort((a, b) => a - b)[xs.length >> 1];

// ---------------------------------------------------------------- format v2
//
// One file, four sections, no permutation and no dictionary — both were
// measured and both cost more than they save (see the report).
//
//   1 names   front-coded against the previous name, restarting every 16 so a
//             single declaration can still be decoded without reading the file
//             from the start. Original case, sorted exactly as the JSON is.
//   2 restarts u32 per block, byte offset into section 1
//   3 kind    one nibble per declaration
//   4 module  u16 per declaration, subscript into the module array both this
//             file and `modules.json` index into

function encodeV2(decls: Decl[]) {
  const out: number[] = [];
  const restarts: number[] = [];
  let prev = new Uint8Array(0);
  for (let i = 0; i < decls.length; i++) {
    const bytes = encoder.encode(decls[i][0]);
    if (i % RESTART === 0) { restarts.push(out.length); prev = new Uint8Array(0); }
    let shared = 0;
    while (shared < prev.length && shared < bytes.length && prev[shared] === bytes[shared] && shared < 255) shared++;
    // A suffix of 255 bytes or more cannot be spelled here; the encoder must
    // refuse rather than truncate. Lean names in the measured corpus reach 129.
    if (bytes.length - shared > 254) throw new Error(`suffix too long: ${decls[i][0]}`);
    out.push(shared, bytes.length - shared);
    for (let k = shared; k < bytes.length; k++) out.push(bytes[k]);
    prev = bytes;
  }
  const kind = new Uint8Array(Math.ceil(decls.length / 2));
  decls.forEach((d, i) => { kind[i >> 1] |= (d[1] & 15) << ((i & 1) ? 4 : 0); });
  return {
    blob: Uint8Array.from(out),
    restarts: Uint32Array.from(restarts),
    kind,
    mod: new Uint16Array(decls.map((d) => d[2])),
  };
}

// ASCII-only case folding. `toLowerCase()` and this agree on the measured
// corpus (0 of 4,584 names differ); an encoder must check that per package and
// carry the exceptions rather than assume it.
const FOLD = new Uint8Array(256);
for (let i = 0; i < 256; i++) FOLD[i] = i >= 65 && i <= 90 ? i + 32 : i;

const utf16Len = (a: Uint8Array, from: number, to: number) => {
  let n = 0;
  for (let i = from; i < to; i++) if ((a[i] & 0xc0) !== 0x80) n++;
  return n;
};

/** One allocation-free pass: decode, fold and score in the same walk. */
function makeSearcher(idx: ReturnType<typeof encodeV2>, n: number) {
  const orig = new Uint8Array(512), fold = new Uint8Array(512);
  const id = new Int32Array(n), score = new Int32Array(n), len = new Int32Array(n);
  const blob = idx.blob;
  return function search(query: string, cap: number) {
    const q = encoder.encode(query), qn = q.length;
    let at = 0, hits = 0, lastDot = -1;
    for (let i = 0; i < n; i++) {
      const shared = blob[at++], suffix = blob[at++];
      for (let k = 0; k < suffix; k++) {
        const b = blob[at + k];
        orig[shared + k] = b;
        fold[shared + k] = FOLD[b];
      }
      at += suffix;
      const end = shared + suffix;
      let dot = -1;
      for (let k = end - 1; k >= shared; k--) if (fold[k] === DOT) { dot = k; break; }
      if (dot < 0) {
        if (lastDot < shared) dot = lastDot;
        else for (let k = shared - 1; k >= 0; k--) if (fold[k] === DOT) { dot = k; break; }
      }
      lastDot = dot;
      const lastStart = dot + 1;
      let s = 0;
      if (end - lastStart >= qn) {
        let ok = true;
        for (let k = 0; k < qn; k++) if (fold[lastStart + k] !== q[k]) { ok = false; break; }
        if (ok) s = 3000 - utf16Len(fold, lastStart, end);
      }
      if (s === 0 && end >= qn) {
        let ok = true;
        for (let k = 0; k < qn; k++) if (fold[k] !== q[k]) { ok = false; break; }
        if (ok) s = 2000 - utf16Len(fold, 0, end);
      }
      if (s === 0 && end >= qn) {
        for (let start = 0; start <= end - qn; start++) {
          let ok = true;
          for (let k = 0; k < qn; k++) if (fold[start + k] !== q[k]) { ok = false; break; }
          if (ok) { s = 1000 - utf16Len(fold, 0, start); break; }
        }
      }
      if (s > 0) { id[hits] = i; score[hits] = s; len[hits] = utf16Len(fold, 0, end); hits++; }
    }
    const order = Array.from({ length: hits }, (_, k) => k)
      .sort((a, b) => score[b] - score[a] || len[a] - len[b] || id[a] - id[b]);
    return order.slice(0, cap).map((k) => ({ id: id[k], score: score[k] }));
  };
}

/** Random access, for the handful of rows a result list actually shows. */
function nameAt(idx: ReturnType<typeof encodeV2>, want: number) {
  const blob = idx.blob;
  let at = idx.restarts[(want / RESTART) | 0];
  const buf = new Uint8Array(512);
  let end = 0;
  for (let i = ((want / RESTART) | 0) * RESTART; i <= want; i++) {
    const shared = blob[at++], suffix = blob[at++];
    buf.set(blob.subarray(at, at + suffix), shared);
    at += suffix;
    end = shared + suffix;
  }
  return decoder.decode(buf.subarray(0, end));
}

// ------------------------------------------------------- the shipped scorer
// Copied verbatim from `crates/litedoc4-render/assets/app.js` — it is the
// oracle, so it must not be rewritten here.
function shippedScore(name: string, query: string) {
  const lower = name.toLowerCase();
  const last = lower.slice(lower.lastIndexOf(".") + 1);
  if (last.startsWith(query)) return 3000 - last.length;
  if (lower.startsWith(query)) return 2000 - lower.length;
  const at = lower.indexOf(query);
  if (at >= 0) return 1000 - at;
  return -1;
}
function shippedSearch(decls: Decl[], query: string) {
  const hits: [number, Decl][] = [];
  for (const d of decls) { const s = shippedScore(d[0], query); if (s > 0) hits.push([s, d]); }
  hits.sort((a, b) => b[0] - a[0] || a[1][0].length - b[1][0].length);
  return hits.map(([s, d]) => ({ name: d[0], score: s }));
}

// ------------------------------------------------------------------- memory
if (memMode) {
  const snap = () => { for (let i = 0; i < 5; i++) (globalThis as any).gc(); const m = Deno.memoryUsage(); return m; };
  const before = snap();
  let keep: unknown;
  if (memMode === "json") {
    let raw: string | null = Deno.readTextFileSync(path);
    keep = JSON.parse(raw!);
    raw = null;
  } else if (memMode === "json+lower") {
    let raw: string | null = Deno.readTextFileSync(path);
    const parsed = JSON.parse(raw!) as Index;
    raw = null;
    const lower = parsed.decls.map((d) => d[0].toLowerCase());
    keep = { parsed, lower, last: lower.map((s) => s.slice(s.lastIndexOf(".") + 1)) };
  } else if (memMode === "json+source") {
    // The peak a page actually reaches: `response.json()` has to materialise
    // the body as a JS string before it can parse it, and V8 stores that string
    // two-byte because Lean names are not all ASCII.
    const raw = Deno.readTextFileSync(path);
    keep = { raw, parsed: JSON.parse(raw) };
  } else if (memMode === "bin") {
    // Reads an already-encoded file, because that is all a browser ever does.
    // Encoding in this process would leave the parsed JSON on the heap and
    // measure the encoder rather than the page.
    keep = Deno.readFileSync(path);
  } else throw new Error(`unknown --mem mode: ${memMode}`);
  const after = snap();
  console.log(`${memMode.padEnd(11)} heap +${((after.heapUsed - before.heapUsed) / 1024).toFixed(0)} KiB   external +${((after.external - before.external) / 1024).toFixed(0)} KiB   kept=${keep ? "yes" : "no"}`);
  Deno.exit(0);
}

// --------------------------------------------------------------------- main
const rawText = Deno.readTextFileSync(path);
const data = JSON.parse(rawText) as Index;
if (emitTo) {
  const built = encodeV2(data.decls);
  const parts = [built.blob, new Uint8Array(built.restarts.buffer), built.kind, new Uint8Array(built.mod.buffer)];
  const file = new Uint8Array(parts.reduce((a, b) => a + b.length, 0));
  let cursor = 0;
  for (const part of parts) { file.set(part, cursor); cursor += part.length; }
  Deno.writeFileSync(emitTo, file);
  console.log(`${emitTo}: ${file.length} B`);
  Deno.exit(0);
}
const decls = data.decls;
const n = decls.length;
const names = decls.map((d) => d[0]);
const fileBytes = Deno.statSync(path).size;

console.log(`# ${path}  ${fileBytes} B on disk`);
console.log(`\n## 1. what is in the shipped file`);
console.log(`declarations ${n}   modules ${data.modules.length}   kinds ${JSON.stringify(data.kinds)}`);
for (const key of Object.keys(data)) {
  const b = encoder.encode(JSON.stringify((data as any)[key])).length;
  console.log(`  ${key.padEnd(13)} ${String(b).padStart(7)} B  ${((100 * b) / fileBytes).toFixed(1)}%`);
}
const nameBytes = names.reduce((a, s) => a + encoder.encode(s).length, 0);
console.log(`name bytes ${nameBytes}  avg ${(nameBytes / n).toFixed(1)}  max ${Math.max(...names.map((s) => encoder.encode(s).length))}`);
console.log(`sorted as shipped: ${names.every((s, i) => i === 0 || names[i - 1] <= s)}`);
const asciiFold = (s: string) => s.replace(/[A-Z]/g, (c) => c.toLowerCase());
console.log(`names where toLowerCase() != ASCII fold: ${names.filter((s) => s.toLowerCase() !== asciiFold(s)).length}`);

console.log(`\n## 2. the proposed format`);
const idx = encodeV2(decls);
const sections: [string, Uint8Array][] = [
  ["1 names (front-coded)", idx.blob],
  ["2 restarts u32", new Uint8Array(idx.restarts.buffer)],
  ["3 kind nibbles", idx.kind],
  ["4 module u16", new Uint8Array(idx.mod.buffer)],
];
let total = 0;
for (const [label, bytes] of sections) {
  total += bytes.length;
  console.log(`  ${label.padEnd(22)} raw ${String(bytes.length).padStart(7)}  gzip ${String(await gzipLen(bytes)).padStart(6)}`);
}
const one = new Uint8Array(total);
let off = 0;
for (const [, b] of sections) { one.set(b, off); off += b.length; }
console.log(`  ${"total".padEnd(22)} raw ${String(total).padStart(7)}  gzip ${String(await gzipLen(one)).padStart(6)}`);
console.log(`  shipped file           raw ${String(fileBytes).padStart(7)}  gzip ${String(await gzipLen(encoder.encode(rawText))).padStart(6)}`);
const searchOnly = encoder.encode(JSON.stringify({ decls: data.decls, kinds: data.kinds }));
console.log(`  shipped, search part   raw ${String(searchOnly.length).padStart(7)}  gzip ${String(await gzipLen(searchOnly)).padStart(6)}`);

console.log(`\n## 3. rejected alternatives, measured`);
{
  const comps = names.map((s) => s.split("."));
  const vocab = [...new Set(comps.flat())].sort();
  const fc = (strs: string[]) => {
    const out: number[] = [];
    let prev = "";
    for (let i = 0; i < strs.length; i++) {
      if (i % RESTART === 0) prev = "";
      let p = 0;
      while (p < prev.length && p < strs[i].length && prev[p] === strs[i][p] && p < 255) p++;
      const suf = encoder.encode(strs[i].slice(p));
      out.push(p, suf.length, ...suf);
      prev = strs[i];
    }
    return Uint8Array.from(out);
  };
  const dict = fc(vocab);
  const ids = new Map(vocab.map((s, i) => [s, i]));
  const seq: number[] = [];
  for (const c of comps) {
    const push = (v: number) => { while (v >= 128) { seq.push((v & 127) | 128); v >>>= 7; } seq.push(v); };
    push(c.length);
    for (const p of c) push(ids.get(p)!);
  }
  const seqBytes = Uint8Array.from(seq);
  console.log(`  component dictionary  ${vocab.length} components: dict gzip ${await gzipLen(dict)} + ids gzip ${await gzipLen(seqBytes)} = ${await gzipLen(dict) + await gzipLen(seqBytes)}`);
  console.log(`  vs front-coded names                                            gzip ${await gzipLen(idx.blob)}`);
  const lower = names.map((s) => s.toLowerCase());
  const perm = (key: (i: number) => string) =>
    new Uint8Array(Uint16Array.from([...names.keys()].sort((a, b) => (key(a) < key(b) ? -1 : key(a) > key(b) ? 1 : a - b))).buffer);
  const byName = perm((i) => lower[i]);
  const byLast = perm((i) => lower[i].slice(lower[i].lastIndexOf(".") + 1));
  console.log(`  sorted permutations   byName gzip ${await gzipLen(byName)} + byLast gzip ${await gzipLen(byLast)} added to the file`);
  const buckets = new Map<string, number>();
  for (const s of lower) { const k = s.slice(s.lastIndexOf(".") + 1).slice(0, 2); buckets.set(k, (buckets.get(k) ?? 0) + 1); }
  const sizes = [...buckets.values()].sort((a, b) => b - a);
  console.log(`  2-char buckets of the last component: ${buckets.size} buckets, max ${sizes[0]}, median ${sizes[sizes.length >> 1]}`);
}

console.log(`\n## 4. equivalence with the shipped scorer`);
const search = makeSearcher(idx, n);
const QUERIES = ["en", "ent", "entropy", "add", "comm", "information", "nat.", "le_", "theory.f", "fano",
  "binary", "mu", "measure", "xy", "zz", "β", "₁", "_le", "sum", "of_"];
let mismatches = 0;
for (const q of QUERIES) {
  const a = shippedSearch(decls, q);
  const b = search(q, 1e9).map((h) => ({ name: nameAt(idx, h.id), score: h.score }));
  const ka = a.map((x) => `${x.score}:${x.name}`).sort();
  const kb = b.map((x) => `${x.score}:${x.name}`).sort();
  const sameSet = ka.length === kb.length && ka.every((x, i) => x === kb[i]);
  const sameTop = a.slice(0, 30).map((x) => x.name).join("|") === b.slice(0, 30).map((x) => x.name).join("|");
  if (!sameSet || !sameTop) {
    mismatches++;
    console.log(`  MISMATCH q=${JSON.stringify(q)} set=${sameSet} top30=${sameTop} v1=${a.length} v2=${b.length}`);
  }
}
console.log(`  ${QUERIES.length - mismatches}/${QUERIES.length} queries: identical hit set and identical first 30`);

console.log(`\n## 5. work per keystroke (wall clock is indicative only)`);
{
  const t = [];
  for (let i = 0; i < 15; i++) { const s = performance.now(); JSON.parse(rawText); t.push(performance.now() - s); }
  console.log(`  JSON.parse of the shipped file: median ${median(t).toFixed(2)} ms   (the proposed file has no parse step)`);
  const bench = (label: string, fn: (q: string) => unknown) => {
    fn("en");
    const per = QUERIES.slice(0, 10).map((q) => {
      const ts = [];
      for (let i = 0; i < 25; i++) { const s = performance.now(); fn(q); ts.push(performance.now() - s); }
      return median(ts);
    });
    console.log(`  ${label.padEnd(26)} median over 10 queries ${median(per).toFixed(2)} ms`);
  };
  bench("shipped (JSON, strings)", (q) => shippedSearch(decls, q));
  bench("proposed (bytes)", (q) => search(q, 30));
  console.log(`  strings allocated per keystroke: shipped 2 x ${n} = ${2 * n};  proposed 0 in the scan, O(hits) in the sort`);
}

console.log(`\n## 6. narrowing while typing`);
{
  const lower = names.map((s) => s.toLowerCase());
  let full = 0, narrow = 0;
  for (const word of ["entropy", "add_comm", "fano", "measure", "information"]) {
    const steps: string[] = [];
    let prev: number[] | null = null;
    for (let k = 2; k <= word.length; k++) {
      const q = word.slice(0, k);
      const pool: number[] = prev ?? [...lower.keys()];
      const hit: number[] = pool.filter((i: number) => lower[i].includes(q));
      steps.push(`${q}:${pool.length}->${hit.length}`);
      full += n;
      narrow += pool.length;
      prev = hit;
    }
    console.log(`  ${word.padEnd(12)} ${steps.join("  ")}`);
  }
  console.log(`  candidates examined: rescan ${full}, narrowing ${narrow} (${((100 * narrow) / full).toFixed(1)}%)`);
}
