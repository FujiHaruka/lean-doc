/**
 * stage 8 — build the four page trees (B0 / M1 / M2 / M3).
 *
 * The four trees come from the SAME `pages-ph` bytes; the only difference is
 * the `<script>` tag added to `<head>` (B0 gets none). The `pages-ph` original
 * is never written to - it is copied first.
 *
 *   B0  no injection at all                                        (baseline)
 *   M1  <script type="module" src=source-url.js data-schedule=auto>
 *       placed BEFORE jump-src.js  -> defer execution, rewrites immediately
 *   M2  <script src=source-url.js data-schedule=auto>  (synchronous)
 *       placed BEFORE jump-src.js  -> registers a DOMContentLoaded listener
 *                                     while the parser is still running
 *   M3  <script type="module" src=source-url.js data-schedule=listener>
 *       placed AFTER jump-src.js   -> registers its DOMContentLoaded listener
 *                                     *after* jump-src.js registered its own
 *                                     (negative control: must break)
 *   M3b <script type="module" src=source-url.js data-schedule=auto>
 *       placed AFTER jump-src.js   -> control on the control. The README calls
 *                                     this case out by name as NOT being M3:
 *                                     defer runs in document order but still
 *                                     before DOMContentLoaded, so it is in
 *                                     time. It isolates "listener registered
 *                                     late" from "tag placed late".
 *
 * Each site root also gets doc-gen4's static assets (jump-src.js verbatim) and
 * source-url.js, so the tree can be served over HTTP exactly as a real site.
 *
 * Usage:
 *   deno run --allow-read --allow-write --allow-run build-sites.ts \
 *     --pages <pages-ph> --static <doc-gen4/static> --script <source-url.js> \
 *     --out <work dir>
 */
const argv = Deno.args.slice();
const opt = (n: string, d = "") => {
  const i = argv.indexOf(n);
  return i >= 0 ? argv[i + 1] : d;
};

const PAGES = opt("--pages");
const STATIC = opt("--static");
const SCRIPT = opt("--script");
const OUT = opt("--out");

const CONFIGS = ["b0", "m1", "m2", "m3", "m3b"] as const;
type Config = typeof CONFIGS[number];

// The prefix differs by page depth ("./", ".././", "../.././", ...), so it is
// captured from the jump-src.js tag that is already in the page.
const JUMP_TAG = /<script type="module" src="([^"]*)jump-src\.js"><\/script>/;

function tagFor(cfg: Config, prefix: string): string {
  switch (cfg) {
    case "m1":
      return `<script type="module" src="${prefix}source-url.js" data-schedule="auto"></script>`;
    case "m2":
      return `<script src="${prefix}source-url.js" data-schedule="auto"></script>`;
    case "m3":
      return `<script type="module" src="${prefix}source-url.js" data-schedule="listener"></script>`;
    case "m3b":
      return `<script type="module" src="${prefix}source-url.js" data-schedule="auto"></script>`;
    default:
      return "";
  }
}

async function* walk(dir: string, base = dir): AsyncGenerator<string> {
  for await (const e of Deno.readDir(dir)) {
    const p = `${dir}/${e.name}`;
    if (e.isDirectory) yield* walk(p, base);
    else yield p.slice(base.length + 1);
  }
}

async function copyTree(src: string, dst: string) {
  const cp = new Deno.Command("/bin/cp", { args: ["-R", src, dst] });
  const { code, stderr } = await cp.output();
  if (code !== 0) throw new Error(new TextDecoder().decode(stderr));
}

const stats: Record<string, unknown>[] = [];

for (const cfg of CONFIGS) {
  const root = `${OUT}/${cfg}`;
  await Deno.remove(root, { recursive: true }).catch(() => {});
  await copyTree(PAGES, root);
  // doc-gen4 static assets (jump-src.js among them) go in verbatim.
  for await (const rel of walk(STATIC)) {
    await Deno.mkdir(`${root}/${rel}`.replace(/\/[^/]*$/, ""), {
      recursive: true,
    });
    await Deno.copyFile(`${STATIC}/${rel}`, `${root}/${rel}`);
  }
  if (cfg !== "b0") await Deno.copyFile(SCRIPT, `${root}/source-url.js`);

  let files = 0, patched = 0, unmatched: string[] = [];
  for await (const rel of walk(root)) {
    if (!rel.endsWith(".html")) continue;
    files += 1;
    if (cfg === "b0") continue;
    const path = `${root}/${rel}`;
    const html = await Deno.readTextFile(path);
    const m = html.match(JUMP_TAG);
    if (!m) {
      unmatched.push(rel);
      continue;
    }
    const tag = tagFor(cfg, m[1]);
    // M1/M2 go before the jump-src.js tag, M3/M3b after it.
    const replacement = cfg.startsWith("m3") ? `${m[0]}${tag}` : `${tag}${m[0]}`;
    await Deno.writeTextFile(path, html.replace(JUMP_TAG, replacement));
    patched += 1;
  }
  stats.push({ config: cfg, htmlFiles: files, patched, unmatched });
  console.log(
    `${cfg}: html=${files} patched=${patched} unmatched=${unmatched.length}`,
  );
}

await Deno.writeTextFile(
  `${OUT}/build-sites.json`,
  JSON.stringify(stats, null, 2) + "\n",
);
