#!/usr/bin/env python3
"""E4: a symlink stand-in for the measurement target's olean tree.

`ledger.ts` derives its lib dir from `--target`, so making a module "disappear"
means removing a file under that tree — which is not allowed here: nothing in
`/Users/haruka/dev/lean-projects` may be written. So this builds a *tree of
symlinks* pointing at the real oleans, plus symlinks for `lean-toolchain` and
`lake-manifest.json` (the two files `envKey` reads). Removing a symlink removes
nothing real; 432 modules cost 0 bytes instead of 227 MB.

usage:
  fake-target.py --target <repo> --out <dir> --modules <list>
  fake-target.py --out <dir> --drop <Module>      # make one module disappear
  fake-target.py --target <repo> --out <dir> --add <NewModule> --copy-of <Module>
"""
import argparse
import os
import shutil

SUFFIXES = (".olean", ".olean.server", ".olean.private", ".olean.hash")
LIB = ".lake/build/lib/lean"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--target")
    ap.add_argument("--out", required=True)
    ap.add_argument("--modules")
    ap.add_argument("--drop")
    ap.add_argument("--add")
    ap.add_argument("--copy-of")
    args = ap.parse_args()
    flib = os.path.join(args.out, LIB)

    if args.modules:
        if os.path.exists(args.out):
            shutil.rmtree(args.out)
        os.makedirs(flib)
        for f in ("lean-toolchain", "lake-manifest.json"):
            os.symlink(os.path.join(args.target, f), os.path.join(args.out, f))
        lib = os.path.join(args.target, LIB)
        mods = [l.strip() for l in open(args.modules) if l.strip() and not l.startswith("#")]
        n = 0
        for m in mods:
            rel = m.replace(".", "/")
            for suf in SUFFIXES:
                src = f"{lib}/{rel}{suf}"
                if os.path.exists(src):
                    dst = f"{flib}/{rel}{suf}"
                    os.makedirs(os.path.dirname(dst), exist_ok=True)
                    os.symlink(src, dst)
                    n += 1
        print(f"fake target {args.out}: {len(mods)} modules, {n} symlinks, 0 bytes copied")
        return

    if args.drop:
        rel = args.drop.replace(".", "/")
        gone = 0
        for suf in SUFFIXES:
            p = f"{flib}/{rel}{suf}"
            if os.path.islink(p) or os.path.exists(p):
                os.remove(p)
                gone += 1
        print(f"dropped {args.drop}: {gone} symlink(s) removed")
        return

    if args.add:
        lib = os.path.join(args.target, LIB)
        src_rel = args.copy_of.replace(".", "/")
        dst_rel = args.add.replace(".", "/")
        n = 0
        for suf in SUFFIXES:
            src = f"{lib}/{src_rel}{suf}"
            if os.path.exists(src):
                dst = f"{flib}/{dst_rel}{suf}"
                os.makedirs(os.path.dirname(dst), exist_ok=True)
                os.symlink(src, dst)
                n += 1
        print(f"added {args.add} (bytes of {args.copy_of}): {n} symlink(s)")
        return

    raise SystemExit("nothing to do: pass --modules, --drop or --add")


if __name__ == "__main__":
    main()
