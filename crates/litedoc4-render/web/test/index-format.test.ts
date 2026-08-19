import { describe, expect, it } from "vitest";
import {
  ENCODER,
  findNames,
  kindAt,
  moduleAt,
  nameAt,
  readIndex,
  utf16Length,
} from "../src/index-format.js";
import { BYTES, EXPECTED, freshIndex } from "./fixture.js";

describe("readIndex", () => {
  it("reads the header and the kind labels", () => {
    const index = freshIndex();
    expect(index.count).toBe(EXPECTED.names.length);
    expect(index.labels).toEqual(EXPECTED.kinds);
  });

  it("carries the names ASCII folding is wrong for, and only those", () => {
    const index = freshIndex();
    const exceptions = [...index.folds.keys()].map((id) => EXPECTED.names[id]);
    // `Γ` lowercases to `γ`; adding 32 to it does not. Every other name in the
    // corpus folds the same either way, so the section holds exactly this one.
    expect(exceptions).toEqual(["Pkg.Γamma"]);
  });

  it("refuses a file that is not one", () => {
    expect(readIndex(new Uint8Array(0))).toBeNull();
    expect(readIndex(new Uint8Array(64))).toBeNull();
    const wrongVersion = BYTES.slice();
    wrongVersion[4] = 99;
    expect(readIndex(wrongVersion)).toBeNull();
  });
});

describe("nameAt", () => {
  it("decodes every declaration, across restart blocks", () => {
    const index = freshIndex();
    const back = Array.from({ length: index.count }, (_, id) => nameAt(index, id));
    expect(back).toEqual(EXPECTED.names);
  });

  it("decodes one name without reading the ones before its block", () => {
    // The point of the restart table: the 20th name is reachable without
    // walking the 19 in front of it.
    const index = freshIndex();
    const id = 20;
    expect(nameAt(index, id)).toBe(EXPECTED.names[id]);
  });
});

describe("kindAt / moduleAt", () => {
  it("agree with what the encoder was given", () => {
    const index = freshIndex();
    const kinds = Array.from({ length: index.count }, (_, id) => kindAt(index, id));
    const modules = Array.from({ length: index.count }, (_, id) => moduleAt(index, id));
    expect(kinds).toEqual(EXPECTED.kindOf.map((k) => EXPECTED.kinds[k]));
    expect(modules).toEqual(EXPECTED.moduleOf);
  });
});

describe("utf16Length", () => {
  /**
   * The regression the browser gate caught on 2026-08-19: a character above the
   * BMP is **two** UTF-16 units, and the score is `2000 - length`, so counting
   * it once moves the name up one place.
   */
  it("counts what String.prototype.length counts", () => {
    for (const name of EXPECTED.names) {
      const bytes = ENCODER.encode(name);
      expect(utf16Length(bytes, 0, bytes.length)).toBe(name.length);
    }
  });

  it("counts an astral character as two", () => {
    const bytes = ENCODER.encode("\u{1D49C}");
    expect(bytes.length).toBe(4);
    expect(utf16Length(bytes, 0, bytes.length)).toBe(2);
  });
});

describe("findNames", () => {
  it("finds every name in one walk", () => {
    const index = freshIndex();
    const wanted = ["Pkg.b", "NoDot", EXPECTED.names[26] as string];
    const found = findNames(index, wanted);
    expect([...found.keys()].sort()).toEqual([...wanted].sort());
    for (const [name, id] of found) expect(EXPECTED.names[id]).toBe(name);
  });

  it("says nothing about a name that is not there", () => {
    const index = freshIndex();
    expect(findNames(index, ["Pkg.absent"]).size).toBe(0);
  });

  it("survives a name longer than the one-byte suffix length", () => {
    const index = freshIndex();
    const long = EXPECTED.names.find((n) => n.length > 254);
    expect(long).toBeDefined();
    expect(findNames(index, [long as string]).size).toBe(1);
  });
});
