#!/usr/bin/env python3
"""E6: move one declaration from module M to module M' inside a copy of the IR.

This is leg 7's *ownership channel* (`experiments/stage5/README.md` §4.2): the
IR stores every reference as a `(defining module, name)` pair, so a declaration
that changes owner invalidates the `refs` of every module that names it — while
those modules' own oleans are untouched.

The target repository cannot be edited, so the move is injected at the IR level.
Two trees are produced from one base tree, and the pair is the experiment:

  --out        D moved, **every `refs` entry left alone**. This is what the
               world looks like when only M and M' are re-extracted: the
               referring modules still claim D lives in M.
  --fixed-out  D moved *and* every `["M", "D"]` ref rewritten to `["M'", "D"]`.
               This is what a full re-extraction would have produced.

Rendering both and byte-comparing gives, respectively, "what the incremental
pipeline would publish" and "what is correct".

`index.json`'s `contentHash` is left stale for the two edited modules, exactly
as `inject-decl.py` does and for the same reason (it is Lean's `String.hash`;
this script does not start Lean). `render.ts` does not read it.

usage:
  move-decl.py --base <ir> --out <ir> --fixed-out <ir> --decl <Name> --to <M'>
               [--json <path>]
"""
import argparse
import json
import os
import shutil


def load_index(ir):
    return json.load(open(os.path.join(ir, "index.json"), encoding="utf-8"))


def write_json(path, obj):
    open(path, "w", encoding="utf-8").write(
        json.dumps(obj, sort_keys=True, separators=(",", ":"), ensure_ascii=False))


def module_path(ir, index, mod):
    for e in index["modules"]:
        if e["module"] == mod:
            return os.path.join(ir, e["file"]), e
    raise SystemExit(f"no such module in the IR: {mod}")


def rewrite(ir, decl_name, src, dst, fix_refs):
    """Apply the move to the tree at `ir` (already a copy)."""
    index = load_index(ir)
    src_path, src_entry = module_path(ir, index, src)
    dst_path, dst_entry = module_path(ir, index, dst)

    src_mod = json.load(open(src_path, encoding="utf-8"))
    hits = [d for d in src_mod["declarations"] if d["name"] == decl_name]
    if not hits:
        raise SystemExit(f"{decl_name} is not declared in {src}")
    decl = hits[0]
    src_mod["declarations"] = [d for d in src_mod["declarations"] if d["name"] != decl_name]

    dst_mod = json.load(open(dst_path, encoding="utf-8"))
    moved = dict(decl)
    moved["index"] = max((d["index"] for d in dst_mod["declarations"]), default=-1) + 1
    last_line = max((d["endLine"] for d in dst_mod["declarations"]), default=0)
    span = decl["endLine"] - decl["line"]
    moved["line"] = last_line + 2
    moved["endLine"] = last_line + 2 + span
    dst_mod["declarations"].append(moved)

    touched = {src: src_mod, dst: dst_mod}
    n_refs_fixed = 0
    ref_mods = set()
    for e in index["modules"]:
        mod = touched.get(e["module"])
        if mod is None:
            mod = json.load(open(os.path.join(ir, e["file"]), encoding="utf-8"))
        dirty = e["module"] in touched
        for d in mod["declarations"]:
            for r in d.get("refs", []):
                if r[1] == decl_name and r[0] == src:
                    ref_mods.add(e["module"])
                    if fix_refs:
                        r[0] = dst
                        n_refs_fixed += 1
                        dirty = True
        if dirty:
            body = json.dumps(mod, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
            open(os.path.join(ir, e["file"]), "w", encoding="utf-8").write(body)
            e["bytes"] = len(body.encode("utf-8"))
            e["declarations"] = len(mod["declarations"])
    write_json(os.path.join(ir, "index.json"), index)
    return {"refModules": sorted(ref_mods), "refsRewritten": n_refs_fixed}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--base", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--fixed-out", required=True)
    ap.add_argument("--decl", required=True)
    ap.add_argument("--to", required=True)
    ap.add_argument("--json")
    args = ap.parse_args()

    index = load_index(args.base)
    src = None
    for e in index["modules"]:
        mod = json.load(open(os.path.join(args.base, e["file"]), encoding="utf-8"))
        if any(d["name"] == args.decl for d in mod["declarations"]):
            src = e["module"]
            break
    if src is None:
        raise SystemExit(f"{args.decl} is declared by no module in the IR")
    if src == args.to:
        raise SystemExit("source and destination are the same module")

    info = {}
    for out, fix in ((args.out, False), (args.fixed_out, True)):
        if os.path.exists(out):
            shutil.rmtree(out)
        shutil.copytree(args.base, out)
        info["stale" if not fix else "fixed"] = rewrite(out, args.decl, src, args.to, fix)
    info["decl"] = args.decl
    info["from"] = src
    info["to"] = args.to
    print(f"moved {args.decl}: {src} -> {args.to}")
    print(f"  modules whose refs name it: {len(info['stale']['refModules'])}")
    print(f"  refs rewritten in the fixed tree: {info['fixed']['refsRewritten']}")
    if args.json:
        json.dump(info, open(args.json, "w", encoding="utf-8"), ensure_ascii=False, indent=1)


if __name__ == "__main__":
    main()
