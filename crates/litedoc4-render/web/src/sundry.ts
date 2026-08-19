/** Two behaviours that belong to no block in particular. */

/** `?jump=src#Name` lands on the declaration's source instead of its entry. */
export function jumpToSource(): void {
  if (new URLSearchParams(location.search).get("jump") !== "src") return;
  const target = document.getElementById(decodeURIComponent(location.hash.slice(1)));
  const src = target?.querySelector<HTMLAnchorElement>(".src")?.href;
  if (src) location.replace(src);
}

/** A `<details>` that is closed on paper is a paragraph the reader cannot get. */
export function openForPrint(): void {
  addEventListener("beforeprint", () => {
    for (const d of document.querySelectorAll<HTMLDetailsElement>("details:not([open])")) {
      d.open = true;
      d.dataset.printOpened = "1";
    }
  });
  addEventListener("afterprint", () => {
    for (const d of document.querySelectorAll<HTMLDetailsElement>("details[data-print-opened]")) {
      d.open = false;
      delete d.dataset.printOpened;
    }
  });
}
