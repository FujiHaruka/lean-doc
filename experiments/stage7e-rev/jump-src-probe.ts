/**
 * stage 7e-rev, point B — what does `jump-src.js` hand to `location.replace`
 * when the page carries `{{SOURCE_URL}}` instead of a revision?
 *
 * THIS IS NOT A BROWSER. It is:
 *   * the *verbatim bytes* of doc-gen4's `static/jump-src.js`, evaluated,
 *   * against a *real HTML parse* of a real generated page (deno-dom), so
 *     `getElementById` / `querySelector(".gh_link a")` / `getAttribute("href")`
 *     are a genuine DOM implementation, not a mock,
 *   * with `document.location` / `window.location` and the DOMContentLoaded
 *     dispatch **shimmed**, because deno-dom has neither a URL nor an event loop.
 *
 * So it answers "what value does the runtime read out of the attribute, and
 * where would that navigate", and it does NOT answer "in what order do the
 * page's module scripts and DOMContentLoaded listeners actually run in a
 * browser". Report the two separately.
 *
 * Usage:
 *   deno run --allow-read --allow-net --allow-env jump-src-probe.ts \
 *     --js <path/to/jump-src.js> --page <page.html> --page-url <url> --id <declId>
 */
import { DOMParser } from "jsr:@b-fuze/deno-dom@0.1.56";

const argv = Deno.args.slice();
const opt = (n: string, d = "") => {
  const i = argv.indexOf(n);
  return i >= 0 ? argv[i + 1] : d;
};

const JS = opt("--js");
const PAGE = opt("--page");
const PAGE_URL = opt("--page-url", "https://example.org/doc/InformationTheory/Asymptotic.html");
const ID = opt("--id");

const src = await Deno.readTextFile(JS);
const html = await Deno.readTextFile(PAGE);
const doc = new DOMParser().parseFromString(html, "text/html")!;

// --- the shimmed half -------------------------------------------------------
const listeners: Array<() => void> = [];
const pageUrl = new URL(PAGE_URL);
const loc = {
  hash: `#${ID}`,
  search: "?jump=src",
  href: `${pageUrl.href}?jump=src#${ID}`,
  replace: (t: string) => {
    replaced = t;
  },
};
let replaced: string | null = null;

const documentShim = new Proxy(doc as unknown as Record<string, unknown>, {
  get(target, prop) {
    if (prop === "location") return loc;
    if (prop === "addEventListener") {
      return (ev: string, fn: () => void) => {
        if (ev === "DOMContentLoaded") listeners.push(fn);
      };
    }
    const v = Reflect.get(target, prop);
    return typeof v === "function" ? v.bind(target) : v;
  },
});
const windowShim = { location: loc };

// --- run the real file ------------------------------------------------------
const run = new Function("document", "window", "URLSearchParams", src);
run(documentShim, windowShim, URLSearchParams);
console.log(`registered DOMContentLoaded listeners: ${listeners.length}`);
for (const fn of listeners) fn();

// --- what the DOM actually holds --------------------------------------------
const el = (doc as unknown as { getElementById(i: string): unknown }).getElementById(ID) as
  | { querySelector(s: string): { getAttribute(a: string): string | null } | null }
  | null;
const a = el?.querySelector(".gh_link a") ?? null;
const raw = a?.getAttribute("href") ?? null;

console.log(`page                 ${PAGE}`);
console.log(`id                   ${ID}`);
console.log(`.gh_link a @href     ${raw}`);
console.log(`location.replace()   ${replaced}`);
if (replaced !== null) {
  let resolved: string;
  try {
    resolved = new URL(replaced, pageUrl).href;
  } catch (e) {
    resolved = `<unresolvable: ${e}>`;
  }
  console.log(`resolves against page to`);
  console.log(`                     ${resolved}`);
}
