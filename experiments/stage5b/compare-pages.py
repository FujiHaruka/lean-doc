#!/usr/bin/env python3
"""Byte-compare two page trees, and score an impact set against the difference.

Three modes, one per experiment:

  --mode count      E1: how many pages differ, and how many times a given
                    substring occurs in each (the `--source-url` revision).
  --mode impact     E2: the true changed-page set, against the sets
                    `impact.ts --mode self|referrers|importers` produced.
                      stale      = true \\ mechanism   (a page left wrong)
                      gratuitous = mechanism \\ true   (a page rewritten for nothing)
  --mode excerpt    E2: the first diff hunks of one page, for the write-up.

usage:
  compare-pages.py --mode count   --a <dir> --b <dir> [--needle <s>] [--needle-b <s>]
  compare-pages.py --mode impact  --a <dir> --b <dir> --sets <name>=<file> ...
                                  [--truth-out <p>] [--stale-out-prefix <p>]
  compare-pages.py --mode excerpt --a <dir> --b <dir> --page <Module> [--hunks N]
"""
import argparse
import difflib
import os
import statistics


def pages(root):
    out = []
    for dirpath, _dirs, files in os.walk(root):
        for f in files:
            if f.endswith(".html"):
                out.append(os.path.relpath(os.path.join(dirpath, f), root))
    return sorted(out)


def read(root, rel):
    return open(os.path.join(root, rel), encoding="utf-8").read()


def rel2mod(rel):
    return rel[:-5].replace(os.sep, ".")


def differing(a, b):
    out = []
    for rel in pages(a):
        if read(a, rel) != read(b, rel):
            out.append(rel)
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--mode", required=True, choices=("count", "impact", "excerpt"))
    ap.add_argument("--a", required=True)
    ap.add_argument("--b", required=True)
    ap.add_argument("--needle")
    ap.add_argument("--needle-b")
    ap.add_argument("--sets", nargs="*", default=[])
    ap.add_argument("--truth-out")
    ap.add_argument("--stale-out-prefix")
    ap.add_argument("--page")
    ap.add_argument("--hunks", type=int, default=3)
    ap.add_argument("--context", type=int, default=90)
    args = ap.parse_args()

    if args.mode == "count":
        allp = pages(args.a)
        diff = differing(args.a, args.b)
        print(f"pages in A                  {len(allp)}")
        print(f"pages differing             {len(diff)}")
        if args.needle:
            occ = []
            total_b = 0
            for rel in diff:
                n = read(args.a, rel).count(args.needle)
                occ.append(n)
                if args.needle_b:
                    total_b += read(args.b, rel).count(args.needle_b)
            print(f"occurrences of the needle in A, over the differing pages: {sum(occ):,}")
            if args.needle_b:
                print(f"occurrences of the replacement in B: {total_b:,}"
                      f"  ({'equal' if total_b == sum(occ) else 'DIFFERENT'})")
            if occ:
                print(f"per page: min {min(occ)}  median {statistics.median(occ)}  "
                      f"max {max(occ)}  mean {sum(occ)/len(occ):.2f}")
        print()
        print("per-page counts:")
        for rel in diff:
            n = read(args.a, rel).count(args.needle) if args.needle else 0
            print(f"{n:6d}  {rel}")
        return

    if args.mode == "excerpt":
        rel = args.page.replace(".", os.sep) + ".html"
        x, y = read(args.a, rel), read(args.b, rel)
        sm = difflib.SequenceMatcher(None, x, y, autojunk=False)
        print(f"--- {args.page}")
        print(f"    A {len(x)} chars, B {len(y)} chars, delta {len(y)-len(x):+d}")
        n = 0
        for op, i1, i2, j1, j2 in sm.get_opcodes():
            if op == "equal":
                continue
            n += 1
            if n > args.hunks:
                continue
            c = args.context
            print(f"    [{op}] A[{i1}:{i2}] -> B[{j1}:{j2}]")
            print(f"      A: ...{x[max(0, i1-c):i2+c]}...")
            print(f"      B: ...{y[max(0, j1-c):j2+c]}...")
        print(f"    diff hunks: {n}")
        return

    truth = {rel2mod(r) for r in differing(args.a, args.b)}
    print(f"true changed pages          {len(truth)}")
    if args.truth_out:
        open(args.truth_out, "w").write("\n".join(sorted(truth)) + "\n")
    for spec in args.sets:
        name, _, path = spec.partition("=")
        s = {l.strip() for l in open(path) if l.strip()}
        stale = truth - s
        grat = s - truth
        print(f"  mode {name:10s} set={len(s):4d}  stale={len(stale):4d}  gratuitous={len(grat):4d}")
        if args.stale_out_prefix:
            open(f"{args.stale_out_prefix}{name}.txt", "w").write(
                "\n".join(sorted(stale)) + ("\n" if stale else ""))


if __name__ == "__main__":
    main()
