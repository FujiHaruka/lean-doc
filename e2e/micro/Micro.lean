import Micro.Basic
import Micro.Notation
import Micro.Shapes
import Micro.Unicode

/-!
# Micro

The e2e fixture package. It depends on nothing but Lean core, which is what
makes it runnable on a CI runner: the measurement target pulls in all of
Mathlib, so it can never be the thing a push is judged by.

This root module imports every other one, so `lake build` over the default
target builds the whole fixture.
-/
