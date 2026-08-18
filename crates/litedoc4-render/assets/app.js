// litedoc4 — every bit of behaviour the site has, in one module.
//
// Loaded as `type="module"`, so: deferred, strict, nothing leaked to globals.
// **The page works without it.** Navigation is links, the declarations are in
// the HTML, and the docstrings are already rendered. What this adds is the
// module tree, search, the theme toggle, and the three lists that cannot be
// placed statically because they are facts about the *whole* site rather than
// about the module being rendered (instances, instances-for, imported-by).
//
// Three data files, split by when they are needed (plan 決定 5,
// `docs/plans/search-v2.md` P0):
//
//   modules.json       every module, its page and what imports it.
//                      Small, wanted immediately — it draws the tree.
//   search-index.json  every declaration and the kind vocabulary. Large,
//                      wanted on the first keystroke. It has no module array
//                      of its own: a declaration names its module by
//                      subscript into `modules.json`'s, which is already here.
//   instances.json     the two instance maps, wanted only when a reader opens
//                      one of the two blocks — which most never do.
//
// None is cached in storage: they are ordinary GETs against the same origin
// and the browser's HTTP cache is better at this than we are. (doc-gen4 tried
// IndexedDB here and disabled it.)

const body = document.body;
const ROOT = body.dataset.root ?? "./";
const MODULE = body.dataset.module ?? "";

const url = (name) => new URL(ROOT + name, location.href).href;

// ---------------------------------------------------------------- fetching

let modulesPromise = null;
let declsPromise = null;
let instancesPromise = null;

/** One GET, parsed, or `null` — every caller here treats a miss as "no data". */
const fetchJson = (name) =>
  fetch(url(name))
    .then((r) => (r.ok ? r.json() : Promise.reject(new Error(String(r.status)))))
    .catch(() => null);

/** `modules.json`, fetched at most once per page. */
function modules() {
  modulesPromise ??= fetchJson("modules.json");
  return modulesPromise;
}

/** `search-index.bin`, fetched at most once per page, on demand. */
function decls() {
  declsPromise ??= fetch(url("search-index.bin"))
    .then((r) => (r.ok ? r.arrayBuffer() : Promise.reject(new Error(String(r.status)))))
    .then((buffer) => readIndex(new Uint8Array(buffer)))
    .catch(() => null);
  return declsPromise;
}

/** `instances.json`, fetched only when a reader opens an instance block. */
function instanceMaps() {
  instancesPromise ??= fetchJson("instances.json");
  return instancesPromise;
}

/**
 * What every result row needs: the index, and the module array it points into.
 *
 * The two live in different files because they are wanted at different times —
 * the tree draws from `modules.json` before a reader has typed anything — but
 * a result row needs both, so this is where they meet. Asking for both at once
 * costs nothing after the first: `modules()` has already resolved.
 */
async function searchData() {
  const [tree, index] = await Promise.all([modules(), decls()]);
  if (!tree?.modules || !index) return null;
  return { modules: tree.modules, index };
}

// --------------------------------------------------------- the index format
//
// `search-index.bin` is read in place: the page holds the file, not a parsed
// copy of it. The layout is `crates/litedoc4-global/src/search_index.rs`, the
// plan and the measurements are `docs/plans/search-v2.md`. In short, the JSON
// this replaces cost 860 KiB of JS heap for a 405,402 B file【実測】; this
// costs the file.
//
// **The ranking is unchanged** — three tiers, same order, same numbers as the
// version that scored JS strings. That is deliberate: an index that also
// ranked differently could not be held against the old one, and
// `tools/search-gate.sh` is exactly that comparison.

const MAGIC = 0x53_34_44_4c; // "LD4S" read little-endian
const TEXT = new TextDecoder();
const ENCODER = new TextEncoder();
const DOT = 46;
/** ASCII lowering. The names it is wrong for are carried in the file. */
const FOLD = new Uint8Array(256);
for (let i = 0; i < 256; i++) FOLD[i] = i >= 65 && i <= 90 ? i + 32 : i;

/** Reads the header and the two small tables; the names stay in the buffer. */
function readIndex(bytes) {
  const u32 = (at) =>
    (bytes[at] | (bytes[at + 1] << 8) | (bytes[at + 2] << 16)) + bytes[at + 3] * 0x1000000;
  const u16 = (at) => bytes[at] | (bytes[at + 1] << 8);
  if (bytes.length < 52 || u32(0) !== MAGIC || u32(4) !== 2) return null;
  const count = u32(8);
  const index = {
    bytes,
    count,
    names: u32(16),
    restarts: u32(24),
    restart: u32(12),
    kindOf: u32(36),
    moduleOf: u32(40),
    labels: [],
    folds: new Map(),
    // The previous query and what it matched — see `search`.
    narrow: null,
    // Scored in place, allocated once per page rather than once per keystroke.
    // Sized to what they hold rather than to a machine word: a score is at most
    // 3000, a name's UTF-16 length is at most the 64 KiB the encoder allows,
    // and a subscript is a subscript. Three `Int32Array`s cost **55 KiB of the
    // 156 KiB** the index took in Chrome【実測 2026-08-19】, against a file of
    // 106 KiB — overhead worth more than a third of the data.
    score: new Uint16Array(count),
    length: new Uint16Array(count),
    id: count < 65536 ? new Uint16Array(count) : new Uint32Array(count),
  };

  const labelsAt = u32(28);
  let at = labelsAt + 4;
  for (let i = 0, n = u32(labelsAt); i < n; i++) {
    index.labels.push(TEXT.decode(bytes.subarray(at + 1, at + 1 + bytes[at])));
    at += 1 + bytes[at];
  }

  // The names `toLowerCase()` does something to that adding 32 to `A`-`Z` does
  // not — `Γ` and its like. Empty for every package measured so far, which is
  // why the scan below asks whether the map is empty before consulting it.
  const foldsAt = u32(44);
  at = foldsAt + 4;
  for (let i = 0, n = u32(foldsAt); i < n; i++) {
    const len = u16(at + 4);
    index.folds.set(u32(at), bytes.subarray(at + 6, at + 6 + len));
    at += 6 + len;
  }
  return index;
}

// One name at a time, reused for the life of the page. Front coding means the
// shared prefix of the previous name is already in place, so a step writes only
// the suffix — which is why these cannot be allocated per declaration.
let scratch = new Uint8Array(512);
let folded = new Uint8Array(512);

function room(need) {
  if (need <= scratch.length) return;
  let size = scratch.length;
  while (size < need) size *= 2;
  // Copied rather than replaced: the bytes already here are the prefix the
  // next name shares.
  const grownScratch = new Uint8Array(size);
  grownScratch.set(scratch);
  const grownFolded = new Uint8Array(size);
  grownFolded.set(folded);
  scratch = grownScratch;
  folded = grownFolded;
}

/**
 * UTF-16 length, which is what the scoring counts (`String.prototype.length`).
 *
 * **Not the code point count.** A character above the BMP is one code point and
 * **two** UTF-16 units, so a 4-byte UTF-8 sequence counts twice — U1 again. The
 * browser gate caught this ranking `Micro.script𝒜` above `Micro.usesDep`
 * 【実測 2026-08-19】: both are prefix matches, and the score is
 * `2000 - length`, so one unit of length is one place in the list.
 */
function utf16Length(bytes, from, to) {
  let n = 0;
  for (let i = from; i < to; i++) {
    const byte = bytes[i];
    if ((byte & 0xc0) !== 0x80) n += byte >= 0xf0 ? 2 : 1;
  }
  return n;
}

/** The declaration at `id`, decoded from the start of its restart block. */
function nameAt(index, id) {
  const bytes = index.bytes;
  const block = Math.floor(id / index.restart);
  const restartAt = index.restarts + block * 4;
  let at =
    index.names +
    ((bytes[restartAt] | (bytes[restartAt + 1] << 8) | (bytes[restartAt + 2] << 16)) +
      bytes[restartAt + 3] * 0x1000000);
  let out = new Uint8Array(256);
  let end = 0;
  for (let i = block * index.restart; i <= id; i++) {
    const shared = bytes[at++];
    let len = bytes[at++];
    if (len === 255) {
      len = bytes[at] | (bytes[at + 1] << 8);
      at += 2;
    }
    if (shared + len > out.length) {
      const grown = new Uint8Array(Math.max(shared + len, out.length * 2));
      grown.set(out);
      out = grown;
    }
    out.set(bytes.subarray(at, at + len), shared);
    at += len;
    end = shared + len;
  }
  return TEXT.decode(out.subarray(0, end));
}

const kindAt = (index, id) => index.labels[index.bytes[index.kindOf + id]] ?? "";
const moduleAt = (index, id) =>
  index.bytes[index.moduleOf + id * 2] | (index.bytes[index.moduleOf + id * 2 + 1] << 8);

// ------------------------------------------------------------------- theme

const THEME_KEY = "litedoc4-theme";
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
        // The maps and the links come from different files now: the names are
        // in `instances.json`, and turning a name into a URL needs the search
        // index. Both are fetched here and nowhere earlier.
        const [maps, data] = await Promise.all([instanceMaps(), searchData()]);
        const map = host.dataset.fill === "instances" ? maps?.instances : maps?.instancesFor;
        const names = map?.[host.dataset.name] ?? [];
        ul.textContent = "";
        if (names.length === 0) {
          const li = document.createElement("li");
          li.className = "search-empty";
          li.textContent = maps ? "None" : "Index unavailable";
          ul.append(li);
          return;
        }
        // One pass over the index resolves every name in the block, rather
        // than one pass per name.
        const found = data ? findNames(data.index, names) : new Map();
        for (const name of names) ul.append(declItem(data, name, found.get(name)));
      },
      { once: true },
    );
  }
}

function declItem(data, name, id) {
  const li = document.createElement("li");
  const a = document.createElement("a");
  a.textContent = name;
  // A name the index does not have is still worth a link: the page it is on is
  // the page the reader is already looking at.
  a.href =
    id === undefined ? `#${name}` : `${url(data.modules[moduleAt(data.index, id)].p)}#${name}`;
  li.append(a);
  return li;
}

/**
 * Where each of `names` is, as one walk of the index.
 *
 * The names come from `instances.json` and are exact, so this is equality
 * rather than scoring — but it is the same walk, for the same reason: front
 * coding is read forwards, and a lookup per name would read the section once
 * per name.
 */
function findNames(index, names) {
  const wanted = new Set(names);
  const found = new Map();
  const bytes = index.bytes;
  let at = index.names;
  for (let i = 0; i < index.count && found.size < wanted.size; i++) {
    const shared = bytes[at++];
    let len = bytes[at++];
    if (len === 255) {
      len = bytes[at] | (bytes[at + 1] << 8);
      at += 2;
    }
    room(shared + len);
    scratch.set(bytes.subarray(at, at + len), shared);
    at += len;
    const name = TEXT.decode(scratch.subarray(0, shared + len));
    if (wanted.has(name)) found.set(name, i);
  }
  return found;
}

// ------------------------------------------------------------------ search

/**
 * Ranks a folded name against a folded query, in bytes.
 *
 * Three tiers, cheapest first: a prefix of the last component beats a prefix of
 * the full name, which beats a substring anywhere. Nothing else matches — a
 * subsequence matcher finds `Nat.add` for `nd` and buries the exact hit.
 *
 * The lengths are UTF-16 lengths because the version this replaces scored
 * `String.prototype.length`, and a name with `β` in it would otherwise rank
 * differently for no reason a reader could see.
 */
function scoreBytes(name, end, lastStart, q, qn) {
  if (end - lastStart >= qn) {
    let ok = true;
    for (let k = 0; k < qn; k++)
      if (name[lastStart + k] !== q[k]) {
        ok = false;
        break;
      }
    if (ok) return 3000 - utf16Length(name, lastStart, end);
  }
  if (end < qn) return -1;
  let ok = true;
  for (let k = 0; k < qn; k++)
    if (name[k] !== q[k]) {
      ok = false;
      break;
    }
  if (ok) return 2000 - utf16Length(name, 0, end);
  for (let start = 1; start <= end - qn; start++) {
    let hit = true;
    for (let k = 0; k < qn; k++)
      if (name[start + k] !== q[k]) {
        hit = false;
        break;
      }
    if (hit) return 1000 - utf16Length(name, 0, start);
  }
  return -1;
}

/**
 * How many hits are worth keeping for the next keystroke.
 *
 * Re-scoring a few hundred names is obviously cheaper than walking the whole
 * section; keeping thousands would cost more to copy than the walk it saves.
 */
const NARROW_MAX = 512;

/** The hits, best first — the one place the ranking order is decided. */
function rank(index, hits) {
  // Ties break by name length and then by position in the file, which is the
  // order the JSON version's stable sort produced.
  const order = Array.from({ length: hits }, (_, k) => k);
  order.sort(
    (a, b) =>
      index.score[b] - index.score[a] ||
      index.length[a] - index.length[b] ||
      index.id[a] - index.id[b],
  );
  return order.map((k) => index.id[k]);
}

/**
 * Every hit for `query`, best first, as subscripts into the index.
 *
 * Two ways in. The walk below decodes, folds and scores in one pass, into
 * buffers that outlive the call — nothing is allocated per declaration, where
 * the version this replaces allocated two strings each, 9,168 per keystroke on
 * the measured package【実測】.
 *
 * The other way is [`searchNarrowed`]: **all three tiers require the query to
 * occur in the folded name**, so typing one more character can only shrink the
 * hit set, and a query that extends the previous one is answered from what the
 * previous one matched. Typing five representative words costs 48.2% of the
 * candidates it would rescanning【実測 2026-08-19】. The browser gate types a
 * query one character at a time rather than pasting it, because a narrowing
 * that is wrong is only wrong on the second keystroke.
 */
function search(index, query) {
  const q = ENCODER.encode(query);
  const qn = q.length;
  const narrow = index.narrow;
  if (narrow && query.startsWith(narrow.query)) return searchNarrowed(index, narrow, q, qn, query);

  const bytes = index.bytes;
  const hasFolds = index.folds.size > 0;
  const kept = { names: [], starts: [], ids: [] };
  let at = index.names;
  let hits = 0;
  let lastDot = -1;
  for (let i = 0; i < index.count; i++) {
    const shared = bytes[at++];
    let len = bytes[at++];
    if (len === 255) {
      len = bytes[at] | (bytes[at + 1] << 8);
      at += 2;
    }
    room(shared + len);
    for (let k = 0; k < len; k++) {
      const b = bytes[at + k];
      scratch[shared + k] = b;
      folded[shared + k] = FOLD[b];
    }
    at += len;
    const end = shared + len;

    // The last `.`, maintained rather than searched for: it is in the suffix,
    // or it is the previous name's and still inside the shared prefix, or the
    // prefix has to be walked back — which only happens when a name loses a
    // component its predecessor had.
    let dot = -1;
    for (let k = end - 1; k >= shared; k--)
      if (folded[k] === DOT) {
        dot = k;
        break;
      }
    if (dot < 0) {
      if (lastDot < shared) dot = lastDot;
      else
        for (let k = shared - 1; k >= 0; k--)
          if (folded[k] === DOT) {
            dot = k;
            break;
          }
    }
    lastDot = dot;

    // A name ASCII folding is wrong for is matched against its own bytes, and
    // `folded` is left alone: the next name's shared prefix is in it.
    let name = folded;
    let nameEnd = end;
    let lastStart = dot + 1;
    if (hasFolds) {
      const exception = index.folds.get(i);
      if (exception) {
        name = exception;
        nameEnd = exception.length;
        lastStart = 0;
        for (let k = nameEnd - 1; k >= 0; k--)
          if (name[k] === DOT) {
            lastStart = k + 1;
            break;
          }
      }
    }

    const s = scoreBytes(name, nameEnd, lastStart, q, qn);
    if (s > 0) {
      index.id[hits] = i;
      index.score[hits] = s;
      index.length[hits] = utf16Length(name, 0, nameEnd);
      if (hits < NARROW_MAX) {
        // `slice` copies: `folded` is about to be written over by the next
        // name, and this has to outlive the walk.
        kept.names.push(name.slice(0, nameEnd));
        kept.starts.push(lastStart);
        kept.ids.push(i);
      }
      hits++;
    }
  }
  // In file order, so that the tie-break by position survives into the next
  // keystroke. Dropped when the set is too big to be worth carrying.
  index.narrow = hits <= NARROW_MAX ? { query, ...kept } : null;
  return rank(index, hits);
}

/** The same scoring, over what the shorter query matched. */
function searchNarrowed(index, narrow, q, qn, query) {
  const kept = { names: [], starts: [], ids: [] };
  let hits = 0;
  for (let k = 0; k < narrow.ids.length; k++) {
    const name = narrow.names[k];
    const s = scoreBytes(name, name.length, narrow.starts[k], q, qn);
    if (s > 0) {
      index.id[hits] = narrow.ids[k];
      index.score[hits] = s;
      index.length[hits] = utf16Length(name, 0, name.length);
      kept.names.push(name);
      kept.starts.push(narrow.starts[k]);
      kept.ids.push(narrow.ids[k]);
      hits++;
    }
  }
  index.narrow = { query, ...kept };
  return rank(index, hits);
}

/** One result row — the same markup in the dropdown and on `search.html`. */
function resultItem(data, id) {
  const li = document.createElement("li");
  const a = document.createElement("a");
  const declared = nameAt(data.index, id);
  const where = data.modules[moduleAt(data.index, id)];
  a.href = `${url(where.p)}#${declared}`;
  const kind = document.createElement("span");
  kind.className = "kind";
  kind.textContent = kindAt(data.index, id);
  const name = document.createElement("span");
  name.textContent = declared;
  const module = document.createElement("span");
  module.className = "where";
  module.textContent = where.n;
  a.append(kind, name, module);
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
    const data = await searchData();
    if (!data) return close();

    const hits = search(data.index, query);
    list.textContent = "";
    if (hits.length === 0) {
      const li = document.createElement("li");
      li.className = "search-empty";
      li.textContent = "No matching declaration";
      list.append(li);
      list.hidden = false;
      return;
    }
    items = hits.slice(0, 30).map((id) => {
      const li = resultItem(data, id);
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
  input.addEventListener("focus", () => void searchData()); // warm the index
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
    const data = await searchData();
    if (!data) {
      if (note) note.textContent = "The search index could not be loaded.";
      return;
    }
    const hits = search(data.index, query);
    for (const id of hits.slice(0, 200)) list.append(resultItem(data, id));
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

  const data = await searchData();
  if (!data) return;
  // A prefix of the *last* component is what a moved declaration matches on, so
  // the plain scorer is already the right one.
  const hits = search(data.index, query).slice(0, 20);
  if (hits.length === 0) return;
  for (const id of hits) list.append(resultItem(data, id));
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
