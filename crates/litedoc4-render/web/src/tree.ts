/** The module tree in the sidebar, drawn from `modules.json`. */
import { modules } from "./data.js";
import { MODULE, url } from "./site.js";
import type { ModuleEntry } from "./types.js";

/** A name is both a node and a page: `A.B` can have a page *and* children. */
export interface TreeNode {
  readonly children: Map<string, TreeNode>;
  page?: ModuleEntry;
}

/** Nests the flat module list on its dot-separated components. */
export function nest(list: readonly ModuleEntry[]): TreeNode {
  const rootNode: TreeNode = { children: new Map() };
  for (const m of list) {
    let node = rootNode;
    for (const part of m.n.split(".")) {
      let child = node.children.get(part);
      if (!child) {
        child = { children: new Map() };
        node.children.set(part, child);
      }
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
export function treeHtml(node: TreeNode, prefix: string, here: string): HTMLUListElement {
  const ul = document.createElement("ul");
  for (const [part, child] of node.children) {
    const full = prefix ? `${prefix}.${part}` : part;
    const li = document.createElement("li");
    const row = document.createElement("div");
    row.className = "row";

    let sub: HTMLUListElement | null = null;
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
      const panel = sub;
      twisty.addEventListener("click", () => {
        panel.hidden = !panel.hidden;
        twisty.setAttribute("aria-expanded", String(!panel.hidden));
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

export async function initTree(): Promise<void> {
  const host = document.getElementById("module-tree");
  if (!host) return;
  const data = await modules();
  if (!data?.modules?.length) return; // `<noscript>` fallback stays visible
  host.textContent = "";
  host.append(treeHtml(nest(data.modules), "", MODULE));
  host.querySelector("[aria-current]")?.scrollIntoView({ block: "center" });
}
