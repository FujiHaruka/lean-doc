/**
 * The three data files as they are on the wire, and the shape the index reader
 * builds out of `search-index.bin`.
 *
 * The one-letter field names are `litedoc4_global::artifacts`'s: every module
 * carries them and the file is fetched by every page, so they were shortened
 * where it is read most and spelled out nowhere else.
 */

/** One module in `modules.json`. */
export interface ModuleEntry {
  /** Its name — `Foo.Bar`. */
  readonly n: string;
  /** Its page, relative to the site root. */
  readonly p: string;
  /** Subscripts, into the same array, of the modules that import it. */
  readonly i?: readonly number[];
}

export interface ModulesFile {
  readonly modules: readonly ModuleEntry[];
}

/** The two maps of `instances.json`, keyed by declaration name. */
export interface InstancesFile {
  readonly instances?: Readonly<Record<string, readonly string[]>>;
  readonly instancesFor?: Readonly<Record<string, readonly string[]>>;
}

/**
 * `declarations/used-by.json`: for each declaration this package documents,
 * the declarations of this package that mention it.
 *
 * A name with no users is **absent**, not an empty array — the file is the
 * largest of the four and 81% of the target package's declarations have no
 * users 【実測 2026-08-22】.
 */
export type UsedByFile = Readonly<Record<string, readonly string[]>>;

/**
 * What the previous query matched, in file order.
 *
 * Kept so that a query extending it can be answered from these rather than from
 * another walk of the whole name section — see `search`. The names are folded
 * copies, because the buffer they were folded into is written over by the next
 * declaration.
 */
export interface Narrow {
  readonly query: string;
  readonly names: Uint8Array[];
  readonly starts: number[];
  readonly ids: number[];
}

/**
 * `search-index.bin`, read in place.
 *
 * The header and the two small tables are decoded once; **the names stay in the
 * buffer** and are decoded per query. The layout is
 * `crates/litedoc4-global/src/search_index.rs`.
 */
export interface SearchIndex {
  readonly bytes: Uint8Array;
  readonly count: number;
  /** Offset of the front-coded name section. */
  readonly names: number;
  /** Offset of the restart table: one u32 per block. */
  readonly restarts: number;
  /** Declarations per restart block. */
  readonly restart: number;
  /** Offset of the kind subscripts: one byte each. */
  readonly kindOf: number;
  /** Offset of the module subscripts: one u16 each. */
  readonly moduleOf: number;
  /** Badge labels, pointed at by `kindOf`. */
  readonly labels: string[];
  /** The names ASCII folding is wrong for, by subscript. Usually empty. */
  readonly folds: Map<number, Uint8Array>;

  /** The previous query and what it matched. Mutated by every search. */
  narrow: Narrow | null;

  /**
   * Scored in place: allocated once per page rather than once per keystroke,
   * and sized to what they hold rather than to a machine word. Three
   * `Int32Array`s cost **55 KiB of the 156 KiB** the index took in Chrome
   * 【実測 2026-08-19】, against a file of 106 KiB.
   */
  readonly score: Uint16Array;
  readonly length: Uint16Array;
  readonly id: Uint16Array | Uint32Array;
}

/** What every result row needs: the index, and the modules it points into. */
export interface SearchData {
  readonly modules: readonly ModuleEntry[];
  readonly index: SearchIndex;
}
