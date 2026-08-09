#!/usr/bin/env python3
"""Read-only: how far could `--open` reach?

The two scoped-notation modules declare notation whose *heads* are constants
defined in OTHER modules:

  InformationTheory.Shannon.TypedRV  -> entropy, condEntropy, mutualInfo,
                                        condMutualInfo, klDivRV
  InformationTheory.Asymptotic       -> InformationTheory.Asymptotic.DotEq

So the set of modules whose printed text could change under
`--open InformationTheory.Shannon,InformationTheory.Asymptotic` is
"modules that name one of those heads", which is NOT contained in
"modules that import the notation-declaring module".

usage: notation_surface.py <ir-dir>
"""
import json
import os
import sys

IR = sys.argv[1]
index = json.load(open(f"{IR}/index.json"))

mods = {}
for e in index["modules"]:
    m = json.load(open(f"{IR}/{e['file']}"))
    mods[m["module"]] = m

# transitive import closure over the package's own modules only would be wrong:
# use the IR imports (full, includes Mathlib names we simply won't expand).
direct = {k: v.get("imports", []) for k, v in mods.items()}


def closure(seed):
    seen, stack = set(), [seed]
    while stack:
        x = stack.pop()
        for n in direct.get(x, []):
            if n not in seen:
                seen.add(n)
                stack.append(n)
    return seen


TYPED = "InformationTheory.Shannon.TypedRV"
ASYM = "InformationTheory.Asymptotic"

heads_typed = {
    "InformationTheory.Shannon.entropy",
    "InformationTheory.Shannon.condEntropy",
    "InformationTheory.Shannon.mutualInfo",
    "InformationTheory.Shannon.condMutualInfo",
    "InformationTheory.Shannon.klDivRV",
}
heads_asym = {"InformationTheory.Asymptotic.DotEq"}

def names_of(mod):
    """Every constant name the module's printed fragments resolved to."""
    out = set()
    for d in mod["declarations"]:
        for _m, n in d.get("refs", []):
            out.add(n)
    return out


for label, heads, owner in (("TypedRV", heads_typed, TYPED),
                            ("Asymptotic", heads_asym, ASYM)):
    hit, hit_no_import = [], []
    for name, mod in mods.items():
        if name == owner:
            continue
        ns = names_of(mod)
        if ns & heads:
            hit.append(name)
            if owner not in closure(name):
                hit_no_import.append(name)
    print(f"[{label}] notation owner {owner}")
    print(f"[{label}] modules naming a notation head          {len(hit)}")
    print(f"[{label}] of those, NOT importing the owner       {len(hit_no_import)}")
    for m in sorted(hit_no_import)[:15]:
        print(f"      {m}")
    # importers of the owner, for comparison
    importers = [m for m in mods if owner in closure(m)]
    print(f"[{label}] transitive importers of the owner       {len(importers)}")
    print()
