#!/usr/bin/env bash
# Is `extract` decided by the toolchain alone, or also by the target package's
# dependency set?  (docs/plans/distribution.md, checks X1 / X3)
#
# ============================================================================
# WHY THIS QUESTION IS WORTH A SCRIPT
# ============================================================================
#   If the answer is "the toolchain alone", one prebuilt extractor can be
#   shipped per toolchain and every user's CI stops spending ~16 s building it.
#   If it is not, the binary cannot be shipped at all and the CI cache is the
#   only option. `tools/ci-build.sh` already reported a byte-for-byte match
#   across two packages, and says in the same breath why that was weak evidence:
#   the two dependency sets were copies of each other. This script exists to be
#   run over packages that are *not* copies of each other.
#
# ============================================================================
# WHY NO `lake build`
# ============================================================================
#   `extractor/Extract.lean` imports only `Lean`. The target package is borrowed
#   for its environment (`lake env` sets LEAN_PATH and picks the toolchain), not
#   for its build output, so a package that has never been built works here.
#   Building would also be the slow, disk-hungry part — CLAUDE.md's disk rule.
#
# ============================================================================
# WHAT IS COMPARED, AND WHAT IS NOT
# ============================================================================
#   The SHA-256 of the binary and of the C that `lean` emits on the way. The
#   binaries are NOT kept side by side: `extractor/build.sh` writes to a fixed
#   path, so this builds, hashes, and moves on. That keeps peak disk at one
#   copy (171 MB) instead of one per package. `--keep-binary` saves exactly one
#   for a caller that wants to ship or upload it.
#
#   NOT compared: the IR two binaries produce. Once the binaries are identical
#   that question is answered; when they are NOT identical this script fails and
#   the IR comparison is the follow-up, not a substitute.
#
# usage:
#   extractor-uniqueness.sh <package> <package> [<package>...]
#                           [--keep-binary <path>] [--json <path>]
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LAKE="${LAKE:-lake}"

PKGS=()
KEEP_BINARY=""
JSON=""
while [ $# -gt 0 ]; do
  case "$1" in
    --keep-binary) KEEP_BINARY="$2"; shift 2 ;;
    --json) JSON="$2"; shift 2 ;;
    -h|--help) sed -n '/^# usage:/,/^set -euo/p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//;$d'; exit 0 ;;
    -*) echo "unknown option: $1" >&2; exit 2 ;;
    *) PKGS+=("$1"); shift ;;
  esac
done

[ "${#PKGS[@]}" -ge 2 ] || { echo "give at least two packages (see --help)" >&2; exit 2; }

BUILT="$REPO/extractor/build/extract"
GENC="$REPO/extractor/build/Extract.c"

# ---------------------------------------------------------------- toolchains
#
# Same toolchain across every package is the *premise*, not a finding: comparing
# binaries built against different toolchains would answer a question nobody
# asked. Checked before anything is built so the run stops in a second.
TOOLCHAIN=""
for p in "${PKGS[@]}"; do
  [ -d "$p" ] || { echo "no such package: $p" >&2; exit 1; }
  [ -f "$p/lean-toolchain" ] || { echo "no lean-toolchain in $p" >&2; exit 1; }
  t="$(tr -d '\n' < "$p/lean-toolchain")"
  if [ -z "$TOOLCHAIN" ]; then TOOLCHAIN="$t"
  elif [ "$t" != "$TOOLCHAIN" ]; then
    echo "these packages pin different toolchains: $TOOLCHAIN vs $t ($p)" >&2
    echo "that is a different experiment — see extractor-mismatch.sh" >&2
    exit 2
  fi
done

echo "toolchain   $TOOLCHAIN"
echo "packages    ${#PKGS[@]}"
uname -srm

NAMES=()
SHAS=()
CSHAS=()
DEPS=()
SECS=()

sha () { shasum -a 256 "$1" 2>/dev/null | cut -d' ' -f1; }

for p in "${PKGS[@]}"; do
  name="$(basename "$p")"
  # Not `ls … | wc -l`: with `pipefail` a missing directory fails the pipeline,
  # and a package with **no dependencies at all** is precisely the interesting
  # end of the range this script measures. It killed the first run silently.
  deps=0
  if [ -d "$p/.lake/packages" ]; then
    deps="$(find "$p/.lake/packages" -mindepth 1 -maxdepth 1 -type d | wc -l | tr -d ' ')"
  fi
  echo
  # Counted under .lake/packages, so a **path** dependency reads as 0 here even
  # though it is on LEAN_PATH. The wide end of the range (Mathlib, 15) is only
  # reachable on a machine that has the target checked out.
  echo "=== $name  (.lake/packages: $deps)"
  rm -f "$BUILT" "$GENC"
  t0=$SECONDS
  TARGET_REPO="$p" LAKE="$LAKE" "$REPO/extractor/build.sh" > /dev/null
  t1=$SECONDS
  [ -x "$BUILT" ] || { echo "build.sh did not produce $BUILT" >&2; exit 1; }
  s="$(sha "$BUILT")"; c="$(sha "$GENC")"
  echo "    binary  $s"
  echo "    C       $c"
  echo "    built   $((t1 - t0))s   $(ls -l "$BUILT" | awk '{print $5}') bytes"
  NAMES+=("$name"); SHAS+=("$s"); CSHAS+=("$c"); DEPS+=("$deps"); SECS+=("$((t1 - t0))")
done

# ------------------------------------------------------------------- verdict
echo
echo "=== verdict"
FIRST="${SHAS[0]}"
IDENTICAL=1
for i in "${!SHAS[@]}"; do
  mark="="
  [ "${SHAS[$i]}" = "$FIRST" ] || { mark="DIFFERS"; IDENTICAL=0; }
  printf '%-14s deps=%-3s %s  %s\n' "${NAMES[$i]}" "${DEPS[$i]}" "${SHAS[$i]:0:16}…" "$mark"
done

# ------------------------------------------------------- what it links against
#
# The other half of "can this be shipped": a binary that is identical everywhere
# is still unusable if it resolves a path from the machine that built it.
echo
echo "=== dynamic dependencies"
if command -v ldd > /dev/null 2>&1; then ldd "$BUILT" || true
elif command -v otool > /dev/null 2>&1; then otool -L "$BUILT" | tail -n +2
fi
if command -v readelf > /dev/null 2>&1; then
  echo "--- RPATH/RUNPATH"
  readelf -d "$BUILT" 2>/dev/null | grep -E "RPATH|RUNPATH" || echo "(none)"
fi
echo "--- absolute paths from this machine's home baked in"
strings -a "$BUILT" 2>/dev/null | grep -c "^$HOME" || true

if [ -n "$KEEP_BINARY" ]; then
  mkdir -p "$(dirname "$KEEP_BINARY")"
  cp "$BUILT" "$KEEP_BINARY"
  echo
  echo "kept        $KEEP_BINARY"
fi

if [ -n "$JSON" ]; then
  {
    printf '{"toolchain":"%s","uname":"%s","identical":%s,"builds":[' \
      "$TOOLCHAIN" "$(uname -srm)" "$([ "$IDENTICAL" = 1 ] && echo true || echo false)"
    for i in "${!NAMES[@]}"; do
      [ "$i" = 0 ] || printf ','
      printf '{"package":"%s","deps":%s,"seconds":%s,"sha256":"%s","cSha256":"%s"}' \
        "${NAMES[$i]}" "${DEPS[$i]}" "${SECS[$i]}" "${SHAS[$i]}" "${CSHAS[$i]}"
    done
    printf ']}\n'
  } > "$JSON"
  echo "json        $JSON"
fi

if [ "$IDENTICAL" = 1 ]; then
  echo
  echo "IDENTICAL: ${#PKGS[@]} packages, one toolchain, one binary."
  exit 0
fi
echo
echo "NOT IDENTICAL — the extractor depends on more than the toolchain." >&2
echo "A prebuilt extractor cannot be shipped per toolchain. Compare the C above:" >&2
echo "if the C matches and the binaries do not, the difference is in leanc." >&2
exit 1
