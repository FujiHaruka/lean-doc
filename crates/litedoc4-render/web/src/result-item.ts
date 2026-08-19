/** One result row — the same markup in the dropdown, on `search.html` and on `404.html`. */
import { kindAt, moduleAt, nameAt } from "./index-format.js";
import { url } from "./site.js";
import type { SearchData } from "./types.js";

export function resultItem(data: SearchData, id: number): HTMLLIElement {
  const li = document.createElement("li");
  const a = document.createElement("a");
  const declared = nameAt(data.index, id);
  const where = data.modules[moduleAt(data.index, id)];
  a.href = where ? `${url(where.p)}#${declared}` : `#${declared}`;
  const kind = document.createElement("span");
  kind.className = "kind";
  kind.textContent = kindAt(data.index, id);
  const name = document.createElement("span");
  name.textContent = declared;
  const module = document.createElement("span");
  module.className = "where";
  module.textContent = where?.n ?? "";
  a.append(kind, name, module);
  li.append(a);
  return li;
}
