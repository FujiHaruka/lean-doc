#!/usr/bin/env bash
# M7-a's acceptance check: every `.lidx` entry's version-pinned blob URL against
# the one doc-gen4 itself wrote for that declaration.
#
# Both halves are offline. The oracle is doc-gen4's own reference tree
# (`extract-decl-source-urls.sh`), and the candidate is the product's own code —
# this script only feeds the two files to the gate test, which calls
# `packages::external_links` and `ExternalLinks::url_for` directly rather than
# re-deriving the URL rule in shell.
#
# usage: check-lidx-urls.sh [<link-index.lidx>] [<oracle.tsv>]
#   both default to $WORK_DIR; the oracle is built if it is not there yet.
# out:   benchmarks/results/m7a-lidx-url-check.txt  (+ -env.txt)
set -u

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./env.sh
source "$HERE/env.sh"

WORK="${WORK_DIR:-/private/tmp/lean-doc-m7a}"
LIDX="${1:-$WORK/link-index.lidx}"
ORACLE="${2:-$WORK/decl-source-urls.tsv}"
OUT="$RESULTS_DIR/m7a-lidx-url-check.txt"
ENV_OUT="$RESULTS_DIR/m7a-lidx-url-check-env.txt"
LAKE="${LAKE:-$HOME/.elan/bin/lake}"

[ -f "$LIDX" ] || {
  echo "no .lidx at $LIDX — build one with:" >&2
  echo "  lean-doc extract --modules <list> --ir-dir <dir> --timings <f> \\" >&2
  echo "    --link-index $LIDX --extractor-bin extractor/build/extract \\" >&2
  echo "    --target $TARGET_REPO" >&2
  exit 1
}
mkdir -p "$(dirname "$ORACLE")"
[ -f "$ORACLE" ] || "$HERE/extract-decl-source-urls.sh" "$ORACLE" || exit 1

# The conditions, recorded the way every other measurement in this directory
# records them (CLAUDE.md「計測条件を毎回記録する」).
{
  echo "date (UTC)          : $(date -u '+%Y-%m-%d %H:%M:%S')"
  echo "host                : $(uname -sr) $(uname -m), $(sysctl -n hw.ncpu) CPU, \
$(( $(sysctl -n hw.memsize) / 1073741824 )) GB RAM"
  echo "target              : $TARGET_REPO"
  echo "target toolchain    : $(cat "$TARGET_REPO/lean-toolchain" 2>/dev/null)"
  echo "target mathlib rev  : $(python3 -c 'import json,sys
m = json.load(open(sys.argv[1]))
print(next((p["rev"] for p in m["packages"] if p["name"] == "mathlib"), "?"))' \
    "$TARGET_REPO/lake-manifest.json" 2>/dev/null)"
  echo "lean core githash   : $(cd "$TARGET_REPO" && "$LAKE" env lean --githash 2>/dev/null)"
  echo "doc-gen4 tree       : ${TREE:-$TARGET_REPO/.lake/build/doc}"
  echo "doc-gen4 tree built : $(stat -f '%Sm' "${TREE:-$TARGET_REPO/.lake/build/doc}" 2>/dev/null)"
  echo "oracle              : $ORACLE ($(wc -l < "$ORACLE" | tr -d ' ') entries, \
$(stat -f '%z' "$ORACLE") B)"
  echo "link index          : $LIDX ($(stat -f '%z' "$LIDX") B, marker $(head -1 "$LIDX"))"
  echo "extractor           : $LEAN_DOC_ROOT/extractor/build/extract \
($(stat -f '%Sm' "$LEAN_DOC_ROOT/extractor/build/extract" 2>/dev/null))"
  echo "rustc               : $(rustc --version)"
  echo "page cache          : not controlled (this check is I/O over two files, not a timing)"
} > "$ENV_OUT"
cat "$ENV_OUT"

LEAN_DOC_LINK_INDEX="$LIDX" LEAN_DOC_DECL_URLS="$ORACLE" LEAN_DOC_TARGET="$TARGET_REPO" \
  cargo test --manifest-path "$LEAN_DOC_ROOT/Cargo.toml" --release -p lean-doc \
  --bin lean-doc every_lidx_entry_matches_doc_gen4s_declaration_urls \
  -- --nocapture --exact packages::tests::every_lidx_entry_matches_doc_gen4s_declaration_urls \
  > "$OUT" 2>&1
status=$?

cat "$OUT"
echo "out: $OUT (exit $status)"
echo "env: $ENV_OUT"
exit "$status"
