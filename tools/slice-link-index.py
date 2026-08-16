#!/usr/bin/env python3
"""Slice a `.lidx` into the part that belongs to a package and the part that does not.

usage: lidx-slice.py <in.lidx> <own-modules.txt> <mode> <out.lidx>

modes
  deps        drop own-package declaration groups; keep every `@` line
  deps-only   drop own-package declaration groups AND own `@` lines
  local       keep ONLY own-package declaration groups and own `@` lines

The point of the first two is to answer, without writing any Lean, whether the
own-package half of the map reaches the rendered bytes at all: the renderer
resolves an own-package name through the IR-derived index (`known`) before it
ever asks the map (`autolink.rs:258`), and a range it does get back is dropped
unless the link resolved into a *dependency* package (`external.rs:146-162`).
If that reading is right, `deps` renders byte-identically to the full map.
"""
import sys

src, own_path, mode, dst = sys.argv[1:5]
own = {line.strip() for line in open(own_path, encoding="utf-8") if line.strip()}

kept_group = False
n_groups_dropped = n_groups_kept = n_decls_dropped = n_at_dropped = 0
with open(src, encoding="utf-8") as f, open(dst, "w", encoding="utf-8") as g:
    for line in f:
        if line.startswith("#"):
            g.write(line)
        elif line.startswith("@"):
            is_own = line[1:].rstrip("\n") in own
            if mode == "deps" or (mode == "local") == is_own:
                g.write(line)
            else:
                n_at_dropped += 1
        elif line.startswith("\t"):
            if kept_group:
                g.write(line)
            else:
                n_decls_dropped += 1
        elif line.strip() == "":
            pass
        else:
            is_own = line.rstrip("\n") in own
            kept_group = (mode == "local") == is_own
            if kept_group:
                g.write(line)
                n_groups_kept += 1
            else:
                n_groups_dropped += 1

print(f"{mode}: groups kept {n_groups_kept}, dropped {n_groups_dropped}, "
      f"declaration lines dropped {n_decls_dropped}, @ lines dropped {n_at_dropped}")
