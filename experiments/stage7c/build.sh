#!/usr/bin/env bash
# Builds the stage-7c extractor (byte-identical copy of stage 7b's) (schema 4) into a native binary under `build/`.
#
# Same shape as stages 2, 3 and 4: lean-doc has no toolchain and no lakefile of its own, so
# the Lean environment is borrowed from the measurement target through `lake env`.
#
# usage: build.sh
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../../benchmarks/tools/env.sh
source "$HERE/../../benchmarks/tools/env.sh"

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
