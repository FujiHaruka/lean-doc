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
echo "PROVENANCE GATE: ok ($checked claims)"
