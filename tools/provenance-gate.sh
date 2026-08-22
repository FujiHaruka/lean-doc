#!/usr/bin/env bash
# Are the attributions still there? — the obligations that went live on public.
#
# `docs/provenance.md` decides which third-party work is in this tree and which
# clauses apply; it is the SoT and this script does not second-guess it. What
# this checks is the part a document cannot: that every file the document says
# carries an attribution **still carries it**.
#
# Why it matters now and did not before: Apache-2.0 §4 fires on distribution,
# and the repository was private until 2026-08-16. The obligations were paid
# ahead of that (provenance.md §6), so nothing here is new work — but an
# attribution deleted by a refactor is now a licence problem rather than an
# untidiness, and refactors do not read NOTICE files.
#
# The inventory is `tools/provenance-files.txt`, in the same spirit as
# `tools/corpus-tests.txt`: a reviewed list, not something derived.
#
# usage: provenance-gate.sh [--list]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
INVENTORY="$HERE/provenance-files.txt"

if [ "${1:-}" = "--list" ]; then
  grep -vE '^\s*(#|$)' "$INVENTORY"
  exit 0
fi

checked=0
failed=0

while IFS= read -r line; do
  case "$line" in
    ''|\#*) continue ;;
  esac
  path="${line%% : *}"
  want="${line#* : }"
  checked=$((checked + 1))
  full="$ROOT/$path"
  if [ ! -f "$full" ]; then
    echo "  MISSING FILE  $path (provenance.md says it carries an attribution)"
    failed=$((failed + 1))
    continue
  fi
  # Fixed-string search over the whole file: an attribution may sit in a header
  # comment, a licence block or a table, and pinning the line number would make
  # this fail on formatting rather than on substance.
  if ! grep -qF -- "$want" "$full"; then
    echo "  MISSING TEXT  $path does not contain \"$want\""
    failed=$((failed + 1))
  fi
done < "$INVENTORY"

echo
if [ "$failed" -ne 0 ]; then
  echo "PROVENANCE GATE: $failed of $checked claims failed" >&2
  echo >&2
  echo "  An attribution named by docs/provenance.md is gone. Restore it — or, if" >&2
  echo "  the third-party code itself is gone, update provenance.md *and* this" >&2
  echo "  inventory in the same commit. Do not delete the line to make this pass." >&2
  exit 1
fi
echo "  files: ok ($checked claims)"

# ---------------------------------------------------------------------------
# 2. NOTICE against the dependency closure — the half that must not be curated
#
# Everything above is a reviewed list, because "does this file still carry its
# header" is a question a list can answer. This is the opposite kind: which
# crates are in the closure changes without anyone editing this repository, so a
# reviewed list is exactly the wrong shape — it goes stale silently, which is
# how `strum` / `phf` / `zmij` were shipping in the binary with no notice from
# 2026-08-22 until this was written (docs/provenance.md §7-8).
#
# The rule has no exception list, on purpose. A crate needs its own notice iff
# its licence expression is **not satisfiable by Apache-2.0 alone** — either it
# does not offer Apache-2.0, or it ANDs something onto it. `MIT OR Apache-2.0`
# is covered by LICENSE and NOTICE already; `MIT` is not; `(MIT OR Apache-2.0)
# AND Unicode-3.0` is not.
#
# The targets come from release.yml rather than from a copy here: the closure is
# platform-dependent, and checking one target while shipping two is the same
# silent narrowing as checking one direction of a two-way diff.
#
# The `|| true` on the two pipelines below is load-bearing and not sloppiness:
# `grep` exits 1 when it matches nothing, and under `set -e` a command
# substitution ending in one kills this script **with no message** — which is
# the first thing that happened when this gate was falsified against the NOTICE
# it was written to fix. The emptiness is the answer here, not an error.

command -v cargo >/dev/null 2>&1 || {
  echo "PROVENANCE GATE: cargo is not on PATH, and the NOTICE half needs it" >&2
  echo "  (this gate does not skip: see CLAUDE.md, skip で緑を返さない)" >&2
  exit 2
}

RELEASE_WORKFLOW="$ROOT/.github/workflows/release.yml"
targets=$(grep -oE '^ +- target: [A-Za-z0-9_.-]+' "$RELEASE_WORKFLOW" | sed 's/.*: //' | sort -u)
[ -n "$targets" ] || {
  echo "PROVENANCE GATE: no '- target:' lines in $RELEASE_WORKFLOW" >&2
  echo "  The release matrix moved; this gate was reading it to know what ships." >&2
  exit 2
}

closure=""
for target in $targets; do
  tree=$(cd "$ROOT" && cargo tree -p litedoc4 -e normal --target "$target" \
           --prefix none --locked -f '{p}|{l}' 2>/dev/null) || {
    echo "PROVENANCE GATE: cargo tree failed for $target" >&2
    exit 2
  }
  closure="$closure
$tree"
done

# `{p}` is "name version [(path or proc-macro)]"; `{l}` is the SPDX expression,
# empty for a crate that declares none (which is itself a failure — nobody has
# read it).
uncovered=$(printf '%s\n' "$closure" \
  | sed 's/ (\*)$//' \
  | grep -F '|' \
  | awk -F'|' '
      {
        name = $1
        sub(/ .*/, "", name)
        licence = $2
        if (licence == "") { print name; next }
        if (licence ~ /Apache-2\.0/ && licence !~ / AND /) next
        print name
      }' \
  | sort -u) || true

listed=$(sed -n '/^Rust crates whose licence does not offer Apache-2.0$/,/^All of the above are licensed under the MIT License:$/p' \
           "$ROOT/NOTICE" \
         | grep -oE '^    [a-z0-9_-]+ ' | tr -d ' ' | sort -u) || true

missing=$(comm -23 <(printf '%s\n' "$uncovered") <(printf '%s\n' "$listed"))
stale=$(comm -13 <(printf '%s\n' "$uncovered") <(printf '%s\n' "$listed"))

if [ -n "$missing" ] || [ -n "$stale" ]; then
  echo >&2
  echo "PROVENANCE GATE: NOTICE and the dependency closure disagree" >&2
  echo >&2
  for crate in $missing; do
    echo "  NO NOTICE     $crate is in the closure and cannot be taken under Apache-2.0" >&2
  done
  for crate in $stale; do
    echo "  STALE ENTRY   $crate is listed in NOTICE but is not in the closure" >&2
  done
  echo >&2
  echo "  Add the crate's copyright line to NOTICE's derived section (or remove the" >&2
  echo "  stale one). Do not widen deny.toml instead: that permits the licence, it" >&2
  echo "  does not reproduce the notice the licence asks for." >&2
  exit 1
fi

count=$(printf '%s\n' "$listed" | grep -c . || true)
echo "  NOTICE: ok ($count crates over $(printf '%s ' $targets | sed 's/ $//'))"
echo
echo "PROVENANCE GATE: ok"
