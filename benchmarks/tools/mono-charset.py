#!/usr/bin/env python3
"""Regenerate the non-ASCII character set the browser gate checks the mono stack against.

`docs/plans/ui-redesign.md` 決定 2 dropped the JuliaMono web font and bet that the
system monospace stack renders what a Lean package's pages contain. The bet is
about a *character set*, and that set comes from the measurement target — not
from the e2e fixture, which is deliberately tiny. So the set is measured once
here, committed as `mono-charset.json`, and read by the gate on a runner that
has never seen the target.

Input is an HTML tree; the default is doc-gen4's own reference tree for the
target's modules, which is where the 178 in `ui-redesign.md` came from.

usage: mono-charset.py [TREE] [-o OUT]
  TREE  a directory of .html files (default: the target's InformationTheory tree)
  -o    where to write the JSON (default: mono-charset.json beside this script)
"""

import argparse
import collections
import html
import json
import os
import re
import sys

DEFAULT_TREE = "/Users/haruka/dev/lean-projects/.lake/build/doc/InformationTheory"

# Script and style bodies are not prose: a `→` inside a JS string literal is
# not a character the reader ever sees in a monospace run.
SCRIPT = re.compile(r"<(script|style)\b.*?</\1>", re.S | re.I)
TAG = re.compile(r"<[^>]*>")


def collect(tree: str) -> tuple[int, collections.Counter]:
    counts: collections.Counter = collections.Counter()
    pages = 0
    for dirpath, _, names in os.walk(tree):
        for name in sorted(names):
            if not name.endswith(".html"):
                continue
            pages += 1
            with open(os.path.join(dirpath, name), encoding="utf-8") as handle:
                text = handle.read()
            text = html.unescape(TAG.sub(" ", SCRIPT.sub(" ", text)))
            counts.update(ch for ch in text if ord(ch) > 127)
    return pages, counts


def main() -> int:
    here = os.path.dirname(os.path.abspath(__file__))
    parser = argparse.ArgumentParser()
    parser.add_argument("tree", nargs="?", default=DEFAULT_TREE)
    parser.add_argument("-o", "--out", default=os.path.join(here, "mono-charset.json"))
    args = parser.parse_args()

    if not os.path.isdir(args.tree):
        print(f"no HTML tree at {args.tree}", file=sys.stderr)
        return 2

    pages, counts = collect(args.tree)
    if not counts:
        print(f"{args.tree}: no non-ASCII character in {pages} page(s)", file=sys.stderr)
        return 1

    # Sorted by code point, not by frequency: the file is a set, and a set that
    # reorders itself when one docstring changes is a diff nobody can read.
    chars = sorted(counts)
    payload = {
        "source": os.path.abspath(args.tree),
        "pages": pages,
        "distinct": len(chars),
        "chars": "".join(chars),
        "top": [[ch, counts[ch]] for ch, _ in counts.most_common(12)],
    }
    with open(args.out, "w", encoding="utf-8") as handle:
        json.dump(payload, handle, ensure_ascii=False, indent=1)
        handle.write("\n")
    print(f"{pages} page(s) -> {len(chars)} distinct non-ASCII -> {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
