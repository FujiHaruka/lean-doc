#!/usr/bin/env python3
"""E2: pick the declaration to inject, and extend the two measured cases to all
   432 possible modules to inject into.

Input is `instrument-render.py`'s dump of unresolved autolink tokens plus the
schema-2 IR (for the import graph). Nothing here starts Lean and nothing reads
the measurement target.

Two things are computed and they must not be confused:

  * the candidate census (measured): which docstring tokens currently fail to
    autolink, on how many pages. This is the basis for choosing what to inject
    and it is a measurement of the base IR.

  * the all-N extension (derived): for a name X injected into module N, the
    pages that newly link are the pages where X appeared as a failing token —
    either as the whole token, or as the part after the last dot, which is
    `autoLinkInline`'s second attempt. Subtracting `{N} ∪ IMPORTERS(N)` gives
    the stale count. This model is **validated against the measured renders**
    before it is used, and the validation line is printed.

usage:
  autolink-analysis.py --ir <dir> --dump <unresolved.jsonl>
                       [--validate <name>=<module>=<truth.txt> ...]
                       [--token log]
"""
import argparse
import collections
import json
import os
import statistics


NAME_LIT_BAD = set(" \t\n\r\f\v()[]{},;\"'`\\")


def is_name_lit(s: str) -> bool:
    """`isNameLit` transcribed from render.ts:845-854 — the gate `nameToLink`
    applies before it will link anything. A token that fails it can never become
    a link, however many declarations are added."""
    if not s:
        return False
    for part in s.split("."):
        if not part:
            return False
        if part.startswith("«") and part.endswith("»"):
            continue
        if part[0].isdigit():
            return False
        if any(c in NAME_LIT_BAD for c in part):
            return False
    return True


def ident_like(tok: str) -> bool:
    """Shaped like a Lean declaration name: every dot component is an identifier.

    `isNameLit` in render.ts is much looser — it accepts `=`, `≤`, `+` — so this
    stricter filter is what the census reports as a *realistic* candidate.
    """
    if len(tok) < 3 or not is_name_lit(tok):
        return False
    for c in tok.split("."):
        if not c:
            return False
        s = c.replace("'", "_").replace("!", "_").replace("?", "_")
        if not s.isidentifier():
            return False
    return True


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ir", required=True)
    ap.add_argument("--dump", required=True)
    ap.add_argument("--validate", nargs="*", default=[])
    ap.add_argument("--token", default="log")
    args = ap.parse_args()

    idx = json.load(open(os.path.join(args.ir, "index.json"), encoding="utf-8"))
    mods = [e["module"] for e in idx["modules"]]
    own = set(mods)
    ix = {m: i for i, m in enumerate(mods)}
    imported_by = collections.defaultdict(list)
    for e in idx["modules"]:
        d = json.load(open(os.path.join(args.ir, e["file"]), encoding="utf-8"))
        for i in d["imports"]:
            if i in own:
                imported_by[i].append(e["module"])

    def closure(seed):
        seen, st = set(), [seed]
        while st:
            for n in imported_by.get(st.pop(), ()):
                if n not in seen:
                    seen.add(n)
                    st.append(n)
        return seen

    impm = {}
    for m in mods:
        v = 1 << ix[m]
        for x in closure(m):
            v |= 1 << ix[x]
        impm[m] = v
    sizes = [bin(v).count("1") for v in impm.values()]
    print(f"modules {len(mods)};  |{{M}} ∪ IMPORTERS(M)|: min {min(sizes)} "
          f"median {statistics.median(sizes)} max {max(sizes)}")

    rows = [json.loads(l) for l in open(args.dump, encoding="utf-8")]
    print()
    print("=== A. census of unresolved docstring autolink tokens (measured) ===")
    print(f"distinct tokens                        {len(rows):,}")
    print(f"failing attempts                       {sum(r['occurrences'] for r in rows):,}")
    nl = [r for r in rows if r["isNameLit"]]
    print(f"  passing render.ts's isNameLit        {len(nl):,} distinct / "
          f"{sum(r['occurrences'] for r in nl):,} occurrences")
    strict = [r for r in rows if ident_like(r["token"])]
    assert all(r["isNameLit"] for r in strict), "ident_like must imply render.ts's isNameLit"
    print(f"  and shaped like a declaration name   {len(strict):,} distinct / "
          f"{sum(r['occurrences'] for r in strict):,} occurrences")
    multi = [r for r in strict if r["pages"] >= 2]
    print(f"  ... on >= 2 pages                    {len(multi):,} distinct / "
          f"{sum(r['occurrences'] for r in multi):,} occurrences")
    pp = sorted(r["pages"] for r in strict)
    print(f"  pages per token: median {statistics.median(pp)}  "
          f"p90 {pp[int(len(pp)*0.9)]}  max {pp[-1]}")
    print()
    print("  top 20 by page count (the injection candidate is the top row):")
    print(f"  {'pages':>6} {'occ':>6}  token")
    for r in sorted(strict, key=lambda r: (-r["pages"], -r["occurrences"]))[:20]:
        print(f"  {r['pages']:6d} {r['occurrences']:6d}  {r['token']}")

    # affected(X): the pages where injecting a declaration named X makes a
    # currently-failing attempt succeed.
    aff = collections.defaultdict(set)
    for r in rows:
        ps = [p for p in r["pageList"] if p in own]
        t = r["token"]
        if is_name_lit(t):
            aff[t].update(ps)
        if "." in t:
            head, _, tail = t.rpartition(".")
            if head and is_name_lit(tail):
                aff[tail].update(ps)

    print()
    print("=== B. validation of the all-N model against the measured renders ===")
    for spec in args.validate:
        name, module, truth_path = spec.split("=", 2)
        truth = {l.strip() for l in open(truth_path) if l.strip()}
        pred = set(aff[args.token]) | {module}
        ok = "EXACT" if pred == truth else f"+{len(pred - truth)} / -{len(truth - pred)}"
        print(f"  {name:6s} N={module}")
        print(f"         model {len(pred)} pages vs measured {len(truth)} -> {ok}")

    def stale_over_n(x):
        a = 0
        for p in aff[x]:
            a |= 1 << ix[p]
        return [bin(a & ~impm[n]).count("1") for n in mods]

    print()
    print(f"=== C. `{args.token}` injected into each of the 432 modules in turn (derived) ===")
    sc = stale_over_n(args.token)
    print(f"  affected pages                       {len(aff[args.token])}")
    print(f"  stale under --mode importers: min {min(sc)}  median {statistics.median(sc)}  "
          f"max {max(sc)}")
    print(f"  modules N for which stale == 0:      {sum(1 for x in sc if x == 0)} / {len(mods)}")

    print()
    print("=== D. every name-shaped candidate on >= 2 pages, over every N (derived) ===")
    cands = [t for t in aff if ident_like(t) and len(aff[t]) >= 2]
    best, meds, pairs, pos = [], [], 0, 0
    for t in cands:
        sc = stale_over_n(t)
        best.append((min(sc), t, len(aff[t])))
        meds.append(statistics.median(sc))
        pairs += len(sc)
        pos += sum(1 for x in sc if x > 0)
    best.sort(reverse=True)
    zero = sum(1 for b, _, _ in best if b == 0)
    print(f"  candidates                            {len(cands):,}")
    print(f"  (candidate, N) pairs                  {pairs:,}")
    print(f"    pairs leaving >= 1 stale page       {pos:,} ({pos/pairs*100:.1f}%)")
    print(f"  best case over N (the luckiest module to have edited):")
    print(f"    stale == 0                          {zero:,} ({zero/len(cands)*100:.1f}%)")
    print(f"    stale >= 1                          {len(cands)-zero:,} "
          f"({(len(cands)-zero)/len(cands)*100:.1f}%)")
    mm = sorted(meds)
    print(f"  per-candidate median stale over N: median {statistics.median(mm)}  "
          f"p90 {mm[int(len(mm)*0.9)]}  max {mm[-1]}")
    print("  top 15 candidates by best-case stale:")
    for b, t, n in best[:15]:
        print(f"    {b:4d} stale (of {n:4d} affected pages)  `{t}`")


if __name__ == "__main__":
    main()
