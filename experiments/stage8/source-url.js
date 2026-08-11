/**
 * stage 8 — the single external supply point for the source revision.
 *
 * The revision must not live in the page bytes (approach.md §5.6): if it did,
 * every commit would invalidate all 432 pages. So it lives here, in ONE file
 * shared by the whole tree, and this script rewrites the `{{SOURCE_URL}}`
 * placeholder in the source links of whichever page loaded it. Updating the
 * revision is a write to this file alone.
 *
 * Scheduling is decided by the tag that loads the file, via `data-schedule`:
 *   "auto"     - rewrite immediately when the parse is already finished
 *                (module/defer execution), otherwise on DOMContentLoaded.
 *                This is the shape a product would ship. Used by M1 and M2.
 *   "listener" - always defer to a DOMContentLoaded listener. Used only by the
 *                stage 8 negative control M3, whose tag sits *after* doc-gen4's
 *                jump-src.js so that its listener registers second.
 *
 * The file is loaded both as a classic script (M2) and as a module (M1, M3),
 * so it must stay valid in both: no `import.meta`, no `export`.
 */
const SOURCE_URL =
  "https://github.com/FujiHaruka/information-theory/blob/573793b243fb1343636088eb62d1789ab2b14cec";
const TOKEN = "{{SOURCE_URL}}";
// doc-gen4 emits the two source-link shapes as `<div class="gh_link"><a href>`
// (one per declaration) and `<p class="gh_nav_link"><a href>` (one per module).
const SELECTOR = ".gh_link a[href], .gh_nav_link a[href]";

function rewriteSourceLinks() {
  const t0 = performance.now();
  const anchors = document.querySelectorAll(SELECTOR);
  let rewritten = 0;
  for (const a of anchors) {
    // jump-src.js reads getAttribute("href"), not the resolved .href property,
    // so the attribute is what has to change.
    const href = a.getAttribute("href");
    if (href !== null && href.startsWith(TOKEN)) {
      a.setAttribute("href", SOURCE_URL + href.slice(TOKEN.length));
      rewritten += 1;
    }
  }
  const t1 = performance.now();
  window.__sourceUrlRewrite = {
    matched: anchors.length,
    rewritten: rewritten,
    ms: t1 - t0,
    readyState: document.readyState,
  };
  console.log(
    `[SOURCE-URL] rewrote ${rewritten}/${anchors.length} in ${(t1 - t0).toFixed(4)} ms`,
  );
}

const tag = document.currentScript ||
  document.querySelector("script[data-schedule]");
const schedule = (tag && tag.dataset.schedule) || "auto";
if (schedule === "listener" || document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", rewriteSourceLinks);
  console.log(
    `[SOURCE-URL] deferred to DOMContentLoaded (schedule=${schedule}, readyState=${document.readyState})`,
  );
} else {
  console.log(
    `[SOURCE-URL] running now (schedule=${schedule}, readyState=${document.readyState})`,
  );
  rewriteSourceLinks();
}
