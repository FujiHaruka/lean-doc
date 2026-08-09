#!/usr/bin/env python3
"""E3, part 1: the reach of the printing channel, computed on the IR's graphs.

Answers three questions about this target, from `ir-base` alone (no Lean):

  1. Which modules declare `scoped notation` / `notation3`, and which constants
     are the *heads* of those notations (the name the notation prints instead
     of)?  The heads are read from the target's sources (read-only, `rg`-style
     scan), because the IR does not record notation.
  2. IMPORTERS(M) — the reverse *transitive* import closure inside the package —
     for the notation-declaring modules.  This is what `impact.ts --mode
     importers` would re-render.
  3. Which modules *name a head* in their `refs` (i.e. their printed text could
     change if the notation were active), and how many of those are inside the
     IMPORTERS closures of (1).  A module in (3) but not in (2) is a page the
     import closure does not bound.

usage:
  notation-reach.py --ir <dir> --sources <target repo> [--json <path>]
"""
import argparse
import json
import os
import re
import sys


def load_ir(ir):
    index = json.load(open(os.path.join(ir, "index.json"), encoding="utf-8"))
    mods = {}
    for e in index["modules"]:
        mods[e["module"]] = json.load(
            open(os.path.join(ir, e["file"]), encoding="utf-8"))
    return index, mods


def importers_closure(mods, roots):
    """Reverse transitive import closure, restricted to the package's modules."""
    own = set(mods)
    rev = {m: set() for m in own}
    for m, d in mods.items():
        for i in set(d.get("imports", [])):
            if i in own:
                rev[i].add(m)
    seen, stack = set(), list(roots)
    while stack:
        m = stack.pop()
        for r in rev.get(m, ()):
            if r not in seen:
                seen.add(r)
                stack.append(r)
    return seen


NOTATION_RE = re.compile(
    r"^\s*(?:@\[[^\]]*\]\s*)?(?:scoped(?:\[[^\]]*\])?\s+)?(notation3?|macro|syntax|elab)\b")


def scan_notations(repo):
    """Every notation-ish declaration in the package's own sources, with the
    namespace it is scoped into and the identifiers on its right-hand side."""
    root = os.path.join(repo, "InformationTheory")
    out = []
    for dirpath, _d, files in os.walk(root):
        for f in sorted(files):
            if not f.endswith(".lean"):
                continue
            p = os.path.join(dirpath, f)
            lines = open(p, encoding="utf-8").read().split("\n")
            for i, line in enumerate(lines):
                m = NOTATION_RE.match(line)
                if not m:
                    continue
                # the notation body may continue on the following lines until a
                # blank line or the next top-level declaration
                body = line
                j = i
                while "=>" not in body and j + 1 < len(lines):
                    j += 1
                    body += " " + lines[j].strip()
                rhs = body.split("=>", 1)[1] if "=>" in body else ""
                # continuation lines of the RHS
                k = j
                while k + 1 < len(lines) and lines[k + 1].startswith("  ") \
                        and not NOTATION_RE.match(lines[k + 1]):
                    k += 1
                    rhs += " " + lines[k].strip()
                scope = re.search(r"scoped\[([^\]]*)\]", line)
                ids = re.findall(r"[A-Za-z_][A-Za-z0-9_.']*", rhs)
                out.append({
                    "file": os.path.relpath(p, repo),
                    "line": i + 1,
                    "kind": m.group(1),
                    "scoped": "scoped" in line,
                    "scope": scope.group(1) if scope else None,
                    "rhs_ids": ids,
                })
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ir", required=True)
    ap.add_argument("--sources", required=True)
    ap.add_argument("--json")
    args = ap.parse_args()

    index, mods = load_ir(args.ir)
    own = set(mods)
    print(f"package modules in the IR      {len(own)}")

    nots = scan_notations(args.sources)
    print(f"\n## notation-ish declarations in the package's sources: {len(nots)}")
    decl_mods = set()
    for n in nots:
        mod = n["file"][:-5].replace("/", ".")
        decl_mods.add(mod)
        print(f"  {n['file']}:{n['line']}  {n['kind']}  scoped={n['scoped']}  "
              f"scope={n['scope']}  rhs={' '.join(n['rhs_ids'][:6])}")
    print(f"declaring modules: {sorted(decl_mods)}")

    # every declaration name, and which module owns it (from the IR)
    owner = {}
    for m, d in mods.items():
        for dec in d["declarations"]:
            owner.setdefault(dec["name"], m)

    # the notation heads: RHS identifiers that resolve to a declaration of this
    # package, either fully qualified or under the scope namespace
    heads = {}
    for n in nots:
        for ident in n["rhs_ids"]:
            for cand in (ident, f"{n['scope']}.{ident}" if n["scope"] else None,
                         f"InformationTheory.Asymptotic.{ident}"):
                if cand and cand in owner:
                    heads[cand] = owner[cand]
                    break
    print(f"\n## notation heads that are declarations of this package: {len(heads)}")
    for h in sorted(heads):
        print(f"  {h}  <- {heads[h]}")

    # who names a head in refs
    ref_mods = {h: set() for h in heads}
    for m, d in mods.items():
        for dec in d["declarations"]:
            for r in dec.get("refs", []):
                dm, name = r[0], r[1]
                if name in heads:
                    ref_mods[name].add(m)
    union_ref = set()
    for h in heads:
        union_ref |= ref_mods[h]
    print(f"\n## modules whose `refs` name a head")
    for h in sorted(heads, key=lambda x: -len(ref_mods[x])):
        print(f"  {len(ref_mods[h]):4d}  {h}")
    print(f"  union: {len(union_ref)}")

    # the import closures
    closures = {}
    for m in sorted(decl_mods):
        if m not in own:
            print(f"  (declaring module not in the IR: {m})")
            continue
        c = importers_closure(mods, [m])
        closures[m] = c
        print(f"\nIMPORTERS({m}) = {len(c)}  {sorted(c) if len(c) <= 10 else ''}")
    union_imp = set()
    for c in closures.values():
        union_imp |= c
    print(f"union of the IMPORTERS closures: {len(union_imp)}")

    outside = union_ref - union_imp - set(closures)
    print(f"\nmodules that name a head but are NOT in any IMPORTERS closure: {len(outside)}")
    for m in sorted(outside)[:10]:
        print(f"  {m}")
    if len(outside) > 10:
        print(f"  ... and {len(outside) - 10} more")

    if args.json:
        json.dump({
            "packageModules": len(own),
            "notations": nots,
            "declaringModules": sorted(decl_mods),
            "heads": heads,
            "refModules": {h: sorted(v) for h, v in ref_mods.items()},
            "refUnion": sorted(union_ref),
            "importersClosures": {k: sorted(v) for k, v in closures.items()},
            "importersUnion": sorted(union_imp),
            "namesHeadOutsideImporters": sorted(outside),
        }, open(args.json, "w", encoding="utf-8"), ensure_ascii=False, indent=1)


if __name__ == "__main__":
    sys.exit(main())
