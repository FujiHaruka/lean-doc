#!/usr/bin/env bash
# Compare two whole sites byte for byte and say what differs.
#
# **M7-c moved dependency links, and this compares against a doc-gen4-era
# reference.** Every link into a dependency is now that package's version-pinned
# GitHub blob URL whenever the run resolves a package root (`build` always does;
# `site` and `render` do when given `--root`), so a diff here is **expected** and
# is no longer a failure of the port. Gate A is suspended, not redefined — see
# `docs/implementation-plan.md` §1.
#
# A "site" here is what full generation produces: the module pages **and** the
# six whole-package artifacts in one tree — 438 files for the target package,
# 439 once a module has been added. `render-compare.sh` looks at `*.html` only,
# so it cannot see `declaration-data.bmp`, `name-map.json` or `references.bib`;
# `global-compare.sh` looks at those six and nothing else. This compares every
# file of both kinds, which is the milestone gate's denominator.
#
# usage: tools/site-compare.sh REFERENCE_DIR CANDIDATE_DIR [--show N]
#
# **No exception list.** A comparator that carries known divergences swallows
# the second one silently (plan §5); the registered divergence is pinned in the
# Rust tests instead, where a *set* is asserted rather than skipped.
#
# The candidate comes from `litedoc4 site`; the M3-d1 gate is
#   URL=https://github.com/FujiHaruka/information-theory/blob/573793b243fb1343636088eb62d1789ab2b14cec
#   cargo build --release -p litedoc4
#   ./target/release/litedoc4 site \
#     --ir /private/tmp/lean-doc-relay/w7h/base-ir --out /tmp/rust-site \
#     --source-url $URL \
#     --link-index /private/tmp/lean-doc-relay/w7c/linkindex/link-index.lidx
#   tools/site-compare.sh /private/tmp/lean-doc-relay/m2/gate/ref-site /tmp/rust-site
# Pass $URL unquoted: a `"..."` that keeps its quotes reaches the renderer as
# part of the URL and every page then differs.
#
# The same comparator answers the other half of M3-d1 — that composing the two
# subcommands adds and drops nothing — by being pointed at a tree built with
# `render` and `global` run separately over the same IR.

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

# Every file, not just `*.html`: the six artifacts include a `.bmp`, a `.json`
# and a `.bib`, and an extension filter here would report a full site while
# comparing three quarters of one.
list() { ( cd "$1" && find . -type f | sort ); }

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
list "$REF" > "$tmp/ref.txt"
list "$CAND" > "$tmp/cand.txt"

# /usr/bin/comm and /usr/bin/cmp: `diff` is aliased to a colordiff that is not
# installed in this shell, and its exit 127 reads as "differences found".
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

# The two kinds are counted apart as well as together: a site comparison whose
# denominator is 438 is dominated by module pages, and the six artifacts can
# fail inside it without moving the percentage that gets quoted.
ARTIFACTS=(
  ./declarations/declaration-data.bmp
  ./declarations/name-map.json
  ./navbar.html
  ./tactics.html
  ./references.bib
  ./references.html
)
artifacts_bad=0
artifacts_seen=0
for f in "${ARTIFACTS[@]}"; do
  if [ -f "$REF/$f" ] && [ -f "$CAND/$f" ]; then
    artifacts_seen=$(( artifacts_seen + 1 ))
    cmp -s "$REF/$f" "$CAND/$f" || artifacts_bad=$(( artifacts_bad + 1 ))
  else
    artifacts_bad=$(( artifacts_bad + 1 ))
  fi
done

echo "reference : $n_ref files ($REF)"
echo "candidate : $n_cand files ($CAND)"
echo "identical : $n_same"
echo "differing : $n_differ"
echo "missing   : $n_missing (in reference, not in candidate)"
echo "extra     : $n_extra (in candidate, not in reference)"
echo "artifacts : $(( artifacts_seen - artifacts_bad ))/6 of the whole-package six identical"

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
  echo "--- differing (all $n_differ, with size delta and first differing byte)"
  while IFS= read -r f; do
    a=$(wc -c < "$REF/$f" | tr -d ' ')
    b=$(wc -c < "$CAND/$f" | tr -d ' ')
    where=$(cmp "$REF/$f" "$CAND/$f" 2>&1 | head -1)
    printf '%s\n    reference %s B, candidate %s B\n    %s\n' "$f" "$a" "$b" "$where"
  done < "$tmp/differ.txt"
fi

if [ "$n_differ" -eq 0 ] && [ "$n_missing" -eq 0 ] && [ "$n_extra" -eq 0 ]; then
  echo
  echo "IDENTICAL"
  exit 0
fi
exit 1
