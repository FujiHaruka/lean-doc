#!/usr/bin/env python3
"""E2: copy an IR tree and add one declaration to one module.

The measurement target is never touched: this reads a schema-2 IR tree produced
by the stage-4b extractor and writes a *copy* with one extra declaration in one
module file. Nothing else in the tree changes.

The declaration carries only the fields `experiments/stage4c/render.ts` needs;
whether that is enough is checked by the render succeeding, not asserted here.

`index.json`'s `contentHash` for the edited module is **left stale on purpose**:
it is `String.hash` of the module JSON as computed by Lean, and this script does
not start Lean. `render.ts` never reads it. Anything that does (merge-ir.ts's
`irChanged`) must not be run against this tree.

usage:
  inject-decl.py --base <ir> --out <ir> --module <M> --name <declName>
                 [--kind definition] [--type "..."]
"""
import argparse
import json
import os
import shutil


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--base", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--module", required=True)
    ap.add_argument("--name", required=True)
    ap.add_argument("--kind", default="definition")
    ap.add_argument("--type", dest="ty", default="ℝ → ℝ")
    args = ap.parse_args()

    if os.path.exists(args.out):
        shutil.rmtree(args.out)
    shutil.copytree(args.base, args.out)

    index = json.load(open(os.path.join(args.out, "index.json"), encoding="utf-8"))
    entries = [e for e in index["modules"] if e["module"] == args.module]
    if not entries:
        raise SystemExit(f"no such module in the IR: {args.module}")
    entry = entries[0]

    path = os.path.join(args.out, entry["file"])
    mod = json.load(open(path, encoding="utf-8"))
    decls = mod["declarations"]
    last_line = max((d["endLine"] for d in decls), default=0)
    last_index = max((d["index"] for d in decls), default=-1)

    decl = {
        "binderCode": [],
        "binders": [],
        "col": 0,
        "doc": None,
        "endCol": 20,
        "endLine": last_line + 3,
        "equationCode": [],
        "equations": [],
        "implicits": [],
        "index": last_index + 1,
        "kind": args.kind,
        "line": last_line + 2,
        "members": [],
        "modifiers": [],
        "name": args.name,
        "refs": [],
        "type": args.ty,
        "typeCode": [],
    }
    decls.append(decl)

    body = json.dumps(mod, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
    open(path, "w", encoding="utf-8").write(body)

    entry["declarations"] = len(decls)
    entry["bytes"] = len(body.encode("utf-8"))
    index["declarationCount"] = index["declarationCount"] + 1
    # contentHash deliberately left as-is; see the module docstring.
    json.dump(
        index,
        open(os.path.join(args.out, "index.json"), "w", encoding="utf-8"),
        sort_keys=True,
        separators=(",", ":"),
        ensure_ascii=False,
    )
    print(f"injected {args.name} ({args.kind}) into {args.module} -> {args.out}")


if __name__ == "__main__":
    main()
