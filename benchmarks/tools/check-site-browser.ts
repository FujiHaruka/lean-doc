#!/usr/bin/env -S deno run --allow-read --allow-net --allow-env --allow-run --allow-write
/**
 * Gate UI-3 and the half of M8 that static checks cannot see.
 *
 * `check-dead-links.py` and `check-site-closure.py` read the bytes. They cannot
 * tell whether the page *works*, and since M8-c most of what this site does is
 * decided at runtime: the module tree is drawn from `modules.json`, Instances
 * For is filled from `search-index.json` on first open, search runs in the
 * browser, and the theme toggle rewrites a root attribute. Every one of those
 * can be perfectly well-formed HTML and still be broken.
 *
 * It also closes the one gate M8 left open. `docs/plans/ui-redesign.md` records
 * UI-3 as **未判定 — CSS is written for 375 px but nobody looked at a browser**,
 * and "そう書いた" and "そう見える" are different claims.
 *
 * WHY A SERVER AND NOT file://
 *   The pages fetch their indexes. Under `file://` those fetches fail on CORS
 *   grounds, so a file:// run would report a broken site that is not broken —
 *   and, worse, would keep reporting it after somebody "fixed" it.
 *
 * WHY puppeteer-core AND NOT puppeteer
 *   `-core` does not download a browser. CI runners ship Chrome and this machine
 *   has one; a 150 MB download per run to drive a browser that is already there
 *   is not a dependency worth taking.
 *
 * usage:
 *   check-site-browser.ts <site dir> [--chrome PATH] [--port N] [--json FILE]
 */

import puppeteer, { type Browser, type Page } from "npm:puppeteer-core@24";

const CHROME_CANDIDATES = [
  Deno.env.get("CHROME_PATH"),
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  "/Applications/Chromium.app/Contents/MacOS/Chromium",
  "/usr/bin/google-chrome",
  "/usr/bin/google-chrome-stable",
  "/usr/bin/chromium",
  "/usr/bin/chromium-browser",
];

interface Failure {
  check: string;
  detail: string;
}

const failures: Failure[] = [];
const counts: Record<string, number | string> = {};

function ok(check: string, note = "") {
  console.log(`  ok   ${check}${note ? ` — ${note}` : ""}`);
}

function bad(check: string, detail: string) {
  console.log(`  FAIL ${check} — ${detail}`);
  failures.push({ check, detail });
}

function findChrome(explicit?: string): string {
  const candidates = explicit ? [explicit] : CHROME_CANDIDATES;
  for (const path of candidates) {
    if (!path) continue;
    try {
      if (Deno.statSync(path).isFile) return path;
    } catch {
      // try the next one
    }
  }
  console.error(
    "no Chrome found. Set CHROME_PATH or pass --chrome; tried:\n  " +
      candidates.filter(Boolean).join("\n  "),
  );
  Deno.exit(2);
}

/** A static file server over the generated site, so that fetch() works. */
function serve(root: string, port: number) {
  const types: Record<string, string> = {
    ".html": "text/html; charset=utf-8",
    ".css": "text/css; charset=utf-8",
    ".js": "text/javascript; charset=utf-8",
    ".json": "application/json; charset=utf-8",
    ".svg": "image/svg+xml",
  };
  return Deno.serve({ port, onListen: () => {} }, async (request) => {
    const url = new URL(request.url);
    let path = decodeURIComponent(url.pathname);
    if (path.endsWith("/")) path += "index.html";
    const file = `${root}${path}`;
    try {
      const body = await Deno.readFile(file);
      const dot = path.lastIndexOf(".");
      const type = types[path.slice(dot)] ?? "application/octet-stream";
      return new Response(body, { headers: { "content-type": type } });
    } catch {
      // The site ships its own 404 page; serving it keeps the gate honest about
      // what a visitor would see.
      try {
        const body = await Deno.readFile(`${root}/404.html`);
        return new Response(body, {
          status: 404,
          headers: { "content-type": types[".html"] },
        });
      } catch {
        return new Response("not found", { status: 404 });
      }
    }
  });
}

/** Every console error and uncaught exception a page produces. */
function watch(page: Page, sink: string[]) {
  page.on("console", (message) => {
    if (message.type() === "error") sink.push(`console: ${message.text()}`);
  });
  page.on("pageerror", (error) => sink.push(`pageerror: ${error.message}`));
  page.on("requestfailed", (request) => {
    sink.push(`requestfailed: ${request.url()} (${request.failure()?.errorText})`);
  });
}

async function main() {
  const args = [...Deno.args];
  let site = "";
  let chrome: string | undefined;
  let port = 8899;
  let jsonOut: string | undefined;
  while (args.length) {
    const arg = args.shift()!;
    if (arg === "--chrome") chrome = args.shift();
    else if (arg === "--port") port = Number(args.shift());
    else if (arg === "--json") jsonOut = args.shift();
    else if (!site) site = arg;
    else {
      console.error(`unexpected argument: ${arg}`);
      Deno.exit(2);
    }
  }
  if (!site) {
    console.error("usage: check-site-browser.ts <site dir> [--chrome PATH]");
    Deno.exit(2);
  }
  site = await Deno.realPath(site);

  const executablePath = findChrome(chrome);
  const server = serve(site, port);
  const base = `http://127.0.0.1:${port}`;

  // The pages to visit: the entry pages plus every module page the index names.
  const index = JSON.parse(await Deno.readTextFile(`${site}/modules.json`));
  const modulePages: string[] = index.modules.map((m: { p: string }) => m.p);
  const pages = [
    "index.html",
    "404.html",
    "search.html",
    "foundational_types.html",
    ...modulePages,
  ];
  counts["pages visited"] = pages.length;

  let browser: Browser | undefined;
  try {
    browser = await puppeteer.launch({
      executablePath,
      headless: true,
      args: ["--no-sandbox", "--disable-dev-shm-usage"],
    });

    // 1 — nothing errors on load, anywhere.
    const noisy: string[] = [];
    for (const path of pages) {
      const page = await browser.newPage();
      const problems: string[] = [];
      watch(page, problems);
      await page.goto(`${base}/${path}`, { waitUntil: "networkidle0" });
      if (problems.length) noisy.push(`${path}: ${problems.join("; ")}`);
      await page.close();
    }
    if (noisy.length) bad("no console errors", noisy.slice(0, 5).join(" | "));
    else ok("no console errors", `${pages.length} pages`);

    // A page that actually carries declarations. The root module of a package
    // is usually nothing but imports, and judging "is the prose there" against
    // a page with no prose on it is a test that fails for the wrong reason.
    //
    // The page is picked by looking at the pages rather than at the index
    // (`docs/plans/search-v2.md` P1 made the index bytes, and a gate that
    // decoded them would be a fourth reader of a format it is here to
    // distrust). A `class="decl"` section is the renderer's own statement that
    // the page carries a declaration.
    let first = modulePages[0];
    let declName = "";
    for (const page of modulePages) {
      const found = /<section\b[^>]*\bclass="decl"[^>]*\bid="([^"]*)"/.exec(
        await Deno.readTextFile(`${site}/${page}`),
      );
      if (found) {
        first = page;
        declName = found[1].replaceAll("&amp;", "&").replaceAll("&lt;", "<")
          .replaceAll("&gt;", ">").replaceAll("&quot;", '"');
        break;
      }
    }

    // 2 — the module tree is drawn from modules.json rather than shipped.
    {
      const page = await browser.newPage();
      await page.goto(`${base}/${first}`, { waitUntil: "networkidle0" });
      const items = await page.$$eval(
        "#module-tree a, #module-tree li",
        (nodes) => nodes.length,
      ).catch(() => 0);
      if (items > 0) ok("module tree is drawn", `${items} nodes`);
      else bad("module tree is drawn", "#module-tree is empty after load");
      await page.close();
    }

    // 3 — search finds a declaration that is in the index. **Two paths, and
    // they render into different elements**: the top bar's dropdown writes into
    // `#search-results`, while `search.html` removes that element on purpose
    // (two boxes on a search page is a question about which one is real) and
    // renders into `#page-results` instead. Checking only one of them leaves the
    // other free to break.
    for (const [where, from, into] of [
      ["dropdown", first, "#search-results"],
      ["search page", "search.html", "#page-results"],
    ] as const) {
      const wanted: string = declName;
      const term = wanted.split(".").pop() ?? "";
      const page = await browser.newPage();
      await page.goto(`${base}/${from}`, { waitUntil: "networkidle0" });
      const input = await page.$("#search-input");
      if (!input || !term) {
        bad(`search returns a hit (${where})`, "no #search-input or an empty index");
      } else {
        await input.click();
        await input.type(term, { delay: 30 });
        // The index loads lazily on the first keystroke.
        const found = await page
          .waitForFunction(
            (name: string, selector: string) =>
              (document.querySelector(selector)?.textContent ?? "").includes(name),
            { timeout: 8000 },
            wanted,
            into,
          )
          .then(() => true)
          .catch(() => false);
        if (found) {
          ok(`search returns a hit (${where})`, `"${term}" -> ${wanted}`);
        } else {
          // Say what the box did show: "no hit" is not a diagnosis.
          const shown = await page.evaluate((selector: string) => ({
            results: (document.querySelector(selector)?.textContent ?? "").slice(0, 200),
            present: !!document.querySelector(selector),
            value: document.querySelector<HTMLInputElement>("#search-input")?.value,
          }), into);
          bad(
            `search returns a hit (${where})`,
            `typed "${term}" (input="${shown.value}"), wanted ${wanted}, ` +
              `${into} present=${shown.present} showed "${shown.results}"`,
          );
        }
      }
      await page.close();
    }

    // 3b — the byte searcher ranks what the string one ranked.
    //
    // `docs/plans/search-v2.md` P1 replaced a scorer over JS strings with one
    // over the bytes of `search-index.bin`, and the entire argument for the
    // change was that it does not change the answer. So the oracle is the
    // scorer it replaced, **frozen** below.
    //
    // Neither side of this comparison reads the index format: the expected list
    // is computed from `declarations/name-map.json`, the actual one is read out
    // of the rendered page. An encoder and a decoder that agreed with each
    // other about the wrong thing still fail here.
    {
      const nameMap = JSON.parse(
        await Deno.readTextFile(`${site}/declarations/name-map.json`),
      ) as Record<string, string>;
      const own = new Set(
        (JSON.parse(await Deno.readTextFile(`${site}/modules.json`)).modules as { n: string }[])
          .map((m) => m.n),
      );
      // `sort()` is UTF-16 code unit order, which is the order the index is
      // written in and therefore the order equal scores fall back to.
      const names = Object.keys(nameMap).filter((name) => own.has(nameMap[name])).sort();

      // **FROZEN — the pre-P1 scorer from `assets/app.js`, verbatim.** Do not
      // edit it to agree with a change in the searcher: catching that is the
      // whole point.
      const frozenScore = (name: string, query: string) => {
        const lower = name.toLowerCase();
        const last = lower.slice(lower.lastIndexOf(".") + 1);
        if (last.startsWith(query)) return 3000 - last.length;
        if (lower.startsWith(query)) return 2000 - lower.length;
        const at = lower.indexOf(query);
        if (at >= 0) return 1000 - at;
        return -1;
      };
      const frozenSearch = (query: string) =>
        names
          .map((name, i) => [frozenScore(name, query), name, i] as [number, string, number])
          .filter(([score]) => score > 0)
          .sort((a, b) => b[0] - a[0] || a[1].length - b[1].length || a[2] - b[2])
          .map(([, name]) => name);

      // Queries taken from the corpus, so this says something on any package:
      // a prefix of a last component (tier 1), a prefix of a whole name (tier
      // 2), something from the middle of one (tier 3), and one that misses.
      const queries: string[] = [];
      for (const name of names.slice(0, 6)) {
        const last = (name.split(".").pop() ?? "").toLowerCase();
        if (last.length >= 2) queries.push(last.slice(0, Math.min(4, last.length)));
        const whole = name.toLowerCase();
        if (whole.length >= 3) queries.push(whole.slice(0, 3));
        if (last.length >= 4) queries.push(last.slice(1, 4));
      }
      queries.push("zzqq");
      const asked = [...new Set(queries)].filter((q) => q.length >= 2).slice(0, 12);

      const disagreements: string[] = [];
      let compared = 0;
      let typed = 0;
      const page = await browser.newPage();
      for (const [i, query] of asked.entries()) {
        // Two ways in, and they are different code paths. Loading `?q=` scores
        // the whole index once; typing the query one character at a time makes
        // every keystroke after the second narrow the previous keystroke's
        // hits (`docs/plans/search-v2.md` P2). A narrowing that drops a hit it
        // should have kept is only visible the second way.
        const byTyping = i < 3;
        if (byTyping) {
          await page.goto(`${base}/search.html`, { waitUntil: "networkidle0" });
          const box = await page.$("#search-input");
          if (!box) {
            disagreements.push(`${query}: no #search-input`);
            continue;
          }
          await box.click();
          // Slower than the 90 ms debounce, so that every character runs a
          // search rather than being coalesced into one.
          await box.type(query, { delay: 130 });
          await new Promise((r) => setTimeout(r, 500));
          typed++;
        } else {
          await page.goto(`${base}/search.html?q=${encodeURIComponent(query)}`, {
            waitUntil: "networkidle0",
          });
        }
        const settled = await page
          .waitForFunction(
            () => (document.querySelector("#page-note")?.textContent ?? "").length > 0,
            { timeout: 8000 },
          )
          .then(() => true)
          .catch(() => false);
        if (!settled) {
          disagreements.push(`${query}: the page never reported a result`);
          continue;
        }
        const shown = (await page.$$eval(
          "#page-results li a",
          (rows) => rows.map((row) => row.children[1]?.textContent ?? ""),
        )) as string[];
        const note = await page.$eval("#page-note", (el) => el.textContent ?? "");
        const want = frozenSearch(query);
        compared++;
        const top = Math.min(30, want.length, shown.length);
        for (let i = 0; i < top; i++) {
          if (shown[i] !== want[i]) {
            disagreements.push(
              `${query}: row ${i} is ${shown[i]}, the frozen scorer says ${want[i]}`,
            );
            break;
          }
        }
        const counted = /^(\d+) match/.exec(note);
        const total = counted ? Number(counted[1]) : /No matching/.test(note) ? 0 : want.length;
        if (total !== want.length) {
          disagreements.push(`${query}: the page counted ${total}, the frozen scorer ${want.length}`);
        }
      }
      await page.close();
      if (compared === 0) {
        bad("the byte searcher ranks like the frozen one", "no query was compared");
      } else if (disagreements.length) {
        bad(
          "the byte searcher ranks like the frozen one",
          `${disagreements.length} of ${compared}: ${disagreements.slice(0, 3).join(" | ")}`,
        );
      } else {
        ok(
          "the byte searcher ranks like the frozen one",
          `${compared} queries over ${names.length} declarations, ${typed} typed one character at a time`,
        );
      }
    }

    // 4 — Instances For fills in on open (M8-c changed where it reads from, and
    // the previous published site had this silently broken for 245 pages).
    {
      const page = await browser.newPage();
      await page.goto(`${base}/${first}`, { waitUntil: "networkidle0" });
      const filled = await page.evaluate(async () => {
        const block = document.querySelector<HTMLDetailsElement>(
          'details[data-fill="instances-for"], details[data-fill="instances"]',
        );
        if (!block) return "none";
        block.open = true;
        block.dispatchEvent(new Event("toggle"));
        await new Promise((r) => setTimeout(r, 750));
        return block.querySelector("ul")?.innerHTML?.length ? "filled" : "empty";
      });
      if (filled === "filled") ok("instances fill on open");
      else if (filled === "none") ok("instances fill on open", "no block on this page");
      else bad("instances fill on open", "the list stayed empty");
      await page.close();
    }

    // 4b — a `Used by` block fills in on open, from the file of its own.
    //
    // Feature-sweep C-2. The same shape as check 4 and for the same reason: the
    // markup ships empty, so "the block is on the page" and "the block answers"
    // are different questions and only this one asks the second. The fixture
    // declaration is chosen because something in the package really does use it
    // — a block that filled with "None" everywhere would pass a test that only
    // looked for text.
    {
      const page = await browser.newPage();
      const usedByPage = pages.find((p) => p.endsWith("Basic.html")) ?? first;
      await page.goto(`${base}/${usedByPage}`, { waitUntil: "networkidle0" });
      const filled = await page.evaluate(async () => {
        const hosts = [...document.querySelectorAll<HTMLElement>('[data-fill="used-by"]')];
        if (hosts.length === 0) return { hosts: 0, named: 0, empty: 0 };
        for (const host of hosts) (host as HTMLDetailsElement).open = true;
        // The fill is a `toggle` listener that awaits a fetch; poll rather than
        // guess a delay.
        for (let tick = 0; tick < 60; tick++) {
          const items = document.querySelectorAll('[data-fill="used-by"] li');
          if (items.length >= hosts.length) break;
          await new Promise((done) => setTimeout(done, 50));
        }
        let named = 0;
        let empty = 0;
        for (const host of hosts) {
          const items = [...host.querySelectorAll("li")];
          if (items.some((li) => li.querySelector("a"))) named++;
          else if (items.some((li) => li.textContent === "None")) empty++;
        }
        return { hosts: hosts.length, named, empty };
      });
      counts["used-by blocks"] = filled.hosts;
      counts["used-by blocks with a name"] = filled.named;
      if (filled.hosts === 0) {
        bad("Used by fills in", `${usedByPage} carries no used-by block`);
      } else if (filled.named + filled.empty < filled.hosts) {
        bad(
          "Used by fills in",
          `${filled.hosts - filled.named - filled.empty} of ${filled.hosts} blocks stayed empty ` +
            "— neither a name nor `None` arrived",
        );
      } else if (filled.named === 0) {
        bad(
          "Used by fills in",
          `all ${filled.hosts} blocks answered "None" — the map reached the page but says nothing, ` +
            "which is what a wrong key looks like",
        );
      } else {
        ok("Used by fills in", `${filled.named} of ${filled.hosts} blocks name a user`);
      }
      await page.close();
    }

    // 5 — the theme toggle actually changes the document.
    {
      const page = await browser.newPage();
      await page.goto(`${base}/${first}`, { waitUntil: "networkidle0" });
      const changed = await page.evaluate(async () => {
        const before = document.documentElement.getAttribute("data-theme");
        document.querySelector<HTMLElement>("#theme-toggle")?.click();
        await new Promise((r) => setTimeout(r, 100));
        return before !== document.documentElement.getAttribute("data-theme");
      });
      if (changed) ok("theme toggle changes the document");
      else bad("theme toggle changes the document", "data-theme did not move");
      await page.close();
    }

    // 5b — the themes' actual colours, not just that the attribute moves.
    //
    // Check 5 clicks the toggle and asks whether `data-theme` changed. That is
    // the mechanism; what a reader gets is the contrast, and a theme can rewrite
    // every colour and still be unreadable. **Both** themes are measured,
    // because a palette that is only ever looked at in one of them is exactly
    // how the other one rots (`docs/plans/unverified-sweep.md` U3).
    //
    // Threshold: WCAG 2.1 SC 1.4.3, 4.5:1 for body-sized text. The elements are
    // named rather than crawled, and **a name that matches nothing fails** — a
    // check that measured zero elements would pass for the wrong reason.
    for (const theme of ["light", "dark"]) {
      const page = await browser.newPage();
      await page.goto(`${base}/${first}`, { waitUntil: "networkidle0" });
      const rows = await page.evaluate((wanted) => {
        document.documentElement.setAttribute("data-theme", wanted);
        const parse = (value: string): number[] | null => {
          const inside = value.match(/rgba?\(([^)]+)\)/);
          if (!inside) return null;
          const parts = inside[1].split(",").map((v) => parseFloat(v.trim()));
          return [parts[0], parts[1], parts[2], parts.length > 3 ? parts[3] : 1];
        };
        // sRGB -> relative luminance, WCAG 2.1 relative-luminance definition.
        const channel = (v: number) => {
          const c = v / 255;
          return c <= 0.04045 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
        };
        const luminance = (rgb: number[]) =>
          0.2126 * channel(rgb[0]) + 0.7152 * channel(rgb[1]) + 0.0722 * channel(rgb[2]);
        // The painted background is the nearest ancestor with a non-transparent
        // one; an element's own `background-color` is usually rgba(0,0,0,0).
        const background = (el: Element): number[] => {
          let node: Element | null = el;
          while (node) {
            const rgba = parse(getComputedStyle(node).backgroundColor);
            if (rgba && rgba[3] > 0) return rgba;
            node = node.parentElement;
          }
          return [255, 255, 255, 1];
        };
        const contrast = (fg: number[], bg: number[]) => {
          const pair = [luminance(fg), luminance(bg)].sort((a, b) => b - a);
          return (pair[0] + 0.05) / (pair[1] + 0.05);
        };
        const visible = (el: Element) => {
          const box = el.getBoundingClientRect();
          return box.width > 0 && box.height > 0;
        };
        return ["body", ".doc", ".decl-name", ".fn", ".src", "main a"].map((selector) => {
          const el = [...document.querySelectorAll(selector)].find(visible);
          if (!el) return { selector, found: false, ratio: 0, fg: "", bg: "" };
          const style = getComputedStyle(el);
          const fg = parse(style.color) ?? [0, 0, 0, 1];
          const bg = background(el);
          return {
            selector,
            found: true,
            ratio: Math.round(contrast(fg, bg) * 100) / 100,
            fg: style.color,
            bg: `rgb(${bg[0]}, ${bg[1]}, ${bg[2]})`,
          };
        });
      }, theme);

      for (const row of rows) {
        const label = `${theme}: ${row.selector} is readable`;
        counts[`${theme} ${row.selector} contrast`] = row.ratio;
        if (!row.found) bad(label, "no element matched the selector");
        else if (row.ratio >= 4.5) ok(label, `${row.ratio}:1  ${row.fg} on ${row.bg}`);
        else bad(label, `${row.ratio}:1 (want 4.5:1)  ${row.fg} on ${row.bg}`);
      }
      await page.close();
    }

    // 6 — UI-3, the gate ui-redesign.md left 未判定.
    for (const width of [375, 1440]) {
      const page = await browser.newPage();
      await page.setViewport({ width, height: 800 });
      await page.goto(`${base}/${first}`, { waitUntil: "networkidle0" });
      const overflow = await page.evaluate(
        () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
      );
      counts[`overflow at ${width}px`] = overflow;
      if (overflow <= 1) ok(`no horizontal scroll at ${width}px`);
      else bad(`no horizontal scroll at ${width}px`, `${overflow}px of overflow`);
      await page.close();
    }

    // 7 — the prose is readable with JavaScript off (the M8-c gate).
    {
      const page = await browser.newPage();
      await page.setJavaScriptEnabled(false);
      await page.goto(`${base}/${first}`, { waitUntil: "domcontentloaded" });
      const declarations = await page.$$eval(
        "section.decl",
        (nodes) => nodes.length,
      );
      if (declarations > 0) ok("readable without JavaScript", `${declarations} declarations`);
      else bad("readable without JavaScript", "no declaration is in the HTML");
      await page.close();
    }

    // 8b — MathML is drawn by the browser rather than merely present.
    //
    // `docs/plans/feature-sweep.md` C-1. `litedoc4-md` converts `$…$` at build
    // time and ships no MathJax, no KaTeX and no math web font, so the whole
    // feature rests on an assumption nothing else here checks: **that this
    // browser lays out MathML Core natively.** A `<math>` element in a browser
    // that does not is `display: inline` with no glyphs and **zero width** —
    // the page still validates, the markup is still right, and the formula is
    // simply gone. This is the only check that can see that.
    //
    // Also asserted: the box is *wider than its own LaTeX source would be*, so
    // that a browser rendering the elements as run-together text (which has a
    // width, and would pass a `> 0` test) does not pass.
    //
    // The page is named rather than searched for. A search that finds no math
    // page would report "ok, 0 formulas" — the shape CLAUDE.md calls
    // 「skip で緑を返さない」— so a missing page is a failure here.
    {
      const mathPage = pages.find((p) => p.endsWith("Math.html"));
      if (!mathPage) {
        bad("MathML is laid out", "no page named Math.html — the fixture is gone");
      } else {
        const page = await browser.newPage();
        await page.goto(`${base}/${mathPage}`, { waitUntil: "networkidle0" });
        const boxes = await page.$$eval("math", (nodes) =>
          nodes.map((node) => {
            const rect = node.getBoundingClientRect();
            return {
              width: rect.width,
              height: rect.height,
              children: node.childElementCount,
              tag: (node.firstElementChild?.tagName ?? "").toLowerCase(),
            };
          }));
        counts["math elements"] = boxes.length;
        const flat = boxes.filter((b) => b.width < 1 || b.height < 1);
        const unstructured = boxes.filter((b) => b.children === 0);
        if (boxes.length === 0) {
          bad("MathML is laid out", `${mathPage} holds no <math> element`);
        } else if (flat.length) {
          bad(
            "MathML is laid out",
            `${flat.length} of ${boxes.length} <math> elements have no box — ` +
              "this browser is not drawing MathML Core",
          );
        } else if (unstructured.length) {
          bad(
            "MathML is laid out",
            `${unstructured.length} <math> elements have no child elements`,
          );
        } else {
          const widest = Math.max(...boxes.map((b) => b.width));
          ok(
            "MathML is laid out",
            `${boxes.length} formulas, widest ${widest.toFixed(0)}px, ` +
              `first child <${boxes[0].tag}>`,
          );
        }
        await page.close();
      }
    }

    // 8 — the monospace stack can draw what a Lean package puts in a signature.
    //
    // `ui-redesign.md` 決定 2 dropped the JuliaMono web font on the **assumption**
    // that the system stack renders the 178 non-ASCII characters the measurement
    // target's pages contain (`mono-charset.json`, regenerable with
    // `mono-charset.py`). That assumption had only ever been looked at on macOS,
    // and this runner is ubuntu-latest — the Linux machine was free the whole
    // time.
    //
    // **The character set comes from the target, not from the site under test.**
    // The e2e fixture is deliberately tiny, so judging the font stack by what it
    // happens to contain would pass a stack that cannot draw `ℝ`.
    //
    // Two different questions, and only one of them is a failure:
    //
    // * **Is there a glyph at all?** 決定 2 is a bet that there is. A character
    //   that draws nothing is unreadable, so it fails.
    // * **Is the advance the monospace advance?** 決定 2 says outright that
    //   「**字幅は崩れうる**」 — a proportional fallback mixing in is an accepted
    //   cost, not a regression. So this is counted and reported, never failed on.
    //   The count is what UI-V1 (subset and vendor JuliaMono) would be decided on.
    {
      const charset = JSON.parse(
        await Deno.readTextFile(new URL("./mono-charset.json", import.meta.url)),
      ) as { chars: string; distinct: number; source: string };

      const page = await browser.newPage();
      await page.goto(`${base}/${first}`, { waitUntil: "networkidle0" });
      const measured = await page.evaluate((chars: string) => {
        // The font a signature is actually drawn in, read off the page rather
        // than written down here: a change to `--mono` must move this check.
        const probe = document.querySelector(".sig, code, pre, .decl-name") ??
          document.body;
        const style = getComputedStyle(probe);
        const font = `${style.fontSize} ${style.fontFamily}`;

        const box = Math.ceil(parseFloat(style.fontSize) * 2) || 32;
        const canvas = document.createElement("canvas");
        canvas.width = box;
        canvas.height = box;
        const g = canvas.getContext("2d", { willReadFrequently: true })!;

        // A span carrying every character, in the same font, for the platform
        // font question below. Off-screen rather than hidden: `display: none`
        // is not laid out, so nothing would be shaped and no font reported.
        const span = document.createElement("span");
        span.id = "mono-probe";
        span.style.cssText =
          `position:absolute;left:-9999px;top:0;white-space:pre;font:${font}`;
        span.textContent = chars;
        document.body.appendChild(span);

        const paint = () => {
          g.font = font;
          g.fillStyle = "#000";
          g.textBaseline = "middle";
        };
        paint();
        const unit = g.measureText("M").width;

        const blank: string[] = [];
        const offWidth: string[] = [];
        let total = 0;
        for (const ch of chars) {
          total += 1;
          const width = g.measureText(ch).width;
          g.clearRect(0, 0, box, box);
          paint();
          g.fillText(ch, 1, box / 2);
          const pixels = g.getImageData(0, 0, box, box).data;
          let inked = false;
          for (let i = 3; i < pixels.length; i += 4) {
            if (pixels[i] !== 0) {
              inked = true;
              break;
            }
          }
          if (!inked) blank.push(ch);
          // Half a pixel: sub-pixel advances differ between platforms even
          // inside one font, and a fallback is never that close.
          if (Math.abs(width - unit) > 0.5) offWidth.push(ch);
        }
        return { font, unit, total, blank, offWidth };
      }, charset.chars);

      // Which families actually drew it. This is the diagnosis a width number
      // cannot give: Chrome reports the real fonts used for a laid-out node, so
      // a failure names the family that is missing the glyphs instead of saying
      // that something was wide.
      let families = "";
      try {
        const cdp = await page.createCDPSession();
        await cdp.send("DOM.enable");
        await cdp.send("CSS.enable");
        const { root } = await cdp.send("DOM.getDocument") as {
          root: { nodeId: number };
        };
        const { nodeId } = await cdp.send("DOM.querySelector", {
          nodeId: root.nodeId,
          selector: "#mono-probe",
        }) as { nodeId: number };
        const { fonts } = await cdp.send("CSS.getPlatformFontsForNode", {
          nodeId,
        }) as { fonts: { familyName: string; glyphCount: number }[] };
        families = fonts
          .map((f) => `${f.familyName}:${f.glyphCount}`)
          .join(", ");
        await cdp.detach();
      } catch (error) {
        families = `unavailable (${error})`;
      }

      counts["mono charset size"] = measured.total;
      counts["mono glyphs missing"] = measured.blank.length;
      counts["mono off-width glyphs"] = measured.offWidth.length;
      console.log(`  mono font: ${measured.font}`);
      console.log(`  mono platform fonts: ${families || "none reported"}`);
      if (measured.offWidth.length) {
        console.log(
          `  mono off-width (accepted by 決定 2): ${
            measured.offWidth.slice(0, 40).join("")
          }`,
        );
      }
      if (measured.blank.length) {
        bad(
          "every non-ASCII the target emits has a glyph",
          `${measured.blank.length} of ${measured.total} draw nothing: ${
            measured.blank.slice(0, 20).join("")
          } — fonts: ${families}`,
        );
      } else {
        ok(
          "every non-ASCII the target emits has a glyph",
          `${measured.total} characters, ${measured.offWidth.length} off-width`,
        );
      }
      await page.close();
    }
  } finally {
    await browser?.close();
    await server.shutdown();
  }

  console.log();
  for (const [key, value] of Object.entries(counts)) {
    console.log(`  ${key}: ${value}`);
  }
  if (jsonOut) {
    await Deno.writeTextFile(
      jsonOut,
      JSON.stringify({ counts, failures }, null, 2) + "\n",
    );
  }
  if (failures.length) {
    console.error(`\nBROWSER GATE: ${failures.length} failed`);
    Deno.exit(1);
  }
  console.log("\nBROWSER GATE: ok");
}

await main();
