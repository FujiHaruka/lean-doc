#!/usr/bin/env bash
# Rebuild the package's own modules inside the clone, so that every olean in it
# carries the *clone's* path.
#
# WHY THIS IS NECESSARY, AND WHY IT WAS NOT OBVIOUS
#   `cp -Rc` copies the build tree along with the sources, so the clone starts
#   with oleans that were produced at the ORIGINAL path. Stage 5c measured that
#   Mathlib's style linter stores its log — including absolute source paths — in
#   an environment extension that is serialized into the olean (429/432 modules).
#   So the moment any module is rebuilt inside the clone, its olean changes by
#   the path-length difference alone: C's went 5,968 -> 6,048 B, the +80 the
#   stage 5c path-dependence experiment predicts.
#
#   The consequence for a *measurement* is severe and easy to miss: a ledger
#   taken over the cloned oleans reports every rebuilt module as changed, for
#   reasons that have nothing to do with the edit under test. The first stage 5e
#   run hit exactly this and appeared to refute stage 5c's P3. It did not — the
#   referring module C showed up as changed even after the edit was *reverted*,
#   which is what gave it away.
#
#   Rebuilding the package's own 432 modules once makes the path constant across
#   the baseline and everything after it. Mathlib is left alone: it is not edited
#   here, so its oleans are never rebuilt and their stale paths never matter.
#
# usage: rebuild-own.sh <clone-dir>
set -euo pipefail

CLONE=${1:?clone dir}
LAKE="${LAKE:-$HOME/.elan/bin/lake}"
SRC="${TARGET_SRC:-/Users/haruka/dev/lean-projects}"
case "$CLONE" in
  "$SRC"|"$SRC"/*) echo "refusing to rebuild inside the measurement target" >&2; exit 2 ;;
esac

LIB="$CLONE/.lake/build/lib/lean"
echo "### removing the package's own build artefacts (Mathlib's are kept)"
BEFORE=$(find "$LIB/InformationTheory" -name '*.olean' 2>/dev/null | wc -l | tr -d ' ')
rm -rf "$LIB/InformationTheory" "$LIB"/InformationTheory.olean*
rm -rf "$CLONE/.lake/build/ir/InformationTheory" "$CLONE/.lake/build/ir"/InformationTheory.*
echo "  removed $BEFORE own oleans"

echo "### lake build (this is a from-scratch build of the package, Mathlib warm)"
cd "$CLONE"
/usr/bin/time -l "$LAKE" build 2>&1 | tail -25
AFTER=$(find "$LIB/InformationTheory" -name '*.olean' 2>/dev/null | wc -l | tr -d ' ')
echo "### rebuilt: $AFTER own oleans"
"$LAKE" build --no-build 2>&1 | tail -1
