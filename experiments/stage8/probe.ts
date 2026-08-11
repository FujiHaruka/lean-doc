/**
 * stage 8 — CDP harness. Opens the four page trees in real headless Chrome and
 * records what `jump-src.js` hands to `location.replace`.
 *
 * The verdict is the URL, never the rendering. Two independent observations of
 * it are kept for every case:
 *   1. `Page.frameRequestedNavigation` with reason "scriptInitiated" on the MAIN
 *      frame - it carries the full URL *including the fragment*, i.e. exactly
 *      the string `location.replace()` was called with.
 *   2. The document-level request that follows (`Fetch.requestPaused`), plus the
 *      HTTP status when the target stays same-origin.
 * A JS wrapper around `location.replace` is deliberately NOT used: `replace` is
 * a non-configurable own property of `Location` in Chrome (measured - see
 * hook-feasibility in the report), so it cannot be patched.
 *
 * Every request whose host is not the local server is FAILED, not forwarded.
 * The URL is recorded; nothing leaves the machine.
 *
 * Usage:
 *   deno run -A probe.ts --suite {nav|cost|v7} --port 8931 --cdp-port 9331 \
 *     --out <dir> [--runs N] [--tag NAME] [--cache-walk yes|no]
 */
import { Cdp, launchChrome, sleep } from "./cdp.ts";

const argv = Deno.args.slice();
const opt = (n: string, d = "") => {
  const i = argv.indexOf(n);
  return i >= 0 ? argv[i + 1] : d;
};

const CHROME = opt("--chrome", "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome");
const SUITE = opt("--suite", "nav");
const PORT = Number(opt("--port", "8931"));
const CDP_PORT = Number(opt("--cdp-port", "9331"));
const OUT = opt("--out", "/private/tmp/lean-doc-relay/w8");
const RUNS = Number(opt("--runs", "5"));
const TAG = opt("--tag", SUITE);
const CACHE_WALK = opt("--cache-walk", "yes") === "yes";
const UDD = opt("--user-data-dir", `${OUT}/udd-${TAG}`);
const SETTLE = Number(opt("--settle", "3000"));

const ORIGIN = `http://127.0.0.1:${PORT}`;

// Same page and declaration stage 7e-rev probed with a DOM shim.
const PAGE = "InformationTheory/Asymptotic.html";
const DECL = "InformationTheory.Asymptotic.DotEq";
// m3b is a control on the negative control: same placement as M3, but it
// rewrites immediately instead of from a listener. The README names this case
// explicitly as NOT being M3, so it is measured rather than assumed.
const CONFIGS = ["b0", "m1", "m2", "m3", "m3b"] as const;
const REV = "https://github.com/FujiHaruka/information-theory/blob/573793b243fb1343636088eb62d1789ab2b14cec";

/** Observational only: logs DOMContentLoaded registrations so listener ORDER is
 *  evidence rather than inference. Not installed for the cost suite. */
const ORDER_HOOK = `
(() => {
  const origAdd = document.addEventListener.bind(document);
  let seq = 0;
  document.addEventListener = function (type, fn, o) {
    if (type === "DOMContentLoaded") {
      const st = (new Error()).stack || "";
      const from = st.split("\\n").slice(1).map(s => s.trim()).filter(s => s.indexOf("<anonymous>") < 0)[0] || "?";
      console.log("[LISTENER] seq=" + (++seq) + " from=" + from);
    }
    return origAdd(type, fn, o);
  };
})();
`;

type Observation = {
  url: string;
  mainFrameId: string;
  locationReplaceUrls: string[];
  allFrameNavigations: { frameId: string; url: string; reason: string; isMain: boolean }[];
  documentRequests: { url: string; blocked: boolean }[];
  blockedExternal: string[];
  requestedUrls: string[];
  loadingFailed: { requestId: string; errorText: string; blockedReason?: string }[];
  localResponses: { url: string; status: number }[];
  console: string[];
  finalHref: string | null;
  rewrite: unknown;
};

function emptyObservation(url: string): Observation {
  return {
    url,
    mainFrameId: "",
    locationReplaceUrls: [],
    allFrameNavigations: [],
    documentRequests: [],
    blockedExternal: [],
    requestedUrls: [],
    loadingFailed: [],
    localResponses: [],
    console: [],
    finalHref: null,
    rewrite: null,
  };
}

/** One attached page target. The Fetch handler lives for the whole life of the
 *  session, so a request that arrives after a measurement window is still
 *  answered instead of hanging the renderer. */
class Session {
  cur: Observation = emptyObservation("");
  urlByRequestId = new Map<string, string>();
  constructor(public browser: Cdp, public targetId: string, public sessionId: string) {
    browser.on((m) => {
      if (m.sessionId !== this.sessionId) return;
      const p = (m.params ?? {}) as any;
      const o = this.cur;
      switch (m.method) {
        case "Runtime.consoleAPICalled":
          o.console.push((p.args ?? []).map((a: any) => a.value ?? a.description ?? JSON.stringify(a)).join(" "));
          break;
        case "Page.frameRequestedNavigation": {
          const isMain = p.frameId === o.mainFrameId;
          o.allFrameNavigations.push({ frameId: p.frameId, url: p.url, reason: p.reason, isMain });
          if (isMain && p.reason === "scriptInitiated") o.locationReplaceUrls.push(p.url);
          break;
        }
        case "Network.requestWillBeSent":
          o.requestedUrls.push(String(p.request?.url ?? ""));
          this.urlByRequestId.set(String(p.requestId), String(p.request?.url ?? ""));
          break;
        case "Network.loadingFailed":
          o.loadingFailed.push({
            requestId: this.urlByRequestId.get(String(p.requestId)) ?? String(p.requestId),
            errorText: String(p.errorText ?? ""),
            blockedReason: p.blockedReason,
          });
          break;
        case "Network.responseReceived":
          if (String(p.response?.url ?? "").startsWith(ORIGIN)) {
            o.localResponses.push({ url: p.response.url, status: p.response.status });
          }
          break;
        case "Fetch.requestPaused": {
          const u = String(p.request.url);
          const local = u.startsWith(ORIGIN);
          if (p.resourceType === "Document") o.documentRequests.push({ url: u, blocked: !local });
          if (local) {
            this.browser.post("Fetch.continueRequest", { requestId: p.requestId }, this.sessionId);
          } else {
            o.blockedExternal.push(u);
            this.browser.post("Fetch.failRequest", { requestId: p.requestId, errorReason: "BlockedByClient" }, this.sessionId);
          }
          break;
        }
      }
    });
  }

  async observe(url: string, settle = SETTLE): Promise<Observation> {
    this.cur = emptyObservation(url);
    const o = this.cur;
    const nav = await this.browser.send<{ frameId: string }>("Page.navigate", { url }, this.sessionId);
    o.mainFrameId = nav.frameId;
    // Retro-tag navigations that arrived before the frame id was known.
    for (const n of o.allFrameNavigations) {
      if (n.frameId === o.mainFrameId && !n.isMain) {
        n.isMain = true;
        if (n.reason === "scriptInitiated") o.locationReplaceUrls.push(n.url);
      }
    }
    await sleep(settle);
    o.finalHref = await this.evalJson<string>("location.href");
    o.rewrite = await this.evalJson<unknown>("JSON.stringify(window.__sourceUrlRewrite ?? null)", true);
    return o;
  }

  async evalJson<T>(expression: string, parse = false): Promise<T | null> {
    try {
      const r = await this.browser.send<any>("Runtime.evaluate", { expression, returnByValue: true }, this.sessionId);
      const v = r.result?.value ?? null;
      return parse && typeof v === "string" ? JSON.parse(v) : v;
    } catch {
      return null; // the document may have been replaced by a network error page
    }
  }

  close() {
    return this.browser.send("Target.closeTarget", { targetId: this.targetId });
  }
}

/** Hosts the doc-gen4 pages reach for. Enumerated from the `nav` suite's
 *  blockedExternal lists, and re-asserted at runtime by the cache suite. */
const EXTERNAL_BLOCK = [
  "*cdnjs.cloudflare.com*",
  "*cdn.jsdelivr.net*",
  "*github.com*",
  "*githubassets.com*",
  "*googleapis*",
  "*gstatic*",
];

async function newSession(browser: Cdp, hook: string | null, useFetch = true): Promise<Session> {
  const { targetId } = await browser.send<{ targetId: string }>("Target.createTarget", { url: "about:blank" });
  const { sessionId } = await browser.send<{ sessionId: string }>("Target.attachToTarget", { targetId, flatten: true });
  const s = new Session(browser, targetId, sessionId);
  await browser.send("Page.enable", {}, sessionId);
  await browser.send("Runtime.enable", {}, sessionId);
  await browser.send("Network.enable", {}, sessionId);
  if (useFetch) {
    await browser.send("Fetch.enable", { patterns: [{ urlPattern: "*", requestStage: "Request" }] }, sessionId);
  } else {
    // Enabling Fetch is itself a cache confound, so the cache suite blocks by
    // URL pattern instead and proves egress-freedom from Network events.
    await browser.send("Network.setBlockedURLs", { urls: EXTERNAL_BLOCK }, sessionId);
  }
  if (hook) await browser.send("Page.addScriptToEvaluateOnNewDocument", { source: hook }, sessionId);
  return s;
}

const results: Record<string, unknown> = {
  suite: SUITE,
  tag: TAG,
  origin: ORIGIN,
  page: PAGE,
  decl: DECL,
  startedAt: new Date().toISOString(),
};

await Deno.remove(UDD, { recursive: true }).catch(() => {});
const { proc, wsUrl, version } = await launchChrome(CHROME, CDP_PORT, UDD);
results.chrome = version;
results.deno = Deno.version;
const browser = await Cdp.connect(wsUrl);

// --------------------------------------------------------------- suite: hook
// Why the harness reads navigations instead of wrapping `location.replace`.
if (SUITE === "hook") {
  const s = await newSession(browser, null);
  await s.observe(`${ORIGIN}/m1/${PAGE}`, 1000);
  results.hook = await s.evalJson(
    `JSON.stringify((() => {
      const out = {};
      const d = Object.getOwnPropertyDescriptor(location, "replace");
      out.ownDescriptor = d ? { configurable: d.configurable, writable: d.writable, type: typeof d.value } : null;
      out.protoIsLocationPrototype = Object.getPrototypeOf(location) === Location.prototype;
      out.prototypeHasOwnReplace = Object.getOwnPropertyDescriptor(Location.prototype, "replace") !== undefined;
      try { Object.defineProperty(location, "replace", { configurable: true, value: function () {} }); out.patchOwn = "ok"; }
      catch (e) { out.patchOwn = String(e); }
      try { Object.defineProperty(Location.prototype, "replace", { configurable: true, value: function () {} }); out.patchPrototype = "ok"; }
      catch (e) { out.patchPrototype = String(e); }
      out.replaceStillOriginal = /native code/.test(Function.prototype.toString.call(location.replace));
      return out; })())`,
    true,
  );
  await s.close();
  console.log(JSON.stringify(results.hook, null, 2));
}

// ---------------------------------------------------------------- suite: nav
if (SUITE === "nav") {
  const cases: { name: string; url: string }[] = [];
  for (const cfg of CONFIGS) cases.push({ name: `${cfg}-jump`, url: `${ORIGIN}/${cfg}/${PAGE}?jump=src#${DECL}` });
  for (const cfg of CONFIGS) cases.push({ name: `${cfg}-nojump`, url: `${ORIGIN}/${cfg}/${PAGE}#${DECL}` });

  // Each case is opened RUNS times in a fresh target. A categorical result
  // still gets repeated: "measured once" is not a result.
  const out: Record<string, Observation[]> = {};
  for (const c of cases) {
    out[c.name] = [];
    for (let r = 0; r < RUNS; r++) {
      const s = await newSession(browser, ORDER_HOOK);
      out[c.name].push(await s.observe(c.url));
      await s.close();
    }
    const distinct = [...new Set(out[c.name].map((o) => JSON.stringify(o.locationReplaceUrls)))];
    console.log(`${c.name}: ${RUNS} runs, ${distinct.length} distinct outcome(s) -> ${distinct.join(" | ")}`);
  }
  results.cases = out;
}

// --------------------------------------------------------------- suite: cost
// V5. Measured WITHOUT `?jump=src`, because a navigation takes the page away.
if (SUITE === "cost") {
  const pages = [
    { label: "min", n: 1, path: "InformationTheory/Probability/TwoSidedExtension.html" },
    { label: "p50", n: 9, path: "InformationTheory/Fano/DPI.html" },
    { label: "target", n: 10, path: PAGE },
    { label: "max", n: 73, path: "InformationTheory/Shannon/TimeBandLimiting/Operator.html" },
    // The rewrite walks the whole document, so the page with the most links is
    // not necessarily the most expensive one. This is the largest page in the
    // tree by bytes (555,846 B) and it carries only 47 links.
    { label: "biggest", n: 47, path: "InformationTheory/Shannon/BroadcastChannel/OuterBoundTransport.html" },
  ];
  // performance.now() is coarsened in Chrome. Measure the floor so the numbers
  // are read against it instead of being over-read.
  const RES = `(() => {
      const s = new Set(); let prev = performance.now();
      for (let i = 0; i < 500000; i++) { const t = performance.now(); if (t !== prev) { s.add(+(t - prev).toFixed(6)); prev = t; } }
      return JSON.stringify([...s].sort((a,b)=>a-b).slice(0, 5));
    })()`;
  // A single pass is below the clock's resolution, so one timed region covers
  // K forward+backward passes. Forward and backward do the same work
  // (querySelectorAll + N setAttribute), so total/(2K) is the per-pass cost.
  const LOOP = (k: number) => `(() => {
      const REV = ${JSON.stringify(REV)};
      const TOKEN = "{{SOURCE_URL}}";
      const SEL = ".gh_link a[href], .gh_nav_link a[href]";
      function pass(from, to) {
        const anchors = document.querySelectorAll(SEL);
        let n = 0;
        for (const a of anchors) {
          const h = a.getAttribute("href");
          if (h !== null && h.startsWith(from)) { a.setAttribute("href", to + h.slice(from.length)); n++; }
        }
        return { matched: anchors.length, rewritten: n };
      }
      pass(REV, TOKEN);                       // back to the placeholder state
      const probe = pass(TOKEN, REV);         // one instrumented pass
      pass(REV, TOKEN);
      const t0 = performance.now();
      for (let i = 0; i < ${k}; i++) { pass(TOKEN, REV); pass(REV, TOKEN); }
      const t1 = performance.now();
      return JSON.stringify({ matched: probe.matched, rewritten: probe.rewritten,
        totalMs: t1 - t0, passes: ${2 * k}, msPerPass: (t1 - t0) / ${2 * k} });
    })()`;

  const out: Record<string, unknown> = {};
  const s0 = await newSession(browser, null);
  await s0.observe(`${ORIGIN}/m1/${PAGE}`, 1500);
  out.clockResolutionMs = await s0.evalJson(RES, true);
  await s0.close();

  for (const pg of pages) {
    const onLoad: unknown[] = [];
    const loop: unknown[] = [];
    let dom: unknown = null;
    for (let r = 0; r < RUNS; r++) {
      const s = await newSession(browser, null);
      const obs = await s.observe(`${ORIGIN}/m1/${pg.path}`, 1500);
      onLoad.push(obs.rewrite);
      loop.push(await s.evalJson(LOOP(200), true));
      if (r === 0) {
        // querySelectorAll walks the whole document, so page size is the other
        // input to the cost besides the number of links.
        dom = await s.evalJson(
          `JSON.stringify({ elements: document.querySelectorAll("*").length,
             htmlBytes: new Blob([document.documentElement.outerHTML]).size })`,
          true,
        );
      }
      await s.close();
    }
    out[pg.label] = { page: pg.path, tokensInFile: pg.n, dom, onLoad, loop };
    console.log(`${pg.label} (${pg.n}): ${JSON.stringify(loop)}`);
  }
  results.cost = out;
}

// ----------------------------------------------------------------- suite: v7
if (SUITE === "v7") {
  const TIMING = `JSON.stringify((() => { const n = performance.getEntriesByType("navigation")[0];
      return { responseEnd: n.responseEnd, domInteractive: n.domInteractive,
               domContentLoadedEventStart: n.domContentLoadedEventStart, domComplete: n.domComplete }; })())`;
  const timings: Record<string, unknown[]> = {};
  for (const cfg of ["b0", "m1", "m2", "m3"]) {
    const arr: unknown[] = [];
    for (let r = 0; r < RUNS; r++) {
      const s = await newSession(browser, null);
      await s.observe(`${ORIGIN}/${cfg}/${PAGE}`, 2000);
      arr.push(await s.evalJson(TIMING, true));
      await s.close();
    }
    timings[cfg] = arr;
    console.log(`${cfg}: ${JSON.stringify(arr)}`);
  }
  results.timings = timings;

  if (CACHE_WALK) {
    // The server log is the source of truth for "did it come back over the
    // wire"; this records what the renderer saw alongside it.
    const walkPages = [
      PAGE,
      "InformationTheory/Fano/DPI.html",
      "InformationTheory/Shannon/TimeBandLimiting/Operator.html",
      "InformationTheory/Probability/TwoSidedExtension.html",
      "InformationTheory.html",
    ];
    const walk: unknown[] = [];
    const s = await newSession(browser, null);
    for (const p of walkPages) {
      const o = await s.observe(`${ORIGIN}/m2/${p}`, 1500);
      walk.push({ page: p, sourceUrlResponses: o.localResponses.filter((x) => x.url.endsWith("source-url.js")) });
    }
    await s.close();
    results.cacheWalk = { config: "m2", pages: walkPages, observed: walk };
  }
}

// -------------------------------------------------------------- suite: cache
// V7, second half. The Fetch domain is NOT enabled here - intercepting every
// request is itself a cache confound. External hosts are blocked by URL
// pattern instead, and every URL the renderer asked for is recorded so that
// "nothing left the machine" stays checkable.
if (SUITE === "cache") {
  const walkPages = [
    PAGE,
    "InformationTheory/Fano/DPI.html",
    "InformationTheory/Shannon/TimeBandLimiting/Operator.html",
    "InformationTheory/Probability/TwoSidedExtension.html",
    "InformationTheory.html",
  ];
  const laps: unknown[] = [];
  const s = await newSession(browser, null, false);
  for (let lap = 0; lap < 2; lap++) {
    for (const p of walkPages) {
      const o = await s.observe(`${ORIGIN}/m2/${p}`, 1200);
      laps.push({
        lap,
        page: p,
        sourceUrlResponses: o.localResponses.filter((x) => x.url.endsWith("source-url.js")),
        externalRequested: o.requestedUrls.filter((u) => !u.startsWith(ORIGIN) && !u.startsWith("data:")),
        // proof that the external requests were aborted rather than sent
        externalFailed: o.loadingFailed.filter((f) => !f.requestId.startsWith(ORIGIN)),
      });
    }
  }
  await s.close();
  results.cacheWalk = { config: "m2", fetchDomain: "disabled", pages: walkPages, observed: laps };
  console.log(JSON.stringify(results.cacheWalk, null, 2));
}

results.finishedAt = new Date().toISOString();
await Deno.writeTextFile(`${OUT}/probe-${TAG}.json`, JSON.stringify(results, null, 2) + "\n");
console.log(`wrote ${OUT}/probe-${TAG}.json`);

browser.close();
proc.kill();
await proc.status;
