#!/usr/bin/env python3
"""Gate UI-2: resolve every relative href a generated site emits and count the dead ones.

A link into the site is dead when the file it names is not in the tree. That is a
question the tree answers by itself — no network, no oracle — which is why this
check is offline and exact.

  href="../.././EPI/Stam/ToBridge.html"  on InformationTheory/Shannon/EPI/L3Integration.html
    -> EPI/Stam/ToBridge.html            which the run never wrote

External hrefs (`http:`, `https:`, `mailto:`), fragment-only hrefs (`#name`) and
protocol-relative ones are not this check's subject and are counted separately.
A query string and a fragment are stripped before the file is looked up, and a
href ending in `/` is read as that directory's `index.html`.

**Source paths are reported on their own** (`--paths`), because they are where
gate UI-2's dead links came from: doc-gen4 turns a docstring word like
`EPI/Stam/ToBridge.lean` into a page by reading it as relative to the repository
root, and a package whose docstrings write module-relative paths gets a link to a
page nobody wrote (`docs/plans/ui-redesign.md` §1).

Usage:
  check-dead-links.py <site dir> [<site dir> ...] [--show N] [--paths]
"""

import argparse
import collections
import html
import os
import posixpath
import re
import sys

HREF = re.compile(r'href="([^"]*)"')
ANCHOR = re.compile(r'<a href="([^"]*)">([^<]*)</a>')
EXTERNAL = ("http://", "https://", "mailto:", "//")


def read_tree(root):
    """Every file under `root`, as site-relative paths."""
    files = set()
    for dirpath, _, names in os.walk(root):
        for name in names:
            files.add(os.path.relpath(os.path.join(dirpath, name), root))
    return files


def target_of(page, href):
    """The site-relative file a href names, or None when it leaves the site."""
    if not href or href.startswith(EXTERNAL) or href.startswith("#"):
        return None
    path = href.split("#")[0].split("?")[0]
    if not path:
        return None
    resolved = posixpath.normpath(posixpath.join(posixpath.dirname(page), path))
    if path.endswith("/"):
        resolved = posixpath.join(resolved, "index.html")
    return resolved


def is_source_path(text):
    """What `nameToLink?`'s first branch takes: a `.lean` word with a `/` in it."""
    return text.endswith(".lean") and "/" in text


def scan(root):
    files = read_tree(root)
    stats = collections.Counter()
    dead = collections.Counter()
    example = {}
    dead_pages = set()
    for page in sorted(f for f in files if f.endswith(".html")):
        with open(os.path.join(root, page), encoding="utf-8") as handle:
            text = handle.read()
        for raw in HREF.findall(text):
            href = html.unescape(raw)
            if href.startswith(EXTERNAL):
                stats["external"] += 1
                continue
            target = target_of(page, href)
            if target is None:
                stats["fragment"] += 1
                continue
            stats["internal"] += 1
            if target not in files:
                dead[target] += 1
                dead_pages.add(page)
                example.setdefault(target, page)
        for raw, label in ANCHOR.findall(text):
            if not is_source_path(html.unescape(label)):
                continue
            href = html.unescape(raw)
            stats["source paths"] += 1
            if href.startswith(EXTERNAL):
                stats["source paths, external"] += 1
                continue
            target = target_of(page, href)
            if target is not None and target not in files:
                stats["source paths, dead"] += 1
    return files, stats, dead, dead_pages, example


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("sites", nargs="+")
    parser.add_argument("--show", type=int, default=12)
    parser.add_argument("--paths", action="store_true", help="list every source-path anchor")
    args = parser.parse_args()

    worst = 0
    for site in args.sites:
        files, stats, dead, dead_pages, example = scan(site)
        total = sum(dead.values())
        worst = max(worst, total)
        print(f"=== {site}")
        print(f"  files                     : {len(files)}")
        print(f"  internal links            : {stats['internal']}")
        print(f"  external links            : {stats['external']}")
        print(f"  fragment-only links       : {stats['fragment']}")
        print(f"  DEAD internal links       : {total} "
              f"({len(dead)} distinct destinations, {len(dead_pages)} pages)")
        print(f"  source-path anchors       : {stats['source paths']} "
              f"({stats['source paths, external']} external, "
              f"{stats['source paths, dead']} dead)")
        for target, count in dead.most_common(args.show):
            print(f"    {count:4d}  {target}   e.g. on {example[target]}")
        print()
    return 1 if worst else 0


if __name__ == "__main__":
    sys.exit(main())
