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
    const searchIndex = JSON.parse(
      await Deno.readTextFile(`${site}/search-index.json`),
    );
    // The module array lives in `modules.json` and only there
    // (`docs/plans/search-v2.md` P0); a declaration names its module by
    // subscript into it, which is exactly what the page's script does.
    const moduleList = JSON.parse(
      await Deno.readTextFile(`${site}/modules.json`),
    ).modules as { n: string; p: string }[];
    const firstDecl: [string, number, number] | undefined = searchIndex.decls?.[0];
    const first = firstDecl ? moduleList[firstDecl[2]].p : modulePages[0];

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
      const wanted: string = firstDecl?.[0] ?? "";
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
