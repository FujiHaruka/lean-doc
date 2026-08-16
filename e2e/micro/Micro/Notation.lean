import Micro.Basic

/-!
# Notation

`scoped notation` is the one thing doc-gen4 cannot print for a package's own
declarations (approach.md §10), so the fixture has to contain it: if lean-doc
ever loses `Lean.activateScoped`, the signature of `useNotation` below stops
printing as `⟦n⟧` and this fixture is where that shows.
-/

namespace Micro

/-- Doubling, written with brackets. -/
scoped notation "⟦" x "⟧" => Micro.double x

/-- A definition whose *signature* uses the scoped notation. -/
def useNotation (n : Nat) : Nat := ⟦n⟧

end Micro
