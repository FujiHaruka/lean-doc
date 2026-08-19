/** The narrow-viewport navigation drawer. */
import { body } from "./site.js";

export function initDrawer(): void {
  const toggle = document.getElementById("nav-toggle");
  const scrim = document.getElementById("scrim");
  if (!toggle) return;

  const set = (open: boolean): void => {
    body.dataset.nav = open ? "open" : "closed";
    toggle.setAttribute("aria-expanded", String(open));
    if (scrim) scrim.hidden = !open;
  };
  set(false);

  toggle.addEventListener("click", () => set(body.dataset.nav !== "open"));
  scrim?.addEventListener("click", () => set(false));
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && body.dataset.nav === "open") set(false);
  });
  // A tap on a link inside the drawer navigates; leaving it open would cover
  // the page it just went to.
  document.getElementById("sidebar")?.addEventListener("click", (e) => {
    if ((e.target as Element | null)?.closest("a")) set(false);
  });
}
