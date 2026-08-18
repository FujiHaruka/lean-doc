#!/usr/bin/env python3
"""Compare litedoc4's --dump-modules output with doc-gen4's database."""
import json, sqlite3, sys, collections

dump, db = sys.argv[1], sys.argv[2]
mods = {}
for line in open(dump):
    r = json.loads(line)
    mods[r["module"]] = r

con = sqlite3.connect(f"file:{db}?mode=ro", uri=True)
cur = con.cursor()

def report(label, ours, theirs):
    o, t = set(ours), set(theirs)
    print(f"{label:28s} litedoc4 {len(o):6d}   doc-gen4 {len(t):6d}   "
          f"{'IDENTICAL' if o == t else 'DIFFER'}")
    if o != t:
        only_o, only_t = o - t, t - o
        print(f"  only litedoc4 ({len(only_o)}): {sorted(only_o)[:5]}")
        print(f"  only doc-gen4 ({len(only_t)}): {sorted(only_t)[:5]}")

# modules
report("modules", mods.keys(), [r[0] for r in cur.execute("select name from modules")])

# imports
ours = [(m, i) for m, r in mods.items() for i in r["imports"]]
report("module_imports", ours, cur.execute("select importer, imported from module_imports"))

# module docstrings: join with declaration_ranges to get the start line
ours = [(m, d["line"], d["text"]) for m, r in mods.items() for d in r["docs"]]
theirs = cur.execute("""
  select d.module_name, r.start_line, d.text
  from module_docs_markdown d
  join declaration_ranges r
    on r.module_name = d.module_name and r.position = d.position""")
report("module_docs (mod,line,text)", ours, theirs)

# tactics
ours = [(m, t["internalName"], t["userName"], t["docString"])
        for m, r in mods.items() for t in r["tactics"]]
report("tactics", ours,
       cur.execute("select module_name, internal_name, user_name, doc_string from tactics"))

ours = [(m, t["internalName"], g) for m, r in mods.items() for t in r["tactics"] for g in t["tags"]]
report("tactic_tags", ours,
       cur.execute("select module_name, internal_name, tag from tactic_tags"))

n = sum(len(r["tactics"]) for r in mods.values())
print(f"\ntactic-defining modules (litedoc4): "
      f"{sum(1 for r in mods.values() if r['tactics'])}, total tactics {n}")
