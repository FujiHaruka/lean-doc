import Consumer.Basic

/-!
# Consumer

The root module of the Lake-wiring fixture. `tools/lake-package-gate.sh` asks
this package one question — does `lake run docs` work when litedoc4 arrives as a
dependency — so the contents are deliberately thin. The declaration shapes a
renderer can get wrong live in `e2e/micro`, which this fixture does not touch.
-/
