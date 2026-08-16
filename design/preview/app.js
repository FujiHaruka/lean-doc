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

function treeHtml(node, prefix, here) {
  const ul = document.createElement("ul");
  for (const [part, child] of node.children) {
    const full = prefix ? `${prefix}.${part}` : part;
    const li = document.createElement("li");
    const link = (text) => {
      const a = document.createElement("a");
      a.textContent = text;
      if (child.page) {
        a.href = url(child.page.p);
        if (full === here) a.setAttribute("aria-current", "page");
      } else {
        a.setAttribute("aria-disabled", "true");
      }
      return a;
    };

    if (child.children.size === 0) {
      li.append(link(part));
    } else {
      const details = document.createElement("details");
      if (here === full || here.startsWith(`${full}.`)) details.open = true;
      const summary = document.createElement("summary");
      summary.append(child.page ? link(part) : document.createTextNode(part));
      details.append(summary, treeHtml(child, full, here));
      li.append(details);
    }
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

    const hits = [];
    for (const d of data.decls) {
      const s = score(d[0], query);
      if (s > 0) hits.push([s, d]);
    }
    hits.sort((a, b) => b[0] - a[0] || a[1][0].length - b[1][0].length);

    list.textContent = "";
    if (hits.length === 0) {
      const li = document.createElement("li");
      li.className = "search-empty";
      li.textContent = "No matching declaration";
      list.append(li);
      list.hidden = false;
      return;
    }
    items = hits.slice(0, 30).map(([, d]) => {
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
initSearch();
initInstances();
openForPrint();
jumpToSource();
void initTree();
void initImportedBy();
