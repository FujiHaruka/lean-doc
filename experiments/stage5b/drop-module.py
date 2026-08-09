#!/usr/bin/env python3
"""E4: copy an IR tree with one module removed (the "module was deleted" case).

Removes the module's `modules/*.json` and its `index.json` entry. `contentHash`
of the survivors is untouched; no Lean is started; the measurement target is not
read from or written to.

usage:
  drop-module.py --base <ir> --out <ir> --module <M>
"""
import argparse
import json
import os
import shutil


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--base", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--module", required=True)
    args = ap.parse_args()
    if os.path.exists(args.out):
        shutil.rmtree(args.out)
    shutil.copytree(args.base, args.out)
    p = os.path.join(args.out, "index.json")
    idx = json.load(open(p, encoding="utf-8"))
    hits = [e for e in idx["modules"] if e["module"] == args.module]
    if not hits:
        raise SystemExit(f"no such module: {args.module}")
    e = hits[0]
    idx["modules"] = [x for x in idx["modules"] if x["module"] != args.module]
    idx["moduleCount"] = len(idx["modules"])
    idx["declarationCount"] -= e["declarations"]
    os.remove(os.path.join(args.out, e["file"]))
    json.dump(idx, open(p, "w", encoding="utf-8"),
              sort_keys=True, separators=(",", ":"), ensure_ascii=False)
    print(f"dropped {args.module} ({e['declarations']} declarations) -> "
          f"{args.out} has {idx['moduleCount']} modules")


if __name__ == "__main__":
    main()
