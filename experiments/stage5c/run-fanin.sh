#!/usr/bin/env bash
# How long does `lake build` take for a one-declaration edit to a module with a
# large fan-in? The incremental doc pipeline runs *after* this, so this is the
# part of the critical path that no amount of doc-side work can remove.
#
# Usage: run-fanin.sh <clone-dir> <module-path-under-project> <out-dir>
set -euo pipefail

CLONE=${1:?clone dir}
REL=${2:?module source path relative to the project root}
OUT=${3:?out dir}

SRC="$CLONE/$REL"
ORIG="$OUT/$(basename "$REL").orig"
mkdir -p "$OUT"
cp "$SRC" "$ORIG"

probe() {
  python3 - "$1" <<'PY'
import re, sys
p = sys.argv[1]
s = open(p).read()
probe = """
/-- Probe declaration for the stage-5c fan-in experiment. Nothing references it. -/
theorem probe_stage5c_fanin (n : ℕ) : n + 0 = n := rfl

"""
m = list(re.finditer(r"^end [\w.]+\s*$", s, re.M))
i = m[-1].start() if m else len(s)
open(p, "w").write(s[:i] + probe + s[i:])
PY
}

cd "$CLONE"
for phase in add revert; do
  [ "$phase" = add ] && probe "$SRC" || cp "$ORIG" "$SRC"
  echo "=== $phase ===" >>"$OUT/fanin.log"
  /usr/bin/time -l lake build >"$OUT/fanin-$phase.out" 2>>"$OUT/fanin.log" || echo "EXIT=$?" >>"$OUT/fanin.log"
  grep -cE '^(✔|⚠) \[' "$OUT/fanin-$phase.out" >>"$OUT/fanin.log" || true
done
cp "$ORIG" "$SRC"
echo "done" >>"$OUT/fanin.log"
