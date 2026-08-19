/** Where this page is, which is the only thing the HTML tells the script. */

export const body = document.body;

/** The site root, relative to this page. `frame.rs` writes it on `<body>`. */
export const ROOT = body.dataset.root ?? "./";

/** The module this page documents, or `""` on the pages that document none. */
export const MODULE = body.dataset.module ?? "";

/** An absolute URL for something at `name` under the site root. */
export const url = (name: string): string => new URL(ROOT + name, location.href).href;
