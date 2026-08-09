#!/usr/bin/env python3
"""Aggregate the event log written by measure-prefetch.sh into the tables that
benchmarks/results/ci-prefetch-summary.txt quotes.

Reports median [min-max] per configuration rather than a mean: with 5 repetitions a
single scheduling hiccup moves a mean more than it moves the claim being tested, and
the acceptance criterion for play 2 is stated on distributions ("the five runs must not
overlap the baseline"), which needs min and max anyway.

usage:
  summarize-prefetch.py <events.jsonl> [--variants|--prefetch|--vm]
"""
import json
import sys
from collections import defaultdict


def load(path):
    out = []
    with open(path) as f:
        for line in f:
            line = line.strip()
            if line:
                out.append(json.loads(line))
    return out


def stat(values):
    vs = sorted(v for v in values if v is not None)
    if not vs:
        return None
    n = len(vs)
    med = vs[n // 2] if n % 2 else (vs[n // 2 - 1] + vs[n // 2]) / 2
    return med, vs[0], vs[-1], n


def fmt(s, prec=2):
    if s is None:
        return "-"
    med, lo, hi, _ = s
    return f"{med:.{prec}f} [{lo:.{prec}f}-{hi:.{prec}f}]"


def variants(records):
    groups = defaultdict(list)
    for r in records:
        if r["mode"] == "prefetch":
            continue
        groups[(r["mode"], r["tag"], r.get("jobs"), r.get("pmode"))].append(r)
    print(f"{'variant':<24}{'n':>3} {'wall s':>22} {'importModules s':>24} "
          f"{'extract real s':>22} {'major faults':>26}")
    for key in groups:
        rs = groups[key]
        mode, tag, jobs, pmode = key
        label = tag if jobs is None else f"{tag} j={jobs} {pmode}"
        agree = all(r["extract"]["agree"] for r in rs)
        decls = {r["extract"]["decls"] for r in rs}
        print(f"{label:<24}{len(rs):>3} "
              f"{fmt(stat([r['wall_s'] for r in rs])):>22} "
              f"{fmt(stat([r['extract']['import_s'] for r in rs]), 3):>24} "
              f"{fmt(stat([r['extract']['real_s'] for r in rs])):>22} "
              f"{fmt(stat([float(r['extract']['major_faults']) for r in rs]), 0):>26}"
              f"  {'agree' if agree else 'DISAGREE'} {decls}")


def prefetch(records):
    groups = defaultdict(list)
    for r in records:
        if r["mode"] != "prefetch":
            continue
        groups[(r["tag"], r["jobs"], r["pmode"])].append(r)
    print(f"{'set / jobs / mode':<24}{'n':>3} {'prefetch s':>20} {'MB/s':>20} "
          f"{'needed-set resident %':>26} {'resident bytes':>24}")
    for (tag, jobs, pmode), rs in groups.items():
        label = f"{tag} j={jobs} {pmode}"
        pct = [100.0 * r["need"]["resident_file_bytes"] / r["need"]["bytes"] for r in rs]
        print(f"{label:<24}{len(rs):>3} "
              f"{fmt(stat([r['prefetch']['elapsed_s'] for r in rs]), 3):>20} "
              f"{fmt(stat([r['prefetch']['mb_per_s'] for r in rs]), 0):>20} "
              f"{fmt(stat(pct), 1):>26} "
              f"{fmt(stat([float(r['need']['resident_file_bytes']) for r in rs]), 0):>24}")


def vm(records):
    print(f"{'variant':<24}{'headroom GiB before':>22}{'compressor GiB before':>24}")
    groups = defaultdict(list)
    for r in records:
        groups[(r["mode"], r["tag"], r.get("jobs"), r.get("pmode"))].append(r)
    for key, rs in groups.items():
        mode, tag, jobs, pmode = key
        label = tag if jobs is None else f"{tag} j={jobs} {pmode}"
        hr = [r["vm_before"]["headroom_bytes"] / 2 ** 30 for r in rs]
        cp = [r["vm_before"]["compressor_bytes"] / 2 ** 30 for r in rs]
        print(f"{label:<24}{fmt(stat(hr)):>22}{fmt(stat(cp)):>24}")


if __name__ == "__main__":
    path = sys.argv[1]
    what = sys.argv[2] if len(sys.argv) > 2 else "--all"
    recs = load(path)
    if what in ("--all", "--prefetch"):
        print("== prefetch only ==")
        prefetch(recs)
        print()
    if what in ("--all", "--variants"):
        print("== extract variants ==")
        variants(recs)
        print()
    if what in ("--all", "--vm"):
        print("== memory state before each run ==")
        vm(recs)
