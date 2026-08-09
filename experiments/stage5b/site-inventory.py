#!/usr/bin/env python3
"""E5: what does doc-gen4 put on the site that `render.ts` does not write?

Read-only over `/Users/haruka/dev/lean-projects/.lake/build/doc` and over the
page tree `experiments/stage4c/render.ts` produced. Nothing is written into the
measurement target.

"Module page" is defined the way doc-gen4 defines it when it rescans its own
output (`DocGen4/Output.lean:322-351 scanModuleHtmlFiles`): every `.html` under
the doc root except seven names, skipping three directories.

Each non-module artifact is classified by what it is a function of:
  a  every module        rebuilding it needs data from all modules
  b  some modules        one file per module, or a fixed subset
  c  doc-gen4 version    `include_str` of a file in doc-gen4's `static/`
The classification is keyed to the doc-gen4 source line that produces the file.

usage:
  site-inventory.py --doc <doc-gen4 doc dir> --pages <render.ts page dir>
                    [--package InformationTheory]
"""
import argparse
import os
import re

# file -> (class, doc-gen4 source line, what it is a function of)
CLASSIFY = {
    "style.css": ("c", "Output.lean:62 / Base.lean:212", "include_str static/style.css"),
    "favicon.svg": ("c", "Output.lean:63 / Base.lean:213", "include_str static/favicon.svg"),
    "declaration-data.js": ("c", "Output.lean:64 / Base.lean:214", "include_str static/declaration-data.js"),
    "color-scheme.js": ("c", "Output.lean:65", "include_str static/color-scheme.js"),
    "nav.js": ("c", "Output.lean:66", "include_str static/nav.js"),
    "jump-src.js": ("c", "Output.lean:67", "include_str static/jump-src.js"),
    "expand-nav.js": ("c", "Output.lean:68", "include_str static/expand-nav.js"),
    "how-about.js": ("c", "Output.lean:69", "include_str static/how-about.js"),
    "search.js": ("c", "Output.lean:71", "include_str static/search.js"),
    "mathjax-config.js": ("c", "Output.lean:72", "include_str static/mathjax-config.js"),
    "instances.js": ("c", "Output.lean:73", "include_str static/instances.js"),
    "importedBy.js": ("c", "Output.lean:74", "include_str static/importedBy.js"),
    "find/find.js": ("c", "Output.lean:88 / Base.lean:223", "include_str static/find/find.js"),
    "search.html": ("c", "Output.lean:70 / Search.lean", "template only (navbar is an iframe)"),
    "index.html": ("c", "Output.lean:75 / Index.lean:13-21", "template + Lean version"),
    "foundational_types.html": ("c", "Output.lean:76 / FoundationalTypes.lean", "template only"),
    "404.html": ("c", "Output.lean:77 / NotFound.lean", "template only"),
    "find/index.html": ("c", "Output.lean:87 / Find.lean", "template only"),
    "navbar.html": ("a", "Output.lean:78 / Navbar.lean; rebuilt by Output.lean:357-383",
                    "Hierarchy over EVERY module name"),
    "references.html": ("a", "Output.lean:59,79 / Output.lean:28-42 collectBackrefs",
                        "every declarations/backrefs-<module>.json"),
    "tactics.html": ("a", "Output.lean:60,80 / Tactics.lean", "tacticInfo gathered from every module"),
    "references.bib": ("a", "user input read by References.lean", "the project's bibliography"),
    "declarations/declaration-data.bmp": ("a", "Output.lean:227-272 htmlOutputIndex",
                                          "every module's JsonModule (merged search index)"),
    "declarations/header-data.bmp": ("a", "Output.lean:276-295 headerDataOutput",
                                     "every declaration-data-<module>.bmp"),
}
PER_MODULE = {
    "declarations/declaration-data-<module>.bmp": ("b", "Output.lean:179", "one per module"),
    "declarations/backrefs-<module>.json": ("b", "Output.lean:169", "one per module"),
}
SKIP_FILES = {"index.html", "404.html", "navbar.html", "search.html",
              "foundational_types.html", "references.html", "tactics.html"}
SKIP_DIRS = {"find", "declarations", "src"}


def walk(root):
    for dirpath, dirnames, filenames in os.walk(root):
        rel = os.path.relpath(dirpath, root)
        if rel == ".":
            rel = ""
        for f in filenames:
            yield os.path.join(rel, f) if rel else f


def is_module_page(rel):
    if not rel.endswith(".html"):
        return False
    parts = rel.split(os.sep)
    if parts[0] in SKIP_DIRS:
        return False
    return parts[-1] not in SKIP_FILES


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--doc", required=True)
    ap.add_argument("--pages", required=True)
    ap.add_argument("--package", default="InformationTheory")
    args = ap.parse_args()

    files = sorted(walk(args.doc))
    mod_pages = [f for f in files if is_module_page(f)]
    others = [f for f in files if not is_module_page(f)]
    size = lambda f: os.path.getsize(os.path.join(args.doc, f))

    print("=== 1. doc-gen4's output tree ===")
    print(f"root                        {args.doc}")
    print(f"files total                 {len(files)}   {sum(size(f) for f in files):,} B")
    print(f"  module pages              {len(mod_pages)}   {sum(size(f) for f in mod_pages):,} B")
    print(f"  everything else           {len(others)}   {sum(size(f) for f in others):,} B")
    own = [f for f in mod_pages if f.split(os.sep)[0] == args.package or f == args.package + ".html"]
    print(f"  of the module pages, {args.package}: {len(own)}")

    print()
    print("=== 2. the non-module artifacts, classified ===")
    print(f"{'class':>5}  {'bytes':>12}  file / doc-gen4 source line / function of")
    by_class = {"a": [], "b": [], "c": [], "?": []}
    for f in others:
        key = f.replace(os.sep, "/")
        cls, src, fn = CLASSIFY.get(key, ("?", "-", "unclassified"))
        by_class[cls].append((f, size(f), src, fn))
    for cls in ("a", "b", "c", "?"):
        for f, b, src, fn in sorted(by_class[cls], key=lambda x: -x[1]):
            print(f"{cls:>5}  {b:>12,}  {f}\n{'':>21}{src}  |  {fn}")
    print()
    for cls, label in (("a", "every module"), ("b", "some modules"), ("c", "doc-gen4 version")):
        n = len(by_class[cls])
        print(f"  class {cls} ({label:16s}): {n} files, {sum(x[1] for x in by_class[cls]):,} B")
    if by_class["?"]:
        print(f"  unclassified: {[x[0] for x in by_class['?']]}")
    print()
    print("  per-module artifacts named in the source but absent from this tree:")
    for k, (cls, src, fn) in PER_MODULE.items():
        print(f"    class {cls}  {k}  ({src}, {fn})")

    print()
    print("=== 3. what render.ts writes ===")
    ours = sorted(walk(args.pages))
    print(f"root                        {args.pages}")
    print(f"files                       {len(ours)}   "
          f"{sum(os.path.getsize(os.path.join(args.pages, f)) for f in ours):,} B")
    print(f"  module pages              {sum(1 for f in ours if is_module_page(f))}")
    print(f"  anything else             {sum(1 for f in ours if not is_module_page(f))}")
    missing = [f for f in others]
    print(f"non-module artifacts render.ts does not write: {len(missing)} / {len(others)}")

    print()
    print("=== 4. links our pages emit that resolve to nothing in our own tree ===")
    pat = re.compile(r'(?:href|src)="([^"]+)"')
    broken = {}
    pages = [f for f in ours if f.endswith(".html")]
    for rel in pages:
        base = os.path.dirname(os.path.join(args.pages, rel))
        for m in pat.finditer(open(os.path.join(args.pages, rel), encoding="utf-8").read()):
            u = m.group(1)
            if u.startswith(("http://", "https://", "#", "javascript:", "data:")):
                continue
            target = u.split("#")[0].split("?")[0]
            if not target:
                continue
            p = os.path.normpath(os.path.join(base, target))
            if not os.path.exists(p):
                key = os.path.relpath(p, args.pages)
                d = broken.setdefault(key, [0, set()])
                d[0] += 1
                d[1].add(rel)
    tot = sum(v[0] for v in broken.values())
    print(f"distinct missing targets    {len(broken)}")
    print(f"broken link occurrences     {tot}")
    print(f"{'occurrences':>12} {'pages':>7}  target")
    for k, v in sorted(broken.items(), key=lambda x: -x[1][0])[:40]:
        print(f"{v[0]:>12} {len(v[1]):>7}  {k}")

    print()
    print("=== 5. the same broken links, split by why they are missing ===")
    buckets = {"own package": [], "site artifact (class a/c)": [], "dependency module page": []}
    doc_others = {f.replace(os.sep, "/") for f in others}
    for k, v in broken.items():
        key = k.replace(os.sep, "/")
        top = key.split("/")[0]
        if key in doc_others or key in ("declarations/declaration-data.bmp",):
            buckets["site artifact (class a/c)"].append((key, v[0], len(v[1])))
        elif top == args.package or key == args.package + ".html":
            buckets["own package"].append((key, v[0], len(v[1])))
        else:
            buckets["dependency module page"].append((key, v[0], len(v[1])))
    for name, rows in buckets.items():
        print(f"  {name:28s} targets {len(rows):4d}  occurrences {sum(r[1] for r in rows):8,}")
    print("  site artifacts referenced but never written:")
    for key, occ, pg in sorted(buckets["site artifact (class a/c)"], key=lambda x: -x[1]):
        cls = CLASSIFY.get(key, ("?",))[0]
        print(f"    class {cls}  {occ:7,} occurrences on {pg:4d} pages  {key}")

    print()
    print("=== 6. scale of the class-a artifacts a one-module change invalidates ===")
    for key in ("declarations/declaration-data.bmp", "navbar.html", "tactics.html",
                "references.html"):
        p = os.path.join(args.doc, key)
        if os.path.exists(p):
            print(f"  {key:40s} {os.path.getsize(p):>12,} B")
    import json as _json
    p = os.path.join(args.doc, "declarations", "declaration-data.bmp")
    if os.path.exists(p):
        idx = _json.load(open(p, encoding="utf-8"))
        for k in ("modules", "declarations", "instances", "instancesFor"):
            if k in idx:
                print(f"  declaration-data.bmp .{k:14s} {len(idx[k]):>9,} entries")
        mods = idx.get("modules", {})
        own_mods = [m for m in mods if m == args.package or m.startswith(args.package + ".")]
        decls = idx.get("declarations", {})
        own_decls = [n for n, v in decls.items()
                     if isinstance(v, dict) and f"/{args.package}/" in v.get("docLink", "")]
        print(f"    of which {args.package}: {len(own_mods):,} modules, "
              f"{len(own_decls):,} declarations")


if __name__ == "__main__":
    main()
