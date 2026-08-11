/**
 * stage 8 — local static HTTP server for the four page trees.
 *
 * `file://` is deliberately not used: module-script and origin behaviour there
 * is not the behaviour a published site has. Every request is appended to a
 * JSONL log, which is the server-side source of truth for
 *   * the 404 that config B0's broken link produces, and
 *   * whether `source-url.js` comes back to the network on page 2..N (V7).
 *
 * Usage:
 *   deno run --allow-read --allow-net --allow-write serve.ts \
 *     --root <dir> --port <n> --log <jsonl> [--delay-source-url <ms>]
 *     [--cache-control <header value>]
 */
const argv = Deno.args.slice();
const opt = (n: string, d = "") => {
  const i = argv.indexOf(n);
  return i >= 0 ? argv[i + 1] : d;
};

const ROOT = opt("--root");
const PORT = Number(opt("--port", "8931"));
const LOG = opt("--log");
const DELAY = Number(opt("--delay-source-url", "0"));
const CACHE_CONTROL = opt("--cache-control", "");

const TYPES: Record<string, string> = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".svg": "image/svg+xml",
  ".json": "application/json",
  ".bmp": "image/bmp",
};

const logFile = LOG ? await Deno.open(LOG, { write: true, create: true, truncate: true }) : null;
const enc = new TextEncoder();
async function jlog(rec: Record<string, unknown>) {
  if (!logFile) return;
  await logFile.write(enc.encode(JSON.stringify(rec) + "\n"));
}

Deno.serve({ port: PORT, hostname: "127.0.0.1", onListen: () => console.log(`listening ${PORT} root=${ROOT}`) }, async (req) => {
  const url = new URL(req.url);
  let path = decodeURIComponent(url.pathname);
  if (path.endsWith("/")) path += "index.html";
  const file = `${ROOT}${path}`;
  const ext = path.slice(path.lastIndexOf("."));
  const isSourceUrl = path.endsWith("/source-url.js");
  if (isSourceUrl && DELAY > 0) await new Promise((r) => setTimeout(r, DELAY));
  let status = 200;
  let body: Uint8Array | string;
  try {
    body = await Deno.readFile(file);
  } catch {
    status = 404;
    body = `404 ${path}`;
  }
  await jlog({
    t: Date.now(),
    method: req.method,
    path,
    query: url.search,
    status,
    ifNoneMatch: req.headers.get("if-none-match"),
    cacheControlReq: req.headers.get("cache-control"),
  });
  const headers: Record<string, string> = {
    "content-type": status === 404 ? "text/plain; charset=utf-8" : (TYPES[ext] ?? "application/octet-stream"),
  };
  if (status === 200 && CACHE_CONTROL) headers["cache-control"] = CACHE_CONTROL;
  return new Response(body, { status, headers });
});
