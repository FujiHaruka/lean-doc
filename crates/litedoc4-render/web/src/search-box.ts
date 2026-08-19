/** The search box in the top bar, and its dropdown. */
import { searchData } from "./data.js";
import { resultItem } from "./result-item.js";
import { search } from "./search.js";

/** How long after the last keystroke the walk runs. */
const DEBOUNCE_MS = 90;
/** The dropdown is a peek, not the result list; `search.html` is that. */
const MAX_ROWS = 30;

export function initSearch(): void {
  const input = document.getElementById("search-input") as HTMLInputElement | null;
  const list = document.getElementById("search-results");
  if (!input || !list) return;

  let items: HTMLLIElement[] = [];
  let active = -1;
  let timer = 0;

  const close = (): void => {
    list.hidden = true;
    list.textContent = "";
    items = [];
    active = -1;
  };

  const run = async (): Promise<void> => {
    const query = input.value.trim().toLowerCase();
    if (query.length < 2) return close();
    const data = await searchData();
    if (!data) return close();

    const hits = search(data.index, query);
    list.textContent = "";
    if (hits.length === 0) {
      const li = document.createElement("li");
      li.className = "search-empty";
      li.textContent = "No matching declaration";
      list.append(li);
      list.hidden = false;
      return;
    }
    items = hits.slice(0, MAX_ROWS).map((id) => {
      const li = resultItem(data, id);
      list.append(li);
      return li;
    });
    active = -1;
    list.hidden = false;
  };

  const move = (delta: number): void => {
    if (items.length === 0) return;
    items[active]?.removeAttribute("aria-selected");
    active = (active + delta + items.length) % items.length;
    const row = items[active];
    if (!row) return;
    row.setAttribute("aria-selected", "true");
    row.scrollIntoView({ block: "nearest" });
  };

  input.addEventListener("input", () => {
    clearTimeout(timer);
    timer = setTimeout(() => void run(), DEBOUNCE_MS);
  });
  input.addEventListener("focus", () => void searchData()); // warm the index
  input.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      move(1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      move(-1);
    } else if (e.key === "Escape") {
      close();
      input.blur();
    } else if (e.key === "Enter" && active >= 0) {
      e.preventDefault();
      items[active]?.querySelector("a")?.click();
    }
  });
  document.addEventListener("click", (e) => {
    if (!(e.target as Element | null)?.closest(".search")) close();
  });

  // `/` focuses search, the way every documentation site with a search box
  // does — but not while the reader is typing somewhere else.
  document.addEventListener("keydown", (e) => {
    const tag = document.activeElement?.tagName;
    if (e.key === "/" && tag !== "INPUT" && tag !== "TEXTAREA") {
      e.preventDefault();
      input.focus();
      input.select();
    }
  });
}
