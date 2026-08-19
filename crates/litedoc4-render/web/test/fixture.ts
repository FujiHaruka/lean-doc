/**
 * The corpus, as `crates/litedoc4-global/tests/web_fixture.rs` wrote it.
 *
 * **Nothing here encodes anything.** The bytes come from the Rust encoder that
 * the site's own indexes come from; if a second encoder lived here, the two
 * would agree about their shared mistakes and the tests would pass anyway.
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { readIndex } from "../src/index-format.js";
import type { SearchIndex } from "../src/types.js";

export interface Expected {
  readonly kinds: string[];
  readonly names: string[];
  readonly kindOf: number[];
  readonly moduleOf: number[];
}

const read = (name: string): Buffer => readFileSync(join(import.meta.dirname, "fixtures", name));

export const BYTES = new Uint8Array(read("search-index.bin"));

export const EXPECTED: Expected = JSON.parse(read("expected.json").toString("utf8")) as Expected;

/** A reader over the fixture. One per test: `search` mutates its narrowing cache. */
export function freshIndex(): SearchIndex {
  const index = readIndex(BYTES);
  if (!index) throw new Error("the committed fixture did not parse");
  return index;
}
