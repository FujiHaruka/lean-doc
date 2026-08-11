#!/usr/bin/env bash
# Regenerate the six whole-package artifacts that the Rust port has to reproduce
# byte for byte, using the frozen TypeScript prototype as the source of truth.
#
# The prototype is the oracle for milestone M2 for the same reason it is for M1:
# it is the thing whose output already scores 99.506% against doc-gen4, and a
# byte diff against it names the broken file where coverage.ts would only give a
# percentage.
#
# `stage7h/global.ts build` **without** `--state` is stage 5's from-scratch
# build, which is what M2-a ports; the cache and the map delta are M2-b.
#
# usage: tools/global-reference.sh [--ir DIR] [--out DIR]

set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GLOBAL_TS="$REPO/experiments/stage7h/global.ts"

IR=/private/tmp/lean-doc-relay/w7h/base-ir
OUT=/private/tmp/lean-doc-relay/m2/ref-global

while [ $# -gt 0 ]; do
  case "$1" in
    --ir) IR="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

for p in "$GLOBAL_TS" "$IR"; do
  [ -e "$p" ] || { echo "missing: $p" >&2; exit 1; }
done

command -v deno >/dev/null || { echo "deno is required (node is broken here)" >&2; exit 1; }

rm -rf "$OUT"
mkdir -p "$(dirname "$OUT")"

echo "building reference artifacts -> $OUT"
deno run --allow-read --allow-write "$GLOBAL_TS" build --ir "$IR" --out "$OUT"

# A manifest makes the reference verifiable later without rerunning the
# prototype, and makes an accidental edit of the tree loud.
( cd "$OUT" && find . -type f | sort | xargs shasum -a 256 ) > "$OUT.sha256"

printf 'artifacts: %s\n' "$(find "$OUT" -type f | wc -l | tr -d ' ')"
printf 'manifest: %s\n' "$OUT.sha256"
