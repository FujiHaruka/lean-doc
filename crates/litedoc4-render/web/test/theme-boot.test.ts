/**
 * The script that runs before the first paint.
 *
 * Importing it *is* running it, so each case resets the module registry and
 * imports again — which is also the only honest way to test a module whose
 * whole body is a side effect.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { THEME_KEY } from "../src/theme-key.js";

const boot = async (): Promise<void> => {
  vi.resetModules();
  await import("../src/theme-boot.js");
};

beforeEach(() => {
  localStorage.clear();
  delete document.documentElement.dataset.theme;
});

describe("theme-boot", () => {
  it("paints the stored theme", async () => {
    localStorage.setItem(THEME_KEY, "dark");
    await boot();
    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  it("writes nothing for auto, which is the absence of the attribute", async () => {
    localStorage.setItem(THEME_KEY, "auto");
    await boot();
    expect(document.documentElement.dataset.theme).toBeUndefined();
  });

  it("writes nothing for a value that is not a theme", async () => {
    // `localStorage` is shared with everything else on the origin.
    localStorage.setItem(THEME_KEY, "dark; drop table");
    await boot();
    expect(document.documentElement.dataset.theme).toBeUndefined();
  });

  it("writes nothing when there is nothing stored", async () => {
    await boot();
    expect(document.documentElement.dataset.theme).toBeUndefined();
  });

  it("survives storage that throws", async () => {
    const getItem = Storage.prototype.getItem;
    Storage.prototype.getItem = () => {
      throw new Error("SecurityError");
    };
    try {
      await expect(boot()).resolves.toBeUndefined();
      expect(document.documentElement.dataset.theme).toBeUndefined();
    } finally {
      Storage.prototype.getItem = getItem;
    }
  });

  it("uses the same key the toggle writes", async () => {
    // The duplication 決定 6 removed: this used to be a string literal in
    // `frame.rs` and another in `app.js`.
    const { initTheme } = await import("../src/theme.js");
    document.body.innerHTML = '<button id="theme-toggle"></button>';
    initTheme();
    document.getElementById("theme-toggle")?.click();
    expect(localStorage.getItem(THEME_KEY)).toBe("light");
    delete document.documentElement.dataset.theme;
    await boot();
    expect(document.documentElement.dataset.theme).toBe("light");
  });
});
