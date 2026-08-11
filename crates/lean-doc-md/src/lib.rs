//! CommonMark for docstrings: md4c through FFI, plus doc-gen4's own rendering.
//!
//! Filled in by milestone **M1** — see `docs/implementation-plan.md`.
//!
//! doc-gen4 does not use md4c's HTML renderer. It parses to an AST and builds
//! the HTML itself (`DocGen4/Output/DocString.lean`), with flags
//! `MD_DIALECT_GITHUB | MD_FLAG_LATEXMATHSPANS | MD_FLAG_NOHTML`. This crate
//! does the same, so only `md4c.c` and `md4c.h` need to be vendored —
//! `entity.c` belongs to the HTML renderer, and entities are passed through
//! verbatim anyway.
//!
//! Linking md4c removes the 594-line hand-written CommonMark subset that the
//! TypeScript prototype needed. It does **not** remove the 153 lines of
//! autolink resolution around it (`nameToLink`, `isNameLit`, `autoLinkInline`,
//! ...) — that part is doc-gen4's, not md4c's, and has to be ported.

use std::ffi::{c_int, c_uint, c_void};

// The real binding — the `MD_PARSER` callback table, the block and span enums,
// and a safe wrapper that turns them into an AST — is milestone M1-c. What is
// here is deliberately only enough to prove that the vendored C compiles and
// links on this toolchain, so that M1-c starts from a working build rather than
// discovering a toolchain problem halfway through a port.
//
// `md_parse` is declared with an opaque parser pointer on purpose: writing the
// struct layout down is part of M1-c, and a wrong layout that happens to link
// is worse than no layout at all.
unsafe extern "C" {
    fn md_parse(
        text: *const u8,
        size: c_uint,
        parser: *const c_void,
        userdata: *mut c_void,
    ) -> c_int;
}

/// Address of md4c's entry point.
///
/// Exists so that something in this crate references the C library and the
/// linker is obliged to resolve it. Calling it needs a valid `MD_PARSER`, which
/// M1-c defines.
#[must_use]
pub fn md4c_entry_point_address() -> usize {
    // Through a function pointer rather than casting the function item
    // directly, which rustc warns about.
    let entry: unsafe extern "C" fn(*const u8, c_uint, *const c_void, *mut c_void) -> c_int =
        md_parse;
    entry as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fails at link time, not at run time, if the vendored md4c did not build.
    #[test]
    fn vendored_md4c_links() {
        assert_ne!(md4c_entry_point_address(), 0);
    }
}
