#!/usr/bin/env bash
# Compare two sets of whole-package artifacts byte for byte and say what differs.
#
# Six files, not 432, so this prints every one of them with its size rather than
# a summary: a percentage is not useful while porting, and with six files there
# is no reason to summarise at all.
#
# usage: tools/global-compare.sh REFERENCE_DIR CANDIDATE_DIR
#
# The candidate comes from the Rust port; the whole loop is
#   tools/global-reference.sh
#   cargo build --release -p lean-doc && ./target/release/lean-doc global \
#     --ir /private/tmp/lean-doc-relay/w7h/base-ir --out /tmp/rust-global
#   tools/global-compare.sh /private/tmp/lean-doc-relay/m2/ref-global /tmp/rust-global
# `cargo test -p lean-doc-global --test global` makes the same comparison in
# process when the reference tree is on the machine.

set -uo pipefail

REF="${1-}"
CAND="${2-}"

[ -n "$REF" ] && [ -n "$CAND" ] || { echo "usage: $0 REFERENCE_DIR CANDIDATE_DIR" >&2; exit 2; }
[ -d "$REF" ] || { echo "no such directory: $REF" >&2; exit 1; }
[ -d "$CAND" ] || { echo "no such directory: $CAND" >&2; exit 1; }

ARTIFACTS=(
  declarations/declaration-data.bmp
  declarations/name-map.json
  navbar.html
  tactics.html
  references.bib
  references.html
)

status=0
for f in "${ARTIFACTS[@]}"; do
  if [ ! -f "$REF/$f" ]; then
    printf '%-36s MISSING in reference\n' "$f"; status=1; continue
  fi
  if [ ! -f "$CAND/$f" ]; then
    printf '%-36s MISSING in candidate\n' "$f"; status=1; continue
  fi
  a=$(wc -c < "$REF/$f" | tr -d ' ')
  b=$(wc -c < "$CAND/$f" | tr -d ' ')
  if cmp -s "$REF/$f" "$CAND/$f"; then
    printf '%-36s identical  %s B\n' "$f" "$a"
  else
    # /usr/bin/cmp, and /usr/bin/diff elsewhere: `diff` is aliased to colordiff
    # in this shell and colordiff is not installed.
    printf '%-36s DIFFERS    reference %s B, candidate %s B\n' "$f" "$a" "$b"
    printf '    %s\n' "$(cmp "$REF/$f" "$CAND/$f" 2>&1 | head -1)"
    status=1
  fi
done

# Anything the candidate wrote that the six do not name is a surprise worth
# hearing about: the site's byte-reproduction denominator is 439 exactly.
extra=$( (cd "$CAND" && find . -type f | sed 's|^\./||' | sort) \
  | grep -vxF -f <(printf '%s\n' "${ARTIFACTS[@]}") || true )
if [ -n "$extra" ]; then
  echo
  echo "--- files in the candidate that are not one of the six"
  printf '%s\n' "$extra"
  status=1
fi

echo
if [ "$status" -eq 0 ]; then echo "IDENTICAL"; else echo "DIFFERENT"; fi
exit "$status"
