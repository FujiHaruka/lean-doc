//! Build the site's `app.js` from the TypeScript in `web/`.
//!
//! `docs/plans/assets-typescript.md` 決定 1: **there is no committed bundle**.
//! `web/src` is the only copy of this code in the repository, and
//! [`crate::assets`] picks the bundle out of cargo's `OUT_DIR`, so it cannot be
//! a version behind its sources — which also means there is no freshness gate
//! to write and no "forgot to commit the build output" to diagnose.
//!
//! The cost is that **node is a build dependency of this crate**. It is paid by
//! whoever builds from source (developers and CI); litedoc4 itself ships as the
//! prebuilt archives `release.yml` produces, and the workspace is
//! `publish = false`.
//!
//! There is deliberately **no fallback**. A tree without node does not quietly
//! use yesterday's bundle — it fails here, saying what to install. Two ways of
//! answering "where does app.js come from" is the shape where only one of them
//! ever gets fixed.

use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
    // A directory is walked recursively, so this covers every module under it.
    for input in [
        "web/src",
        "web/package.json",
        "web/package-lock.json",
        "web/tsconfig.json",
        "web/vite.config.ts",
    ] {
        println!("cargo:rerun-if-changed={input}");
    }

    let out_dir = env::var("OUT_DIR").expect("cargo sets OUT_DIR");

    // `npm ci` rather than `npm install`: the lockfile is the version everything
    // downstream was tested against, and a build is not the place to resolve a
    // new one. Only when there is nothing installed — after that, `npm run
    // build` is the whole cost, and the `rerun-if-changed` above means even that
    // is skipped unless a source moved.
    if !Path::new("web/node_modules").exists() {
        npm(&["ci", "--no-audit", "--no-fund"], &out_dir);
    }
    npm(&["run", "build"], &out_dir);

    let bundle = Path::new(&out_dir).join("app.js");
    assert!(
        bundle.exists(),
        "vite reported success but wrote no {}. \
         `web/vite.config.ts` decides the name; it and this file have to agree.",
        bundle.display(),
    );
}

/// One npm invocation in `web/`, with cargo's `OUT_DIR` handed to vite.
///
/// Output is captured and only shown when the command fails: a build script's
/// stdout is cargo's directive channel, and npm's progress lines are not
/// directives.
fn npm(args: &[&str], out_dir: &str) {
    let shown = args.join(" ");
    let result = Command::new("npm")
        .args(args)
        .current_dir("web")
        // `web/vite.config.ts` reads this. Absent (a bare `npm run build` by
        // hand) it writes to `web/dist` instead, so the two never contend for
        // the same file.
        .env("LITEDOC4_ASSET_OUT_DIR", out_dir)
        .output();

    let output = match result {
        Ok(output) => output,
        Err(e) => panic!(
            "could not run `npm {shown}` in crates/litedoc4-render/web: {e}\n\
             \n\
             This crate builds the site's app.js from TypeScript, so node is \
             required to build it from source (docs/plans/assets-typescript.md \
             決定 1). The version this tree is pinned to is in mise.toml; \
             `mise install` puts it on PATH.",
        ),
    };

    assert!(
        output.status.success(),
        "`npm {shown}` failed in crates/litedoc4-render/web ({})\n\
         \n\
         --- stdout ---\n{}\n--- stderr ---\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}
