#!/usr/bin/env python3
"""Sample the version-pinned GitHub URLs a site writes, and ask GitHub whether
they exist.

M7-b closed the *offline* oracle question: for the roots doc-gen4's reference
tree also documents, litedoc4's URLs match it byte for byte. That left the roots
the reference tree never had a page for -- `Archive`, `Counterexamples`, `Cli`,
`MD4Lean`, `UnicodeBasic` and the rest -- with **no offline way to be checked at
all**. They are not unverifiable, though: they are URLs, and the server that
serves them will say whether they resolve.

What this can and cannot judge:

- it CAN say the file exists at that revision (a wrong `rev`, a wrong path
  prefix, or a wrong repository is a 404 here)
- it CANNOT say the **line anchor** is right: GitHub serves the page for
  `#L1-L1` and for `#L99999-L99999` alike, and the fragment never reaches the
  server

Run with `--negative` first. It appends a component that cannot exist to every
sampled path and expects a 404 from each one; if that pass reports 200s, the
checker is not reaching GitHub and its green means nothing.

Two inputs, two different questions:

- `--links <links.json>` (from `litedoc4 links --out`) asks about **the map**:
  every root the resolver produced, including the ones this target never links
  to. This is the one that reaches the roots with no offline oracle.
- `--site <dir>` asks about **the output**: only the URLs a page actually
  carries. On the measurement target that is 3 repositories out of 9 packages,
  because a link appears only where a declaration is referenced.

usage: check-external-links.py (--links FILE | --site DIR) [--per-root N]
                               [--json FILE] [--negative] [--timeout S]
"""

import argparse
import json
import re
import sys
import urllib.error
import urllib.request
from collections import defaultdict
from pathlib import Path

# https://github.com/<owner>/<repo>/blob/<rev>/<path>
BLOB = re.compile(
    r"https://github\.com/([A-Za-z0-9_.-]+)/([A-Za-z0-9_.-]+)/blob/([A-Za-z0-9_.-]+)/([^\"'#\s]+)"
)

UA = "litedoc4-external-link-check (+https://github.com/FujiHaruka/litedoc4)"


def root_of(path):
    """The Lean root a blob path belongs to.

    Not the first segment: core lives under `src/` and Lake under `src/lake/`,
    so the module root is the first segment that starts with an upper-case
    letter (`src/Init/Prelude.lean` -> `Init`, `src/lake/Lake/Load.lean` ->
    `Lake`). Falls back to the first segment when nothing is capitalised.
    """
    parts = path.split("/")
    for part in parts:
        if part[:1].isupper():
            return part.split(".")[0]
    return parts[0]


def collect(site):
    """root -> {url -> count}, over every .html under the site."""
    found = defaultdict(lambda: defaultdict(int))
    pages = 0
    for page in sorted(Path(site).rglob("*.html")):
        pages += 1
        text = page.read_text(encoding="utf-8", errors="replace")
        for owner, repo, rev, path in BLOB.findall(text):
            url = "https://github.com/%s/%s/blob/%s/%s" % (owner, repo, rev, path)
            found[root_of(path)][url] += 1
    return found, pages


def from_map(path):
    """root -> {url -> 1}, from `litedoc4 links --out`.

    A root whose `url` is null carries no version-pinned URL by design (the
    resolver knew the package and could not pin it), so there is nothing to ask
    GitHub about; it is counted and skipped rather than reported as a failure.
    """
    data = json.loads(Path(path).read_text(encoding="utf-8"))
    found = defaultdict(lambda: defaultdict(int))
    for row in data.get("rows", []):
        # Both samples when the map was dumped with a link index: the root
        # module's file, and a module below it. Only the second one exercises
        # the dot-to-slash path building, so a run that checks the first alone
        # is weaker than its 200s look.
        for key in ("url", "moduleUrl"):
            if row.get(key):
                found[row["root"]][row[key]] = 1
    return found, len(data.get("rows", []))


def status_of(url, timeout):
    req = urllib.request.Request(url, method="HEAD", headers={"User-Agent": UA})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return resp.status
    except urllib.error.HTTPError as err:
        return err.code
    except Exception as err:  # noqa: BLE001 - the report names the failure
        return "ERR %s" % type(err).__name__


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--site")
    ap.add_argument("--links")
    ap.add_argument("--per-root", type=int, default=1)
    ap.add_argument("--json")
    ap.add_argument("--negative", action="store_true")
    ap.add_argument("--timeout", type=float, default=20.0)
    args = ap.parse_args()

    if bool(args.site) == bool(args.links):
        print("pass exactly one of --links <file> and --site <dir>")
        return 2

    if args.links:
        source = args.links
        found, pages = from_map(args.links)
    else:
        source = args.site
        found, pages = collect(args.site)
    if not found:
        print("no version-pinned github blob URL in %s" % source)
        return 1

    want = 404 if args.negative else 200
    unit = "map row" if args.links else "page"
    print("== %d root(s) over %d %s(s), %d URL(s) sampled per root"
          % (len(found), pages, unit, args.per_root))
    print("%-22s %7s %7s  %s" % ("root", "urls", "status", "sampled url"))

    checked = 0
    ok = 0
    bad = []
    rows = []
    for root in sorted(found):
        urls = sorted(found[root])
        for url in urls[: args.per_root]:
            probe = url + "/does-not-exist-" + root if args.negative else url
            code = status_of(probe, args.timeout)
            checked += 1
            hit = code == want
            if hit:
                ok += 1
            else:
                bad.append((root, probe, code))
            print("%-22s %7d %7s  %s" % (root, len(found[root]), code, probe))
            rows.append({"root": root, "urls": len(found[root]),
                         "sampled": probe, "status": code, "ok": hit})

    if args.json:
        Path(args.json).write_text(json.dumps(
            {"source": source, "pages": pages, "roots": len(found),
             "checked": checked, "ok": ok, "expected": want, "rows": rows},
            indent=2) + "\n", encoding="utf-8")

    print()
    for root, url, code in bad:
        print("  %s: expected %s, got %s -- %s" % (root, want, code, url))
    label = "NEGATIVE" if args.negative else "EXTERNAL LINKS"
    print("%s: %d root(s), %d checked, %d as expected (%s), %d not"
          % (label, len(found), checked, ok, want, len(bad)))
    return 0 if not bad else 1


if __name__ == "__main__":
    sys.exit(main())
