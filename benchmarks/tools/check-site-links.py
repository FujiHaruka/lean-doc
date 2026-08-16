#!/usr/bin/env python3
"""The M7-c gate: every GitHub href a generated site emits, against doc-gen4's own tree.

M7-c makes every link *into a dependency* a version-pinned blob URL. The check
that it is the **right** URL is offline and exact, because doc-gen4's reference
tree already carries the URL on every page it wrote:

  per declaration : <div class="decl" id="X"><div class="K"><div class="gh_link"><a href="…#L7-L9">
  per module      : <p class="gh_nav_link"><a href="…/Mathlib/Order/Basic.lean">

The first is mined by `benchmarks/tools/extract-decl-source-urls.sh` into a TSV;
the second is read straight out of the tree here, keyed by the page's own path.

Usage:
  check-site-links.py --site <dir> --oracle <decl-source-urls.tsv>
                      [--tree <doc-gen4 tree>] [--source-url <own prefix>]
                      [--baseline <dir>] [--lidx <file>]

`--baseline` is a site rendered with **no** dependency map. Given, the run also
proves the two differ only inside href attributes and that nothing belonging to
the package being documented moved. `--lidx` lets the residue be named rather
than counted.

**What the buckets do and do not say.** The per-declaration oracle is silent
about declarations doc-gen4 rendered inside a *parent's* decl div — structure
fields and constructors — so an emitted URL that is not in it is not yet a
disagreement 【実測 M7-a】. Those are separated out and explained by name, and
only a URL naming a file the tree has no page for, or a name the oracle holds at
a *different* range, is counted as a mismatch.
"""

import argparse
import collections
import os
import posixpath
import re
import sys

HREF = 'href="'


def read_pages(root):
    """Every `.html` under `root`, keyed by its path relative to it."""
    pages = {}
    for directory, _, files in os.walk(root):
        for name in files:
            if not name.endswith(".html"):
                continue
            path = os.path.join(directory, name)
            key = os.path.relpath(path, root)
            with open(path, encoding="utf-8") as handle:
                pages[key] = handle.read()
    return pages


def split_hrefs(html):
    """`(the bytes between href attributes, the href values)`.

    Two documents whose first halves agree differ *only* inside hrefs, which is
    the claim M7-c has to be held to.
    """
    between, hrefs, rest = [], [], html
    while True:
        at = rest.find(HREF)
        if at < 0:
            break
        between.append(rest[:at])
        value = rest[at + len(HREF):]
        end = value.index('"')
        hrefs.append(value[:end])
        rest = value[end:]
    between.append(rest)
    return between, hrefs


def unescape(name):
    """doc-gen4 writes the name into an attribute, the `.lidx` writes it raw."""
    for entity, char in (("&lt;", "<"), ("&gt;", ">"), ("&quot;", '"'), ("&#39;", "'")):
        name = name.replace(entity, char)
    return name.replace("&amp;", "&")


def load_declaration_oracle(path):
    urls, names = set(), set()
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            name, _, url = line.rstrip("\n").partition("\t")
            if not url:
                continue
            urls.add(url)
            names.add(unescape(name))
    return urls, names


def load_module_oracle(tree):
    """module name -> the file URL doc-gen4 put in that page's `gh_nav_link`."""
    pattern = re.compile(r'<p class="gh_nav_link"><a href="(https://[^"]+)"')
    by_module, by_url = {}, {}
    for directory, _, files in os.walk(tree):
        for name in files:
            if not name.endswith(".html"):
                continue
            path = os.path.join(directory, name)
            with open(path, encoding="utf-8", errors="replace") as handle:
                found = pattern.search(handle.read())
            if not found:
                continue
            module = os.path.relpath(path, tree)[: -len(".html")].replace(os.sep, ".")
            by_module[module] = found.group(1)
            by_url[found.group(1)] = module
    return by_module, by_url


def load_lidx_ranges(path):
    """(module, from, to) -> the names the `.lidx` gives that range."""
    ranges = collections.defaultdict(list)
    current = ""
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            line = line.rstrip("\n")
            if not line:
                continue
            if line[0] == "\t":
                fields = line[1:].split("\t")
                if len(fields) >= 3:
                    ranges[(current, fields[1], fields[2])].append(fields[0])
            elif line[0] not in "@#":
                current = line
    return ranges


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--site", required=True)
    parser.add_argument("--oracle", required=True)
    parser.add_argument("--tree", default="/Users/haruka/dev/lean-projects/.lake/build/doc")
    parser.add_argument("--source-url", default="", help="the package's own /blob/<rev> prefix")
    parser.add_argument("--baseline", default="")
    parser.add_argument("--lidx", default="")
    args = parser.parse_args()

    site = read_pages(args.site)
    decl_urls, decl_names = load_declaration_oracle(args.oracle)
    module_url, url_module = load_module_oracle(args.tree)
    file_urls = {url.split("#")[0] for url in decl_urls} | set(module_url.values())
    ranges = load_lidx_ranges(args.lidx) if args.lidx else {}

    emitted = []
    for page in sorted(site):
        for href in split_hrefs(site[page])[1]:
            if href.startswith("https://github.com/"):
                emitted.append((page, href))
    own = [(p, u) for p, u in emitted if args.source_url and u.startswith(args.source_url)]
    dep = [(p, u) for p, u in emitted if not (args.source_url and u.startswith(args.source_url))]
    anchored = [(p, u) for p, u in dep if "#L" in u]
    plain = [(p, u) for p, u in dep if "#L" not in u]

    print(f"site                             : {args.site} ({len(site)} pages)")
    print(f"oracle                           : {args.oracle} "
          f"({len(decl_urls)} declaration URLs, {len(decl_names)} names)")
    print(f"                                   {args.tree} ({len(module_url)} module URLs)")
    print()
    print(f"github hrefs emitted             : {len(emitted)}")
    print(f"  the package's own source links : {len(own)}")
    print(f"  into a dependency              : {len(dep)}  "
          f"({len(set(u for _, u in dep))} distinct)")
    print(f"    with a #L anchor             : {len(anchored)}")
    print(f"    a whole file, no anchor      : {len(plain)}")
    print()

    matched = [u for _, u in anchored if u in decl_urls]
    residue = [(p, u) for p, u in anchored if u not in decl_urls]
    plain_ok = [u for _, u in plain if u in file_urls]
    plain_bad = [(p, u) for p, u in plain if u not in file_urls]

    kinds = collections.Counter()
    mismatched = []
    for page, url in residue:
        path, _, anchor = url.partition("#L")
        first, _, last = anchor.partition("-L")
        module = url_module.get(path)
        if module is None:
            kinds["MISMATCH: the reference tree has no page for this file"] += 1
            mismatched.append((page, url, "no page"))
            continue
        names = ranges.get((module, first, last), [])
        if not names and ranges:
            kinds["MISMATCH: no .lidx entry has this module and range"] += 1
            mismatched.append((page, url, "no .lidx entry"))
        elif any("." in n and n.rsplit(".", 1)[0] in decl_names for n in names):
            kinds["a field or constructor: doc-gen4 renders it inside its parent's div"] += 1
        elif any(n in decl_names for n in names):
            kinds["MISMATCH: the oracle holds this name at a different range"] += 1
            mismatched.append((page, url, f"names={names[:4]}"))
        else:
            kinds["the oracle has neither the name nor its parent"] += 1
            mismatched.append((page, url, f"names={names[:4]}"))

    print(f"anchored, matching the oracle exactly            : {len(matched)}")
    print(f"anchored, not in the oracle                      : {len(residue)} "
          f"({len(set(u for _, u in residue))} distinct)")
    for kind, count in kinds.most_common():
        print(f"    {count:7d}  {kind}")
    print(f"unanchored, the file is one doc-gen4 links too   : {len(plain_ok)}")
    print(f"unanchored, MISMATCH (file unknown to the oracle): {len(plain_bad)}")
    print()
    print(f"MISMATCHED (the milestone fails if this is not 0): "
          f"{len(mismatched) + len(plain_bad)}")
    for page, url, why in (mismatched + [(p, u, "unknown file") for p, u in plain_bad])[:20]:
        print(f"    {url}\n      on {page} — {why}")
    print()

    # Links into a dependency that stayed relative: a relative href whose target
    # page the site does not hold and whose first component is a module root the
    # dependency map resolved (read off the emitted URLs' own module paths).
    dep_roots = set()
    for _, url in dep:
        for module, murl in module_url.items():
            if url.startswith(murl.split("/blob/")[0]):
                dep_roots.add(module.split(".")[0])
    stayed = collections.Counter()
    example = {}
    for page in sorted(site):
        here = posixpath.dirname(page)
        for href in split_hrefs(site[page])[1]:
            if href.startswith("http") or ".html" not in href:
                continue
            target = posixpath.normpath(posixpath.join(here, href.split("#")[0]))
            if target in site:
                continue
            if target.split("/")[0] in dep_roots:
                stayed[target] += 1
                example.setdefault(target, (page, href))
    print(f"links into a dependency that stayed relative     : {sum(stayed.values())}")
    for target, count in stayed.most_common(20):
        page, href = example[target]
        print(f"    {count:4d}  {target}\n      e.g. {href!r} on {page}")
    print()

    if args.baseline:
        base = read_pages(args.baseline)
        assert base.keys() == site.keys(), "the two trees hold different pages"
        outside, moved, own_moved, other = [], 0, 0, collections.Counter()
        for page in sorted(base):
            before_between, before_hrefs = split_hrefs(base[page])
            after_between, after_hrefs = split_hrefs(site[page])
            if before_between != after_between or len(before_hrefs) != len(after_hrefs):
                outside.append(page)
                continue
            for was, now in zip(before_hrefs, after_hrefs):
                if was == now:
                    continue
                moved += 1
                if args.source_url and (was.startswith(args.source_url)
                                        or now.startswith(args.source_url)):
                    own_moved += 1
                if not (not was.startswith("http") and now.startswith("https://github.com/")):
                    other[f"{was} -> {now}"] += 1
        print(f"baseline                         : {args.baseline}")
        print(f"  pages whose bytes moved outside an href       : {len(outside)}")
        print(f"  hrefs that moved                              : {moved}")
        print(f"  of those, relative page link -> blob URL      : {moved - sum(other.values())}")
        print(f"  of those, anything else                       : {sum(other.values())}")
        for what, count in other.most_common(10):
            print(f"      {count:6d}  {what}")
        print(f"  the package's own source links that moved     : {own_moved}")

    failed = len(mismatched) + len(plain_bad)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
