import Micro.Basic

/-!
# Sorry

The three shapes doc-gen4 issue #270 asks to be told apart, and the only place
the extractor's answer is checked against a real Lean environment: `sorry` is a
property of the *elaborated term*, so a hand-written IR fixture can assert what
the renderer does with the key but never that the extractor puts the right value
there.

They are here rather than in `Micro/Basic.lean` because `sorry` makes `lake
build` print a warning for the whole module, and GATE 6 appends a probe
declaration to `Basic.lean` — a module whose build is already noisy is a bad
place to read a new warning out of.

**Do not "fix" these proofs.** `sorryHole` is the input; the other two are the
two answers that have to differ from it.
-/

namespace Micro.Sorry

/-- A theorem proved by `sorry`: its own proof term is `sorryAx`, so the IR says
`"direct"`. This is the declaration the other two are defined against. -/
theorem sorryHole (n : Nat) : Micro.double n = n + n := by
  sorry

/-- A theorem with a complete proof of its own that *uses* `sorryHole`. Its term
never mentions `sorryAx`, so the IR says `"transitive"` — a different claim from
`sorryHole`'s, and the whole point of the two-valued key. -/
theorem usesHole (n : Nat) : Micro.double n = 2 * n := by
  rw [sorryHole, Nat.two_mul]

/-- A theorem that depends on neither. The IR omits the key entirely; a run that
marks this one has stopped distinguishing anything. -/
theorem noHole (n : Nat) : Micro.double n + 0 = Micro.double n :=
  Nat.add_zero _

end Micro.Sorry
