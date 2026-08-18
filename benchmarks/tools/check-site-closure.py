#!/usr/bin/env python3
"""Does the site close over itself? — the gate that replaced byte reproduction.

Gate A compared every page with doc-gen4's own tree, and it ended at M8: the UI
is ours now, so no third party knows what these bytes should be. What survives
the loss of an external oracle is what the tree can be asked about *itself*, and
that is this check. Nothing here needs the network, doc-gen4, or the corpus.

Seven questions, each printed with its 母数 so that a passing run says how much
it looked at rather than only that it was happy:

  1. modules.json      every module names a page that exists
  2. search-index      every declaration's module and kind subscript resolves
  3. search-index      every declaration is an anchor on its own module's page
  4. pages             every `class="decl"` anchor is in the search index
  4b. search-index     is the same fact as `declarations/name-map.json`
  5. instances         every instance name is a declaration the index knows
  6. instancesFor      every key and every value is a declaration
  7. resources         no <script src> / <link href> points at another host

(2) used to be "the search index's module array names pages that exist", back
when it had one. `docs/plans/search-v2.md` P0 took it out — there is one module
array, in `modules.json`, and a declaration names its module by subscript into
it. So the question became whether those subscripts land, which is the same
failure (a result row linking to a page nobody wrote) caught one file earlier.

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


def read_search_index(site, problems):
    """`search-index.bin`, decoded — a third implementation on purpose.

    The site's own reader is `assets/app.js` and the writer is
    `crates/litedoc4-global/src/search_index.rs`. A checker that imported
    either would agree with it about a format both had got wrong, so this
    reads the bytes itself. The layout is documented in the writer.
    """
    path = os.path.join(site, "search-index.bin")
    if not os.path.isfile(path):
        problems.append("search-index.bin: missing — the site is not complete")
        return None
    with open(path, "rb") as handle:
        data = handle.read()

    def u32(at):
        return int.from_bytes(data[at : at + 4], "little")

    if len(data) < 52 or data[0:4] != b"LD4S" or u32(4) != 2:
        problems.append("search-index.bin: not a version 2 index")
        return None
    count = u32(8)
    names_off, restart_off, labels_off = u32(16), u32(24), u32(28)
    kind_of_off, module_off = u32(36), u32(40)

    names = []
    at = names_off
    previous = b""
    try:
        for _ in range(count):
            shared = data[at]
            at += 1
            length = data[at]
            if length == 255:
                length = int.from_bytes(data[at + 1 : at + 3], "little")
                at += 3
            else:
                at += 1
            previous = previous[:shared] + data[at : at + length]
            at += length
            names.append(previous.decode("utf-8"))
        labels = []
        at = labels_off + 4
        for _ in range(u32(labels_off)):
            length = data[at]
            labels.append(data[at + 1 : at + 1 + length].decode("utf-8"))
            at += 1 + length
    except (IndexError, UnicodeDecodeError) as error:
        problems.append("search-index.bin: truncated or corrupt ({})".format(error))
        return None
    if len(names) != count:
        problems.append("search-index.bin: {} names for a count of {}".format(len(names), count))
        return None

    kinds = [data[kind_of_off + i] for i in range(count)]
    modules = [
        int.from_bytes(data[module_off + i * 2 : module_off + i * 2 + 2], "little")
        for i in range(count)
    ]
    del restart_off  # only a reader that seeks needs it
    return {"names": names, "labels": labels, "kind_of": kinds, "modules": modules}


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
    search_index = read_search_index(site, problems)
    instance_maps = load_json(site, "instances.json", problems)
    name_map = load_json(site, "declarations/name-map.json", problems)
    # The one module array. Both of the other files point into it.
    modules = (modules_json or {}).get("modules") or []

    def fail(check, missing, total, note=""):
        counts[check] = {"checked": total, "failed": len(missing)}
        if not missing:
            return
        head = ", ".join(str(m) for m in sorted(missing)[: args.show])
        more = f" (+{len(missing) - args.show} more)" if len(missing) > args.show else ""
        problems.append(f"{check}: {len(missing)}/{total} failed — {head}{more}{note}")

    # 1 — a module index that names a page nobody wrote.
    if modules_json:
        missing = {
            entry.get("p")
            for entry in modules
            if not os.path.isfile(os.path.join(site, entry.get("p", "")))
        }
        fail("modules.json: module pages exist", missing, len(modules))

    if search_index:
        decls = list(zip(search_index["names"], search_index["kind_of"], search_index["modules"]))
        names = set(search_index["names"])

        # 2 — every declaration's module subscript lands in the module array
        # that lives in the other file.
        out_of_range = {
            f"{name} (module index {module})" for name, _, module in decls if module >= len(modules)
        }
        fail("search-index: module subscripts resolve", out_of_range, len(decls))

        # 2b — and the kind subscript lands in the vocabulary the same file
        # carries, because a badge is what a reader trusts a result row for.
        labels = search_index["labels"]
        bad_kind = {f"{name} (kind {kind})" for name, kind, _ in decls if kind >= len(labels)}
        fail("search-index: kind subscripts resolve", bad_kind, len(decls))

        # 3 — every indexed declaration is an anchor on the page the index sends
        # a reader to. Anchors, not `class="decl"` anchors: a member is a real
        # destination and is not wrapped in a section of its own.
        missing = set()
        for name, _, module_index in decls:
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

    # 4b — the index and the name map are two serialisations of one fact, and
    # the binary one is the only place the names are not readable text. This is
    # what would catch an encoder that dropped, reordered or truncated a name:
    # every own-package name in the map is in the index under the same module,
    # and nothing else is.
    if search_index and name_map and modules_json:
        own = {entry.get("n") for entry in modules}
        expected = {name: module for name, module in name_map.items() if module in own}
        indexed = {
            name: modules[module].get("n")
            for name, _, module in decls
            if module < len(modules)
        }
        disagree = {
            f"{name}: index says {indexed.get(name)}, the map says {module}"
            for name, module in expected.items()
            if indexed.get(name) != module
        }
        extra = {f"{name} (not in the map)" for name in indexed if name not in expected}
        fail("search-index == name-map", disagree | extra, len(expected))

    # 5 / 6 — the instance tables are name references too. They moved to their
    # own file in P0, but they still refer to declarations the search index has
    # to know, so this needs both files and says so if either is missing.
    if search_index and instance_maps:
        names = set(search_index["names"])
        instances = instance_maps.get("instances") or {}
        values = [name for group in instances.values() for name in group]
        fail(
            "instances -> declarations",
            {name for name in values if name not in names},
            len(values),
        )

        # Only the values. A key is the *type* an instance is for, and that type
        # is very often not this package's — `instance : Greet Nat` is keyed by
        # `Nat`, which lives in Lean core. The instance itself always is ours.
        instances_for = instance_maps.get("instancesFor") or {}
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
