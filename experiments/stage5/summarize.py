#!/usr/bin/env python3
"""Summarize a stage-5 JSONL series written by `time-step.sh`.

Deliberately generic: `time-step.sh` times *any* command, so this reads the
outer numbers (`/usr/bin/time -l` + a monotonic wall clock, both put in the
record by `experiments/stage4c/merge-timing.py`) and whatever scalar keys the
measured program wrote into its own `--timings` JSON.

`CLAUDE.md` asks for major page faults as the primary warm/cold signal rather
than (user+sys)/wall, because a multi-threaded runtime pushes that ratio above 1
for reasons that have nothing to do with the page cache. Both are printed; the
fault column is the one to read.

usage:
  summarize.py FILE.jsonl [--keys k1,k2,...] [--title T]
"""
import argparse
import json
import statistics
import sys


def dig(d, key):
    """`a.b.c` walks nested objects; a plain key is looked up directly."""
    cur = d
    for part in key.split("."):
        if not isinstance(cur, dict):
            return None
        cur = cur.get(part)
    return cur


def med(xs):
    return statistics.median(xs), min(xs), max(xs)


def fmt(xs, places=4):
    m, lo, hi = med(xs)
    return f"{m:.{places}f} [{lo:.{places}f}-{hi:.{places}f}]"


def fmt_int(xs):
    m, lo, hi = med(xs)
    return f"{m:.0f} [{lo:.0f}-{hi:.0f}]"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("file")
    ap.add_argument("--keys", default="")
    ap.add_argument("--title", default="")
    ap.add_argument("--keep-first", action="store_true",
                    help="do not drop run 1 (single-run / cold series)")
    args = ap.parse_args()

    recs = [json.loads(l) for l in open(args.file, encoding="utf-8") if l.strip()]
    if not recs:
        sys.exit(f"no records in {args.file}")
    keys = [k for k in args.keys.split(",") if k]

    out = []
    if args.title:
        out.append(f"### {args.title}")
        out.append("")
    out.append("| run | wall | user+sys | (u+s)/wall | major faults | reclaims | peak RSS MB | "
               + " | ".join(keys) + " |")
    out.append("|---|---:|---:|---:|---:|---:|---:|" + "---:|" * len(keys))
    for r in recs:
        ip = r.get("inProcess", {})
        vals = []
        for k in keys:
            v = dig(ip, k)
            vals.append(f"{v:.4f}" if isinstance(v, float) else ("" if v is None else str(v)))
        out.append(
            f"| {r['run']} | {r['wallSeconds']:.4f} | {r['cpuSeconds']:.4f} | "
            f"{r['cpuOverWall']:.3f} | {r.get('pageFaults', 0)} | {r.get('pageReclaims', 0)} | "
            f"{r.get('peakRssBytes', 0)/1e6:.1f} | " + " | ".join(vals) + " |"
        )
    out.append("")

    keep = [r for r in recs if r["run"] != 1] if not args.keep_first else recs
    if not keep:
        keep = recs
        label = "single run, nothing dropped"
    else:
        label = "run 1 dropped" if not args.keep_first else "all runs"
    out.append(f"median [min-max] over runs {min(r['run'] for r in keep)}.."
               f"{max(r['run'] for r in keep)} ({label}):")
    out.append("")
    out.append("| | |")
    out.append("|---|---:|")
    out.append(f"| wall clock | **{fmt([r['wallSeconds'] for r in keep])}** |")
    out.append(f"| user+sys | {fmt([r['cpuSeconds'] for r in keep])} |")
    out.append(f"| major page faults | {fmt_int([r.get('pageFaults', 0) for r in keep])} |")
    out.append(f"| page reclaims | {fmt_int([r.get('pageReclaims', 0) for r in keep])} |")
    out.append(f"| peak RSS MB | {fmt([r.get('peakRssBytes', 0)/1e6 for r in keep], 1)} |")
    for k in keys:
        vals = [dig(r.get("inProcess", {}), k) for r in keep]
        if all(isinstance(v, (int, float)) for v in vals):
            out.append(f"| {k} | {fmt([float(v) for v in vals])} |")
    print("\n".join(out))


if __name__ == "__main__":
    main()
