#!/bin/bash
# Build an offline oracle for M7: (declaration name -> version-pinned source URL)
# straight out of doc-gen4's own reference tree.
#
# doc-gen4 already emits, on each declaration, the blob URL M7 wants to link to:
#   <div class="decl" id="LE.ext"><div class="theorem"><div class="gh_link">
#     <a href="https://github.com/…/blob/<rev>/Mathlib/Order/Basic.lean#L67-L67">
# so the whole dependency closure can be checked without touching the network.
#
# Usage: benchmarks/tools/extract-decl-source-urls.sh [out.tsv]
# Output: one `<name>\t<url>` per line, sorted.
#
# **What the counts below do and do not say.** They compare `decl` divs that
# carry a source link against `decl` divs in total, so they can only report on
# declarations doc-gen4 wrote a div for. They are silent about declarations it
# rendered inside a parent's div (structure fields and constructors) and about
# declarations it left off a page altogether — both exist【実測 M7-a】. So this
# is not a coverage number for the oracle; it is a check that no div was mined
# without its link. Do not quote it as "every declaration has a source link".
set -u

TARGET=${TARGET:-/Users/haruka/dev/lean-projects}
TREE=${TREE:-$TARGET/.lake/build/doc}
OUT=${1:-/tmp/decl-source-urls.tsv}

[ -d "$TREE" ] || { echo "no doc-gen4 reference tree at $TREE" >&2; exit 1; }

pages=$(find "$TREE" -name '*.html' | wc -l | tr -d ' ')
echo "pages: $pages"

# `decl` divs that carry a gh_link, and `decl` divs in total — the difference is
# the set with no source location.
rg --no-filename -o \
   '<div class="decl" id="[^"]+"><div class="[^"]*"><div class="gh_link"><a href="[^"]+"' \
   "$TREE" -g '*.html' \
  | sed -E 's|<div class="decl" id="([^"]+)"><div class="[^"]*"><div class="gh_link"><a href="([^"]+)"|\1\t\2|' \
  | sort -u > "$OUT"

linked=$(wc -l < "$OUT" | tr -d ' ')
total=$(rg --no-filename -o -c '<div class="decl" id="[^"]+"' "$TREE" -g '*.html' \
        | awk '{s+=$1} END {print s}')

echo "declarations with a source link: $linked"
echo "declaration divs in total       : $total"
echo "out: $OUT"
