// What the search index costs a browser, measured in a browser.
//
// `docs/plans/search-v2.md` §12 set a retreat line — the binary index goes in
// only if the peak drops by 5x — and the numbers behind it were Deno's V8
// (`heapUsed` / `external`), which is not a browser's accounting. This is the
// same question asked of Chrome, through the real page: load `search.html`,
// let `app.js` fetch and read the index, and read `JSHeapUsedSize` off the
// DevTools protocol on either side of a forced collection.
//
// Two directories are compared so that the two answers come from the same
// browser in the same run. Anything else — two Chromes, two sessions, a number
// from a document — is a comparison of conditions rather than of formats.
//
// usage:
//   deno run -A benchmarks/tools/measure-index-memory.ts <site A> <site B> \
//     [--chrome PATH] [--port N] [--query Q] [--runs N]
import puppeteer, { type Browser } from "npm:puppeteer-core@24";

const CHROME_CANDIDATES = [
  Deno.env.get("CHROME_PATH"),
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  "/Applications/Chromium.app/Contents/MacOS/Chromium",
  "/usr/bin/google-chrome",
  "/usr/bin/google-chrome-stable",
  "/usr/bin/chromium",
];

function findChrome(): string {
  for (const candidate of CHROME_CANDIDATES) {
    if (candidate && (() => { try { return Deno.statSync(candidate).isFile; } catch { return false; } })()) {
      return candidate;
    }
  }
  throw new Error("no Chrome found; set CHROME_PATH");
}

function serve(root: string, port: number) {
  const types: Record<string, string> = {
    ".html": "text/html; charset=utf-8",
    ".css": "text/css; charset=utf-8",
    ".js": "text/javascript; charset=utf-8",
    ".json": "application/json; charset=utf-8",
    ".svg": "image/svg+xml",
  };
  return Deno.serve({ port, onListen: () => {} }, async (request) => {
    const path = decodeURIComponent(new URL(request.url).pathname);
    try {
      const body = await Deno.readFile(`${root}${path}`);
      const type = types[path.slice(path.lastIndexOf("."))] ?? "application/octet-stream";
      return new Response(body, { headers: { "content-type": type } });
    } catch {
      return new Response("not found", { status: 404 });
    }
  });
}

const positional = Deno.args.filter((a) => !a.startsWith("--"));
const flag = (name: string, fallback: string) => {
  const at = Deno.args.indexOf(`--${name}`);
  return at >= 0 ? Deno.args[at + 1] : fallback;
};
const [siteA, siteB] = positional;
if (!siteA || !siteB) {
  console.error("usage: measure-index-memory.ts <site A> <site B> [--query Q] [--runs N]");
  Deno.exit(2);
}
const query = flag("query", "inf");
const runs = Number(flag("runs", "5"));
const basePort = Number(flag("port", "8901"));

const median = (xs: number[]) => xs.slice().sort((a, b) => a - b)[xs.length >> 1];

async function measure(browser: Browser, base: string, withQuery: boolean) {
  const page = await browser.newPage();
  // Off, because two runs of this tool are usually two different `app.js`, and
  // a number that did not move because the browser served the previous one is
  // the most convincing wrong measurement there is (this happened).
  await page.setCacheEnabled(false);
  const cdp = await page.createCDPSession();
  const url = withQuery ? `${base}/search.html?q=${encodeURIComponent(query)}` : `${base}/search.html`;
  await page.goto(url, { waitUntil: "networkidle0" });
  if (withQuery) {
    // The index is fetched on the first render, so the number is only about a
    // loaded index once the page has said what it found.
    await page
      .waitForFunction(() => (document.querySelector("#page-note")?.textContent ?? "").length > 0, {
        timeout: 15000,
      })
      .catch(() => {});
  }
  // Read once before collecting anything: that is what the page was holding
  // when it finished, garbage included, and it is the closest this can get to
  // the peak — the JSON path has to materialise the whole body as a string
  // before it can parse it, and that string is garbage the moment it has.
  const atRest = (await page.metrics()).JSHeapUsedSize ?? 0;
  // Twice: the first collection frees the fetch machinery, the second what the
  // first made unreachable.
  await cdp.send("HeapProfiler.collectGarbage");
  await cdp.send("HeapProfiler.collectGarbage");
  const metrics = await page.metrics();
  const shown = withQuery
    ? await page.$eval("#page-note", (el) => el.textContent ?? "").catch(() => "")
    : "";
  await page.close();
  return { heap: metrics.JSHeapUsedSize ?? 0, atRest, shown };
}

const chrome = findChrome();
const browser = await puppeteer.launch({
  executablePath: chrome,
  headless: true,
  args: ["--no-sandbox", "--disable-dev-shm-usage"],
});
console.log(`chrome  ${chrome}`);
console.log(`query   ${JSON.stringify(query)}   runs ${runs}`);

for (const [i, site] of [siteA, siteB].entries()) {
  const port = basePort + i;
  const server = serve(site, port);
  const base = `http://127.0.0.1:${port}`;
  const idle: number[] = [];
  const loaded: number[] = [];
  const idleAtRest: number[] = [];
  const loadedAtRest: number[] = [];
  let note = "";
  for (let run = 0; run < runs; run++) {
    const empty = await measure(browser, base, false);
    idle.push(empty.heap);
    idleAtRest.push(empty.atRest);
    const withIndex = await measure(browser, base, true);
    loaded.push(withIndex.heap);
    loadedAtRest.push(withIndex.atRest);
    note = withIndex.shown;
  }
  const kib = (n: number) => (n / 1024).toFixed(0).padStart(6);
  console.log(`\n${site}`);
  console.log(`  search.html, no query   ${kib(median(idle))} KiB   (${idle.map((n) => (n / 1024) | 0).join(" ")})`);
  console.log(`  search.html?q=${query}     ${kib(median(loaded))} KiB   (${loaded.map((n) => (n / 1024) | 0).join(" ")})`);
  console.log(`  the index costs         ${kib(median(loaded) - median(idle))} KiB retained`);
  console.log(`  before collecting       ${kib(median(loadedAtRest) - median(idleAtRest))} KiB`);
  console.log(`  the page reported       ${note.trim()}`);
  await server.shutdown();
}
await browser.close();
