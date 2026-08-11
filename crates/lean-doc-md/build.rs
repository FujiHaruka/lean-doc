//! Compile the vendored md4c parser and link it into this crate.
//!
//! Only `md4c.c` is built. `entity.c` and `md4c-html.c` belong to md4c's HTML
//! renderer, which doc-gen4 does not use and neither do we — see
//! `vendor/md4c/PROVENANCE.md`.

fn main() {
    let dir = "vendor/md4c";

    println!("cargo:rerun-if-changed={dir}/md4c.c");
    println!("cargo:rerun-if-changed={dir}/md4c.h");

    cc::Build::new()
        .file(format!("{dir}/md4c.c"))
        .include(dir)
        // md4c is a released library, not our code: its warnings are noise in
        // our build output and we must not "fix" them, because every local
        // change is a place our parser can drift from doc-gen4's.
        .warnings(false)
        .compile("md4c");
}
