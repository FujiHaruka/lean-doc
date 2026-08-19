/**
 * The module tree, which is the one piece of the sidebar that is a fact about
 * the whole package rather than about the page it is on.
 *
 * These run against happy-dom, which is not a browser: it answers "did this
 * build the elements it says it does". Whether the result is *readable* at
 * 375 px is `tools/browser-gate.sh`'s question and cannot be asked here.
 */
import { describe, expect, it } from "vitest";
import { nest, treeHtml } from "../src/tree.js";
import type { ModuleEntry } from "../src/types.js";

const mods = (...names: string[]): ModuleEntry[] =>
  names.map((n) => ({ n, p: `${n.replaceAll(".", "/")}.html` }));

describe("nest", () => {
  it("splits on dots", () => {
    const root = nest(mods("A.B.C"));
    const a = root.children.get("A");
    const b = a?.children.get("B");
    expect(b?.children.get("C")?.page?.n).toBe("A.B.C");
  });

  it("lets a name be a page and a parent at once", () => {
    // The case `<details>`/`<summary>` cannot express: `A` is a page, and it
    // also has children.
    const root = nest(mods("A", "A.B"));
    const a = root.children.get("A");
    expect(a?.page?.n).toBe("A");
    expect(a?.children.size).toBe(1);
  });

  it("makes a node for a name no module has", () => {
    // `A.B` was never compiled; it is still a level of the tree.
    const root = nest(mods("A.B.C"));
    expect(root.children.get("A")?.page).toBeUndefined();
  });

  it("does not care what order the list arrives in", () => {
    const one = nest(mods("A.B", "A"));
    const two = nest(mods("A", "A.B"));
    expect(one.children.get("A")?.page?.n).toBe(two.children.get("A")?.page?.n);
  });
});

describe("treeHtml", () => {
  it("gives a page-and-parent node both a link and a disclosure", () => {
    const ul = treeHtml(nest(mods("A", "A.B")), "", "A");
    const row = ul.querySelector("li > .row");
    expect(row?.querySelector("button.twisty")).not.toBeNull();
    expect(row?.querySelector("a")?.textContent).toBe("A");
  });

  it("renders a name with no page as a span, not a link", () => {
    const ul = treeHtml(nest(mods("A.B.C")), "", "");
    const row = ul.querySelector("li > .row");
    expect(row?.querySelector("a")).toBeNull();
    expect(row?.querySelector("span.node-name")?.textContent).toBe("A");
  });

  it("opens exactly the spine down to the current page", () => {
    const ul = treeHtml(nest(mods("A.B.C", "X.Y")), "", "A.B.C");
    const [a, x] = [...ul.children] as HTMLLIElement[];
    expect(a?.querySelector("ul")?.hidden).toBe(false);
    expect(x?.querySelector("ul")?.hidden).toBe(true);
  });

  it("marks the current page and nothing else", () => {
    const ul = treeHtml(nest(mods("A", "A.B")), "", "A.B");
    const current = ul.querySelectorAll("[aria-current]");
    expect(current.length).toBe(1);
    expect(current[0]?.textContent).toBe("B");
  });

  it("toggles a subtree from its twisty", () => {
    const ul = treeHtml(nest(mods("A.B")), "", "");
    const twisty = ul.querySelector<HTMLButtonElement>("button.twisty");
    const sub = ul.querySelector("ul");
    expect(sub?.hidden).toBe(true);
    twisty?.click();
    expect(sub?.hidden).toBe(false);
    expect(twisty?.getAttribute("aria-expanded")).toBe("true");
  });
});
