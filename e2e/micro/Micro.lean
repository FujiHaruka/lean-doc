import Micro.Attrs
import Micro.Basic
import Micro.Dep
import Micro.Gen
import Micro.Math
import Micro.Notation
import Micro.Shapes
import Micro.Sorry
import Micro.Unicode

/-!
# Micro

The e2e fixture package. It depends on Lean core and on one sibling package
reached by path (`../micro-dep`), which is what keeps it runnable on a CI
runner: the measurement target pulls in all of Mathlib, so it can never be the
thing a push is judged by.

This root module imports every other one, so `lake build` over the default
target builds the whole fixture.
-/
