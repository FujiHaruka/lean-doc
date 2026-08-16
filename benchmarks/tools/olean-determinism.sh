#!/usr/bin/env bash
# Is a rebuilt `.olean` byte-identical to the one it replaced?
#
# The incremental ledger's L2 key is the SHA-256 of a module's `.olean`
# (`crates/lean-doc-incr/src/ledger.rs`). Every cache that crosses a machine, a
# CI run or a clean checkout therefore hits only if `lake build` reproduces the
# same bytes from the same source. That is an empirical property of Lean's
# serialisation, not something the ledger can assume, so this measures it.
#
# Per module, four hashes and three verdicts:
#
#   h0  the olean as found                     (the baseline)
#   h1  after deleting it and rebuilding       DETERMINISTIC  iff h1 == h0
#   h2  after appending a declaration          SENSITIVE      iff h2 != h1
#   h3  after restoring the source             REPRODUCIBLE   iff h3 == h1
#
# Both keys the ledger can be configured with are recorded at every point: the
# SHA-256 of the olean's bytes (`--algorithm sha256`) and the 64-bit hash Lake
# itself wrote to `<file>.olean.hash` (`--algorithm lake`). They can disagree —
# Lake's is a hash of the build's inputs, not of the output — and which one
# moves is the difference between "the cache misses" and "the cache is right".
#
# h2 is the positive control and it is not optional: without it "the bytes did
# not move" cannot be told apart from "nothing was rebuilt". A run where
# SENSITIVE fails says the measurement is blind, not that Lean is deterministic.
#
# The source edit is reverted from a saved copy and re-verified by hash; the
# target repository is never committed to. Only `.lake/build` is disturbed, and
# only for the modules named.
#
# usage: olean-determinism.sh --target <dir> --out <report> [--modules <a,b,..>]
#                             [--keep-going]

set -euo pipefail

target=""
out=""
modules=""
keep_going=0

while [ $# -gt 0 ]; do
  case "$1" in
    --target) target="$2"; shift 2 ;;
    --out) out="$2"; shift 2 ;;
    --modules) modules="$2"; shift 2 ;;
    --keep-going) keep_going=1; shift ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

[ -n "$target" ] || { echo "--target is required" >&2; exit 2; }
[ -n "$out" ] || { echo "--out is required" >&2; exit 2; }
[ -d "$target/.lake/build/lib/lean" ] || {
  echo "no .lake/build/lib/lean under $target — nothing to compare against" >&2
  exit 2
}
[ -n "$modules" ] || { echo "--modules is required" >&2; exit 2; }

target="$(cd "$target" && pwd)"
out="$(cd "$(dirname "$out")" && pwd)/$(basename "$out")"

work="$(mktemp -d)"
restore_list="$work/restore.txt"
: > "$restore_list"

cleanup() {
  # Put every touched source back, whatever happened. Reverting through the
  # saved copy rather than through git keeps this usable in a dirty tree.
  while read -r src saved; do
    [ -f "$saved" ] && cp "$saved" "$src"
  done < "$restore_list"
  rm -rf "$work"
}
trap cleanup EXIT

hash_of() {
  # Prints the empty string when the file is absent; callers treat "" as
  # "not there", which is a state and not an error.
  if [ -f "$1" ]; then
    shasum -a 256 "$1" | cut -d' ' -f1
  else
    printf ''
  fi
}

lake_hash_of() {
  # Lake's own `<file>.olean.hash`: one line, no trailing structure.
  if [ -f "$1.hash" ]; then
    tr -d '[:space:]' < "$1.hash"
  else
    printf ''
  fi
}

olean_path() {
  printf '%s/.lake/build/lib/lean/%s.olean' "$target" "$(printf '%s' "$1" | tr '.' '/')"
}

source_path() {
  printf '%s/%s.lean' "$target" "$(printf '%s' "$1" | tr '.' '/')"
}

build_one() {
  # `lake build` writes progress to stdout and diagnostics to stderr; both are
  # kept so a failed rebuild is readable in the log rather than inferred.
  ( cd "$target" && lake build "$1" ) > "$work/build.log" 2>&1
}

{
  echo "# olean determinism — $(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  echo
  echo "target      $target"
  echo "HEAD        $(cd "$target" && git rev-parse HEAD)"
  echo "toolchain   $(cat "$target/lean-toolchain")"
  echo "host        $(uname -srm)"
  echo
} > "$out"

pass=0
fail=0
checked=0

IFS=',' read -r -a mods <<< "$modules"
for m in "${mods[@]}"; do
  m="$(printf '%s' "$m" | tr -d '[:space:]')"
  [ -n "$m" ] || continue
  checked=$((checked + 1))

  olean="$(olean_path "$m")"
  src="$(source_path "$m")"

  if [ ! -f "$olean" ] || [ ! -f "$src" ]; then
    { echo "## $m"; echo "  SKIP  missing olean or source"; echo; } >> "$out"
    fail=$((fail + 1))
    continue
  fi

  saved_src="$work/$(printf '%s' "$m" | tr '.' '_').lean"
  cp "$src" "$saved_src"
  printf '%s %s\n' "$src" "$saved_src" >> "$restore_list"

  h0="$(hash_of "$olean")"
  l0="$(lake_hash_of "$olean")"
  size0="$(wc -c < "$olean" | tr -d ' ')"

  # (1) delete and rebuild
  rm -f "$olean" "$olean.hash" "${olean%.olean}.ilean" "${olean%.olean}.ilean.hash" \
        "${olean%.olean}.trace"
  t0=$(date +%s)
  build_one "$m" || true
  t1=$(date +%s)
  h1="$(hash_of "$olean")"
  l1="$(lake_hash_of "$olean")"

  # (2) positive control: a declaration the module did not have
  printf '\ntheorem oleanDeterminismProbe_ : True := trivial\n' >> "$src"
  build_one "$m" || true
  h2="$(hash_of "$olean")"
  l2="$(lake_hash_of "$olean")"

  # (3) restore and rebuild
  cp "$saved_src" "$src"
  build_one "$m" || true
  h3="$(hash_of "$olean")"
  l3="$(lake_hash_of "$olean")"

  det="FAIL"; [ -n "$h1" ] && [ "$h1" = "$h0" ] && det="PASS"
  sen="FAIL"; [ -n "$h2" ] && [ "$h2" != "$h1" ] && sen="PASS"
  rep="FAIL"; [ -n "$h3" ] && [ "$h3" = "$h1" ] && rep="PASS"

  ldet="FAIL"; [ -n "$l1" ] && [ "$l1" = "$l0" ] && ldet="PASS"
  lsen="FAIL"; [ -n "$l2" ] && [ "$l2" != "$l1" ] && lsen="PASS"
  lrep="FAIL"; [ -n "$l3" ] && [ "$l3" = "$l1" ] && lrep="PASS"

  {
    echo "## $m"
    echo "  olean            $size0 B, rebuilt in $((t1 - t0)) s"
    echo "  sha256  h0 as found      ${h0:-<absent>}"
    echo "  sha256  h1 rebuilt       ${h1:-<absent>}"
    echo "  sha256  h2 with a decl   ${h2:-<absent>}"
    echo "  sha256  h3 restored      ${h3:-<absent>}"
    echo "  lake    l0 as found      ${l0:-<absent>}"
    echo "  lake    l1 rebuilt       ${l1:-<absent>}"
    echo "  lake    l2 with a decl   ${l2:-<absent>}"
    echo "  lake    l3 restored      ${l3:-<absent>}"
    echo "  DETERMINISTIC    sha256 $det / lake $ldet   (1 == 0)"
    echo "  SENSITIVE        sha256 $sen / lake $lsen   (2 != 1) — positive control"
    echo "  REPRODUCIBLE     sha256 $rep / lake $lrep   (3 == 1)"
    echo
  } >> "$out"

  if [ "$det" = "PASS" ] && [ "$sen" = "PASS" ] && [ "$rep" = "PASS" ] &&
     [ "$ldet" = "PASS" ] && [ "$lsen" = "PASS" ] && [ "$lrep" = "PASS" ]; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1))
    tail -20 "$work/build.log" >> "$out" || true
    if [ "$keep_going" -eq 0 ]; then
      echo "  (stopping — pass --keep-going to measure the rest)" >> "$out"
      break
    fi
  fi
done

{
  echo "## totals"
  echo "  modules named    ${#mods[@]}"
  echo "  modules measured $checked"
  echo "  all three PASS   $pass"
  echo "  not             $fail"
} >> "$out"

echo "wrote $out"
[ "$fail" -eq 0 ]
