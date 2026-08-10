#!/usr/bin/env python3
"""Build the intra-package import graph of the target project from .lean headers.

Only edges between modules of the root package are kept (Mathlib/loogle imports
are counted but not followed). Prints candidates for the olean-propagation
experiment: modules ranked by how many other package modules import them,
directly and transitively.

Usage: import-graph.py <project-root> [--json <out>]
"""
import json
import re
import sys
from collections import defaultdict, deque
from pathlib import Path

IMPORT_RE = re.compile(r"^\s*import\s+([A-Za-z_][\w.]*)", re.M)


def module_name(root: Path, path: Path) -> str:
    rel = path.relative_to(root).with_suffix("")
    return ".".join(rel.parts)


def main() -> int:
    root = Path(sys.argv[1]).resolve()
    out_json = None
    if "--json" in sys.argv:
        out_json = Path(sys.argv[sys.argv.index("--json") + 1])

    lib = "InformationTheory"
    files = sorted((root / lib).rglob("*.lean"))
    top = root / f"{lib}.lean"
    if top.exists():
        files.append(top)

    imports: dict[str, list[str]] = {}
    for f in files:
        name = module_name(root, f)
        header = f.read_text(encoding="utf-8", errors="replace")
        # imports may only appear before the first command; cut at the first
        # non-import, non-comment line to avoid matching `import` inside strings
        deps = [m for m in IMPORT_RE.findall(header)]
        imports[name] = [d for d in deps if d == lib or d.startswith(lib + ".")]

    rev: dict[str, set[str]] = defaultdict(set)
    for m, deps in imports.items():
        for d in deps:
            rev[d].add(m)

    def transitive_importers(m: str) -> set[str]:
        seen: set[str] = set()
        q = deque(rev.get(m, ()))
        while q:
            x = q.popleft()
            if x in seen:
                continue
            seen.add(x)
            q.extend(rev.get(x, ()))
        return seen

    rows = []
    for m in imports:
        ti = transitive_importers(m)
        rows.append((len(rev.get(m, ())), len(ti), m))
    rows.sort(reverse=True)

    print(f"modules: {len(imports)}  (files: {len(files)})")
    print(f"{'direct':>6} {'transitive':>10}  module")
    for direct, trans, m in rows[:25]:
        print(f"{direct:6d} {trans:10d}  {m}")

    if out_json:
        out_json.write_text(
            json.dumps(
                {
                    "modules": sorted(imports),
                    "imports": {k: sorted(v) for k, v in imports.items()},
                    "importers": {k: sorted(v) for k, v in rev.items()},
                },
                indent=1,
            )
        )
        print(f"\nwrote {out_json}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
