#!/usr/bin/env python3
"""E3: what `--open` changes in the IR, and which module is affected by which
of the two mechanisms behind the flag.

`--open <ns>` does two separate things in `experiments/stage4b/Extract.lean`:

  * `Lean.activateScoped` (Extract.lean:1517-1524) — makes the `scoped notation`
    declared *inside* `<ns>` fire, so `mutualInfo μ X Y` prints as `I(μ; X ; Y)`.
    The notation only exists in the environment if the module that declares it
    is in the run's import closure.
  * `Core.Context.openDecls` (Extract.lean:1562) — makes the pretty printer
    shorten any name under `<ns>`, e.g. `Shannon.typeCount` -> `typeCount`.
    This needs no particular module: it is a printer option.

`classify` splits the modules whose IR changed into those two causes, using the
printed text only (`type` / `binders` / `equations` / `members[].text`):

  notation  a notation glyph (`H(`, `I(`, `D(`, `≐`) occurs more often than before
  shorten   no new glyph, but the printed text got shorter

`pick` then names the modules the single-module re-extraction is run on, by a
rule fixed in advance: **the module with the fewest declarations in each class,
ties broken by module name ascending.** Fewest declarations = cheapest to print;
the class is what decides which of the two mechanisms is exercised.

usage:
  open-diff.py classify --base <ir> --open <ir> --reach <json> --out-dir <dir>
  open-diff.py bytecmp  --pair <label>=<fileA>:<fileB> ...
"""
import argparse
import hashlib
import json
import os
import re
import sys

GLYPH = re.compile(r"(?:H\(|I\(|D\(|≐)")


def texts(mod):
    out = []
    for d in mod["declarations"]:
        out.append(d.get("type") or "")
        out.extend(d.get("binders") or [])
        out.extend(d.get("equations") or [])
        for m in (d.get("members") or []):
            out.append(m.get("text") or "")
    return "\n".join(t for t in out if isinstance(t, str))


def classify(args):
    base, opn = args.base, args.open
    a, b = os.path.join(base, "modules"), os.path.join(opn, "modules")
    fa, fb = set(os.listdir(a)), set(os.listdir(b))
    if fa != fb:
        print(f"module files differ: only-base={sorted(fa-fb)} only-open={sorted(fb-fa)}")
    changed, notation, shorten = [], [], []
    for f in sorted(fa & fb):
        x, y = open(os.path.join(a, f), "rb").read(), open(os.path.join(b, f), "rb").read()
        if x == y:
            continue
        m = f[:-5]
        changed.append((m, len(x), len(y)))
        ta = texts(json.loads(x.decode("utf-8")))
        tb = texts(json.loads(y.decode("utf-8")))
        if len(GLYPH.findall(tb)) > len(GLYPH.findall(ta)):
            notation.append(m)
        else:
            shorten.append(m)
    os.makedirs(args.out_dir, exist_ok=True)
    w = lambda n, xs: open(os.path.join(args.out_dir, n), "w").write("\n".join(xs) + "\n")
    w("changed-modules.txt", [m for m, _, _ in changed])
    w("cls-notation.txt", sorted(notation))
    w("cls-shorten.txt", sorted(shorten))

    print(f"modules compared                       {len(fa & fb)}")
    print(f"modules whose IR bytes changed         {len(changed)}")
    print(f"  cause: a scoped notation now fires   {len(notation)}")
    print(f"  cause: name shortening only          {len(shorten)}")

    reach = json.load(open(args.reach, encoding="utf-8"))
    ref = set(reach["refUnion"])
    imp = set(reach["importersUnion"]) | set(reach["importersClosures"])
    print()
    print(f"modules whose `refs` name a notation head           {len(ref)}")
    print(f"  == the set changed by the notation mechanism?     {set(notation) == ref}")
    print(f"IMPORTERS(notation modules) + the modules themselves {len(imp)}")
    print(f"  changed modules outside that bound                {len(set(m for m,_,_ in changed) - imp)}")
    print(f"  notation-caused modules outside that bound        {len(set(notation) - imp)}")

    # the pick rule, applied
    picks = {}
    for cls, names in (("notation", notation), ("shorten", shorten)):
        sized = sorted(
            (len(json.load(open(os.path.join(a, n + ".json"), encoding="utf-8"))["declarations"]), n)
            for n in names)
        picks[cls] = sized[0][1] if sized else None
        print(f"\npick[{cls}] = {picks[cls]}  (declarations: {sized[0][0] if sized else '-'})")
        print(f"  next four: {sized[1:5]}")
    json.dump(picks, open(os.path.join(args.out_dir, "picks.json"), "w"))


def bytecmp(args):
    for spec in args.pair:
        label, _, rest = spec.partition("=")
        p, _, q = rest.partition(":")
        x, y = open(p, "rb").read(), open(q, "rb").read()
        h = lambda s: hashlib.sha256(s).hexdigest()[:16]
        print(f"{label:44s} {'IDENTICAL' if x == y else 'DIFFER':10s} "
              f"{len(x):7d} vs {len(y):7d}   {h(x)} {h(y)}")


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("classify")
    c.add_argument("--base", required=True)
    c.add_argument("--open", required=True)
    c.add_argument("--reach", required=True)
    c.add_argument("--out-dir", required=True)
    c.set_defaults(fn=classify)
    b = sub.add_parser("bytecmp")
    b.add_argument("--pair", action="append", required=True)
    b.set_defaults(fn=bytecmp)
    args = ap.parse_args()
    args.fn(args)


if __name__ == "__main__":
    sys.exit(main())
