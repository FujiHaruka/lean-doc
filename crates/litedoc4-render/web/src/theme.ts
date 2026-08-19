/**
 * The theme toggle.
 *
 * The *boot* half is `theme-boot.ts`, a second bundle that `frame.rs` inlines
 * into `<head>`: it sets `data-theme` before the first paint, because this
 * module is deferred and a theme applied after paint is a flash of the wrong
 * one. The names the two share are in `theme-key.ts`.
 */

import { THEME_KEY, THEMES, type Theme } from "./theme-key.js";

const isTheme = (value: string | null): value is Theme =>
  value !== null && (THEMES as readonly string[]).includes(value);

function readTheme(): Theme {
  try {
    const t = localStorage.getItem(THEME_KEY);
    return isTheme(t) ? t : "auto";
  } catch {
    return "auto";
  }
}

function applyTheme(theme: Theme): void {
  if (theme === "auto") delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = theme;
  const btn = document.getElementById("theme-toggle");
  if (btn) {
    btn.title = `Theme: ${theme}`;
    btn.ariaLabel = btn.title;
  }
}

export function initTheme(): void {
  applyTheme(readTheme());
  document.getElementById("theme-toggle")?.addEventListener("click", () => {
    // biome-ignore lint/style/noNonNullAssertion: the modulus keeps it in range
    const next = THEMES[(THEMES.indexOf(readTheme()) + 1) % THEMES.length]!;
    try {
      localStorage.setItem(THEME_KEY, next);
    } catch {
      /* private mode: the choice just does not survive the page */
    }
    applyTheme(next);
  });
}
