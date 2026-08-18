#!/usr/bin/env bash
# Shared settings for every benchmark run. Source this, don't execute it.
#
# The measurement target is fixed on purpose: numbers are only comparable when
# they come from the same workload. See CLAUDE.md "ベンチマーク".
set -u

# The Lean project being documented. Override only to add a target, never to
# replace the baseline one.
TARGET_REPO="${TARGET_REPO:-/Users/haruka/dev/lean-projects}"

# This repository (litedoc4), resolved from the script location.
LITEDOC4_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

# Where raw timing logs land. Committed, so keep them small.
RESULTS_DIR="${RESULTS_DIR:-$LITEDOC4_ROOT/benchmarks/results}"

# The instrumented doc-gen4 binary inside the target repo.
DOCGEN_BIN="$TARGET_REPO/.lake/packages/doc-gen4/.lake/build/bin/doc-gen4"

[ -d "$TARGET_REPO" ] || { echo "target repo not found: $TARGET_REPO" >&2; exit 1; }
mkdir -p "$RESULTS_DIR"
