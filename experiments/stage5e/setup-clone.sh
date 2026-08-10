#!/usr/bin/env bash
# Make an APFS clonefile copy of the measurement target, or apply stage 5c's E-B
# move to an existing one.
#
# WHY A CLONE
#   CLAUDE.md forbids modifying the measurement target, and this experiment needs
#   `lake build` to run over an edited source. `cp -Rc` on APFS is copy-on-write:
#   the 12 GB tree costs ~0 real disk and ~34 s, and the original is not written
#   to at all. Stage 5c established this; it is reused verbatim.
#
# THE MOVE (E-B)
#   A = InformationTheory.Shannon.GaussianPDFVarianceDerivative
#   X = InformationTheory.Shannon.GaussianPDFVarianceDerivativeCore   (new)
#   C = InformationTheory.Shannon.FisherDeBruijnGaussian              (untouched)
#
#   A's body moves to X; A becomes a shim that imports X. Full names do not
#   change — a namespace comes from the `namespace` command, not the file path —
#   so the only thing that changes is *which module defines the names*. That is
#   the change stage 5c measured to be invisible to L2.
#
# THE SHIM STYLE IS A PARAMETER, AND IT MATTERS
#   minimal       A becomes `import X` and nothing else. This is stage 5c's E-B.
#   keep-imports  A keeps its original imports and adds `import X`.
#
#   The two are not interchangeable. `keep-imports` leaves eleven imports that
#   the now-empty A does not use, which changes what Mathlib's style linter
#   logs — and the linter's log is an environment extension that is serialized
#   into oleans (stage 5c found it embeds absolute source paths in 429/432
#   modules). So the choice can decide whether a *referring* module's olean
#   changes, which is exactly the thing under test. Both are measured.
#
# usage:
#   setup-clone.sh clone <clone-dir>              clone and verify it is up to date
#   setup-clone.sh move  <clone-dir> [style]      apply E-B and `lake build`
#   setup-clone.sh reset <clone-dir>              undo the move and `lake build`
set -euo pipefail

SRC="${TARGET_SRC:-/Users/haruka/dev/lean-projects}"
CMD=${1:?clone|move|reset}
CLONE=${2:?clone dir}
STYLE=${3:-minimal}
LAKE="${LAKE:-$HOME/.elan/bin/lake}"

A_REL=InformationTheory/Shannon/GaussianPDFVarianceDerivative.lean
X_REL=InformationTheory/Shannon/GaussianPDFVarianceDerivativeCore.lean
X_MOD=InformationTheory.Shannon.GaussianPDFVarianceDerivativeCore

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
  echo "### undoing the move"
  rm -f "$CLONE/$X_REL"
  git -C "$CLONE" checkout -- "$A_REL"
  (cd "$CLONE" && "$LAKE" build 2>&1 | tail -3)
  exit 0
fi

[ "$CMD" = move ] || { echo "usage: setup-clone.sh clone|move|reset <dir> [style]" >&2; exit 2; }

if [ -f "$CLONE/$X_REL" ]; then
  echo "### the move is already applied"
  exit 0
fi

echo "### applying E-B (shim style: $STYLE)"
python3 - "$CLONE/$A_REL" "$CLONE/$X_REL" "$X_MOD" "$STYLE" <<'PY'
import sys
a_path, x_path, x_mod, style = sys.argv[1:]
src = open(a_path, encoding="utf-8").read()

# X is A verbatim: same imports, same namespace, same declarations. Only the
# file it lives in differs, which is exactly the change under test.
open(x_path, "w", encoding="utf-8").write(src)

# A becomes a pure re-export. It must still exist and still be importable,
# because C imports A and C is not allowed to change.
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
