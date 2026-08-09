#!/usr/bin/env python3
"""E6 step 4: after a declaration moves, where does a referring page's link
actually point — at the old module (a), the new one (b), or nowhere (c)?

For each rendered tree, over the pages of the referring modules, this counts the
`href="...#<decl>"` targets and prints one real excerpt. It also checks whether
the anchor `id="<decl>"` still exists in the page each link points at, which is
what decides whether (a) is merely outdated or actually broken.

usage:
  link-target.py --decl <Name> --from-module <M> --referrer-file <path>
                 --trees <label>=<pages dir> ...
"""
import argparse
import collections
import os
import re


def page(root, module):
    return os.path.join(root, module.replace(".", os.sep) + ".html")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--decl", required=True)
    ap.add_argument("--from-module", required=True)
    ap.add_argument("--referrer-file", required=True)
    ap.add_argument("--trees", nargs="+", required=True)
    args = ap.parse_args()

    referrers = [l.strip() for l in open(args.referrer_file) if l.strip()]
    href = re.compile(r'href="([^"]*)#' + re.escape(args.decl) + r'"')
    anchor = f'id="{args.decl}"'
    print(f"declaration           {args.decl}")
    print(f"originally declared in {args.from_module}")
    print(f"referring pages        {len(referrers)}")

    for spec in args.trees:
        label, _, root = spec.partition("=")
        targets = collections.Counter()
        pages_with = 0
        example = None
        for m in referrers:
            p = page(root, m)
            if not os.path.exists(p):
                continue
            html = open(p, encoding="utf-8").read()
            hits = href.findall(html)
            if hits:
                pages_with += 1
                for h in hits:
                    targets[h.split("/")[-1]] += 1
                if example is None:
                    example = (m, hits[0])
        print(f"\n[{label}]  pages containing a link to {args.decl}: {pages_with}/{len(referrers)}")
        for t, n in targets.most_common():
            print(f"    -> {t}   ({n} links)")
        if example:
            print(f"    e.g. {example[0]} -> {example[1]}#{args.decl}")
        # is the anchor there?
        for t in targets:
            mod = t[:-5]
            # find the page whose basename matches, anywhere in the tree
            found = None
            for dirpath, _d, files in os.walk(root):
                if t in files:
                    found = os.path.join(dirpath, t)
                    break
            ok = found is not None and anchor in open(found, encoding="utf-8").read()
            print(f"    anchor {anchor} present in {t}: {ok}")


if __name__ == "__main__":
    main()
