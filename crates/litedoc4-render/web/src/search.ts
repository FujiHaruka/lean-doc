/**
 * The two ways into the index, both producing the same ranked list.
 *
 * [`search`] decodes, folds and scores in one pass, into buffers that outlive
 * the call — nothing is allocated per declaration, where the version this
 * replaces allocated two strings each, 9,168 per keystroke on the measured
 * package【実測】.
 *
 * [`searchNarrowed`] is the other way. **All three tiers require the query to
 * occur in the folded name**, so typing one more character can only shrink the
 * hit set, and a query that extends the previous one is answered from what the
 * previous one matched. Typing five representative words costs 48.2% of the
 * candidates it would rescanning【実測 2026-08-19】. The browser gate types a
 * query one character at a time rather than pasting it, because a narrowing
 * that is wrong is only wrong on the second keystroke.
 *
 * The `!` convention is `index-format.ts`'s — same reason, same two files.
 */
import { DOT, ENCODER, FOLD, utf16Length } from "./index-format.js";
import { NARROW_MAX, rank, scoreBytes } from "./score.js";
import { folded, room, scratch } from "./scratch.js";
import type { Narrow, SearchIndex } from "./types.js";

/** Every hit for `query`, best first, as subscripts into the index. */
export function search(index: SearchIndex, query: string): number[] {
  const q = ENCODER.encode(query);
  const qn = q.length;
  const narrow = index.narrow;
  if (narrow && query.startsWith(narrow.query)) return searchNarrowed(index, narrow, q, qn, query);

  const bytes = index.bytes;
  const hasFolds = index.folds.size > 0;
  const kept: Omit<Narrow, "query"> = { names: [], starts: [], ids: [] };
  let at = index.names;
  let hits = 0;
  let lastDot = -1;
  for (let i = 0; i < index.count; i++) {
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
    for (let k = 0; k < len; k++) {
      // biome-ignore lint/style/noNonNullAssertion: offsets the format guarantees
      const b = bytes[at + k]!;
      scratch[shared + k] = b;
      // biome-ignore lint/style/noNonNullAssertion: `FOLD` has all 256 entries
      folded[shared + k] = FOLD[b]!;
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
    // Widened from `folded`: a fold exception is a subarray of the file, whose
    // buffer type is not the one `new Uint8Array` gives.
    let name: Uint8Array = folded;
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
function searchNarrowed(
  index: SearchIndex,
  narrow: Narrow,
  q: Uint8Array,
  qn: number,
  query: string,
): number[] {
  const kept: Omit<Narrow, "query"> = { names: [], starts: [], ids: [] };
  let hits = 0;
  for (let k = 0; k < narrow.ids.length; k++) {
    // biome-ignore lint/style/noNonNullAssertion: the three arrays are the same length
    const name = narrow.names[k]!;
    // biome-ignore lint/style/noNonNullAssertion: the three arrays are the same length
    const s = scoreBytes(name, name.length, narrow.starts[k]!, q, qn);
    if (s > 0) {
      // biome-ignore lint/style/noNonNullAssertion: the three arrays are the same length
      index.id[hits] = narrow.ids[k]!;
      index.score[hits] = s;
      index.length[hits] = utf16Length(name, 0, name.length);
      kept.names.push(name);
      // biome-ignore lint/style/noNonNullAssertion: the three arrays are the same length
      kept.starts.push(narrow.starts[k]!);
      // biome-ignore lint/style/noNonNullAssertion: the three arrays are the same length
      kept.ids.push(narrow.ids[k]!);
      hits++;
    }
  }
  index.narrow = { query, ...kept };
  return rank(index, hits);
}
