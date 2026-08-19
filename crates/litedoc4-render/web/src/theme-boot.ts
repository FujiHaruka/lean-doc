/**
 * Applies the stored theme before the first paint.
 *
 * Inlined into `<head>` by `frame.rs` as a **classic** `<script>`, not a module:
 * a module is deferred, and a theme applied after paint is a flash of the wrong
 * one. That is also why this is a second bundle rather than part of `app.js` —
 * it has to be the smallest thing that can run first.
 *
 * It writes nothing for `auto`, which is the absence of the attribute, and
 * nothing for a value that is not a theme at all — `localStorage` is shared
 * with everything else on the origin.
 */
import { PAINTED, THEME_KEY } from "./theme-key.js";

try {
  const stored = localStorage.getItem(THEME_KEY);
  if (stored !== null && (PAINTED as readonly string[]).includes(stored)) {
    document.documentElement.dataset.theme = stored;
  }
} catch {
  // Private mode, or storage disabled. The page renders in `auto`.
}
