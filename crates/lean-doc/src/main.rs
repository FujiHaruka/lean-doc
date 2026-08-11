//! The `lean-doc` command line tool.
//!
//! The subcommand surface is decided by milestone **M4**; this binary exists
//! from **M0** so the workspace has something to build and the later
//! milestones have somewhere to attach. See `docs/implementation-plan.md`.

fn main() {
    println!(
        "lean-doc {} — no subcommands yet (milestone M4)",
        env!("CARGO_PKG_VERSION")
    );
}
