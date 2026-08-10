#!/usr/bin/env python3
"""Snapshot / diff the .olean artifacts of the target project.

The question this serves: when module A changes, do the .olean files of the
modules that import A actually change *in content*? Lake rebuilds them either
way, so mtime is not the answer -- only the bytes are. Lake's own
`<file>.olean.hash` is recorded alongside, because that is what the incremental
ledger (L2) actually reads.

Usage:
  snapshot-oleans.py snap <clone-dir> <out.json>
  snapshot-oleans.py diff <before.json> <after.json> [--quiet]
"""
import hashlib
import json
import sys
from pathlib import Path

BUILD = ".lake/build/lib/lean"


def sha256(p: Path) -> str:
    h = hashlib.sha256()
    with p.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def snap(clone: Path, out: Path) -> None:
    root = clone / BUILD
    entries = {}
    for p in sorted(root.rglob("*.olean")):
        rel = str(p.relative_to(root))
        st = p.stat()
        hash_file = p.with_suffix(".olean.hash")
        entries[rel] = {
            "sha256": sha256(p),
            "size": st.st_size,
            "mtime_ns": st.st_mtime_ns,
            "lake_hash": hash_file.read_text().strip() if hash_file.exists() else None,
        }
    out.write_text(json.dumps({"root": str(root), "entries": entries}, indent=1))
    print(f"{len(entries)} oleans -> {out}")


def diff(before: Path, after: Path, quiet: bool) -> int:
    b = json.loads(before.read_text())["entries"]
    a = json.loads(after.read_text())["entries"]

    added = sorted(set(a) - set(b))
    removed = sorted(set(b) - set(a))
    common = sorted(set(a) & set(b))

    content_changed = [k for k in common if a[k]["sha256"] != b[k]["sha256"]]
    hash_changed = [k for k in common if a[k]["lake_hash"] != b[k]["lake_hash"]]
    touched = [k for k in common if a[k]["mtime_ns"] != b[k]["mtime_ns"]]
    # rebuilt but byte-identical: the case that decides whether an importer's
    # olean hash can carry the "this page needs regenerating" signal
    rebuilt_identical = [k for k in touched if k not in content_changed]

    print(f"olean total:        {len(a)}")
    print(f"added:              {len(added)}")
    print(f"removed:            {len(removed)}")
    print(f"mtime touched:      {len(touched)}")
    print(f"content changed:    {len(content_changed)}")
    print(f"lake_hash changed:  {len(hash_changed)}")
    print(f"rebuilt but byte-identical: {len(rebuilt_identical)}")

    if a and b:
        mism = [k for k in common if (a[k]["sha256"] != b[k]["sha256"]) != (a[k]["lake_hash"] != b[k]["lake_hash"])]
        print(f"sha256 と lake_hash の食い違い: {len(mism)}" + (f" -> {mism[:5]}" if mism else ""))

    if not quiet:
        for label, xs in (
            ("ADDED", added),
            ("REMOVED", removed),
            ("CONTENT-CHANGED", content_changed),
            ("REBUILT-IDENTICAL", rebuilt_identical),
        ):
            if xs:
                print(f"\n--- {label} ({len(xs)}) ---")
                for k in xs[:60]:
                    print(f"  {k}")
                if len(xs) > 60:
                    print(f"  ... and {len(xs) - 60} more")
    return 0


def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    if sys.argv[1] == "snap":
        snap(Path(sys.argv[2]).resolve(), Path(sys.argv[3]))
        return 0
    if sys.argv[1] == "diff":
        return diff(Path(sys.argv[2]), Path(sys.argv[3]), "--quiet" in sys.argv)
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main())
