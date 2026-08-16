#!/usr/bin/env python3
# Moved here from `experiments/stage4c/` on 2026-08-16, when `experiments/` was
# removed (tag `experiments-frozen`). The contents are unchanged.
"""Run one measured process, and merge its `/usr/bin/time -l` block with the
`render.ts --timings` JSON.

Three jobs, all deliberately dumb:

  --exec -- CMD...     run CMD under `/usr/bin/time -l`, timed from here with a
                       monotonic clock, and emit one JSONL record
  --time-l/--timings   emit a record from files a previous run left behind
  --summarize FILE     read those records back and print the per-run table and
                       the medians

Why this wraps `/usr/bin/time -l` rather than just parsing it: macOS prints
`real`/`user`/`sys` to **two decimal places**, i.e. 10 ms granularity. That is
0.5% of a two-second run but 50% of a 20 ms one, and the Deno start-up floor is
a 20 ms measurement. So the wall clock is taken here with `time.monotonic()`
(microseconds) and `/usr/bin/time -l` is still run, for the CPU split, the peak
RSS and the page faults that `CLAUDE.md` asks to keep. Both numbers go into the
record; `wallSeconds` is the precise one and `wallSecondsTimeL` is the coarse
one they are checked against.

Keeping the outside timer (the process) and the inside timers
(`performance.now()`, the phases) in the SAME record is what lets them check
each other: phases that sum to more than the wall clock, or a wall clock far
above user+sys, show up on their own line -- `CLAUDE.md`: wall ~ user+sys means
warm.

usage:
  merge-timing.py --name N --run I --time-l FILE --timings FILE --exec -- CMD...
  merge-timing.py --name N --run I --time-l FILE --timings FILE
  merge-timing.py --summarize FILE.jsonl
"""
import argparse
import json
import re
import statistics
import subprocess
import sys
import time

# `/usr/bin/time -l` on macOS: "  1.23 real   0.98 user   0.20 sys" then a block
# of "<number>  <label>" lines.
RE_TOP = re.compile(r"([\d.]+)\s+real\s+([\d.]+)\s+user\s+([\d.]+)\s+sys")
RE_ROW = re.compile(r"^\s*(\d+)\s+(.+?)\s*$")

WANTED = {
    "maximum resident set size": "peakRssBytes",
    "page reclaims": "pageReclaims",
    "page faults": "pageFaults",
    "involuntary context switches": "involuntaryCtxSwitches",
    "voluntary context switches": "voluntaryCtxSwitches",
    "instructions retired": "instructionsRetired",
}


def parse_time_l(path):
    out = {}
    with open(path, encoding="utf-8", errors="replace") as fh:
        for line in fh:
            m = RE_TOP.search(line)
            if m:
                out["wallSeconds"] = float(m.group(1))
                out["userSeconds"] = float(m.group(2))
                out["sysSeconds"] = float(m.group(3))
                continue
            m = RE_ROW.match(line)
            if m and m.group(2) in WANTED:
                out[WANTED[m.group(2)]] = int(m.group(1))
    if "wallSeconds" not in out:
        sys.exit(f"no 'real' line in {path} -- did the run fail?")
    return out


def run(cmd, time_l_path):
    """`/usr/bin/time -l CMD`, timed from out here. stdout is dropped; the
    process writes what matters to its --timings / --stats files."""
    with open(time_l_path, "wb") as err:
        t0 = time.monotonic()
        rc = subprocess.call(
            ["/usr/bin/time", "-l"] + cmd,
            stdout=subprocess.DEVNULL,
            stderr=err,
        )
        t1 = time.monotonic()
    if rc != 0:
        sys.stderr.write(open(time_l_path, encoding="utf-8", errors="replace").read())
        sys.exit(f"command failed with {rc}: {' '.join(cmd)}")
    return t1 - t0


def emit(args):
    rec = {"name": args.name, "run": args.run}
    wall = run(args.exec_cmd, args.time_l) if args.exec_cmd else None
    rec.update(parse_time_l(args.time_l))
    if wall is not None:
        # The precise wall clock replaces the 10 ms-granular one, which is kept
        # beside it so the two can be compared.
        rec["wallSecondsTimeL"] = rec["wallSeconds"]
        rec["wallSeconds"] = round(wall, 6)
    with open(args.timings, encoding="utf-8") as fh:
        rec["inProcess"] = json.load(fh)
    cpu = rec["userSeconds"] + rec["sysSeconds"]
    rec["cpuSeconds"] = round(cpu, 4)
    # `CLAUDE.md`: wall ~ user+sys is the warm signature. Recorded per run, not
    # asserted -- a cold run is a legitimate measurement, it just is not warm.
    rec["cpuOverWall"] = round(cpu / rec["wallSeconds"], 3) if rec["wallSeconds"] else None
    print(json.dumps(rec))


def median_spread(xs):
    return statistics.median(xs), min(xs), max(xs)


def fmt(xs, places=4):
    med, lo, hi = median_spread(xs)
    return f"{med:.{places}f} [{lo:.{places}f}-{hi:.{places}f}]"


def summarize(path):
    recs = [json.loads(line) for line in open(path, encoding="utf-8") if line.strip()]
    if not recs:
        sys.exit(f"no records in {path}")
    empty = "modulesRead" not in recs[0]["inProcess"]
    lines = []
    lines.append("## per run (実測)")
    lines.append("")
    if empty:
        lines.append("| run | wall | boot (in-process) | user | sys | user+sys | (user+sys)/wall | peak RSS MB | page faults |")
        lines.append("|---|---:|---:|---:|---:|---:|---:|---:|---:|")
        for r in recs:
            lines.append(
                f"| {r['run']} | {r['wallSeconds']:.4f} | "
                f"{r['inProcess'].get('bootSeconds', float('nan')):.4f} | "
                f"{r['userSeconds']:.4f} | "
                f"{r['sysSeconds']:.4f} | {r['cpuSeconds']:.4f} | {r['cpuOverWall']:.3f} | "
                f"{r.get('peakRssBytes', 0)/1e6:.1f} | {r.get('pageFaults', 0)} |"
            )
    else:
        lines.append("| run | wall | user+sys | (user+sys)/wall | preMain | read IR | index | render hdr | render page | flatten | write | docstr* | in-proc total | peak RSS MB | page faults |")
        lines.append("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|")
        for r in recs:
            s = r["inProcess"]["seconds"]
            lines.append(
                f"| {r['run']} | {r['wallSeconds']:.4f} | {r['cpuSeconds']:.4f} | "
                f"{r['cpuOverWall']:.3f} | {s['preMain']:.4f} | {s['readIr']:.4f} | "
                f"{s['indexBuild']:.4f} | {s['renderHeaders']:.4f} | {s['renderPage']:.4f} | "
                f"{s['flatten']:.4f} | {s['write']:.4f} | {s.get('docstring', float('nan')):.4f} | {s['total']:.4f} | "
                f"{r.get('peakRssBytes', 0)/1e6:.1f} | {r.get('pageFaults', 0)} |"
            )
    lines.append("")

    # Run 1 is dropped, exactly as stage 4 increment 2 did.
    keep = [r for r in recs if r["run"] != 1]
    dropped = "run 1 dropped"
    if not keep:  # a single-run series (the cold side): there is nothing to drop
        keep, dropped = recs, "single run, nothing dropped"
    lines.append(
        f"## median [min-max] over runs {min(r['run'] for r in keep)}.."
        f"{max(r['run'] for r in keep)} ({dropped})"
    )
    lines.append("")
    lines.append("| | seconds |")
    lines.append("|---|---:|")
    lines.append(f"| wall clock | **{fmt([r['wallSeconds'] for r in keep])}** |")
    lines.append(f"| user+sys | {fmt([r['cpuSeconds'] for r in keep])} |")
    if empty:
        boots = [r["inProcess"].get("bootSeconds") for r in keep]
        if all(b is not None for b in boots):
            lines.append(f"| boot (in-process: start -> first line) | {fmt(boots)} |")
            wall = [r["wallSeconds"] for r in keep]
            lines.append(f"| exec + teardown (wall - boot) | {fmt([w - b for w, b in zip(wall, boots)])} |")
    if not empty:
        for key, label in [
            ("preMain", "preMain (module init, not deno start-up)"),
            ("readIr", "read IR"),
            ("indexBuild", "index build"),
            ("renderHeaders", "render decl_header"),
            ("renderPage", "render page"),
            ("flatten", "flatten probe"),
            ("write", "write pages"),
            ("docstring", "  of \"render page\": docstrings"),
            ("total", "in-process total"),
        ]:
            lines.append(f"| {label} | {fmt([r['inProcess']['seconds'][key] for r in keep])} |")
        acc = [r["inProcess"]["secondsAccounted"] for r in keep]
        lines.append(f"| sum of phases | {fmt(acc)} |")
        tot = [r["inProcess"]["seconds"]["total"] for r in keep]
        lines.append(f"| unaccounted (total - sum) | {fmt([t - a for t, a in zip(tot, acc)])} |")
        wall = [r["wallSeconds"] for r in keep]
        lines.append(f"| process teardown (wall - in-process total) | {fmt([w - t for w, t in zip(wall, tot)])} |")
        p = recs[-1]["inProcess"]
        lines.append("")
        lines.append("| | |")
        lines.append("|---|---:|")
        lines.append(f"| modules read | {p['modulesRead']:,} |")
        lines.append(f"| pages written | {p['pagesWritten']:,} |")
        lines.append(f"| declarations rendered | {p['declarationsRendered']:,} |")
        lines.append(f"| page UTF-16 code units | {p['pageCodeUnits']:,} |")
        lines.append(f"| IR bytes read | {p['irBytes']:,} |")
        lines.append(f"| flatten probe | {'on' if p['flattenProbe'] else 'OFF'} |")
    if not empty:
        lines.append("")
        lines.append("`docstr*` is a **slice of `render page`**, not a phase: it is not in the sum.")
    lines.append("")
    rss = [r.get("peakRssBytes", 0) for r in keep]
    lines.append(f"peak RSS (median [min-max]): {statistics.median(rss)/1e6:.1f} [{min(rss)/1e6:.1f}-{max(rss)/1e6:.1f}] MB")
    ratios = [r["cpuOverWall"] for r in keep]
    lines.append(f"(user+sys)/wall: {statistics.median(ratios):.3f} [{min(ratios):.3f}-{max(ratios):.3f}]  -- >= ~0.95 means warm")
    print("\n".join(lines))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--name")
    ap.add_argument("--run", type=int)
    ap.add_argument("--time-l")
    ap.add_argument("--timings")
    ap.add_argument("--summarize")
    # `--exec -- CMD...` is split off by hand: argparse's REMAINDER does not
    # cooperate with an optional that is preceded by other optionals.
    argv = sys.argv[1:]
    exec_cmd = None
    if "--exec" in argv:
        i = argv.index("--exec")
        exec_cmd = argv[i + 1:]
        if exec_cmd[:1] == ["--"]:
            exec_cmd = exec_cmd[1:]
        argv = argv[:i]
    args = ap.parse_args(argv)
    args.exec_cmd = exec_cmd
    if args.summarize:
        summarize(args.summarize)
    else:
        emit(args)


if __name__ == "__main__":
    main()
