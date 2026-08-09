#!/usr/bin/env python3
"""Read-only: does page(M) link to modules outside M's transitive import closure?

Refinements over revdep.py:
  * the closure always contains Init's closure (Lean's implicit prelude import,
    which the .lean header parser cannot see);
  * for the package's own 432 modules the direct imports come from the IR
    (`modules/*.json`.imports), which is the environment's own list;
  * links are split into `nav` (everything before <main>: the Imports list and
    the module source link) and `main` (the declaration bodies).

usage: revdep2.py <doc-root-dir> <ir-dir> <label>
"""
import json
import os
import re
import sys
import importlib.util

spec = importlib.util.spec_from_file_location(
    "ic", "/Users/haruka/dev/lean-doc/benchmarks/tools/import-closure.py")
ic = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ic)

DOC, IR, LABEL = sys.argv[1], sys.argv[2], sys.argv[3]

SKIP = {"index.html", "404.html", "navbar.html", "search.html",
        "foundational_types.html", "references.html", "tactics.html"}

# ---- IR imports for the package's own modules
ir_imports = {}
index = json.load(open(f"{IR}/index.json"))
for e in index["modules"]:
    m = json.load(open(f"{IR}/{e['file']}"))
    ir_imports[m["module"]] = m.get("imports", [])

_imp = {}


def imports(mod):
    if mod in _imp:
        return _imp[mod]
    if mod in ir_imports:
        out = list(ir_imports[mod])
    else:
        src = ic.source_of(mod)
        out = ic.imports_of(src) if src else []
    _imp[mod] = out
    return out


def raw_closure(seed):
    seen, stack = set(), [seed]
    while stack:
        m = stack.pop()
        for n in imports(m):
            if n not in seen:
                seen.add(n)
                stack.append(n)
    return seen


INIT = raw_closure("Init") | {"Init"}
print(f"[{LABEL}] |closure(Init)| = {len(INIT)}")

_clo = {}


def closure(mod):
    if mod not in _clo:
        _clo[mod] = raw_closure(mod) | INIT
    return _clo[mod]


pages = []
for root, dirs, files in os.walk(DOC, followlinks=True):
    dirs[:] = [d for d in dirs if d not in ("find", "declarations", "src")]
    for f in files:
        if not f.endswith(".html") or f in SKIP:
            continue
        p = os.path.join(root, f)
        pages.append((os.path.relpath(p, DOC)[:-5].replace("/", "."), p))
pages.sort()
print(f"[{LABEL}] pages {len(pages)}")

HREF = re.compile(r'href="([^"]*)"')
counts = {"nav": 0, "main": 0}
out_counts = {"nav": 0, "main": 0}
out_pairs = {}
out_pages = {}


def scan(mod, clo, html, where):
    n = 0
    for h in HREF.findall(html):
        if h.startswith("http") or h.startswith("#") or h.startswith("javascript"):
            continue
        h2 = h.split("#")[0]
        if not h2.endswith(".html"):
            continue
        parts = [p for p in h2.split("/") if p not in ("..", ".", "")]
        if not parts or parts[-1] in SKIP:
            continue
        target = "/".join(parts)[:-5].replace("/", ".")
        counts[where] += 1
        if target == mod or target in clo:
            continue
        n += 1
        out_counts[where] += 1
        key = (mod, target, where, "#" in h)
        out_pairs[key] = out_pairs.get(key, 0) + 1
    return n


for mod, path in pages:
    clo = closure(mod)
    html = open(path, encoding="utf-8", errors="replace").read()
    i = html.find("<main>")
    nav, main = (html[:i], html[i:]) if i >= 0 else (html, "")
    a = scan(mod, clo, nav, "nav")
    b = scan(mod, clo, main, "main")
    if a + b:
        out_pages[mod] = (a, b)

print(f"[{LABEL}] module-page links  nav {counts['nav']}  main {counts['main']}")
print(f"[{LABEL}] outside closure    nav {out_counts['nav']}  main {out_counts['main']}")
print(f"[{LABEL}] pages with >=1 outside link  {len(out_pages)}")
frag = {}
for (m, t, w, f), c in out_pairs.items():
    frag[(w, f)] = frag.get((w, f), 0) + c
print(f"[{LABEL}] by (section, has #decl fragment): {frag}")
print(f"[{LABEL}] examples (main only):")
for (m, t, w, f), c in sorted(out_pairs.items()):
    if w == "main":
        print(f"    {m} -> {t}  frag={f} x{c}")
