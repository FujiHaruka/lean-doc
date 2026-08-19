/**
 * `404.html`: says what was asked for and offers the nearest declarations.
 *
 * The guess is the fragment when there is one (`…/Foo.html#Bar.baz` — the page
 * moved but the reader knows the name) and otherwise the file name with its
 * path separators read back as dots, which is exactly how a module page's URL
 * is built.
 */
import { searchData } from "./data.js";
import { resultItem } from "./result-item.js";
import { search } from "./search.js";

const MAX_ROWS = 20;

export async function initNotFound(): Promise<void> {
  const list = document.getElementById("how-about");
  const shown = document.getElementById("missing-path");
  if (shown) shown.textContent = location.pathname + location.hash;
  if (!list) return;

  const fragment = decodeURIComponent(location.hash.slice(1));
  const guess =
    fragment ||
    decodeURIComponent(location.pathname)
      .replace(/\.html$/, "")
      .split("/")
      .filter(Boolean)
      .join(".");
  const query = guess.trim().toLowerCase();
  if (query.length < 2) return;

  const data = await searchData();
  if (!data) return;
  // A prefix of the *last* component is what a moved declaration matches on, so
  // the plain scorer is already the right one.
  const hits = search(data.index, query).slice(0, MAX_ROWS);
  if (hits.length === 0) return;
  for (const id of hits) list.append(resultItem(data, id));
  document.getElementById("how-about-heading")?.removeAttribute("hidden");
}
