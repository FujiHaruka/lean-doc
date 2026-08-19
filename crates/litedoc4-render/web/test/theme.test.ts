/**
 * The theme toggle. The *boot* half — the synchronous script that sets
 * `data-theme` before the first paint — is in `frame.rs` and is not this.
 */
import { beforeEach, describe, expect, it } from "vitest";
import { initTheme } from "../src/theme.js";

const KEY = "litedoc4-theme";

beforeEach(() => {
  localStorage.clear();
  delete document.documentElement.dataset.theme;
  document.body.innerHTML = '<button id="theme-toggle"></button>';
});

const toggle = (): HTMLButtonElement =>
  document.getElementById("theme-toggle") as HTMLButtonElement;

describe("initTheme", () => {
  it("starts on auto, which sets no attribute at all", () => {
    initTheme();
    expect(document.documentElement.dataset.theme).toBeUndefined();
    expect(toggle().title).toBe("Theme: auto");
  });

  it("applies what was stored", () => {
    localStorage.setItem(KEY, "dark");
    initTheme();
    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  it("ignores a stored value that is not a theme", () => {
    // Anything can be in localStorage; a site on the same origin, an older
    // version of this file, a reader with the console open.
    localStorage.setItem(KEY, "chartreuse");
    initTheme();
    expect(document.documentElement.dataset.theme).toBeUndefined();
  });

  it("cycles auto → light → dark → auto and stores each step", () => {
    initTheme();
    toggle().click();
    expect(document.documentElement.dataset.theme).toBe("light");
    expect(localStorage.getItem(KEY)).toBe("light");
    toggle().click();
    expect(document.documentElement.dataset.theme).toBe("dark");
    toggle().click();
    expect(document.documentElement.dataset.theme).toBeUndefined();
    expect(localStorage.getItem(KEY)).toBe("auto");
  });

  it("still applies the theme when storage throws", () => {
    // Private mode: the choice does not survive the page, and that is the only
    // thing that should go wrong.
    const setItem = Storage.prototype.setItem;
    Storage.prototype.setItem = () => {
      throw new Error("QuotaExceededError");
    };
    try {
      initTheme();
      toggle().click();
      expect(document.documentElement.dataset.theme).toBe("light");
    } finally {
      Storage.prototype.setItem = setItem;
    }
  });

  it("does nothing when the page has no toggle", () => {
    document.body.innerHTML = "";
    expect(() => initTheme()).not.toThrow();
  });
});
