/**
 * `search.html`: the same index, rendered into the page instead of a dropdown.
 *
 * There is no second input — the one in the top bar is the input, seeded from
 * `?q=` so a submitted form and a typed query land in the same place. Two boxes
 * on a search page is a question about which one is real.
 */
import { searchData } from "./data.js";
import { resultItem } from "./result-item.js";
import { search } from "./search.js";

const DEBOUNCE_MS = 90;
/** Past this the list stops being a list and starts being the index again. */
const MAX_ROWS = 200;

export function initSearchPage(): void {
  const list = document.getElementById("page-results");
  const note = document.getElementById("page-note");
  const input = document.getElementById("search-input") as HTMLInputElement | null;
  if (!list || !input) return;

  // The dropdown would cover the results it duplicates. Removing it also makes
  // `initSearch` a no-op, which is why this runs first.
  document.getElementById("search-results")?.remove();

  const seed = new URLSearchParams(location.search).get("q");
  if (seed && !input.value) input.value = seed;

  const render = async (): Promise<void> => {
    const query = input.value.trim().toLowerCase();
    list.textContent = "";
    if (query.length < 2) {
      if (note) note.textContent = "Type at least two characters.";
      return;
    }
    const data = await searchData();
    if (!data) {
      if (note) note.textContent = "The search index could not be loaded.";
      return;
    }
    const hits = search(data.index, query);
    for (const id of hits.slice(0, MAX_ROWS)) list.append(resultItem(data, id));
    if (note) {
      note.textContent =
        hits.length === 0
          ? "No matching declaration."
          : hits.length > MAX_ROWS
            ? `${hits.length} matches, showing the first ${MAX_ROWS}.`
            : `${hits.length} match${hits.length === 1 ? "" : "es"}.`;
    }
  };

  let timer = 0;
  input.addEventListener("input", () => {
    clearTimeout(timer);
    timer = setTimeout(() => void render(), DEBOUNCE_MS);
  });
  input.form?.addEventListener("submit", (e) => {
    // Staying on the page is the whole point; a reload would refetch the index.
    e.preventDefault();
    void render();
  });
  input.focus();
  void render();
}
