/** The "Imported by" block: a fact about the whole package, not this page. */
import { modules } from "./data.js";
import { MODULE, url } from "./site.js";
import type { ModuleEntry } from "./types.js";

/** The ` 12` after a summary. Shared with nothing — the instance blocks count differently. */
export function countBadge(n: number): HTMLSpanElement {
  const span = document.createElement("span");
  span.className = "count";
  span.textContent = ` ${n}`;
  return span;
}

export async function initImportedBy(): Promise<void> {
  const host = document.querySelector<HTMLElement>('[data-fill="imported-by"]');
  if (!host) return;
  const data = await modules();
  const self = data?.modules?.find((m) => m.n === MODULE);
  const names = (self?.i ?? [])
    .map((i) => data?.modules[i])
    .filter((m): m is ModuleEntry => m !== undefined);
  if (names.length === 0) {
    // Nothing in this package imports it. Dropping the whole block is safe
    // here — this runs before the reader has had a chance to reach for it,
    // unlike the instance blocks.
    host.remove();
    return;
  }
  host.hidden = false;
  const ul = host.querySelector("ul");
  if (!ul) return;
  for (const m of [...names].sort((a, b) => a.n.localeCompare(b.n))) {
    const li = document.createElement("li");
    const a = document.createElement("a");
    a.href = url(m.p);
    a.textContent = m.n;
    li.append(a);
    ul.append(li);
  }
  host.querySelector("summary")?.append(countBadge(names.length));
}
