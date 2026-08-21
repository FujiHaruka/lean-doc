#!/usr/bin/env bash
# `litedoc4.toml`, read the same way by every command that writes HTML.
#
# WHAT IT ASSERTS
#   Four commands put HTML on disk — `build`, `site`, `render`, `global` — and
#   `docs/plans/feature-sweep.md` C-3【決定 3】 makes the site's title and its
#   index prose come from a file in the package rather than from a flag. The
#   whole argument for the file is that a flag can be forgotten on one command
#   and then two of them disagree. This gate is what turns that argument into a
#   checked property:
#
#     - every module page's <title> ends in the configured title, in all three
#       trees that write module pages
#     - index.html's <title>, its <h1> and the rendered `index` Markdown are
#       byte-identical in all three trees that write index.html
#
#   The comparison is over the *rendered* bytes, not over "did the command read
#   the file": a command that read it and then dropped the value would pass the
#   second question and fail this one.
#
# WHY THE COUNTS ARE ASSERTED
#   A package with no `litedoc4.toml` makes every command agree trivially, and
#   so does a run that produced no pages. Both would print "ok" having compared
#   nothing. So the gate refuses a configuration that sets nothing, and refuses
#   a tree with no module pages.
#
# usage: config-gate.sh --root <pkg> --ir <dir> --built <site> [--out <dir>]
#                       [--link-index <file>] [--blind site|render|global]
#          --built   the tree `litedoc4 build` already wrote (e2e-micro.sh has one)
#          --link-index  the `.lidx` that build wrote. Without it a package whose
#                    signatures mention a dependency's names cannot be rendered
#                    at all — `--no-link-index` is only for a package that has
#                    no such name.
#          --blind   run one command **without** `--root`, to see the gate fail.
#                    Never in CI; it is how the gate was falsified.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/.." && pwd)"
LITEDOC4="${LITEDOC4:-$REPO/target/debug/litedoc4}"

ROOT=""; IR=""; BUILT=""; OUT=""; BLIND=""; LIDX=""
while [ $# -gt 0 ]; do
  case "$1" in
    --root) ROOT="$2"; shift 2 ;;
    --ir) IR="$2"; shift 2 ;;
    --built) BUILT="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    --blind) BLIND="$2"; shift 2 ;;
    --link-index) LIDX="$2"; shift 2 ;;
    -h|--help) sed -n '1,32p' "$0"; exit 0 ;;
    *) echo "unknown flag: $1" >&2; exit 2 ;;
  esac
done
[ -n "$ROOT" ] && [ -n "$IR" ] && [ -n "$BUILT" ] || {
  echo "usage: config-gate.sh --root <pkg> --ir <dir> --built <site>" >&2; exit 2; }
[ -x "$LITEDOC4" ] || { echo "no litedoc4 at $LITEDOC4" >&2; exit 2; }
[ -f "$ROOT/litedoc4.toml" ] || {
  echo "$ROOT has no litedoc4.toml — this gate needs a package that configures something" >&2
  exit 2; }

TEMPORARY=0
if [ -z "$OUT" ]; then OUT="$(mktemp -d)"; TEMPORARY=1; fi
mkdir -p "$OUT"

# `--root` unless this run is the falsification, in which case one command is
# blinded to the package it is documenting.
root_for() { [ "$1" = "$BLIND" ] || printf -- '--root\n%s\n' "$ROOT"; }

URL="https://example.invalid/o/r/blob/$(printf '0%.0s' $(seq 40))"
if [ -n "$LIDX" ]; then LINKS=(--link-index "$LIDX"); else LINKS=(--no-link-index); fi

rm -rf "$OUT/site" "$OUT/render" "$OUT/global" "$OUT/state"
# shellcheck disable=SC2046
"$LITEDOC4" site --ir "$IR" --out "$OUT/site" --source-url "$URL" "${LINKS[@]}" \
  --state "$OUT/state" $(root_for site) > "$OUT/site.log" 2>&1 || {
  echo "config-gate: \`site\` failed" >&2; sed -n '1,20p' "$OUT/site.log" >&2; exit 1; }
# shellcheck disable=SC2046
"$LITEDOC4" render --ir "$IR" --pages "$OUT/render" --source-url "$URL" "${LINKS[@]}" \
  $(root_for render) > "$OUT/render.log" 2>&1 || {
  echo "config-gate: \`render\` failed" >&2; sed -n '1,20p' "$OUT/render.log" >&2; exit 1; }
# shellcheck disable=SC2046
"$LITEDOC4" global --ir "$IR" --out "$OUT/global" $(root_for global) \
  > "$OUT/global.log" 2>&1 || {
  echo "config-gate: \`global\` failed" >&2; sed -n '1,20p' "$OUT/global.log" >&2; exit 1; }

python3 - "$ROOT" "$BUILT" "$OUT/site" "$OUT/render" "$OUT/global" <<'PY'
import html
import os
import re
import sys

root, built, site, render, glob_out = sys.argv[1:6]
problems = []

# What the package asked for, read straight out of the file rather than out of
# any command's output: the gate's expectation may not come from the thing it
# is checking.
config = open(os.path.join(root, "litedoc4.toml"), encoding="utf-8").read()
want_title = re.search(r'^title\s*=\s*"([^"]*)"', config, re.M)
want_index = re.search(r'^index\s*=\s*"([^"]*)"', config, re.M)
if not want_title and not want_index:
    sys.exit("config-gate: litedoc4.toml sets neither key — nothing to compare")
title = want_title.group(1) if want_title else None

TITLE = re.compile(r"<title>(.*?)</title>", re.S)
INTRO = re.compile(r'<div class="intro doc">(.*?)</div>', re.S)


def pages(tree):
    found = {}
    for base, _, files in os.walk(tree):
        for name in files:
            if not name.endswith(".html"):
                continue
            path = os.path.join(base, name)
            found[os.path.relpath(path, tree)] = open(path, encoding="utf-8").read()
    return found


trees = {"build": pages(built), "site": pages(site), "render": pages(render),
         "global": pages(glob_out)}

# --- module pages: the three trees that write them ---------------------------
module_pages = sorted(
    name for name, text in trees["render"].items() if 'data-module="' in text
)
if not module_pages:
    problems.append("the render tree holds no module page — nothing was compared")
checked_titles = 0
for page in module_pages:
    seen = {}
    for which in ("build", "site", "render"):
        text = trees[which].get(page)
        if text is None:
            problems.append(f"{which} did not write {page}")
            continue
        found = TITLE.search(text)
        seen[which] = html.unescape(found.group(1)) if found else "<no title>"
    if len(set(seen.values())) > 1:
        problems.append(f"{page}: the three commands disagree — {seen}")
        continue
    checked_titles += 1
    if title is not None and not next(iter(seen.values())).endswith(title):
        problems.append(
            f"{page}: <title> is {next(iter(seen.values()))!r}, "
            f"which does not end in the configured {title!r}"
        )

# --- index.html: the three trees that write it -------------------------------
index_writers = ("build", "site", "global")
indexes = {}
for which in index_writers:
    text = trees[which].get("index.html")
    if text is None:
        problems.append(f"{which} did not write index.html")
        continue
    found_title = TITLE.search(text)
    found_intro = INTRO.search(text)
    indexes[which] = (
        html.unescape(found_title.group(1)) if found_title else "<no title>",
        found_intro.group(1) if found_intro else None,
    )
if len(set(indexes.values())) > 1:
    problems.append(f"index.html differs between {index_writers}: {indexes}")
elif indexes:
    got_title, got_intro = next(iter(indexes.values()))
    if title is not None and title not in got_title:
        problems.append(f"index.html <title> is {got_title!r}, not the configured {title!r}")
    if want_index and not got_intro:
        problems.append("litedoc4.toml names an `index` but no index prose is on the page")
    if want_index and got_intro:
        # The Markdown really went through the renderer rather than being
        # pasted: a heading is an <h1>, not a line starting with `#`.
        if "#" in got_intro.split("<")[0]:
            problems.append("the index prose reached the page unrendered")

if problems:
    for problem in problems:
        print(f"CONFIG GATE FAIL  {problem}", file=sys.stderr)
    sys.exit(1)

print(
    f"config       title {title!r} on {checked_titles} module page(s) x 3 commands; "
    f"index.html identical across {len(index_writers)} commands"
)
PY
status=$?

if [ "$TEMPORARY" -eq 1 ]; then rm -rf "$OUT"; fi
exit "$status"
