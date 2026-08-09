#!/usr/bin/env python3
"""E2 step 1: make an instrumented copy of `experiments/stage4c/render.ts`.

The copy adds one thing: a dump of every docstring autolink token that both of
`autoLinkInline`'s `nameToLink` attempts failed on, with the pages it appeared
on. It changes no output — the check that it did not is that the 432 pages it
writes are byte-identical to the pristine renderer's, and that
`autolinkAttempts` / `autolinkResolved` are unchanged.

The measurements in E1/E2/E4 use the **pristine** `render.ts`; this copy is only
used to enumerate the candidate tokens to inject.

usage:
  instrument-render.py --out <path.ts> [--render <render.ts>]
"""
import argparse

INSERTS = [
    # (anchor, replacement)
    (
        "/** `autoLinkInline` (DocString.lean:175-197): split on Unicode Z|C, keep separators. */",
        '''/** INSTRUMENTATION (leg 8 / E2 only): every autolink token that both
 *  `nameToLink` attempts failed on, with the pages it appeared on. */
const UNRESOLVED = new Map<string, { pages: Set<string>; occ: number }>();
let currentModule = "";
function recordUnresolved(tok: string) {
  let e = UNRESOLVED.get(tok);
  if (!e) { e = { pages: new Set<string>(), occ: 0 }; UNRESOLVED.set(tok, e); }
  e.pages.add(currentModule);
  e.occ++;
}

/** `autoLinkInline` (DocString.lean:175-197): split on Unicode Z|C, keep separators. */''',
    ),
    (
        """    } else {
      out += escapeHtml(part);
    }
  }
  return out;
}""",
        """    } else {
      recordUnresolved(part);
      out += escapeHtml(part);
    }
  }
  return out;
}""",
    ),
    (
        """function pageHtml(mod: ModuleFile, headers: Map<string, string>, r: Renderer): string {
  const module = mod.module;""",
        """function pageHtml(mod: ModuleFile, headers: Map<string, string>, r: Renderer): string {
  const module = mod.module;
  currentModule = module;""",
    ),
]

TAIL = '''
const DUMP = opt("--autolink-dump");
if (DUMP) {
  const rows = [...UNRESOLVED.entries()].map(([tok, e]) => ({
    token: tok,
    occurrences: e.occ,
    pages: e.pages.size,
    isNameLit: isNameLit(tok),
    pageList: [...e.pages].sort(),
  })).sort((a, b) =>
    b.pages - a.pages || b.occurrences - a.occurrences || (a.token < b.token ? -1 : 1)
  );
  await Deno.writeTextFile(DUMP, rows.map((r) => JSON.stringify(r)).join("\\n") + "\\n");
  console.log(`unresolved autolink tokens: ${rows.length} distinct -> ${DUMP}`);
}
'''


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    ap.add_argument(
        "--render",
        default="/Users/haruka/dev/lean-doc/experiments/stage4c/render.ts",
    )
    args = ap.parse_args()
    src = open(args.render, encoding="utf-8").read()
    for anchor, repl in INSERTS:
        if src.count(anchor) != 1:
            raise SystemExit(f"anchor not found exactly once in render.ts:\n{anchor[:60]}...")
        src = src.replace(anchor, repl)
    src = src.rstrip("\n") + "\n" + TAIL
    open(args.out, "w", encoding="utf-8").write(src)
    print(f"instrumented copy -> {args.out}")


if __name__ == "__main__":
    main()
