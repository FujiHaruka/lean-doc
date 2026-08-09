#!/usr/bin/env python3
"""E6: choose the declaration to move and the two modules to move it to.

The rules are fixed before the numbers are looked at, and are mechanical:

  D   the declaration of this package that appears in the `refs` of the most
      *distinct other* modules. Ties broken by name ascending. (The most
      referenced name is the worst case for the ownership channel, which is what
      the experiment is for.)
  M   D's defining module, read from the IR.
  M'  two of them, so that the destination's own position in the import graph
      cannot be what carries the result:
        down  among the modules that transitively import M, do not reference D,
              and declare at least one declaration: fewest declarations,
              ties by name ascending.
        far   the same, among the modules that do *not* transitively import M.

`>= 1 declaration` excludes the package's import-aggregator modules, whose pages
have no declarations at all; moving into one of them would make the destination
page's diff trivial for a reason that has nothing to do with the channel.

usage:
  pick-move.py --ir <dir> [--json <path>]
"""
import argparse
import collections
import json
import os


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ir", required=True)
    ap.add_argument("--json")
    args = ap.parse_args()

    index = json.load(open(os.path.join(args.ir, "index.json"), encoding="utf-8"))
    mods = {e["module"]: json.load(open(os.path.join(args.ir, e["file"]), encoding="utf-8"))
            for e in index["modules"]}
    own = set(mods)

    ref = collections.defaultdict(set)
    for m, d in mods.items():
        for dec in d["declarations"]:
            for r in dec.get("refs", []):
                if r[0] in own and r[0] != m:
                    ref[(r[0], r[1])].add(m)
    rank = sorted(((len(v), k[1], k[0]) for k, v in ref.items()), key=lambda t: (-t[0], t[1]))
    print("most-referenced declarations of this package (distinct other modules):")
    for n, name, mod in rank[:6]:
        print(f"  {n:4d}  {name}   <- {mod}")
    n_ref, D, M = rank[0]

    rev = {m: set() for m in own}
    for m, d in mods.items():
        for i in set(d["imports"]):
            if i in own:
                rev[i].add(m)
    seen, stack = set(), [M]
    while stack:
        x = stack.pop()
        for r in rev.get(x, ()):
            if r not in seen:
                seen.add(r)
                stack.append(r)

    refmods = ref[(M, D)]
    nz = [m for m in own if len(mods[m]["declarations"]) > 0]
    pool = lambda ms: sorted((len(mods[m]["declarations"]), m) for m in ms)
    down = pool([m for m in nz if m in seen and m != M and m not in refmods])
    far = pool([m for m in nz if m not in seen and m != M and m not in refmods])

    out = {
        "decl": D, "from": M, "referencedByModules": n_ref,
        "refModules": sorted(refmods),
        "importersOfM": len(seen),
        "down": down[0][1], "far": far[0][1],
        "downCandidates": down[:4], "farCandidates": far[:4],
    }
    print(f"\nD  = {D}")
    print(f"M  = {M}   (|IMPORTERS(M)| = {len(seen)}, refs from {n_ref} other modules)")
    print(f"M' down = {down[0][1]}  (declarations {down[0][0]}; next {down[1:4]})")
    print(f"M' far  = {far[0][1]}  (declarations {far[0][0]}; next {far[1:4]})")
    if args.json:
        json.dump(out, open(args.json, "w", encoding="utf-8"), ensure_ascii=False, indent=1)


if __name__ == "__main__":
    main()
