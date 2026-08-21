#!/usr/bin/env bash
# Builds litedoc4's extractor (Extract.lean, IR schema 5) into a native binary
# under `build/`.
#
# Moved from `experiments/stage7d/build.sh` in M4-a. **The shape is unchanged**
# — only the path to `env.sh` moved one directory up, because this script now
# sits at the repository root instead of two levels down. In particular the two
# steps below are still two steps and `leanc` is still called with `-rdynamic`;
# see the comment there.
#
# litedoc4 has no toolchain and no Mathlib of its own, so the Lean environment is
# borrowed from the measurement target through `lake env` (CLAUDE.md).
# `TARGET_REPO` selects it.
#
# There *is* a `lakefile.lean` at the root now, and it builds the same extractor
# as a `lean_exe` (`docs/plans/lake-package.md`). This script is still the one
# the benchmarks and `tools/ci-build.sh` use: the two builds do not produce the
# same bytes (Lake adds a package symbol prefix and `-O3`) even though they write
# byte-identical IR, and every number in `benchmarks/` was taken with this one.
# `tools/lake-package-gate.sh` item 4 is what keeps the two paths honest.
#
# usage: build.sh
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../benchmarks/tools/env.sh
source "$HERE/../benchmarks/tools/env.sh"

LAKE="${LAKE:-$HOME/.elan/bin/lake}"
BUILD="$HERE/build"
mkdir -p "$BUILD"

cd "$TARGET_REPO"

# `--root` is required because the source lives outside the target repository;
# `lean` derives the module name relative to it.
"$LAKE" env lean --root="$HERE" \
  -o "$BUILD/Extract.olean" -c "$BUILD/Extract.c" "$HERE/Extract.lean"

# -rdynamic: `importModules (loadExts := true)` runs module initializers through
# the Lean interpreter, which resolves symbols in the running executable (Lake
# spells this `supportInterpreter := true`). Without it the binary dies with
# "Could not find native implementation of external declaration".
"$LAKE" env leanc -rdynamic -o "$BUILD/extract" "$BUILD/Extract.c"

echo "built $BUILD/extract"
