#!/usr/bin/env python3
"""Does the site close over itself? — the gate that replaced byte reproduction.

Gate A compared every page with doc-gen4's own tree, and it ended at M8: the UI
is ours now, so no third party knows what these bytes should be. What survives
the loss of an external oracle is what the tree can be asked about *itself*, and
that is this check. Nothing here needs the network, doc-gen4, or the corpus.

Seven questions, each printed with its 母数 so that a passing run says how much
it looked at rather than only that it was happy:

  1. modules.json      every module names a page that exists
  2. search-index      every module names a page that exists
  3. search-index      every declaration is an anchor on its own module's page
  4. pages             every `class="decl"` anchor is in the search index
  5. instances         every instance name is a declaration the index knows
  6. instancesFor      every key and every value is a declaration
  7. resources         no <script src> / <link href> points at another host

(3) and (4) are deliberately the two directions of the same statement. One of
them alone is satisfied by an index that is a subset of the pages, or by pages
that are a subset of the index — and both of those are exactly how a renderer
and an index generator drift apart: the search box stops finding a declaration
that is on the page, or finds one that is not.

(7) is the M8-a gate (UI-1, "self-contained: 0 external hosts") made repeatable.
`<a href>` is *not* its subject: a link into a dependency's source is a
version-pinned GitHub blob URL by design (M7), and clicking it is not the page
loading a resource.

usage:
  check-site-closure.py <site dir> [--show N] [--json <file>]
"""

import argparse
import collections
import html
import json
import os
import posixpath
import re
import sys

# `<section class="decl" id="Micro.double">` — the element the renderer wraps a
# top-level declaration in. Members (structure fields, constructors) are `<li
# id=…>` inside their parent and are checked in direction (3) only, because an
# `<li id>` is not by itself evidence that the id is a declaration name.
DECL_ANCHOR = re.compile(r'<section\b[^>]*\bclass="decl"[^>]*\bid="([^"]*)"')
ANY_ANCHOR = re.compile(r'<[a-zA-Z][^>]*\bid="([^"]*)"')
# A resource the page loads. `<a href>` is not here on purpose (see above).
RESOURCE = re.compile(
    r'<(?:script\b[^>]*\bsrc|link\b[^>]*\bhref)="([^"]*)"', re.IGNORECASE
)
EXTERNAL = re.compile(r"^(?:[a-zA-Z][a-zA-Z0-9+.-]*:)?//")


def read(path):
    with open(path, encoding="utf-8") as handle:
        return handle.read()


def load_json(site, name, problems):
    path = os.path.join(site, name)
    if not os.path.isfile(path):
        problems.append(f"{name}: missing — the site is not complete")
        return None
    try:
        return json.loads(read(path))
    except (OSError, ValueError) as error:
        problems.append(f"{name}: unreadable ({error})")
        return None


def html_files(site):
    """Every page, as a site-relative posix path."""
    found = []
    for base, _, names in os.walk(site):
        for name in names:
            if name.endswith(".html"):
                full = os.path.join(base, name)
                found.append(os.path.relpath(full, site).replace(os.sep, "/"))
    return sorted(found)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("site")
    parser.add_argument(
        "--show", type=int, default=10, help="how many examples to print per failure"
    )
    parser.add_argument("--json", help="write the counts here")
    args = parser.parse_args()

    site = args.site
    if not os.path.isdir(site):
        print(f"not a directory: {site}", file=sys.stderr)
        return 2

    problems = []
    counts = collections.OrderedDict()

    pages = html_files(site)
    counts["pages"] = len(pages)

    # Anchors, per page and in total.
    decl_anchors = {}
    all_anchors = {}
    resources = []
    for page in pages:
        text = read(os.path.join(site, page))
        # `html.unescape`: an `id` attribute is escaped text, so the id of
        # `id="List.«term_&lt;+~_»"` is `List.«term_<+~_»` — which is what the
        # search index carries. Comparing the raw attribute bytes reported
        # **two** false failures per direction on `batteries`【実測 2026-08-17】,
        # in both directions at once, which is the signature of a comparison
        # done in the wrong alphabet rather than of a site that is inconsistent.
        # `crates/litedoc4/src/packages.rs`'s oracle undoes the same escape for
        # the same reason.
        decl_anchors[page] = {html.unescape(a) for a in DECL_ANCHOR.findall(text)}
        all_anchors[page] = {html.unescape(a) for a in ANY_ANCHOR.findall(text)}
        for url in RESOURCE.findall(text):
            if EXTERNAL.match(url):
                resources.append((page, url))

    modules_json = load_json(site, "modules.json", problems)
    search_index = load_json(site, "search-index.json", problems)

    def fail(check, missing, total, note=""):
        counts[check] = {"checked": total, "failed": len(missing)}
        if not missing:
            return
        head = ", ".join(str(m) for m in sorted(missing)[: args.show])
        more = f" (+{len(missing) - args.show} more)" if len(missing) > args.show else ""
        problems.append(f"{check}: {len(missing)}/{total} failed — {head}{more}{note}")

    # 1 / 2 — a module index that names a page nobody wrote.
    for name, doc in (("modules.json", modules_json), ("search-index", search_index)):
        if not doc:
            continue
        entries = doc.get("modules") or []
        missing = {
            entry.get("p")
            for entry in entries
            if not os.path.isfile(os.path.join(site, entry.get("p", "")))
        }
        fail(f"{name}: module pages exist", missing, len(entries))

    if search_index:
        modules = search_index.get("modules") or []
        decls = search_index.get("decls") or []
        names = {entry[0] for entry in decls}

        # 3 — every indexed declaration is an anchor on the page the index sends
        # a reader to. Anchors, not `class="decl"` anchors: a member is a real
        # destination and is not wrapped in a section of its own.
        missing = set()
        for entry in decls:
            name, _, module_index = entry[0], entry[1], entry[2]
            if module_index >= len(modules):
                missing.add(f"{name} (module index {module_index} out of range)")
                continue
            page = modules[module_index].get("p")
            if name not in all_anchors.get(page, ()):
                missing.add(f"{name} (not on {page})")
        fail("search-index -> pages", missing, len(decls))

        # 4 — the other direction. A declaration the renderer put on a page and
        # the index never heard of is a hole in the search box.
        indexed_pages = {entry.get("p") for entry in modules}
        orphans = set()
        checked = 0
        for page, anchors in decl_anchors.items():
            if page not in indexed_pages:
                continue  # entry pages (index.html, 404.html) carry no declarations
            for anchor in anchors:
                checked += 1
                if anchor not in names:
                    orphans.add(f"{anchor} (on {page})")
        fail("pages -> search-index", orphans, checked)

        # 5 / 6 — the instance tables are name references too.
        instances = search_index.get("instances") or {}
        values = [name for group in instances.values() for name in group]
        fail(
            "instances -> declarations",
            {name for name in values if name not in names},
            len(values),
        )

        # Only the values. A key is the *type* an instance is for, and that type
        # is very often not this package's — `instance : Greet Nat` is keyed by
        # `Nat`, which lives in Lean core. The instance itself always is ours.
        instances_for = search_index.get("instancesFor") or {}
        pairs = [(key, name) for key, group in instances_for.items() for name in group]
        bad = {f"{key} -> {name}" for key, name in pairs if name not in names}
        fail("instancesFor -> declarations", bad, len(pairs))

    # 7 — UI-1, repeatable.
    counts["external resources"] = {"checked": len(pages), "failed": len(resources)}
    if resources:
        head = ", ".join(f"{page}: {url}" for page, url in resources[: args.show])
        problems.append(
            f"external resources: {len(resources)} — the site is not self-contained: {head}"
        )

    print(f"=== {site}")
    for check, value in counts.items():
        if isinstance(value, dict):
            mark = "ok " if value["failed"] == 0 else "FAIL"
            print(f"  {mark} {check:<34}: {value['checked']} checked, {value['failed']} failed")
        else:
            print(f"      {check:<34}: {value}")

    if args.json:
        with open(args.json, "w", encoding="utf-8") as handle:
            json.dump(counts, handle, indent=2, ensure_ascii=False)
            handle.write("\n")

    if problems:
        print()
        for problem in problems:
            print(f"  {problem}")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
