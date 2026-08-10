// Reads the `--decl-profile` JSONL the stage-7d extractor writes and answers the
// three questions §8 leaves open about the 75%: is the per-declaration cost
// uniform, which kinds carry it, and do the blacklisted candidates cost anything.
//
// Diagnosis only. The acceptance oracle is `experiments/stage4c/coverage.ts`;
// nothing here decides whether the output is right.
//
// usage: deno run --allow-read --allow-write decl-profile-report.ts \
//          --profile <decl-profile.jsonl> [--report <out.txt>]

type Row = {
  i: number;
  name: string;
  module: string;
  kind: string;
  outcome: string;
  ns: number;
  ppNs: number;
  eqNs: number;
  refNs: number;
  blNs: number;
  eqs: number;
  bytes: number;
};

function arg(flag: string): string | undefined {
  const i = Deno.args.indexOf(flag);
  return i >= 0 ? Deno.args[i + 1] : undefined;
}

const profilePath = arg("--profile");
if (!profilePath) {
  console.error("usage: decl-profile-report.ts --profile <p> [--report <p>]");
  Deno.exit(2);
}

const rows: Row[] = [];
for (const line of (await Deno.readTextFile(profilePath)).split("\n")) {
  if (line.trim() === "") continue;
  rows.push(JSON.parse(line) as Row);
}

const out: string[] = [];
const say = (s = "") => out.push(s);

const us = (n: number) => (n / 1000).toFixed(0);
const s = (n: number) => (n / 1e9).toFixed(4);
const pct = (a: number, b: number) => b === 0 ? "0.00%" : ((a / b) * 100).toFixed(2) + "%";

const total = rows.reduce((a, r) => a + r.ns, 0);
const totalPp = rows.reduce((a, r) => a + r.ppNs, 0);
const totalEq = rows.reduce((a, r) => a + r.eqNs, 0);
const totalRef = rows.reduce((a, r) => a + r.refNs, 0);
const totalBl = rows.reduce((a, r) => a + r.blNs, 0);

say(`# decl profile — ${profilePath}`);
say();
say(`candidates                ${rows.length}`);
say(`  produced a declaration  ${rows.filter((r) => r.outcome === "ok").length}`);
say(`  blacklisted             ${rows.filter((r) => r.outcome === "blacklisted").length}`);
say(`  failed                  ${rows.filter((r) => r.outcome === "failed").length}`);
say();
say(`sum of per-candidate wall ${s(total)} s   (the analyze phase minus the loop's own overhead)`);
say(`  of which pp             ${s(totalPp)} s  ${pct(totalPp, total)}`);
say(`  of which eq             ${s(totalEq)} s  ${pct(totalEq, total)}`);
say(`  of which refs           ${s(totalRef)} s  ${pct(totalRef, total)}  (inside the two above)`);
say(`  of which blacklist test ${s(totalBl)} s  ${pct(totalBl, total)}`);
say();

// ---- 1. do the blacklisted candidates cost anything?
const bl = rows.filter((r) => r.outcome === "blacklisted");
const blSum = bl.reduce((a, r) => a + r.ns, 0);
const blBlSum = bl.reduce((a, r) => a + r.blNs, 0);
const blPp = bl.reduce((a, r) => a + r.ppNs + r.eqNs, 0);
say(`## 1. the ${bl.length} blacklisted candidates`);
say();
say(`total wall               ${s(blSum)} s  = ${pct(blSum, total)} of the analyzed time`);
say(`  of which the test      ${s(blBlSum)} s`);
say(`  pretty printing        ${s(blPp)} s   (must be 0: they return before any pp)`);
say(`mean per candidate       ${us(blSum / Math.max(1, bl.length))} us`);
const blSorted = [...bl].sort((a, b) => b.ns - a.ns);
say(`slowest 5:`);
for (const r of blSorted.slice(0, 5)) say(`  ${us(r.ns).padStart(8)} us  ${r.name}`);
say();

// ---- 2. distribution over the produced declarations
const ok = rows.filter((r) => r.outcome === "ok");
const okSum = ok.reduce((a, r) => a + r.ns, 0);
const sorted = [...ok].sort((a, b) => b.ns - a.ns);
say(`## 2. distribution over the ${ok.length} declarations that produced output`);
say();
say(`total ${s(okSum)} s, mean ${us(okSum / ok.length)} us/declaration`);
say();
say(`| slice | declarations | time | share |`);
say(`|---|---:|---:|---:|`);
for (const frac of [0.001, 0.01, 0.05, 0.1, 0.25, 0.5, 1.0]) {
  const n = Math.max(1, Math.round(ok.length * frac));
  const sum = sorted.slice(0, n).reduce((a, r) => a + r.ns, 0);
  say(
    `| top ${(frac * 100).toFixed(1)}% | ${n} | ${s(sum)} s | ${pct(sum, okSum)} |`,
  );
}
say();
const median = sorted[Math.floor(ok.length / 2)].ns;
const p90 = sorted[Math.floor(ok.length * 0.1)].ns;
const p99 = sorted[Math.floor(ok.length * 0.01)].ns;
say(
  `median ${us(median)} us   p90 ${us(p90)} us   p99 ${us(p99)} us   max ${us(sorted[0].ns)} us`,
);
say(`ratio max/median ${(sorted[0].ns / median).toFixed(0)}x, p99/median ${(p99 / median).toFixed(1)}x`);
say();
say(`slowest 15 declarations:`);
say(`| us | kind | eqs | bytes | name |`);
say(`|---:|---|---:|---:|---|`);
for (const r of sorted.slice(0, 15)) {
  say(`| ${us(r.ns)} | ${r.kind} | ${r.eqs} | ${r.bytes} | \`${r.name}\` |`);
}
say();

// ---- 3. by kind
const byKind = new Map<string, Row[]>();
for (const r of ok) {
  const a = byKind.get(r.kind) ?? [];
  a.push(r);
  byKind.set(r.kind, a);
}
say(`## 3. by kind`);
say();
say(`| kind | n | total | share | mean us | median us | max us | bytes/decl |`);
say(`|---|---:|---:|---:|---:|---:|---:|---:|`);
const kinds = [...byKind.entries()].sort((a, b) =>
  b[1].reduce((x, r) => x + r.ns, 0) - a[1].reduce((x, r) => x + r.ns, 0)
);
for (const [kind, rs] of kinds) {
  const sum = rs.reduce((a, r) => a + r.ns, 0);
  const sortedK = [...rs].sort((a, b) => a.ns - b.ns);
  const med = sortedK[Math.floor(rs.length / 2)].ns;
  const mx = sortedK[rs.length - 1].ns;
  const bytes = rs.reduce((a, r) => a + r.bytes, 0) / rs.length;
  say(
    `| ${kind} | ${rs.length} | ${s(sum)} s | ${pct(sum, okSum)} | ${us(sum / rs.length)} | ${
      us(med)
    } | ${us(mx)} | ${bytes.toFixed(0)} |`,
  );
}
say();

// ---- 4. equations
const withEq = ok.filter((r) => r.eqs > 0);
say(`## 4. equations`);
say();
say(`declarations with equations ${withEq.length} (${withEq.reduce((a, r) => a + r.eqs, 0)} lemmas)`);
say(`their eq time               ${s(withEq.reduce((a, r) => a + r.eqNs, 0))} s`);
say(`their total time            ${s(withEq.reduce((a, r) => a + r.ns, 0))} s`);
say();

// ---- 5. what a parallel run has to balance
say(`## 5. what a striding parallel run has to balance`);
say();
for (const jobs of [2, 4, 8, 16]) {
  const buckets = new Array(jobs).fill(0);
  for (const r of rows) buckets[r.i % jobs] += r.ns;
  const mx = Math.max(...buckets);
  const mean = buckets.reduce((a, b) => a + b, 0) / jobs;
  say(
    `jobs=${String(jobs).padStart(2)}  heaviest stride ${s(mx)} s, mean ${s(mean)} s → ` +
      `best possible speedup ${(total / mx).toFixed(2)}x of ${jobs}x`,
  );
}

const text = out.join("\n") + "\n";
const reportPath = arg("--report");
if (reportPath) await Deno.writeTextFile(reportPath, text);
console.log(text);
