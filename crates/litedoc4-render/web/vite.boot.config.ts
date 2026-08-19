/**
 * The second bundle: the theme boot script `frame.rs` inlines into `<head>`.
 *
 * A separate config rather than a second entry in `vite.config.ts`, for two
 * reasons that both come from what this file has to be:
 *
 * - **Classic script, not a module.** `format: "iife"`, because a module in
 *   `<head>` is deferred and would paint the wrong theme first.
 * - **Standalone.** It shares `theme-key.ts` with `app.js`, and a single build
 *   with two entries would answer that by emitting a shared chunk — a third
 *   file, which cannot be inlined into a `<script>` tag.
 *
 * Sharing the source and duplicating the ~30 bytes it compiles to is the trade
 * this makes: one place to rename the storage key, two copies at runtime.
 */
import { defineConfig } from "vite";

const outDir = process.env.LITEDOC4_ASSET_OUT_DIR ?? "dist";

export default defineConfig({
  build: {
    outDir,
    emptyOutDir: false,
    lib: {
      entry: "src/theme-boot.ts",
      formats: ["iife"],
      name: "litedoc4ThemeBoot",
      fileName: () => "theme-boot.js",
    },
    target: "es2022",
    minify: "oxc",
    modulePreload: false,
    reportCompressedSize: false,
  },
});
