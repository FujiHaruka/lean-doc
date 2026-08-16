/-!
# Basic

A module docstring, so that the IR's `moduleDocs` is not empty on this fixture.
Everything here exists to make the extractor emit one more shape of IR — see
`e2e/README.md` for which shape each declaration is responsible for.
-/

namespace Micro

/-- A plain definition with a docstring. -/
def double (n : Nat) : Nat := n + n

/-- A theorem whose statement mentions `double`, so the signature carries a
`Const` span pointing at another declaration of this package. -/
theorem double_eq (n : Nat) : double n = 2 * n := by
  simp [double, Nat.two_mul]

/-- A structure, so that the IR carries `members` with their own docstrings. -/
structure Point where
  /-- The first coordinate. -/
  x : Nat
  /-- The second coordinate. -/
  y : Nat

/-- An instance, so that `instTypes` — the field UI-V5 turned out to need — is
non-empty on this fixture. -/
instance : Inhabited Point := ⟨{ x := 0, y := 0 }⟩

/-- An `abbrev`, which L3-1 singled out: a moved `abbrev` changes what other
pages print, an `instance` does not. -/
abbrev Nat2 := Nat

/-- An inductive, so that constructors are rendered as members. -/
inductive Colour where
  /-- The first constructor. -/
  | red
  /-- The second constructor. -/
  | green

/-- A definition that matches on `Colour`, so equations are generated. -/
def Colour.name : Colour → String
  | .red => "red"
  | .green => "green"

end Micro
