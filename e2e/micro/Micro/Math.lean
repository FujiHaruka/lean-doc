import Micro.Basic

/-!
# Math

Docstring mathematics, which `litedoc4-md` converts to MathML while the page is
written (`docs/plans/feature-sweep.md` C-1). doc-gen4 leaves the dollars in the
page for MathJax to find in the browser; nothing here loads a script.

The four declarations below are the four answers, and the last one is the one
that matters: **a span the LaTeX parser refuses is written back as its source**,
so the page is never worse than doc-gen4's. The build counts those and prints
the count, because a page full of unconverted formulas is otherwise
indistinguishable from a page full of converted ones.

This module is a separate file for the same reason `Micro/Sorry.lean` is: it is
the only place where the *whole* path — Lean docstring, extractor, IR, renderer
— is walked for math, and a hand-written IR fixture cannot show that md4c's
`MD_FLAG_LATEXMATHSPANS` saw a `$` where Lean put one.

The measurement target has **three** math spans in 5,079 docstrings, so it could
never have exercised this 【実測 2026-08-22 →
`benchmarks/results/mathml-2026-08-22.txt`】.
-/

namespace Micro.Math

/-- An inline span: $x^2 + 1$ sits in the run of text around it. -/
def inlineSpan (n : Nat) : Nat := n * n + 1

/-- A displayed span, which is its own block:

$$\sum_{i = 0}^{n} i = \frac{n(n+1)}{2}$$

and the prose continues after it. -/
def displaySpan (n : Nat) : Nat := n * (n + 1) / 2

/-- Escaping still applies to whatever is *not* converted, and the conversion
has to survive characters HTML cares about: $a < b$ and $c \& d$ are both
legal LaTeX. -/
def escapes (a : Nat) : Nat := a

/-- **The fallback.** `\colim` is not a command the converter implements, so
this span stays $\colim_k F(k)$ — dollars, source, escaping, exactly what the
page held before MathML existed. It is one of the nine spans in Mathlib that
fail 【実測 2026-08-22】.

**Do not "fix" this to a command that parses.** It is the input. -/
theorem fallbackStays (n : Nat) : Micro.double n = n + n := rfl

end Micro.Math
