// Aggregates the stage-7d timing rounds: one directory of `<cfg>-r<N>.jsonl`
// (the extractor's phase timers) next to `<cfg>-r<N>.time` (`/usr/bin/time -l`).
//
// Round 1 is dropped as the cold one and the remaining rounds' median is
// reported, which is what stages 7a/7b/7c did. Every run of every configuration
// is printed too, because a median without its spread cannot be argued with.
//
// usage: deno run --allow-read --allow-write rounds-report.ts \
//          --dir <rounds dir> --configs 7c,j1,bd,j2,j4,j8,j16 --rounds 7 \
//          [--drop 1] [--report <out.txt>]

function arg(flag: string, dflt?: string): string {
  const i = Deno.args.indexOf(flag);
  return i >= 0 ? Deno.args[i + 1] : (dflt ?? "");
}

const dir = arg("--dir");
const configs = arg("--configs", "7c,j1,bd,j2,j4,j8,j16").split(",");
const rounds = Number(arg("--rounds", "7"));
const drop = Number(arg("--drop", "1"));
if (!dir) {
  console.error("usage: rounds-report.ts --dir <d> [--configs ..] [--rounds N] [--drop N]");
  Deno.exit(2);
}

type Phase = Record<string, number | string>;
type Run = {
  cfg: string;
  round: number;
  phases: Map<string, Phase>;
  time: Record<string, number>;
};

function parseTime(text: string): Record<string, number> {
  const r: Record<string, number> = {};
  for (const line of text.split("\n")) {
    let m = line.match(/([\d.]+)\s+real\s+([\d.]+)\s+user\s+([\d.]+)\s+sys/);
    if (m) {
      r.real = Number(m[1]);
      r.user = Number(m[2]);
      r.sys = Number(m[3]);
      continue;
    }
    m = line.match(/^\s*(\d+)\s+(.*?)\s*$/);
    if (m) {
      const key = m[2].replace(/\s+/g, "_");
      r[key] = Number(m[1]);
    }
  }
  return r;
}

const runs: Run[] = [];
for (const cfg of configs) {
  for (let round = 1; round <= rounds; round++) {
    const jsonl = `${dir}/${cfg}-r${round}.jsonl`;
    let text: string;
    try {
      text = await Deno.readTextFile(jsonl);
    } catch {
      continue;
    }
    const phases = new Map<string, Phase>();
    for (const line of text.split("\n")) {
      if (line.trim() === "") continue;
      const o = JSON.parse(line) as Phase;
      phases.set(o.phase as string, o);
    }
    let time: Record<string, number> = {};
    try {
      time = parseTime(await Deno.readTextFile(`${dir}/${cfg}-r${round}.time`));
    } catch { /* no /usr/bin/time output */ }
    runs.push({ cfg, round, phases, time });
  }
}

const median = (xs: number[]) => {
  const a = [...xs].sort((p, q) => p - q);
  return a.length % 2 ? a[(a.length - 1) / 2] : (a[a.length / 2 - 1] + a[a.length / 2]) / 2;
};

function pick(r: Run, phase: string, key: string): number {
  const p = r.phases.get(phase);
  if (!p) return NaN;
  return Number(p[key]);
}

const out: string[] = [];
const say = (s = "") => out.push(s);
const S = (us: number) => (us / 1e6).toFixed(4);

say(`# stage 7d timing rounds — ${dir}`);
say();
say(`configurations ${configs.join(" ")}   rounds 1..${rounds}, first ${drop} dropped as cold`);
say();

type Metric = { name: string; get: (r: Run) => number; fmt: (n: number) => string };
const metrics: Metric[] = [
  { name: "total", get: (r) => pick(r, "stage4b.total", "us"), fmt: S },
  { name: "importModules", get: (r) => pick(r, "stage4b.importModules", "us"), fmt: S },
  { name: "analyze", get: (r) => pick(r, "stage4b.analyze", "us"), fmt: S },
  { name: "  ppUs", get: (r) => pick(r, "stage4b.analyze", "ppUs"), fmt: S },
  { name: "  eqUs", get: (r) => pick(r, "stage4b.analyze", "eqUs"), fmt: S },
  { name: "  refUs", get: (r) => pick(r, "stage4b.analyze", "refUs"), fmt: S },
  { name: "  blUs", get: (r) => pick(r, "stage4b.analyze", "blUs"), fmt: S },
  { name: "writeIR", get: (r) => pick(r, "stage4b.writeIR", "us"), fmt: S },
  { name: "real (s)", get: (r) => r.time.real * 1e6, fmt: S },
  { name: "user+sys (s)", get: (r) => (r.time.user + r.time.sys) * 1e6, fmt: S },
  {
    name: "peak RSS (MB)",
    get: (r) => r.time.maximum_resident_set_size,
    fmt: (n) => (n / 1e6).toFixed(1),
  },
  {
    name: "peak footprint (MB)",
    get: (r) => r.time.peak_memory_footprint,
    fmt: (n) => (n / 1e6).toFixed(1),
  },
  {
    name: "instructions (G)",
    get: (r) => r.time.instructions_retired,
    fmt: (n) => (n / 1e9).toFixed(1),
  },
  { name: "cycles (G)", get: (r) => r.time.cycles_elapsed, fmt: (n) => (n / 1e9).toFixed(1) },
];

say(`## warm medians (rounds ${drop + 1}..${rounds})`);
say();
say(`| metric | ${configs.join(" | ")} |`);
say(`|---|${configs.map(() => "---:").join("|")}|`);
for (const m of metrics) {
  const cells = configs.map((cfg) => {
    const xs = runs.filter((r) => r.cfg === cfg && r.round > drop).map(m.get).filter((x) =>
      Number.isFinite(x)
    );
    return xs.length ? m.fmt(median(xs)) : "—";
  });
  say(`| ${m.name} | ${cells.join(" | ")} |`);
}
say();

say(`## cold round (round 1) — do not mix with the table above`);
say();
say(`| metric | ${configs.join(" | ")} |`);
say(`|---|${configs.map(() => "---:").join("|")}|`);
for (const m of metrics) {
  const cells = configs.map((cfg) => {
    const r = runs.find((r) => r.cfg === cfg && r.round === 1);
    if (!r) return "—";
    const v = m.get(r);
    return Number.isFinite(v) ? m.fmt(v) : "—";
  });
  say(`| ${m.name} | ${cells.join(" | ")} |`);
}
say();

say(`## every warm run (ascending), for the spread`);
say();
say("```");
for (const m of ["total", "analyze", "real (s)"]) {
  const metric = metrics.find((x) => x.name === m)!;
  for (const cfg of configs) {
    const xs = runs.filter((r) => r.cfg === cfg && r.round > drop).map(metric.get).filter((x) =>
      Number.isFinite(x)
    ).sort((a, b) => a - b);
    if (!xs.length) continue;
    say(`${m.padEnd(10)} ${cfg.padEnd(4)} ${xs.map(metric.fmt).join(" ")}`);
  }
  say("");
}
say("```");
say();

// pp breakdown, from the configuration that carries it
say(`## pp breakdown (--pp-breakdown runs only), warm median`);
say();
const bdRuns = runs.filter((r) => r.cfg === "bd" && r.round > drop);
if (bdRuns.length) {
  const keys = [
    ["delabUs", "delaboration   Expr -> Syntax"],
    ["sanitizeUs", "sanitize       Syntax -> Syntax"],
    ["parenUs", "parenthesize   Syntax -> Syntax"],
    ["formatUs", "format         Syntax -> Format"],
    ["prettyUs", "pretty         Format -> String"],
    ["refUs", "span collect   Format -> spans (--tagged-code)"],
    ["eqGenUs", "eq generation  getEqnsFor?/valueToEq"],
    ["tagCodeInfosUs", "tagCodeInfos   (--tag only, off here)"],
  ];
  const pp = median(bdRuns.map((r) => pick(r, "stage4b.analyze", "ppUs")));
  const eq = median(bdRuns.map((r) => pick(r, "stage4b.analyze", "eqUs")));
  const an = median(bdRuns.map((r) => pick(r, "stage4b.analyze", "us")));
  say(`analyze ${S(an)} s;  ppUs ${S(pp)} s + eqUs ${S(eq)} s = ${S(pp + eq)} s`);
  say();
  say(`| step | s | % of ppUs+eqUs | % of analyze | of which equations |`);
  say(`|---|---:|---:|---:|---:|`);
  let acc = 0;
  for (const [k, label] of keys) {
    const v = median(bdRuns.map((r) => pick(r, "stage4b.analyze", k)));
    acc += v;
    const eqKey: Record<string, string> = {
      delabUs: "eqDelabUs",
      sanitizeUs: "eqSanitizeUs",
      parenUs: "eqParenUs",
      formatUs: "eqFormatUs",
      prettyUs: "eqPrettyUs",
    };
    const eqv = eqKey[k] ? median(bdRuns.map((r) => pick(r, "stage4b.analyze", eqKey[k]))) : NaN;
    say(
      `| ${label} | ${S(v)} | ${((v / (pp + eq)) * 100).toFixed(1)}% | ${
        ((v / an) * 100).toFixed(1)
      }% | ${Number.isFinite(eqv) ? S(eqv) : "—"} |`,
    );
  }
  say(
    `| **accounted** | **${S(acc)}** | **${((acc / (pp + eq)) * 100).toFixed(1)}%** | ${
      ((acc / an) * 100).toFixed(1)
    }% | |`,
  );
  say(
    `| unaccounted (telescopes, inferType, monad plumbing) | ${S(pp + eq - acc)} | ${
      (((pp + eq - acc) / (pp + eq)) * 100).toFixed(1)
    }% | | |`,
  );
  say();
  const counts = ["ppSigCalls", "ppTermCalls", "ppBinders", "ppBytes", "blCalls"];
  say(
    counts.map((k) => `${k}=${median(bdRuns.map((r) => pick(r, "stage4b.analyze", k)))}`).join("  "),
  );
}

const text = out.join("\n") + "\n";
const reportPath = arg("--report");
if (reportPath) await Deno.writeTextFile(reportPath, text);
console.log(text);
