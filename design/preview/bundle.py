#!/usr/bin/env python3
"""Inline the preview's stylesheet, script and icon into one standalone file.

The preview exists to be looked at, and the places it gets looked at (a chat
attachment, a published page) serve a single file with no siblings. This does
not touch what ships: `lean-doc build` writes the three assets separately.

    python3 bundle.py [out.html]
"""

import base64
import pathlib
import sys

HERE = pathlib.Path(__file__).parent


def bundle(page: str = "module.html") -> str:
    html = (HERE / page).read_text(encoding="utf-8")
    css = (HERE / "style.css").read_text(encoding="utf-8")
    js = (HERE / "app.js").read_text(encoding="utf-8")
    icon = base64.b64encode((HERE / "favicon.svg").read_bytes()).decode()

    html = html.replace('<link rel="stylesheet" href="style.css">', f"<style>\n{css}\n</style>")
    html = html.replace('<script type="module" src="app.js"></script>', f'<script type="module">\n{js}\n</script>')
    return html.replace('href="favicon.svg"', f'href="data:image/svg+xml;base64,{icon}"')


if __name__ == "__main__":
    out = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "preview-bundle.html")
    text = bundle(sys.argv[2] if len(sys.argv) > 2 else "module.html")
    out.write_text(text, encoding="utf-8")
    print(f"{out}: {len(text)} bytes")
