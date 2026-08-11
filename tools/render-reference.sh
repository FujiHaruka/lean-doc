#!/usr/bin/env bash
# Regenerate the reference page tree that the Rust renderer has to reproduce
# byte for byte, using the frozen TypeScript prototype as the source of truth.
#
# The prototype is the oracle for milestone M1 because it is the thing whose
# output already scores 99.506% against doc-gen4. Comparing against it directly
# gives a byte diff that names the broken file, where coverage.ts would only
# give a percentage.
#
# Note this passes --link-index, which the prototype pipeline did NOT do by
# default. The product does (implementation plan, decision 4), so the reference
# has to as well.
#
# usage: tools/render-reference.sh [--ir DIR] [--out DIR] [--link-index FILE]
#                                  [--source-url URL]

set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RENDER_TS="$REPO/experiments/stage7d/render.ts"

IR=/private/tmp/lean-doc-relay/w7h/base-ir
OUT=/private/tmp/lean-doc-relay/m1/ref-pages
LIDX=/private/tmp/lean-doc-relay/w7c/linkindex/link-index.lidx
# 40 hex digits: coverage.ts only recognises a full SHA when it normalises the
# revision away. A tag or branch name here silently lowers the score.
URL="https://github.com/FujiHaruka/information-theory/blob/573793b243fb1343636088eb62d1789ab2b14cec"

while [ $# -gt 0 ]; do
  case "$1" in
    --ir) IR="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    --link-index) LIDX="$2"; shift 2 ;;
    --source-url) URL="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

for p in "$RENDER_TS" "$IR" "$LIDX"; do
  [ -e "$p" ] || { echo "missing: $p" >&2; exit 1; }
done

command -v deno >/dev/null || { echo "deno is required (node is broken here)" >&2; exit 1; }

rm -rf "$OUT"
mkdir -p "$(dirname "$OUT")"

echo "rendering reference pages -> $OUT"
deno run -A "$RENDER_TS" \
  --ir "$IR" --pages "$OUT" --source-url "$URL" --link-index "$LIDX"

# A manifest makes the reference verifiable later without rerunning the
# prototype, and makes an accidental edit of the tree loud.
( cd "$OUT" && find . -type f -name '*.html' | sort | xargs shasum -a 256 ) \
  > "$OUT.sha256"

printf 'pages: %s\n' "$(find "$OUT" -name '*.html' | wc -l | tr -d ' ')"
printf 'manifest: %s\n' "$OUT.sha256"
