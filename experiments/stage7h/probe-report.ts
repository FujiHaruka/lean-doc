#!/usr/bin/env -S deno run --allow-read --allow-write
//
// Aggregation for `probe.sh`. Separate from it so that fixing the summary never
// costs a re-measurement.
//
// usage: probe-report.ts <work-dir>   (reads <work>/probe/probe.jsonl)

const W = Deno.args[0];
if (!W) {
  console.error("usage: probe-report.ts <work-dir>");
  Deno.exit(2);
}
interface Rec {
  tag: string;
  wallSeconds: number;
  // deno-lint-ignore no-explicit-any
  inner?: Record<string, any>;
}
const recs: Rec[] = Deno.readTextFileSync(`${W}/probe/probe.jsonl`).trim().split("\n")
  .map((l) => JSON.parse(l));
const med = (xs: number[]) => {
  const s = [...xs].sort((a, b) => a - b);
  const n = s.length;
  return n % 2 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2;
};
const tags = [...new Set(recs.map((r) => r.tag))];
const byTag = new Map(tags.map((t) => [t, recs.filter((r) => r.tag === t)]));
const wall = (t: string) => med(byTag.get(t)!.map((r) => r.wallSeconds));
const inner = (t: string, k: string) => {
  const xs = byTag.get(t)!.filter((r) => r.inner && typeof r.inner[k] === "number")
    .map((r) => r.inner![k] as number);
  return xs.length ? med(xs) : null;
};

const L: string[] = [];
const f = (x: number | null) => x === null ? "       -" : x.toFixed(4).padStart(8);
L.push("## the six invocations, median of " + byTag.get(tags[0])!.length + " reps");
L.push("");
L.push("tag".padEnd(12) + "wall".padStart(8) + "[min-max]".padStart(19) +
  "inner".padStart(8) + "startup".padStart(9));
for (const t of tags) {
  const w = byTag.get(t)!.map((r) => r.wallSeconds);
  const i = inner(t, "totalSeconds");
  L.push(
    t.padEnd(12) + f(med(w)) +
      `[${Math.min(...w).toFixed(4)}-${Math.max(...w).toFixed(4)}]`.padStart(19) +
      f(i) + f(i === null ? null : med(w) - i).padStart(9),
  );
}
L.push("");
L.push("  empty        an empty script: the floor of `deno run`");
L.push("  usage        global.ts with no command: the same floor plus its own module load");
L.push("  readall      read + JSON.parse all 433 module files, nothing else");
L.push("  build        stage5/global.ts build");
L.push("  delta-hit    stage5/global.ts delta, 52 names moved -> the scan runs");
L.push("  delta-miss   stage5/global.ts delta, nothing moved -> the scan is skipped");
L.push("");
L.push("## where the two processes' time goes");
L.push("");
const rows: [string, number | null, string][] = [
  ["deno startup (build)", wall("build") - (inner("build", "totalSeconds") ?? 0),
    "floor for one process; the second one is removable"],
  ["build: read 433 module IRs", inner("build", "readSeconds"), "REMOVABLE (cache)"],
  ["build: derive + write 6 artifacts", inner("build", "writeSeconds"),
    "not removable: each artifact is a function of the whole set"],
  ["deno startup (delta)", wall("delta-hit") - (inner("delta-hit", "totalSeconds") ?? 0),
    "REMOVABLE: one process can do both"],
  ["delta: diff the two name maps", inner("delta-hit", "diffSeconds"), "not removable"],
  ["delta: scan 433 module IRs", inner("delta-hit", "scanSeconds"), "REMOVABLE (cache)"],
];
L.push("piece".padEnd(36) + "seconds".padStart(9) + "  verdict");
for (const [n, v, note] of rows) L.push(n.padEnd(36) + f(v) + "  " + note);
const total = rows.reduce((a, [, v]) => a + (v ?? 0), 0);
const removable = (rows[1][1] ?? 0) + (rows[3][1] ?? 0) + (rows[5][1] ?? 0);
L.push("");
L.push(`sum of the pieces                   ${total.toFixed(4)}`);
L.push(`  of which removable                ${removable.toFixed(4)}`);
L.push(`  floor                             ${(total - removable).toFixed(4)}`);
L.push("");
L.push("The floor is not the whole story: a cache has to be read and written, and");
L.push("that cost is not in this table because it does not exist in the program");
L.push("being measured. The ceiling this gives is therefore an upper bound on the");
L.push("saving, and the A/B in `run.sh` is what says how much of it survives.");
L.push("");
L.push("A caveat that cuts the other way: `delta-miss` shows the scan is already");
L.push(`skipped when no name moved (${wall("delta-miss").toFixed(4)} s vs ${
  wall("delta-hit").toFixed(4)
} s), so on a run`);
L.push("that changes no name the removable part is smaller by that much.");

// --- the scale probe, when `probe-scale.sh` has run --------------------------
try {
  const sc: {
    tag: string;
    misses: number;
    wallSeconds: number;
    readSeconds?: number;
    totalSeconds?: number;
    cacheMisses?: number;
  }[] = Deno.readTextFileSync(`${W}/scale/scale.jsonl`).trim().split("\n")
    .map((l) => JSON.parse(l));
  const tags = [...new Set(sc.map((r) => r.tag))];
  L.push("");
  L.push("## how far the cache scales — N of 433 module entries invalidated");
  L.push("");
  L.push(
    "misses".padStart(8) + "wall".padStart(9) + "inner".padStart(9) +
      "read".padStart(9) + "  (median of " +
      sc.filter((r) => r.tag === tags[0]).length + " reps)",
  );
  const oldWall = med(sc.filter((r) => r.tag === "old").map((r) => r.wallSeconds));
  for (const t of tags) {
    const g = sc.filter((r) => r.tag === t);
    const label = t === "old" ? "stage5" : String(g[0].misses);
    L.push(
      label.padStart(8) + med(g.map((r) => r.wallSeconds)).toFixed(4).padStart(9) +
        (g[0].totalSeconds === undefined
          ? "        -"
          : med(g.map((r) => r.totalSeconds!)).toFixed(4).padStart(9)) +
        (g[0].readSeconds === undefined
          ? "        -"
          : med(g.map((r) => r.readSeconds!)).toFixed(4).padStart(9)) +
        (t === "old" ? "   the control: no cache, always reads all 433" : ""),
    );
  }
  L.push("");
  L.push(
    `even with every entry invalidated the cached build is ${
      (med(sc.filter((r) => r.tag === "new-433").map((r) => r.wallSeconds)) - oldWall >= 0)
        ? "slower"
        : "no slower"
    } than the control,`,
  );
  L.push("which is the case where the cache buys nothing and still has to be written.");
} catch { /* no scale probe in this work directory */ }

let conditions = "";
try {
  conditions = Deno.readTextFileSync(`${W}/probe/conditions.txt`);
} catch { /* an older probe run; the report says so by having no header */ }
const body = "# stage7h probe — the ceiling on L3-3, piece by piece\n\n" +
  (conditions ? conditions + "\n" : "(no conditions.txt: this probe predates the header)\n\n") +
  L.join("\n") + "\n";

const out = `${W}/probe/probe-report.txt`;
Deno.writeTextFileSync(out, body);
const results = new URL("../../benchmarks/results/stage7h-probe.txt", import.meta.url).pathname;
Deno.writeTextFileSync(results, body);
console.log(body);
console.error(`-> ${out}\n-> ${results}`);
