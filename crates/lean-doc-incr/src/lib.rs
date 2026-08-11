//! Incremental rebuild: what to re-extract and what to re-render.
//!
//! Filled in by milestone **M3** — see `docs/implementation-plan.md`.
//!
//! These are two questions asked at two different times, not one question
//! asked twice. What to re-extract is decided before Lean runs, from the
//! `.olean` files; what to re-render is decided after, from the IR.
//!
//! Order within a run is constrained, and the constraints are not obvious
//! from the stage names:
//!
//! - ownership runs *before* merge, because merge overwrites the previous
//!   owner of every name it touches;
//! - the global artifacts run *before* impact, because their diff is what
//!   tells impact which pages have a docstring link that just went stale;
//! - extract/ownership/merge form a loop, bounded by `--max-rounds`;
//! - a `renderKey` change overrides the impact mode and re-renders everything.
//!
//! Key comparison is a union: a key present on only one side counts as a
//! change. Under-rendering has to be loud, never silent.
