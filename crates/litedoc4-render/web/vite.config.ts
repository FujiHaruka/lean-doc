/**
 * The one bundle the generated site loads.
 *
 * `crates/litedoc4-render/build.rs` runs this and hands cargo's `OUT_DIR` in
 * `LITEDOC4_ASSET_OUT_DIR`, so the file `include_str!` picks up is always the
 * one built from the sources next to it — there is no committed copy to go
 * stale (`docs/plans/assets-typescript.md` 決定 1). A bare `npm run build`
 * writes to `dist/` instead, which is gitignored and exists only so the build
 * can be run by hand without a cargo invocation around it.
 */
import { defineConfig } from "vitest/config";

const outDir = process.env.LITEDOC4_ASSET_OUT_DIR ?? "dist";

export default defineConfig({
  build: {
    outDir,
    // `OUT_DIR` is cargo's, and it has other things in it.
    emptyOutDir: false,
    // What the pages ask for: `<script type="module">`, one file, no imports to
    // resolve at load time. `lib` rather than an HTML entry because there is no
    // HTML here — the pages are written by Rust.
    lib: {
      entry: "src/main.ts",
      formats: ["es"],
      fileName: () => "app.js",
    },
    target: "es2022",
    minify: "oxc",
    // Nothing is code-split, so there is nothing to preload.
    modulePreload: false,
    reportCompressedSize: true,
  },
  test: {
    // Most of what is tested here is byte arithmetic and needs no DOM at all;
    // the few that build elements get one that is not a browser and does not
    // pretend to be. The browser gate is still the thing that answers "does the
    // site work" (`tools/browser-gate.sh`).
    environment: "happy-dom",
    include: ["test/**/*.test.ts"],
  },
});
