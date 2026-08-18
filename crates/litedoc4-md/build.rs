//! Compile the vendored md4c parser and link it into this crate.
//!
//! Only `md4c.c` is built. `entity.c` and `md4c-html.c` belong to md4c's HTML
//! renderer, which doc-gen4 does not use and neither do we — see
//! `vendor/md4c/PROVENANCE.md`.
//!
//! `csrc/layout_probe.c` is ours: it reports what the C compiler makes of the
//! vendored header so that `tests/abi.rs` can check the hand-written bindings
//! in `src/ffi.rs` against it.

fn main() {
    let dir = "vendor/md4c";

    println!("cargo:rerun-if-changed={dir}/md4c.c");
    println!("cargo:rerun-if-changed={dir}/md4c.h");
    println!("cargo:rerun-if-changed=csrc/layout_probe.c");

    cc::Build::new()
        .file(format!("{dir}/md4c.c"))
        .include(dir)
        // md4c is a released library, not our code: its warnings are noise in
        // our build output and we must not "fix" them, because every local
        // change is a place where our parser can drift from doc-gen4's.
        .warnings(false)
        .compile("md4c");

    // Ours, so warnings stay on.
    cc::Build::new()
        .file("csrc/layout_probe.c")
        .include(dir)
        .compile("md4c_layout_probe");
}
