/**
 * Fills the two instance blocks the moment one is opened.
 *
 * They are `<details>` on purpose: the maps live in a file of their own, and a
 * reader who never opens one never pays for it.
 *
 * An empty result says "none" rather than deleting the block. The reader has
 * already clicked by the time the answer arrives, and a section that vanishes
 * under the cursor reads as a bug — "there are no instances" is the answer
 * they asked for.
 */
import { instanceMaps, searchData } from "./data.js";
import { findNames, moduleAt } from "./index-format.js";
import { url } from "./site.js";
import type { SearchData } from "./types.js";

export function initInstances(): void {
  const hosts = document.querySelectorAll<HTMLElement>(
    '[data-fill="instances"], [data-fill="instances-for"]',
  );
  for (const host of hosts) {
    host.addEventListener(
      "toggle",
      async () => {
        const ul = host.querySelector("ul");
        if (!ul) return;
        // The maps and the links come from different files: the names are in
        // `instances.json`, and turning a name into a URL needs the search
        // index. Both are fetched here and nowhere earlier.
        const [maps, data] = await Promise.all([instanceMaps(), searchData()]);
        const map = host.dataset.fill === "instances" ? maps?.instances : maps?.instancesFor;
        const key = host.dataset.name ?? "";
        const names = map?.[key] ?? [];
        ul.textContent = "";
        if (names.length === 0) {
          const li = document.createElement("li");
          li.className = "search-empty";
          li.textContent = maps ? "None" : "Index unavailable";
          ul.append(li);
          return;
        }
        // One pass over the index resolves every name in the block, rather
        // than one pass per name.
        const found = data ? findNames(data.index, names) : new Map<string, number>();
        for (const name of names) ul.append(declItem(data, name, found.get(name)));
      },
      { once: true },
    );
  }
}

/** A name in an instance block, linked to wherever the index says it is. */
export function declItem(
  data: SearchData | null,
  name: string,
  id: number | undefined,
): HTMLLIElement {
  const li = document.createElement("li");
  const a = document.createElement("a");
  a.textContent = name;
  // A name the index does not have is still worth a link: the page it is on is
  // the page the reader is already looking at.
  const where = data && id !== undefined ? data.modules[moduleAt(data.index, id)] : undefined;
  a.href = where ? `${url(where.p)}#${name}` : `#${name}`;
  li.append(a);
  return li;
}
