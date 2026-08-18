#!/usr/bin/env bash
# Apply (or verify) the timing instrumentation on the target repo's doc-gen4.
#
# `.lake/packages` is a build artifact of the target repo: `lake update` wipes it,
# taking the instrumentation with it. This script puts it back.
#
# Usage:
#   apply-instrumentation.sh            apply the patch and rebuild doc-gen4
#   apply-instrumentation.sh --check    report whether it is currently applied
set -uo pipefail

. "$(dirname "${BASH_SOURCE[0]}")/env.sh"

PATCH="$LITEDOC4_ROOT/benchmarks/doc-gen4-instrumentation.patch"
PKG="$TARGET_REPO/.lake/packages/doc-gen4"

[ -f "$PATCH" ] || { echo "patch not found: $PATCH" >&2; exit 1; }
[ -d "$PKG/.git" ] || { echo "doc-gen4 checkout not found: $PKG (run 'lake update' in the target repo first)" >&2; exit 1; }

cd "$PKG"

if git apply --check --reverse "$PATCH" 2>/dev/null; then
  echo "instrumentation: APPLIED ($(git describe --tags 2>/dev/null || git rev-parse --short HEAD))"
  [ "${1:-}" = "--check" ] && exit 0
  echo "nothing to do"
  exit 0
fi

if [ "${1:-}" = "--check" ]; then
  echo "instrumentation: NOT APPLIED"
  exit 1
fi

git apply --check "$PATCH" || {
  echo "patch does not apply cleanly — doc-gen4 has moved off the version this was measured on." >&2
  echo "Re-derive the instrumentation instead of forcing it; the numbers would not be comparable anyway." >&2
  exit 1
}
git apply "$PATCH"
echo "patch applied; building doc-gen4"
cd "$TARGET_REPO" && lake build doc-gen4
