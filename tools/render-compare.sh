#!/usr/bin/env bash
# Compare two rendered page trees byte for byte and say what differs.
#
# **M7-c moved dependency links, and this compares against a doc-gen4-era
# reference.** Every link into a dependency is now that package's version-pinned
# GitHub blob URL whenever the run resolves a package root (`build` always does;
# `site` and `render` do when given `--root`), so a diff here is **expected** and
# is no longer a failure of the port. Gate A is suspended, not redefined — see
# `docs/implementation-plan.md` §1.
#
# The point is to fail loudly and specifically: which files are missing, which
# are extra, and for the ones that differ, where the first differing byte is.
# A percentage is not useful while porting — a path and an offset are.
#
# usage: tools/render-compare.sh REFERENCE_DIR CANDIDATE_DIR [--show N]
#
# This takes both trees as arguments and cares about neither's provenance, so it
# still works — but **the way M1's reference tree was built is no longer in this
# repository**. `tools/render-reference.sh` ran the frozen TypeScript prototype
# (`experiments/stage7d/render.ts`) and went with `experiments/` on 2026-08-16;
# it exists only at tag `experiments-frozen`, and the tree it wrote lived under
# /private/tmp. Point REFERENCE_DIR at whatever tree you actually want to hold
# the candidate to, and say which one it was.
#
# The candidate tree comes from the Rust renderer; the whole loop is
#   cargo build --release -p litedoc4 && ./target/release/litedoc4 render \
#     --ir /private/tmp/lean-doc-relay/w7h/base-ir --pages /tmp/rust-pages \
#     --source-url "$URL" --link-index /private/tmp/lean-doc-relay/w7c/linkindex/link-index.lidx
# `cargo test -p litedoc4-render --test pages` makes the same comparison in
# process against a committed fixture, and pins the one page the prototype and
# md4c disagree on so that a second divergence fails.

set -uo pipefail

SHOW=10
REF=""
CAND=""

while [ $# -gt 0 ]; do
  case "$1" in
    --show) SHOW="$2"; shift 2 ;;
    -*) echo "unknown argument: $1" >&2; exit 2 ;;
    *) if [ -z "$REF" ]; then REF="$1"; elif [ -z "$CAND" ]; then CAND="$1"; else
         echo "too many arguments" >&2; exit 2; fi; shift ;;
  esac
done

[ -n "$REF" ] && [ -n "$CAND" ] || { echo "usage: $0 REFERENCE_DIR CANDIDATE_DIR [--show N]" >&2; exit 2; }
[ -d "$REF" ] || { echo "no such directory: $REF" >&2; exit 1; }
[ -d "$CAND" ] || { echo "no such directory: $CAND" >&2; exit 1; }

list() { ( cd "$1" && find . -type f -name '*.html' | sort ); }

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
list "$REF" > "$tmp/ref.txt"
list "$CAND" > "$tmp/cand.txt"

# /usr/bin/diff because `diff` is aliased to colordiff in this shell and
# colordiff is not installed.
/usr/bin/comm -23 "$tmp/ref.txt" "$tmp/cand.txt" > "$tmp/missing.txt"
/usr/bin/comm -13 "$tmp/ref.txt" "$tmp/cand.txt" > "$tmp/extra.txt"
/usr/bin/comm -12 "$tmp/ref.txt" "$tmp/cand.txt" > "$tmp/common.txt"

: > "$tmp/differ.txt"
while IFS= read -r f; do
  cmp -s "$REF/$f" "$CAND/$f" || printf '%s\n' "$f" >> "$tmp/differ.txt"
done < "$tmp/common.txt"

n_ref=$(wc -l < "$tmp/ref.txt" | tr -d ' ')
n_cand=$(wc -l < "$tmp/cand.txt" | tr -d ' ')
n_missing=$(wc -l < "$tmp/missing.txt" | tr -d ' ')
n_extra=$(wc -l < "$tmp/extra.txt" | tr -d ' ')
n_common=$(wc -l < "$tmp/common.txt" | tr -d ' ')
n_differ=$(wc -l < "$tmp/differ.txt" | tr -d ' ')
n_same=$(( n_common - n_differ ))

echo "reference : $n_ref files ($REF)"
echo "candidate : $n_cand files ($CAND)"
echo "identical : $n_same"
echo "differing : $n_differ"
echo "missing   : $n_missing (in reference, not in candidate)"
echo "extra     : $n_extra (in candidate, not in reference)"

show_list() {
  local title="$1" file="$2"
  [ -s "$file" ] || return 0
  echo
  echo "--- $title (first $SHOW)"
  head -n "$SHOW" "$file"
}

show_list "missing" "$tmp/missing.txt"
show_list "extra" "$tmp/extra.txt"

if [ -s "$tmp/differ.txt" ]; then
  echo
  echo "--- differing (first $SHOW, with size delta and first differing byte)"
  head -n "$SHOW" "$tmp/differ.txt" | while IFS= read -r f; do
    a=$(wc -c < "$REF/$f" | tr -d ' ')
    b=$(wc -c < "$CAND/$f" | tr -d ' ')
    where=$(cmp "$REF/$f" "$CAND/$f" 2>&1 | head -1)
    printf '%s\n    reference %s B, candidate %s B\n    %s\n' "$f" "$a" "$b" "$where"
  done
fi

if [ "$n_differ" -eq 0 ] && [ "$n_missing" -eq 0 ] && [ "$n_extra" -eq 0 ]; then
  echo
  echo "IDENTICAL"
  exit 0
fi
exit 1
