//! Rendering module pages from the IR.
//!
//! Filled in by milestone **M1** — see `docs/implementation-plan.md`.
//!
//! The acceptance oracle compares bytes, so a few defaults have to be
//! overridden rather than inherited (plan §7):
//!
//! - HTML escaping covers `& < > "` and nothing else. `'` is left alone.
//! - Sorting follows UTF-16 code unit order, not UTF-8 byte order. The two
//!   disagree above U+FFFF, which is exactly where the mathematical
//!   alphanumerics live.
//! - String literals inside `<script>` follow Lean's `String.quote`.
//!
//! One interface rule: the set of modules to render is `Option<Vec<..>>`, and
//! `Some(vec![])` means *render nothing*. The prototype expressed this as the
//! presence or absence of a flag, which collapsed "empty set" into "every
//! module" and silently re-rendered all 432 pages. See plan §5.
