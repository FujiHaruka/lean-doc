/**
 * The four data files, split by when they are needed (plan 決定 5,
 * `docs/plans/search-v2.md` P0):
 *
 *   modules.json       every module, its page and what imports it.
 *                      Small, wanted immediately — it draws the tree.
 *   search-index.bin   every declaration and the kind vocabulary. Large,
 *                      wanted on the first keystroke. It has no module array
 *                      of its own: a declaration names its module by
 *                      subscript into `modules.json`'s, which is already here.
 *   instances.json     the two instance maps, wanted only when a reader opens
 *                      one of the two blocks — which most never do.
 *   declarations/used-by.json
 *                      who mentions each declaration. The largest of the four,
 *                      and wanted least often: only when a reader opens a
 *                      `Used by` block.
 *
 * None is cached in storage: they are ordinary GETs against the same origin
 * and the browser's HTTP cache is better at this than we are. (doc-gen4 tried
 * IndexedDB here and disabled it.)
 */
import { readIndex } from "./index-format.js";
import { url } from "./site.js";
import type { InstancesFile, ModulesFile, SearchData, SearchIndex, UsedByFile } from "./types.js";

let modulesPromise: Promise<ModulesFile | null> | null = null;
let declsPromise: Promise<SearchIndex | null> | null = null;
let instancesPromise: Promise<InstancesFile | null> | null = null;
let usedByPromise: Promise<UsedByFile | null> | null = null;

/** One GET, parsed, or `null` — every caller here treats a miss as "no data". */
const fetchJson = <T>(name: string): Promise<T | null> =>
  fetch(url(name))
    .then((r) => (r.ok ? (r.json() as Promise<T>) : Promise.reject(new Error(String(r.status)))))
    .catch(() => null);

/** `modules.json`, fetched at most once per page. */
export function modules(): Promise<ModulesFile | null> {
  modulesPromise ??= fetchJson<ModulesFile>("modules.json");
  return modulesPromise;
}

/** `search-index.bin`, fetched at most once per page, on demand. */
export function decls(): Promise<SearchIndex | null> {
  declsPromise ??= fetch(url("search-index.bin"))
    .then((r) => (r.ok ? r.arrayBuffer() : Promise.reject(new Error(String(r.status)))))
    .then((buffer) => readIndex(new Uint8Array(buffer)))
    .catch(() => null);
  return declsPromise;
}

/** `instances.json`, fetched only when a reader opens an instance block. */
export function instanceMaps(): Promise<InstancesFile | null> {
  instancesPromise ??= fetchJson<InstancesFile>("instances.json");
  return instancesPromise;
}

/** `declarations/used-by.json`, fetched only when a reader opens a Used by block. */
export function usedByMap(): Promise<UsedByFile | null> {
  usedByPromise ??= fetchJson<UsedByFile>("declarations/used-by.json");
  return usedByPromise;
}

/**
 * What every result row needs: the index, and the module array it points into.
 *
 * The two live in different files because they are wanted at different times —
 * the tree draws from `modules.json` before a reader has typed anything — but
 * a result row needs both, so this is where they meet. Asking for both at once
 * costs nothing after the first: `modules()` has already resolved.
 */
export async function searchData(): Promise<SearchData | null> {
  const [tree, index] = await Promise.all([modules(), decls()]);
  if (!tree?.modules || !index) return null;
  return { modules: tree.modules, index };
}
