/-!
# Consumer.Basic

A handful of documented declarations — enough for the pipeline to produce a page
and for `tools/site-gate.sh` to have something to close over. Nothing here is
trying to cover a renderer branch; that is `e2e/micro`'s job.
-/

/-- The greeting this fixture is documented by. -/
def Consumer.greeting : String := "hello from a Lake consumer"

/-- Twice `n`, so the pipeline has a declaration with a signature to print. -/
def Consumer.double (n : Nat) : Nat := 2 * n

/-- `Consumer.double` never shrinks its argument. A theorem, so the site has a
declaration whose kind is not `def`. -/
theorem Consumer.le_double (n : Nat) : n ≤ Consumer.double n := by
  simp [Consumer.double]
  omega

/-- A structure, so the page has members to render and the search index has
names that are not top-level definitions. -/
structure Consumer.Config where
  /-- Where the documentation is written. -/
  out : String
  /-- Extractor threads. -/
  jobs : Nat := 1
