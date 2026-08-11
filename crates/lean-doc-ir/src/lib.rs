//! Reading the intermediate representation (IR schema 4) and the ledger.
//!
//! Filled in by milestone **M1** — see `docs/implementation-plan.md`.
//!
//! Every read of the IR goes through this crate. That is a deliberate
//! structural constraint, not an accident of layering: the incremental
//! pipeline still reads the whole IR five times (ownership, merge twice,
//! impact, render), and the `contentHash` cache that removes those reads
//! belongs in one place rather than five. See plan §3.
//!
//! Two things the IR is not:
//!
//! - It is not binary. Each module is one `.json` text file named after the
//!   module's full name, with keys in alphabetical order (Lean's `Json.mkObj`).
//! - Its spans are **UTF-16 code unit offsets**, not byte offsets. Anything
//!   that indexes into source text has to convert. See plan §7 (U2).
