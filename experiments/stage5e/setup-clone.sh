#!/usr/bin/env bash
# Make an APFS clonefile copy of the measurement target, and move one module's
# body into a new module inside it.
#
# WHY A CLONE
#   CLAUDE.md forbids modifying the measurement target, and this experiment needs
#   `lake build` to run over an edited source. `cp -Rc` on APFS is copy-on-write:
#   the 12 GB tree costs ~0 real disk and ~34 s, and the original is not written
#   to at all. Stage 5c established this; it is reused verbatim.
#
# THE MOVE
#   A's body moves to a new module X = A ++ "Core"; A becomes a shim that
#   imports X. Full names do not change — a namespace comes from the `namespace`
#   command, not the file path — so the only thing that changes is *which module
#   defines the names*. Stage 5c measured that change to be invisible in a
#   referring module's olean.
#
#   A IS A PARAMETER, AND CHOOSING IT WRONG WASTES THE EXPERIMENT. Stage 5c used
#   `Shannon.GaussianPDFVarianceDerivative` and described
#   `Shannon.FisherDeBruijnGaussian` as referring to it "in two places". Those
#   two places are backticks in a **module docstring**: that module has **zero
#   declarations**, and *no module in the package names anything of A's in a
#   printed signature*. For an olean-level question that did not matter. For a
#   question about what the IR's `refs` say, it makes the move unobservable by
#   construction. Pick A from `impact.ts --census`: it must have referrers.
#
# THE SHIM STYLE IS ALSO A PARAMETER
#   minimal       A becomes `import X` and nothing else.
#   keep-imports  A keeps its original imports and adds `import X`.
#
#   `keep-imports` leaves imports the now-empty A does not use, which changes
#   what Mathlib's style linter logs — and that log is an environment extension
#   serialized into oleans (stage 5c found it embeds absolute source paths in
#   429/432 modules). `minimal` is the default for that reason.
#
# usage:
#   setup-clone.sh clone <clone-dir>
#   setup-clone.sh move  <clone-dir> <module> [style]
#   setup-clone.sh reset <clone-dir>
set -euo pipefail

SRC="${TARGET_SRC:-/Users/haruka/dev/lean-projects}"
CMD=${1:?clone|move|reset}
CLONE=${2:?clone dir}
LAKE="${LAKE:-$HOME/.elan/bin/lake}"

case "$CLONE" in
  "$SRC"|"$SRC"/*) echo "refusing to write inside the measurement target" >&2; exit 2 ;;
esac

if [ "$CMD" = clone ]; then
  if [ ! -d "$CLONE" ]; then
    echo "### cloning $SRC -> $CLONE (APFS clonefile)"
    time cp -Rc "$SRC" "$CLONE"
  fi
  # The clone is only usable as a baseline if Lake agrees nothing needs building.
  echo "### verifying the clone is up to date"
  (cd "$CLONE" && "$LAKE" build --no-build 2>&1 | tail -2)
  exit 0
fi

if [ "$CMD" = reset ]; then
  echo "### undoing every edit in the clone's sources"
  git -C "$CLONE" clean -fd -- InformationTheory InformationTheory.lean
  git -C "$CLONE" checkout -- InformationTheory InformationTheory.lean
  (cd "$CLONE" && "$LAKE" build 2>&1 | tail -3)
  exit 0
fi

[ "$CMD" = move ] || { echo "usage: setup-clone.sh clone|move|reset <dir> ..." >&2; exit 2; }
A_MOD=${3:?module to move}
STYLE=${4:-minimal}
X_MOD="${A_MOD}Core"
A_REL="$(echo "$A_MOD" | tr '.' '/').lean"
X_REL="$(echo "$X_MOD" | tr '.' '/').lean"

if [ -f "$CLONE/$X_REL" ]; then
  echo "### the move is already applied"
  exit 0
fi
[ -f "$CLONE/$A_REL" ] || { echo "no such module file: $A_REL" >&2; exit 2; }

echo "### moving $A_MOD -> $X_MOD (shim style: $STYLE)"
python3 - "$CLONE/$A_REL" "$CLONE/$X_REL" "$X_MOD" "$STYLE" <<'PY'
import sys
a_path, x_path, x_mod, style = sys.argv[1:]
src = open(a_path, encoding="utf-8").read()

# X is A verbatim: same imports, same namespace, same declarations. Only the
# file it lives in differs, which is exactly the change under test.
open(x_path, "w", encoding="utf-8").write(src)

# A becomes a pure re-export. It must still exist and still be importable,
# because the referring modules import A and they are not allowed to change.
if style == "minimal":
    shim = f"import {x_mod}\n"
elif style == "keep-imports":
    imports = [l for l in src.split("\n") if l.startswith("import ")]
    shim = "\n".join(imports + [f"import {x_mod}", ""])
else:
    sys.exit("unknown shim style: " + style)
open(a_path, "w", encoding="utf-8").write(shim)
print(f"A is now a {len(shim.splitlines())}-line shim ({style}); "
      f"X has {len(src.splitlines())} lines")
PY

echo "### building the clone (the lake build a real move would pay)"
(cd "$CLONE" && "$LAKE" build 2>&1 | tail -5)
echo "### done"
