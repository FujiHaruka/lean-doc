/**
 * The names the two halves of the theme share.
 *
 * There are two halves because one of them cannot wait: `theme-boot.ts` is
 * inlined into every page's `<head>` and runs **before the first paint**, while
 * `theme.ts` arrives with the deferred module. Until 2026-08-19 the boot half
 * was a string literal in `frame.rs`, so this key existed in two languages and
 * either could be renamed alone (`docs/plans/assets-typescript.md` 決定 6).
 */

export const THEME_KEY = "litedoc4-theme";

export const THEMES = ["auto", "light", "dark"] as const;
export type Theme = (typeof THEMES)[number];

/**
 * The themes that are an attribute on `<html>`.
 *
 * `auto` is the *absence* of one — which is why the boot script cannot simply
 * write whatever it read.
 */
export const PAINTED: readonly Theme[] = ["light", "dark"];
