// lean-doc — every bit of behaviour the site has, in one module.
//
// Loaded as `type="module"`, so: deferred, strict, nothing leaked to globals.
// **The page works without it.** Navigation is links, the declarations are in
// the HTML, and the docstrings are already rendered. What this adds is the
// module tree, search, the theme toggle, and the three lists that cannot be
// placed statically because they are facts about the *whole* site rather than
// about the module being rendered (instances, instances-for, imported-by).
//
// Two data files, split by when they are needed (plan 決定 5):
//
//   modules.json       every module, its page and what imports it.
//                      Small, wanted immediately — it draws the tree.
//   search-index.json  every declaration, plus the instance maps.
//                      Large, wanted on the first keystroke or the first time
//                      an "Instances" block is opened.
//
// Neither is cached in storage: they are ordinary GETs against the same origin
// and the browser's HTTP cache is better at this than we are. (doc-gen4 tried
// IndexedDB here and disabled it.)

const body = document.body;
const ROOT = body.dataset.root ?? "./";
const MODULE = body.dataset.module ?? "";

const url = (name) => new URL(ROOT + name, location.href).href;

// ---------------------------------------------------------------- fetching

let modulesPromise = null;
let declsPromise = null;

/** `modules.json`, fetched at most once per page. */
function modules() {
  modulesPromise ??= fetch(url("modules.json"))
    .then((r) => (r.ok ? r.json() : Promise.reject(new Error(String(r.status)))))
    .catch(() => null);
  return modulesPromise;
}

/** `search-index.json`, fetched at most once per page, on demand. */
function decls() {
  declsPromise ??= fetch(url("search-index.json"))
    .then((r) => (r.ok ? r.json() : Promise.reject(new Error(String(r.status)))))
    .catch(() => null);
  return declsPromise;
}

// ------------------------------------------------------------------- theme

const THEME_KEY = "lean-doc-theme";
const THEMES = ["auto", "light", "dark"];

function readTheme() {
  try {
    const t = localStorage.getItem(THEME_KEY);
    return THEMES.includes(t) ? t : "auto";
  } catch {
    return "auto";
  }
}

function applyTheme(theme) {
  if (theme === "auto") delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = theme;
  const btn = document.getElementById("theme-toggle");
  if (btn) btn.title = btn.ariaLabel = `Theme: ${theme}`;
}

function initTheme() {
  applyTheme(readTheme());
  document.getElementById("theme-toggle")?.addEventListener("click", () => {
    const next = THEMES[(THEMES.indexOf(readTheme()) + 1) % THEMES.length];
    try {
      localStorage.setItem(THEME_KEY, next);
    } catch {
      /* private mode: the choice just does not survive the page */
    }
    applyTheme(next);
  });
}

// ------------------------------------------------------------------ drawer

function initDrawer() {
  const toggle = document.getElementById("nav-toggle");
  const scrim = document.getElementById("scrim");
  if (!toggle) return;

  const set = (open) => {
    body.dataset.nav = open ? "open" : "closed";
    toggle.setAttribute("aria-expanded", String(open));
    if (scrim) scrim.hidden = !open;
  };
  set(false);

  toggle.addEventListener("click", () => set(body.dataset.nav !== "open"));
  scrim?.addEventListener("click", () => set(false));
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && body.dataset.nav === "open") set(false);
  });
  // A tap on a link inside the drawer navigates; leaving it open would cover
  // the page it just went to.
  document.getElementById("sidebar")?.addEventListener("click", (e) => {
    if (e.target.closest("a")) set(false);
  });
}

// -------------------------------------------------------------- module tree

/**
 * Nests the flat module list on its dot-separated components.
 *
 * A name is both a node and a page: `A.B` can have a page *and* children, so
 * every node carries an optional page and an optional child map.
 */
function nest(list) {
  const rootNode = { children: new Map() };
  for (const m of list) {
    let node = rootNode;
    for (const part of m.n.split(".")) {
      let child = node.children.get(part);
      if (!child) node.children.set(part, (child = { children: new Map() }));
      node = child;
    }
    node.page = m;
  }
  return rootNode;
}

/**
 * One `<ul>` per level.
 *
 * **Not `<details>`/`<summary>`**, which is the obvious spelling and the wrong
 * one here: a module can be both a page and a parent (`Foo` and `Foo.Bar` both
 * exist), so its row has to carry a link *and* a disclosure. Put the link inside
 * a `<summary>` and clicking it both navigates and toggles — the toggle is the
 * summary's activation behaviour, not something a handler on the link can call
 * off. A button next to the link keeps the two targets apart, which is also what
 * a reader expects from a file tree.
 */
function treeHtml(node, prefix, here) {
  const ul = document.createElement("ul");
  for (const [part, child] of node.children) {
    const full = prefix ? `${prefix}.${part}` : part;
    const li = document.createElement("li");
    const row = document.createElement("div");
    row.className = "row";

    let sub = null;
    if (child.children.size > 0) {
      sub = treeHtml(child, full, here);
      // Open exactly the spine down to the current page; everything else stays
      // folded, or the sidebar is 432 lines long on arrival.
      sub.hidden = !(here === full || here.startsWith(`${full}.`));
      const twisty = document.createElement("button");
      twisty.type = "button";
      twisty.className = "twisty";
      twisty.setAttribute("aria-expanded", String(!sub.hidden));
      twisty.setAttribute("aria-label", full);
      twisty.addEventListener("click", () => {
        sub.hidden = !sub.hidden;
        twisty.setAttribute("aria-expanded", String(!sub.hidden));
      });
      row.append(twisty);
    } else {
      const spacer = document.createElement("span");
      spacer.className = "twisty-spacer";
      row.append(spacer);
    }

    if (child.page) {
      const a = document.createElement("a");
      a.href = url(child.page.p);
      a.textContent = part;
      if (full === here) a.setAttribute("aria-current", "page");
      row.append(a);
    } else {
      // A name that is only a prefix — no module of that name was compiled.
      const span = document.createElement("span");
      span.className = "node-name";
      span.textContent = part;
      row.append(span);
    }

    li.append(row);
    if (sub) li.append(sub);
    ul.append(li);
  }
  return ul;
}

async function initTree() {
  const host = document.getElementById("module-tree");
  if (!host) return;
  const data = await modules();
  if (!data?.modules?.length) return; // `<noscript>` fallback stays visible
  host.textContent = "";
  host.append(treeHtml(nest(data.modules), "", MODULE));
  host.querySelector("[aria-current]")?.scrollIntoView({ block: "center" });
}

// ------------------------------------------------------------- imported by

async function initImportedBy() {
  const host = document.querySelector('[data-fill="imported-by"]');
  if (!host) return;
  const data = await modules();
  const self = data?.modules?.find((m) => m.n === MODULE);
  const names = (self?.i ?? []).map((i) => data.modules[i]).filter(Boolean);
  if (names.length === 0) {
    // Nothing in this package imports it. Dropping the whole block is safe
    // here — this runs before the reader has had a chance to reach for it,
    // unlike the instance blocks below.
    host.remove();
    return;
  }
  host.hidden = false;
  const ul = host.querySelector("ul");
  for (const m of names.sort((a, b) => a.n.localeCompare(b.n))) {
    const li = document.createElement("li");
    const a = document.createElement("a");
    a.href = url(m.p);
    a.textContent = m.n;
    li.append(a);
    ul.append(li);
  }
  host.querySelector("summary").append(countBadge(names.length));
}

function countBadge(n) {
  const span = document.createElement("span");
  span.className = "count";
  span.textContent = ` ${n}`;
  return span;
}

// --------------------------------------------------- instances / instances-for

/**
 * Fills the two instance blocks the moment one is opened.
 *
 * They are `<details>` on purpose: the maps live in the large index, and a
 * reader who never opens one never pays for it.
 *
 * An empty result says "none" rather than deleting the block. The reader has
 * already clicked by the time the answer arrives, and a section that vanishes
 * under the cursor reads as a bug — "there are no instances" is the answer
 * they asked for.
 */
function initInstances() {
  for (const host of document.querySelectorAll('[data-fill="instances"], [data-fill="instances-for"]')) {
    host.addEventListener(
      "toggle",
      async () => {
        const ul = host.querySelector("ul");
        const data = await decls();
        const map = host.dataset.fill === "instances" ? data?.instances : data?.instancesFor;
        const names = map?.[host.dataset.name] ?? [];
        ul.textContent = "";
        if (names.length === 0) {
          const li = document.createElement("li");
          li.className = "search-empty";
          li.textContent = data ? "None" : "Index unavailable";
          ul.append(li);
          return;
        }
        for (const name of names) ul.append(declItem(data, name));
      },
      { once: true },
    );
  }
}

function declItem(data, name) {
  const li = document.createElement("li");
  const at = data.decls?.find((d) => d[0] === name);
  const a = document.createElement("a");
  a.textContent = name;
  a.href = at ? declHref(data, at) : `#${name}`;
  li.append(a);
  return li;
}

const declHref = (data, d) => `${url(data.modules[d[2]].p)}#${d[0]}`;

// ------------------------------------------------------------------ search

/**
 * Ranks a query against a name.
 *
 * Three tiers, cheapest first: a prefix of the last component beats a prefix of
 * the full name, which beats a substring anywhere. Nothing else matches — a
 * subsequence matcher finds `Nat.add` for `nd` and buries the exact hit.
 */
function score(name, query) {
  const lower = name.toLowerCase();
  const last = lower.slice(lower.lastIndexOf(".") + 1);
  if (last.startsWith(query)) return 3000 - last.length;
  if (lower.startsWith(query)) return 2000 - lower.length;
  const at = lower.indexOf(query);
  if (at >= 0) return 1000 - at;
  return -1;
}

/** Every hit for `query`, best first. */
function search(data, query) {
  const hits = [];
  for (const d of data.decls) {
    const s = score(d[0], query);
    if (s > 0) hits.push([s, d]);
  }
  hits.sort((a, b) => b[0] - a[0] || a[1][0].length - b[1][0].length);
  return hits.map(([, d]) => d);
}

/** One result row — the same markup in the dropdown and on `search.html`. */
function resultItem(data, d) {
  const li = document.createElement("li");
  const a = document.createElement("a");
  a.href = declHref(data, d);
  const kind = document.createElement("span");
  kind.className = "kind";
  kind.textContent = data.kinds[d[1]];
  const name = document.createElement("span");
  name.textContent = d[0];
  const where = document.createElement("span");
  where.className = "where";
  where.textContent = data.modules[d[2]].n;
  a.append(kind, name, where);
  li.append(a);
  return li;
}

function initSearch() {
  const input = document.getElementById("search-input");
  const list = document.getElementById("search-results");
  if (!input || !list) return;

  let items = [];
  let active = -1;
  let timer = 0;

  const close = () => {
    list.hidden = true;
    list.textContent = "";
    items = [];
    active = -1;
  };

  const run = async () => {
    const query = input.value.trim().toLowerCase();
    if (query.length < 2) return close();
    const data = await decls();
    if (!data) return close();

    const hits = search(data, query);
    list.textContent = "";
    if (hits.length === 0) {
      const li = document.createElement("li");
      li.className = "search-empty";
      li.textContent = "No matching declaration";
      list.append(li);
      list.hidden = false;
      return;
    }
    items = hits.slice(0, 30).map((d) => {
      const li = resultItem(data, d);
      list.append(li);
      return li;
    });
    active = -1;
    list.hidden = false;
  };

  const move = (delta) => {
    if (items.length === 0) return;
    items[active]?.removeAttribute("aria-selected");
    active = (active + delta + items.length) % items.length;
    items[active].setAttribute("aria-selected", "true");
    items[active].scrollIntoView({ block: "nearest" });
  };

  input.addEventListener("input", () => {
    clearTimeout(timer);
    timer = setTimeout(run, 90);
  });
  input.addEventListener("focus", () => void decls()); // warm the index
  input.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown") (e.preventDefault(), move(1));
    else if (e.key === "ArrowUp") (e.preventDefault(), move(-1));
    else if (e.key === "Escape") (close(), input.blur());
    else if (e.key === "Enter" && active >= 0) {
      e.preventDefault();
      items[active].querySelector("a").click();
    }
  });
  document.addEventListener("click", (e) => {
    if (!e.target.closest(".search")) close();
  });

  // `/` focuses search, the way every documentation site with a search box
  // does — but not while the reader is typing somewhere else.
  document.addEventListener("keydown", (e) => {
    const tag = document.activeElement?.tagName;
    if (e.key === "/" && tag !== "INPUT" && tag !== "TEXTAREA") {
      e.preventDefault();
      input.focus();
      input.select();
    }
  });
}

// ------------------------------------------------- search page / not found

/**
 * `search.html`: the same index, rendered into the page instead of a dropdown.
 *
 * There is no second input — the one in the top bar is the input, seeded from
 * `?q=` so a submitted form and a typed query land in the same place. Two boxes
 * on a search page is a question about which one is real.
 */
function initSearchPage() {
  const list = document.getElementById("page-results");
  const note = document.getElementById("page-note");
  const input = document.getElementById("search-input");
  if (!list || !input) return;

  // The dropdown would cover the results it duplicates. Removing it also makes
  // `initSearch` a no-op, which is why this runs first.
  document.getElementById("search-results")?.remove();

  const seed = new URLSearchParams(location.search).get("q");
  if (seed && !input.value) input.value = seed;

  const render = async () => {
    const query = input.value.trim().toLowerCase();
    list.textContent = "";
    if (query.length < 2) {
      if (note) note.textContent = "Type at least two characters.";
      return;
    }
    const data = await decls();
    if (!data) {
      if (note) note.textContent = "The search index could not be loaded.";
      return;
    }
    const hits = search(data, query);
    for (const d of hits.slice(0, 200)) list.append(resultItem(data, d));
    if (note) {
      note.textContent =
        hits.length === 0
          ? "No matching declaration."
          : hits.length > 200
            ? `${hits.length} matches, showing the first 200.`
            : `${hits.length} match${hits.length === 1 ? "" : "es"}.`;
    }
  };

  let timer = 0;
  input.addEventListener("input", () => {
    clearTimeout(timer);
    timer = setTimeout(render, 90);
  });
  input.form?.addEventListener("submit", (e) => {
    // Staying on the page is the whole point; a reload would refetch the index.
    e.preventDefault();
    void render();
  });
  input.focus();
  void render();
}

/**
 * `404.html`: says what was asked for and offers the nearest declarations.
 *
 * The guess is the fragment when there is one (`…/Foo.html#Bar.baz` — the page
 * moved but the reader knows the name) and otherwise the file name with its
 * path separators read back as dots, which is exactly how a module page's URL
 * is built.
 */
async function initNotFound() {
  const list = document.getElementById("how-about");
  const shown = document.getElementById("missing-path");
  if (shown) shown.textContent = location.pathname + location.hash;
  if (!list) return;

  const fragment = decodeURIComponent(location.hash.slice(1));
  const guess =
    fragment ||
    decodeURIComponent(location.pathname)
      .replace(/\.html$/, "")
      .split("/")
      .filter(Boolean)
      .join(".");
  const query = guess.trim().toLowerCase();
  if (query.length < 2) return;

  const data = await decls();
  if (!data) return;
  // A prefix of the *last* component is what a moved declaration matches on, so
  // the plain scorer is already the right one.
  const hits = search(data, query).slice(0, 20);
  if (hits.length === 0) return;
  for (const d of hits) list.append(resultItem(data, d));
  document.getElementById("how-about-heading")?.removeAttribute("hidden");
}

// ------------------------------------------------------------------ sundry

/** `?jump=src#Name` lands on the declaration's source instead of its entry. */
function jumpToSource() {
  if (new URLSearchParams(location.search).get("jump") !== "src") return;
  const target = document.getElementById(decodeURIComponent(location.hash.slice(1)));
  const src = target?.querySelector(".src")?.href;
  if (src) location.replace(src);
}

/** A `<details>` that is closed on paper is a paragraph the reader cannot get. */
function openForPrint() {
  addEventListener("beforeprint", () => {
    for (const d of document.querySelectorAll("details:not([open])")) {
      d.open = true;
      d.dataset.printOpened = "1";
    }
  });
  addEventListener("afterprint", () => {
    for (const d of document.querySelectorAll("details[data-print-opened]")) {
      d.open = false;
      delete d.dataset.printOpened;
    }
  });
}

// -------------------------------------------------------------------- boot

initTheme();
initDrawer();
initSearchPage(); // before `initSearch`: it removes the dropdown on that page
initSearch();
initInstances();
openForPrint();
jumpToSource();
void initTree();
void initImportedBy();
void initNotFound();
