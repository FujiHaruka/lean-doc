/**
 * `search-index.bin`, read in place: the page holds the file, not a parsed copy
 * of it.
 *
 * The layout is `crates/litedoc4-global/src/search_index.rs`, the plan and the
 * measurements are `docs/plans/search-v2.md`. In short, the JSON this replaces
 * cost 860 KiB of JS heap for a 405,402 B file【実測】; this costs the file.
 *
 * **The ranking is unchanged** — three tiers, same order, same numbers as the
 * version that scored JS strings. That is deliberate: an index that also ranked
 * differently could not be held against the old one, and `tools/search-gate.sh`
 * is exactly that comparison.
 *
 * # A note on `!`
 *
 * `tsconfig.json` sets `noUncheckedIndexedAccess`, which is right for the map
 * and array lookups everywhere else in this tree and wrong here: every read
 * below is at an offset the format guarantees, and `bytes[at] | undefined` has
 * no failure to describe — a truncated file fails the header check in
 * [`readIndex`] and never reaches these walks. So the byte reads assert, and
 * `biome.json` allows the assertion in this file and in `search.ts` only.
 */
import { room, scratch } from "./scratch.js";
import type { SearchIndex } from "./types.js";

const MAGIC = 0x53_34_44_4c; // "LD4S" read little-endian
const VERSION = 2;
/** The smallest file a valid header can occupy. */
const HEADER = 52;

export const TEXT = new TextDecoder();
export const ENCODER = new TextEncoder();
export const DOT = 46;

/** ASCII lowering. The names it is wrong for are carried in the file. */
export const FOLD = new Uint8Array(256);
for (let i = 0; i < 256; i++) FOLD[i] = i >= 65 && i <= 90 ? i + 32 : i;

/** Reads the header and the two small tables; the names stay in the buffer. */
export function readIndex(bytes: Uint8Array): SearchIndex | null {
  const u32 = (at: number): number =>
    // biome-ignore lint/style/noNonNullAssertion: offsets the format guarantees
    (bytes[at]! | (bytes[at + 1]! << 8) | (bytes[at + 2]! << 16)) + bytes[at + 3]! * 0x100_0000;
  // biome-ignore lint/style/noNonNullAssertion: offsets the format guarantees
  const u16 = (at: number): number => bytes[at]! | (bytes[at + 1]! << 8);
  if (bytes.length < HEADER || u32(0) !== MAGIC || u32(4) !== VERSION) return null;

  const count = u32(8);
  const index: SearchIndex = {
    bytes,
    count,
    names: u32(16),
    restarts: u32(24),
    restart: u32(12),
    kindOf: u32(36),
    moduleOf: u32(40),
    labels: [],
    folds: new Map(),
    narrow: null,
    score: new Uint16Array(count),
    length: new Uint16Array(count),
    id: count < 65536 ? new Uint16Array(count) : new Uint32Array(count),
  };

  const labelsAt = u32(28);
  let at = labelsAt + 4;
  for (let i = 0, n = u32(labelsAt); i < n; i++) {
    // biome-ignore lint/style/noNonNullAssertion: offsets the format guarantees
    const len = bytes[at]!;
    index.labels.push(TEXT.decode(bytes.subarray(at + 1, at + 1 + len)));
    at += 1 + len;
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

/**
 * UTF-16 length, which is what the scoring counts (`String.prototype.length`).
 *
 * **Not the code point count.** A character above the BMP is one code point and
 * **two** UTF-16 units, so a 4-byte UTF-8 sequence counts twice — U1 again. The
 * browser gate caught this ranking `Micro.script𝒜` above `Micro.usesDep`
 * 【実測 2026-08-19】: both are prefix matches, and the score is
 * `2000 - length`, so one unit of length is one place in the list.
 */
export function utf16Length(bytes: Uint8Array, from: number, to: number): number {
  let n = 0;
  for (let i = from; i < to; i++) {
    // biome-ignore lint/style/noNonNullAssertion: `from`..`to` is a decoded name
    const byte = bytes[i]!;
    if ((byte & 0xc0) !== 0x80) n += byte >= 0xf0 ? 2 : 1;
  }
  return n;
}

/** The declaration at `id`, decoded from the start of its restart block. */
export function nameAt(index: SearchIndex, id: number): string {
  const bytes = index.bytes;
  const block = Math.floor(id / index.restart);
  const restartAt = index.restarts + block * 4;
  let at =
    index.names +
    // biome-ignore lint/style/noNonNullAssertion: the restart table has one entry per block
    ((bytes[restartAt]! | (bytes[restartAt + 1]! << 8) | (bytes[restartAt + 2]! << 16)) +
      // biome-ignore lint/style/noNonNullAssertion: the restart table has one entry per block
      bytes[restartAt + 3]! * 0x100_0000);
  let out = new Uint8Array(256);
  let end = 0;
  for (let i = block * index.restart; i <= id; i++) {
    // biome-ignore lint/style/noNonNullAssertion: offsets the format guarantees
    const shared = bytes[at++]!;
    // biome-ignore lint/style/noNonNullAssertion: offsets the format guarantees
    let len = bytes[at++]!;
    if (len === 255) {
      // biome-ignore lint/style/noNonNullAssertion: offsets the format guarantees
      len = bytes[at]! | (bytes[at + 1]! << 8);
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

/** The badge label for `id`, or `""` if the file names a kind it has no label for. */
export const kindAt = (index: SearchIndex, id: number): string =>
  // biome-ignore lint/style/noNonNullAssertion: one kind byte per declaration
  index.labels[index.bytes[index.kindOf + id]!] ?? "";

/** The subscript, into `modules.json`'s array, of the module `id` is declared in. */
export const moduleAt = (index: SearchIndex, id: number): number =>
  // biome-ignore lint/style/noNonNullAssertion: one u16 per declaration
  index.bytes[index.moduleOf + id * 2]! | (index.bytes[index.moduleOf + id * 2 + 1]! << 8);

/**
 * Where each of `names` is, as one walk of the index.
 *
 * The names come from `instances.json` and are exact, so this is equality
 * rather than scoring — but it is the same walk, for the same reason: front
 * coding is read forwards, and a lookup per name would read the section once
 * per name.
 */
export function findNames(index: SearchIndex, names: readonly string[]): Map<string, number> {
  const wanted = new Set(names);
  const found = new Map<string, number>();
  const bytes = index.bytes;
  let at = index.names;
  for (let i = 0; i < index.count && found.size < wanted.size; i++) {
    // biome-ignore lint/style/noNonNullAssertion: offsets the format guarantees
    const shared = bytes[at++]!;
    // biome-ignore lint/style/noNonNullAssertion: offsets the format guarantees
    let len = bytes[at++]!;
    if (len === 255) {
      // biome-ignore lint/style/noNonNullAssertion: offsets the format guarantees
      len = bytes[at]! | (bytes[at + 1]! << 8);
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
