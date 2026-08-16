#!/usr/bin/env python3
"""Take one page out of a real site tree and make it work on its own.

`bundle.py` inlines the hand-written preview; this one inlines a page
`lean-doc` actually produced, which is the only way to look at the tree's own
output somewhere that serves a single file (a chat attachment, a published
page).

Inlining the assets is not enough: the tree, the search and the "Imported by"
block are `fetch`ed, and a lone file has nothing to fetch from. So the two
index files are embedded as data and `fetch` is shimmed to answer from them —
**in the wrapper, never in `app.js`**, which must not learn that previews
exist.

    python3 bundle-site.py <site-dir> <page-relative-path> [out.html]
"""

import base64
import json
import pathlib
import sys

SHIM = """<script>
(() => {
  const canned = window.__leanDocCanned;
  const real = window.fetch.bind(window);
  window.fetch = (input, init) => {
    const name = String(input && input.url ? input.url : input).split("/").pop();
    if (Object.hasOwn(canned, name)) {
      return Promise.resolve(new Response(JSON.stringify(canned[name]),
        { headers: { "content-type": "application/json" } }));
    }
    return real(input, init);
  };
})();
</script>"""


def bundle(site: pathlib.Path, page: str) -> str:
    path = site / page
    html = path.read_text(encoding="utf-8")
    # The page's own root prefix, as `page_root` built it: one `../` per
    # component below the top, and the assets hang off it.
    depth = len(pathlib.PurePosixPath(page).parts) - 1
    root = "../" * depth + "./" if depth else "./"

    css = (site / "style.css").read_text(encoding="utf-8")
    js = (site / "app.js").read_text(encoding="utf-8")
    icon = base64.b64encode((site / "favicon.svg").read_bytes()).decode()
    canned = {
        name: json.loads((site / name).read_text(encoding="utf-8"))
        for name in ("modules.json", "search-index.json")
        if (site / name).exists()
    }

    html = html.replace(f'<link rel="stylesheet" href="{root}style.css">', f"<style>\n{css}\n</style>")
    html = html.replace(f'href="{root}favicon.svg"', f'href="data:image/svg+xml;base64,{icon}"')
    html = html.replace(
        f'<script type="module" src="{root}app.js"></script>',
        f"<script>window.__leanDocCanned = {json.dumps(canned)};</script>\n{SHIM}\n"
        f'<script type="module">\n{js}\n</script>',
    )
    if f'src="{root}app.js"' in html or f'href="{root}style.css"' in html:
        raise SystemExit(f"an asset reference survived inlining — is the root {root!r} right?")
    return html


if __name__ == "__main__":
    if len(sys.argv) < 3:
        raise SystemExit(__doc__)
    site = pathlib.Path(sys.argv[1])
    page = sys.argv[2]
    out = pathlib.Path(sys.argv[3] if len(sys.argv) > 3 else "site-bundle.html")
    text = bundle(site, page)
    out.write_text(text, encoding="utf-8")
    print(f"{out}: {len(text)} bytes")
