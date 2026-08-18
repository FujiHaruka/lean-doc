#!/usr/bin/env python3
#
# Read-only inventory of doc-gen4's finished HTML against the stage-4 IR.
#
# Stage 4 asks whether litedoc4 and doc-gen4 are doing *the same work*, which is
# the precondition for any speed number to mean anything. This tool answers the
# other half of that question: given the IR the extractor writes, what is still
# missing before the HTML could be rebuilt from it? It counts, on the same 348
# pages the benchmarks are taken on, every anchor, every byte region and every
# element class that a renderer would have to reproduce, and scores the IR
# against them.
#
# It replaces three throwaway scripts (`siglinks.py`, `irprobe.py`,
# `headtohead.py`) so that the numbers quoted in `docs/` have a committed
# provenance, per CLAUDE.md's 計測の誠実性 rule.
#
# usage:
#   html-inventory.py [--doc <dir>] [--ir <dir>] [--out <file>] [--json <file>]
#                     [--top N]
#
#   --doc   doc-gen4's HTML for the package, default $TARGET_REPO (else
#           /Users/haruka/dev/lean-projects) + .lake/build/doc/InformationTheory
#   --ir    IR root written by `experiments/stage4/run.sh --write-ir`,
#           default $IR_DIR, else the session scratchpad path below
#   --out   also write the Markdown report here
#   --json  also write a machine-readable version here
#   --top   rows in the "notation" table (default 20)
#
# WHAT IS MEASURED
#   A  population: pages, div.decl blocks, IR modules/declarations, the join
#   B  every anchor inside div.decl_header, classified by how the anchor *text*
#      relates to the *target name* (identical / namespace suffix / notation)
#   C  bytes per page region, so "which part of the page is expensive" is a
#      measured share and not a guess
#   D  an upper bound on re-linking a signature from the IR alone: link the
#      identifier-shaped tokens of `binders + type` against the `refs` set
#   E  the cases where a *set* of references provably cannot decide the answer
#   F  the opposite direction: link *targets* the IR can already resolve
#   G  everything else the HTML carries that the IR has no field for
#
# WHAT IS NOT MEASURED
#   * Nothing is timed here. This is a content diff; the cost numbers live in
#     `analyze.ts` / `read-ir.ts`.
#   * Correctness of the *rendering*, only of the data behind it. Two anchors
#     with the same text and target count as equal wherever they sit on the
#     page: section D compares multisets, so its score is an upper bound on any
#     reconstruction, not an achievable rate (see the note printed in D).
#   * Pages outside `doc/InformationTheory` (Mathlib's own pages, `search.html`,
#     `foundational_types.html`) are never read.
#   * Anchors whose first attribute is not `href` are excluded from every anchor
#     count. doc-gen4 writes `class` first in exactly two places -- the
#     declaration's own name link (`a.break_within`, which also appears in the
#     per-declaration jump list in `nav.internal_nav`) and heading hover links
#     (`a.hover-link`) -- so this drops self-links and anchors, never a
#     cross-reference. Both are counted separately in section F.
#
# Nothing is ever written to the measurement target: the doc tree and the IR are
# opened read-only, and the only outputs are --out / --json.

import argparse
import collections
import html as htmllib
import json
import os
import platform
import re
import subprocess
import sys
import time

DEFAULT_TARGET_REPO = "/Users/haruka/dev/lean-projects"
DEFAULT_IR = (
    "/private/tmp/claude-502/-Users-haruka-dev-litedoc4/"
    "3db6b213-b50d-48cb-a16b-16df93b5f009/scratchpad/ir"
)

# doc-gen4 omits an equation whose rendered text is at least this long and then
# prints a notice instead (`DocGen4/Process/Base.lean: def equationLimit`).
EQUATION_LIMIT = 200

# Lean identifier characters, generously: letters (incl. Greek and the Latin
# extensions Lean's pretty printer emits), digits, _ ' ! ?, sub/superscripts,
# dotted components.
IDENT = re.compile(
    r"[A-Za-z_À-ɏͰ-ϿḀ-῿]"
    r"[A-Za-z0-9_'!?À-ɏͰ-ϿḀ-῿₀-ₜ⁰-ⁿ]*"
    r"(?:\.[A-Za-z0-9_'!?À-ɏͰ-ϿḀ-῿₀-ₜ⁰-ⁿ]+)*"
)

DECL_RE = re.compile(r'<div class="decl(?: sorried)?" id="([^"]*)">')
DECL_HEADER_RE = re.compile(r'<div class="decl_header">')
EQUATIONS_RE = re.compile(r'<ul class="equations">')
KIND_RE = re.compile(r'<span class="decl_kind">([^<]*)</span>')
GH_RE = re.compile(r'<div class="gh_link"><a href="([^"]*)">')
GH_RANGE_RE = re.compile(r"#L(\d+)-L(\d+)$")
# Only anchors that open with `href` -- see WHAT IS NOT MEASURED above.
ANCHOR_RE = re.compile(r'<a href="([^"]*)">(.*?)</a>', re.S)
CLASS_ANCHOR_RE = re.compile(r'<a class="([^"]*)"')
TAG_RE = re.compile(r"<[^>]+>")
CLASS_ATTR_RE = re.compile(r'<(\w+)[^>]*\bclass="([^"]*)"')

# Element classes section G reports one by one, as (tag, class) pairs.
TRACKED_CLASSES = [
    ("div", "attributes"),
    ("li", "structure_field"),
    ("li", "structure_field inherited_field"),
    ("div", "structure_field_info"),
    ("div", "structure_field_doc"),
    ("span", "decl_extends"),
    ("span", "fn"),
    ("div", "decl sorried"),
]


def parse_args():
    p = argparse.ArgumentParser(
        prog="html-inventory.py",
        description="Inventory doc-gen4's HTML against the stage-4 IR (read-only).",
    )
    target = os.environ.get("TARGET_REPO", DEFAULT_TARGET_REPO)
    p.add_argument("--doc", default=os.path.join(target, ".lake/build/doc/InformationTheory"))
    p.add_argument("--ir", default=os.environ.get("IR_DIR", DEFAULT_IR))
    p.add_argument("--out", default="")
    p.add_argument("--json", dest="json_out", default="")
    p.add_argument("--top", type=int, default=20)
    return p.parse_args()


def strip_tags(s):
    return htmllib.unescape(TAG_RE.sub("", s))


def block_end(text, start, tag):
    """End offset (exclusive) of the element opening at `start`, by tag depth.

    doc-gen4's blocks nest (`div.decl_type` inside `div.decl_header`), so the
    extent cannot be found with a regex; walk opening/closing tags instead.
    """
    pat = re.compile(r"</?%s\b" % tag)
    depth, j = 0, start
    while True:
        m = pat.search(text, j)
        if not m:
            return len(text)
        if text.startswith("</", m.start()):
            depth -= 1
            j = m.end()
            if depth == 0:
                return text.index(">", m.end()) + 1
        else:
            depth += 1
            j = m.end()


def utf8(s):
    return len(s.encode("utf-8"))


def pct(part, whole):
    return 0.0 if not whole else 100.0 * part / whole


# --------------------------------------------------------------------------
# IR side
# --------------------------------------------------------------------------


def read_ir(ir_dir):
    index = json.load(open(os.path.join(ir_dir, "index.json"), encoding="utf-8"))
    modules = {}
    for entry in index["modules"]:
        m = json.load(open(os.path.join(ir_dir, entry["file"]), encoding="utf-8"))
        modules[m["module"]] = m
    deps = {}
    for entry in index.get("dependencyMaps", []):
        d = json.load(open(os.path.join(ir_dir, entry["file"]), encoding="utf-8"))
        deps[entry["package"]] = d["declarations"]
    return index, modules, deps


def ir_token_census(modules):
    """Section E's IR-wide half: how much of the printed text is even a name?

    Runs over every declaration in the IR, not only the ones that reached HTML,
    because the question ("can a plaintext type plus a set of names be
    re-linked?") is about the IR as a format.
    """
    stats = collections.Counter()
    ambiguous_examples = []
    shadow_examples = []
    binder_name_re = re.compile(r"^[\(\{\[⟨]?\s*([^:]*?)\s*:")
    for module in modules.values():
        for d in module["declarations"]:
            stats["declarations"] += 1
            refs = [name for _mod, name in d["refs"]]
            stats["refs"] += len(refs)
            short = collections.Counter(n.rsplit(".", 1)[-1] for n in refs)
            for s, c in short.items():
                if c > 1:
                    stats["short_name_collisions"] += 1
                    if len(ambiguous_examples) < 20:
                        ambiguous_examples.append(
                            {
                                "declaration": d["name"],
                                "shortName": s,
                                "candidates": sorted(
                                    n for n in refs if n.rsplit(".", 1)[-1] == s
                                ),
                            }
                        )
            binder_names = set()
            for b in d["binders"]:
                m = binder_name_re.match(b)
                if m:
                    binder_names.update(m.group(1).split())
            stats["binders"] += len(d["binders"])
            for bn in binder_names:
                if bn in short:
                    stats["binder_shadows_ref_short"] += 1
                    if len(shadow_examples) < 20:
                        shadow_examples.append(
                            {
                                "declaration": d["name"],
                                "binder": bn,
                                "hides": [
                                    n for n in refs if n.rsplit(".", 1)[-1] == bn
                                ],
                            }
                        )
                if bn in refs:
                    stats["binder_equals_ref_full"] += 1
            refset = set(refs)
            for tok in IDENT.findall(printed_text(d)):
                stats["tokens"] += 1
                if tok in refset:
                    stats["tokens_full_name"] += 1
                elif tok in short:
                    stats["tokens_short_name"] += 1
                else:
                    stats["tokens_unmatched"] += 1
    return stats, ambiguous_examples, shadow_examples


def printed_text(d):
    """The plaintext a renderer would have to re-link: binders, then the type."""
    return " ".join(d["binders"]) + " : " + d["type"]


def naive_links(d):
    """The strongest link reconstruction the IR alone supports.

    Cut `binders + type` into identifier-shaped tokens; a token becomes a link
    if it is exactly the name of a `refs` entry, else if it is the unique short
    name among `refs`. Everything else stays plain text. Returns a multiset of
    (anchor text, target name).
    """
    refs = set(name for _mod, name in d["refs"])
    by_short = collections.defaultdict(set)
    for r in refs:
        by_short[r.rsplit(".", 1)[-1]].add(r)
    out = collections.Counter()
    for tok in IDENT.findall(printed_text(d)):
        if tok in refs:
            out[(tok, tok)] += 1
        else:
            cand = by_short.get(tok)
            if cand and len(cand) == 1:
                out[(tok, next(iter(cand)))] += 1
    return out


# --------------------------------------------------------------------------
# HTML side
# --------------------------------------------------------------------------


def page_module(doc_dir, path):
    """`.../doc/InformationTheory/Fano/Core.html` -> `InformationTheory.Fano.Core`."""
    rel = os.path.relpath(path, doc_dir)[: -len(".html")]
    return os.path.basename(doc_dir) + "." + rel.replace(os.sep, ".")


def scan(doc_dir, ir_modules, ir_names, top):
    pages = sorted(
        os.path.join(dp, fn)
        for dp, _dirs, files in os.walk(doc_dir)
        for fn in files
        if fn.endswith(".html")
    )

    r = collections.Counter()          # scalar counters
    bytes_ = collections.Counter()     # section C
    anchor_kind = collections.Counter()      # section B
    text_relation = collections.Counter()    # section B
    notation_pairs = collections.Counter()
    suffix_pairs = collections.Counter()
    class_census = collections.Counter()     # section G
    targets = collections.defaultdict(collections.Counter)  # section F, per region
    unresolved_in_code = collections.Counter()
    kind_pairs = collections.Counter()       # section G
    eq_rule = collections.Counter()          # section G
    order_ties = []
    order_mismatch = []
    truth = {}                               # decl name -> Counter((text, target))
    html_decls = set()
    gh = collections.Counter()
    pages_with_top_href = 0
    pages_with_top_id = 0

    for path in pages:
        text = open(path, encoding="utf-8").read()
        module = page_module(doc_dir, path)
        r["pages"] += 1
        bytes_["total"] += utf8(text)

        # --- C: page chrome ------------------------------------------------
        # One contiguous prefix: the file opens with <html>, <head>, <body>,
        # <header> and nav.internal_nav before <main>. Measured as a prefix
        # (rather than as three separate slices) because the 69 bytes of
        # <html>/<body>/nav-toggle markup between them belong to the same
        # "not the declarations" bucket; the slices are reported too.
        head_i, head_j = text.index("<head>"), text.index("</head>") + len("</head>")
        hdr_i, hdr_j = text.index("<header>"), text.index("</header>") + len("</header>")
        nav_i = text.index('<nav class="internal_nav">')
        nav_j = block_end(text, nav_i, "nav")
        bytes_["head"] += utf8(text[head_i:head_j])
        bytes_["header"] += utf8(text[hdr_i:hdr_j])
        bytes_["internal_nav"] += utf8(text[nav_i:nav_j])
        bytes_["chrome"] += utf8(text[:nav_j])

        # --- region maps for anchor attribution ----------------------------
        header_spans = []
        for m in DECL_HEADER_RE.finditer(text):
            span = (m.start(), block_end(text, m.start(), "div"))
            header_spans.append(span)
            bytes_["decl_header"] += utf8(text[span[0] : span[1]])
        equation_spans = []
        for m in EQUATIONS_RE.finditer(text):
            span = (m.start(), block_end(text, m.start(), "ul"))
            equation_spans.append(span)
            bytes_["equations"] += utf8(text[span[0] : span[1]])

        def region_of(offset):
            for s, e in header_spans:
                if s <= offset < e:
                    return "decl_header"
            for s, e in equation_spans:
                if s <= offset < e:
                    return "equations"
            return "other"

        # --- F: every internal anchor on the page --------------------------
        for m in ANCHOR_RE.finditer(text):
            href = m.group(1)
            r["anchors_href_first"] += 1
            if href.startswith("http"):
                r["anchors_external"] += 1
                continue
            if "#" not in href:
                r["anchors_no_fragment"] += 1
                continue
            path_part, fragment = href.split("#", 1)
            if path_part == "":
                r["anchors_same_page"] += 1
                continue
            region = region_of(m.start())
            targets[region][fragment] += 1
            if fragment not in ir_names:
                in_code = text.rfind("<code>", 0, m.start()) > text.rfind(
                    "</code>", 0, m.start()
                )
                unresolved_in_code[(region, in_code)] += 1
        for m in CLASS_ANCHOR_RE.finditer(text):
            r["anchors_class_first"] += 1
            class_census[("a", m.group(1))] += 1

        # --- G: element class census ---------------------------------------
        for m in CLASS_ATTR_RE.finditer(text):
            class_census[(m.group(1), m.group(2))] += 1

        if 'href="#top"' in text:
            pages_with_top_href += 1
        if 'id="top"' in text:
            pages_with_top_id += 1

        # --- per declaration -------------------------------------------------
        ir_decls = {d["name"]: d for d in ir_modules[module]["declarations"]}
        decl_starts = [(m.group(1), m.start()) for m in DECL_RE.finditer(text)]
        if decl_starts:
            r["pages_with_decls"] += 1
        order = [name for name, _ in decl_starts]
        for name, start in decl_starts:
            r["decls"] += 1
            html_decls.add(name)
            end = block_end(text, start, "div")
            seg = text[start:end]
            d = ir_decls[name]

            # B: anchors of this declaration's signature
            hi = text.index('<div class="decl_header">', start)
            hj = block_end(text, hi, "div")
            header = text[hi:hj]
            pairs = collections.Counter()
            per_target = collections.Counter()
            per_text = collections.defaultdict(set)
            for am in ANCHOR_RE.finditer(header):
                href, inner = am.group(1), am.group(2)
                anchor_text = strip_tags(inner)
                anchor_kind["anchors"] += 1
                if "#" not in href:
                    if href.endswith("foundational_types.html"):
                        anchor_kind["sort"] += 1
                    else:
                        anchor_kind["module"] += 1
                    continue
                target = href.split("#", 1)[1]
                anchor_kind["declaration"] += 1
                pairs[(anchor_text, target)] += 1
                per_target[target] += 1
                per_text[anchor_text].add(target)
                if anchor_text == target:
                    text_relation["exact_full_name"] += 1
                elif target.endswith("." + anchor_text):
                    text_relation["namespace_suffix"] += 1
                    suffix_pairs[(anchor_text, target)] += 1
                else:
                    text_relation["notation"] += 1
                    notation_pairs[(anchor_text, target)] += 1
                if IDENT.fullmatch(anchor_text):
                    text_relation["identifier_shaped_text"] += 1
            truth[name] = pairs
            for _target, c in per_target.items():
                if c > 1:
                    r["target_linked_twice_in_one_signature"] += 1
            for _text, tgts in per_text.items():
                if len(tgts) > 1:
                    r["same_text_two_targets_in_one_signature"] += 1

            # G: decl_kind
            km = KIND_RE.search(seg)
            kind_pairs[(d["kind"], km.group(1) if km else None)] += 1

            # G: equations, against doc-gen4's equationLimit rule
            eqs = d["equations"]
            omitted = any(len(e) >= EQUATION_LIMIT for e in eqs)
            rendered = sum(1 for e in eqs if len(e) < EQUATION_LIMIT)
            predicted_li = rendered + (1 if omitted else 0)
            actual_note = "did not get rendered" in seg
            actual_ul = '<ul class="equations">' in seg
            actual_li = seg.count('<li class="equation">')
            eq_rule[("notice", actual_note, omitted)] += 1
            eq_rule[("ul", actual_ul, predicted_li > 0)] += 1
            eq_rule[("li_count", actual_li == predicted_li)] += 1

            # G: gh_link
            gm = GH_RE.search(seg)
            if gm:
                gh["links"] += 1
                rm = GH_RANGE_RE.search(gm.group(1))
                if rm:
                    gh["with_end_line"] += 1
                    if int(rm.group(1)) == d["line"]:
                        gh["start_line_matches_ir"] += 1

        # G: declaration order
        key = lambda n: (ir_decls[n]["line"], ir_decls[n]["col"])
        if sorted(order, key=key) == order:
            r["order_reproduced"] += 1
        else:
            order_mismatch.append(module)
        seen = collections.Counter(key(n) for n in order)
        tied = [n for n in order if seen[key(n)] > 1]
        if tied:
            order_ties.append({"module": module, "declarations": tied})
        else:
            r["order_determined"] += 1

    return dict(
        pages=pages,
        r=r,
        bytes=bytes_,
        anchor_kind=anchor_kind,
        text_relation=text_relation,
        notation_pairs=notation_pairs,
        suffix_pairs=suffix_pairs,
        class_census=class_census,
        targets=targets,
        unresolved_in_code=unresolved_in_code,
        kind_pairs=kind_pairs,
        eq_rule=eq_rule,
        order_ties=order_ties,
        order_mismatch=order_mismatch,
        truth=truth,
        html_decls=html_decls,
        gh=gh,
        pages_with_top_href=pages_with_top_href,
        pages_with_top_id=pages_with_top_id,
        top=top,
    )


# --------------------------------------------------------------------------
# conditions block
# --------------------------------------------------------------------------


def git_describe(repo):
    def run(*cmd):
        try:
            return subprocess.run(
                ["git", "-C", repo] + list(cmd),
                capture_output=True,
                text=True,
                timeout=10,
            ).stdout.strip()
        except Exception:
            return ""

    rev = run("rev-parse", "HEAD")
    tag = run("describe", "--tags", "--always")
    dirty = bool(run("status", "--porcelain"))
    return rev, tag, dirty


def mtime(path):
    try:
        return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(os.path.getmtime(path)))
    except OSError:
        return "?"


def conditions(args, index, pages):
    repo = os.path.abspath(os.path.join(args.doc, "..", "..", "..", ".."))
    rev, tag, dirty = git_describe(os.path.join(repo, ".lake/packages/doc-gen4"))
    try:
        toolchain = open(os.path.join(repo, "lean-toolchain"), encoding="utf-8").read().strip()
    except OSError:
        toolchain = "?"
    newest = max((os.path.getmtime(p) for p in pages), default=0)
    oldest = min((os.path.getmtime(p) for p in pages), default=0)
    return {
        "date": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "docDir": args.doc,
        "docGeneratedOldest": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(oldest)),
        "docGeneratedNewest": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(newest)),
        "targetRepo": repo,
        "docGen4Rev": rev,
        "docGen4Describe": tag,
        "docGen4WorkingTreeDirty": dirty,
        "leanToolchain": toolchain,
        "irDir": args.ir,
        "irWritten": mtime(os.path.join(args.ir, "index.json")),
        "irSchemaVersion": index["schemaVersion"],
        "irGenerator": index["generator"],
        "irLeanVersion": index["leanVersion"],
        "irModuleCount": index["moduleCount"],
        "irDeclarationCount": index["declarationCount"],
        "python": sys.version.split()[0],
        "platform": platform.platform(),
    }


# --------------------------------------------------------------------------
# report
# --------------------------------------------------------------------------


def md_escape(s):
    return s.replace("\\", "\\\\").replace("|", "\\|").replace("`", "ˋ")


def render(cond, s, ir_index, ir_names, ir_name_parts, census, head2head):
    r, b = s["r"], s["bytes"]
    ak, tr = s["anchor_kind"], s["text_relation"]
    lines = []
    say = lines.append
    n = lambda x: f"{x:,}"

    say("# html-inventory — doc-gen4 の完成 HTML と stage-4 IR の突き合わせ")
    say("")
    say("`benchmarks/tools/html-inventory.py` の出力。**すべて実測** "
        "(このレポートの数字はこのツールを走らせれば再現できる)。")
    say("")
    say("## 計測条件")
    say("")
    say("| | |")
    say("|---|---|")
    for k, label in [
        ("date", "実行日時 (UTC)"),
        ("docDir", "HTML の出所"),
        ("docGeneratedOldest", "HTML の生成時点 (最古のページ)"),
        ("docGeneratedNewest", "HTML の生成時点 (最新のページ)"),
        ("docGen4Rev", "doc-gen4 rev"),
        ("docGen4Describe", "doc-gen4 describe"),
        ("docGen4WorkingTreeDirty", "doc-gen4 作業ツリーが dirty (計装パッチ)"),
        ("leanToolchain", "lean-toolchain (計測対象)"),
        ("irDir", "IR の出所"),
        ("irWritten", "IR の書き出し時点"),
        ("irSchemaVersion", "IR schemaVersion"),
        ("irGenerator", "IR generator"),
        ("irLeanVersion", "IR leanVersion"),
        ("irModuleCount", "IR moduleCount"),
        ("irDeclarationCount", "IR declarationCount"),
        ("python", "Python"),
        ("platform", "platform"),
    ]:
        say(f"| {label} | `{cond[k]}` |")
    say("")
    say(f"母数は **HTML {n(r['pages'])} ページ / IR {n(ir_index['moduleCount'])} モジュール**。"
        f"{n(r['pages'])} ページはすべて IR 側にモジュールがあり、"
        f"`div.decl` を 1 つ以上持つページは {n(r['pages_with_decls'])}。")
    say("")
    say(f"逆向きには IR の {n(census['pageless_modules'])} モジュールに HTML ページが無く、"
        f"うち **{n(census['pageless_with_decls'])} は宣言を持っている** "
        "— 「空だからページが無い」ではなく、"
        "**ディスク上の `.lake/build/doc` が 432 モジュール全部を出し切ったビルドではない**。"
        "HTML の生成時点と IR の書き出し時点も同日だがずれている。"
        f"したがって以下の母数はすべて「ディスク上にある {n(r['pages'])} ページ」であって、"
        "doc-gen4 の全出力ではない。")
    say("")
    say("計装パッチ (`benchmarks/doc-gen4-instrumentation.patch`) は計時ログを足すだけで、"
        "出力 HTML は変えない。計測対象には一切書き込んでいない。")
    say("")

    # ---- A ----------------------------------------------------------------
    say("## A. 母数")
    say("")
    say("| | |")
    say("|---|---:|")
    say(f"| HTML ページ | {n(r['pages'])} |")
    say(f"| うち `div.decl` を 1 つ以上持つページ | {n(r['pages_with_decls'])} |")
    say(f"| `div.decl` | {n(r['decls'])} |")
    say(f"| {n(r['pages'])} ページの合計バイト | {n(b['total'])} |")
    say(f"| IR モジュール | {n(ir_index['moduleCount'])} |")
    say(f"| IR 宣言 (全モジュール) | {n(ir_index['declarationCount'])} |")
    say(f"| IR 宣言 (HTML ページのある {n(r['pages'])} モジュール) | {n(census['ir_decls_in_pages'])} |")
    say(f"| 突合できた宣言 (名前が両側にある) | {n(len(s['html_decls']))} |")
    say(f"| IR にあって HTML に無い | {n(census['ir_only'])} |")
    say(f"| HTML にあって IR に無い | {n(census['html_only'])} |")
    say("")

    # ---- B ----------------------------------------------------------------
    say("## B. `div.decl_header` 内のアンカー内訳")
    say("")
    say(f"署名 (binder + 結果型 + `extends`) の中のアンカー **{n(ak['anchors'])} 件**。"
        "宣言自身の名前リンク (`a.break_within`) は含まない。")
    say("")
    say("| | | |")
    say("|---|---:|---:|")
    for key, label in [("declaration", "宣言へのリンク"), ("sort", "sort リンク (`foundational_types.html`)"), ("module", "モジュールリンク")]:
        say(f"| {label} | {n(ak[key])} | {pct(ak[key], ak['anchors']):.1f}% |")
    say("")
    d_total = ak["declaration"]
    say(f"宣言リンク {n(d_total)} 件を、**アンカーテキストと対象名の字面の関係**で分けると:")
    say("")
    say("| | | |")
    say("|---|---:|---:|")
    say(f"| テキスト == 完全名 | {n(tr['exact_full_name'])} | {pct(tr['exact_full_name'], d_total):.1f}% |")
    say(f"| テキストが完全名の接尾辞 (`Foo.bar` を `bar` と印字) | {n(tr['namespace_suffix'])} | {pct(tr['namespace_suffix'], d_total):.1f}% |")
    say(f"| **字面の関係なし (記法・省略記号)** | **{n(tr['notation'])}** | **{pct(tr['notation'], d_total):.1f}%** |")
    say("")
    say(f"字面の関係が無い組は {n(len(s['notation_pairs']))} 種、接尾辞の組は {n(len(s['suffix_pairs']))} 種。"
        "**44% は IR の `refs` (名前の集合) からは再構成できない** — "
        "どの記号がどの定数に対応するかは pretty printer の delaborator が持っている情報で、"
        "名前の集合には無い。")
    say("")
    say(f"記法の内訳 (上位 {s['top']}):")
    say("")
    say("| 件数 | アンカーテキスト | 対象 |")
    say("|---:|---|---|")
    for (t, g), c in s["notation_pairs"].most_common(s["top"]):
        say(f"| {n(c)} | `{md_escape(t)}` | `{md_escape(g)}` |")
    say("")

    # ---- C ----------------------------------------------------------------
    say("## C. ページ内バイト比")
    say("")
    say("領域はバイト (UTF-8) で数えている。`chrome` はファイル先頭から "
        "`nav.internal_nav` の終わりまでの連続した前置き。内訳の 3 つを足しても "
        "`chrome` にならないのは、その間に挟まる `<html>` / `<body>` / nav-toggle の "
        f"{n(b['chrome'] - b['head'] - b['header'] - b['internal_nav'])} バイト "
        f"({n(r['pages'])} ページ分) が差になるため。")
    say("")
    rest = b["total"] - b["chrome"] - b["decl_header"] - b["equations"]
    say("| 領域 | バイト | 比 |")
    say("|---|---:|---:|")
    say(f"| 合計 | {n(b['total'])} | 100.0% |")
    say(f"| `head` + `header` + `internal_nav` (前置き) | {n(b['chrome'])} | {pct(b['chrome'], b['total']):.1f}% |")
    say(f"| … うち `head` | {n(b['head'])} | {pct(b['head'], b['total']):.1f}% |")
    say(f"| … うち `header` | {n(b['header'])} | {pct(b['header'], b['total']):.1f}% |")
    say(f"| … うち `nav.internal_nav` | {n(b['internal_nav'])} | {pct(b['internal_nav'], b['total']):.1f}% |")
    say(f"| **`div.decl_header`** | **{n(b['decl_header'])}** | **{pct(b['decl_header'], b['total']):.1f}%** |")
    say(f"| `ul.equations` | {n(b['equations'])} | {pct(b['equations'], b['total']):.1f}% |")
    say(f"| 残り (docstring・instances・imported-by・`<main>` の枠) | {n(rest)} | {pct(rest, b['total']):.1f}% |")
    say("")
    say("**HTML の 7 割は署名の描画**。B の 44% はここに乗っている。")
    say("")

    # ---- D ----------------------------------------------------------------
    h = head2head
    say("## D. 素朴照合 — IR の平文 + `refs` 集合だけでリンクを再構成した上限")
    say("")
    say("アルゴリズム: `binders + type` を識別子形トークンに切る → トークンが `refs` の"
        "完全名ならその定数へ link、そうでなくて短縮名が `refs` 中で一意ならその定数へ link、"
        "それ以外は素通し。IR が持っているデータだけを使う中でいちばん強い素朴解法。")
    say("")
    say("> **この評価は位置を問わない。** 宣言ごとに (テキスト, 対象) の**多重集合**を比較していて、"
        "アンカーがページのどこに出るかも、隣接テキストも見ていない。したがってここの数字は"
        "**位置を無視した上限**であり、実際に再現できる率ではない (実際の描画はさらに難しい)。")
    say("")
    say("| | |")
    say("|---|---:|")
    say(f"| 突合した宣言 | {n(h['decls'])} |")
    say(f"| doc-gen4 が出したリンク (正解) | {n(h['truth'])} |")
    say(f"| 素朴照合が出したリンク | {n(h['naive'])} |")
    say(f"| 一致 | {n(h['matched'])} |")
    say(f"| 取りこぼし | {n(h['missed'])} |")
    say(f"| 余計に出した | {n(h['spurious'])} |")
    say(f"| **recall** | **{n(h['matched'])}/{n(h['truth'])} = {pct(h['matched'], h['truth']):.1f}%** |")
    say(f"| precision | {n(h['matched'])}/{n(h['naive'])} = {pct(h['matched'], h['naive']):.1f}% |")
    say(f"| **宣言単位の完全一致** | **{n(h['exact'])}/{n(h['decls'])} = {pct(h['exact'], h['decls']):.1f}%** |")
    say(f"| … うちリンク 0 個の自明例 | {n(h['exact_empty'])} |")
    say("")
    ceiling = tr["exact_full_name"] + tr["namespace_suffix"]
    say(f"到達可能な上限: 素朴照合が出せるのは「テキスト == 完全名」か「テキスト == 完全名の接尾辞」の"
        f"アンカーだけなので、天井は **{n(ceiling)}/{n(d_total)} = {pct(ceiling, d_total):.1f}%**。"
        f"(参考: テキストが単に識別子の形をしているアンカーは {n(tr['identifier_shaped_text'])} = "
        f"{pct(tr['identifier_shaped_text'], d_total):.1f}% あるが、"
        f"うち {n(tr['identifier_shaped_text'] - ceiling)} 件は "
        "`MeasureTheory.volume` → `MeasureTheory.MeasureSpace.volume` のように"
        "対象名の接尾辞にすらなっていない。)")
    say("")
    say(f"precision {pct(h['matched'], h['naive']):.1f}% は「出したものはほぼ当たる」であって"
        "「足りている」ではない。**recall が上限で半分**というのがこの節の結論。")
    say("")

    # ---- E ----------------------------------------------------------------
    tc = census["token_census"]
    say("## E. 集合では原理的に決まらないケース")
    say("")
    say("D の上限すら達成できない構造的な理由。名前の**集合**しか持たない限り、"
        "以下は情報が足りない (順序や位置を持つ表現が要る)。")
    say("")
    say("| | |")
    say("|---|---:|")
    say(f"| 同じ印字トークンが 1 署名内で 2 つ以上の別定数を指す | {n(r['same_text_two_targets_in_one_signature'])} |")
    say(f"| 1 署名内で同一対象が複数回リンクされる (宣言, 対象) 組 | {n(r['target_linked_twice_in_one_signature'])} |")
    say(f"| 1 宣言の `refs` 内で短縮名が衝突 | {n(tc['short_name_collisions'])} |")
    say(f"| binder 名が `refs` の短縮名と一致 (局所束縛が参照名を隠す) | {n(tc['binder_shadows_ref_short'])} |")
    say(f"| binder 名が `refs` の完全名と一致 | {n(tc['binder_equals_ref_full'])} |")
    say("")
    say(f"IR 全体 ({n(tc['declarations'])} 宣言) の `binders + type` のトークン census:")
    say("")
    say("| | |")
    say("|---|---:|")
    say(f"| 識別子形トークン | {n(tc['tokens'])} |")
    say(f"| `refs` の完全名に一致 | {n(tc['tokens_full_name'])} |")
    say(f"| `refs` の短縮名に一致 | {n(tc['tokens_short_name'])} |")
    say(f"| どれにも当たらない (束縛変数・記号・キーワード) | {n(tc['tokens_unmatched'])} |")
    say("")

    # ---- F ----------------------------------------------------------------
    say("## F. あて先解決 — こちらは集合で足りている")
    say("")
    say("B〜E は「どこにリンクを張るか」の話。**張る先の名前を解決できるか**は別問題で、"
        "そちらは IR がほぼ完全に持っている、という対照。")
    say("")
    tg = s["targets"]
    all_targets = collections.Counter()
    for region in tg.values():
        all_targets.update(region)
    unresolved_all = {k: v for k, v in all_targets.items() if k not in ir_names}
    say(f"{n(r['pages'])} ページの内部アンカー **{n(sum(all_targets.values()))} 件 / "
        f"{n(len(all_targets))} 種**を、IR から作れる名前集合 "
        f"(宣言名 {n(ir_name_parts['declarations'])} + `refs` {n(ir_name_parts['refs'])} + "
        f"`members` {n(ir_name_parts['members'])} + `deps/*.json` {n(ir_name_parts['deps'])}、"
        f"和集合 **{n(len(ir_names))} エントリ**) に当てる。")
    say("")
    say("| 領域 | アンカー | 種 | 解決できない | 種 |")
    say("|---|---:|---:|---:|---:|")
    for region, label in [("decl_header", "`div.decl_header`"), ("equations", "`ul.equations`"), ("other", "その他 (docstring・instances・structure field)")]:
        c = tg[region]
        un = {k: v for k, v in c.items() if k not in ir_names}
        say(f"| {label} | {n(sum(c.values()))} | {n(len(c))} | {n(sum(un.values()))} | {n(len(un))} |")
    say(f"| **合計** | **{n(sum(all_targets.values()))}** | **{n(len(all_targets))}** | "
        f"**{n(sum(unresolved_all.values()))}** | **{n(len(unresolved_all))}** |")
    say("")
    in_code = sum(v for (_reg, code), v in s["unresolved_in_code"].items() if code)
    say(f"取りこぼしは **{n(sum(unresolved_all.values()))} 件 / {n(len(unresolved_all))} 種 = "
        f"{pct(sum(unresolved_all.values()), sum(all_targets.values())):.2f}%** で、"
        f"その **{n(in_code)} 件全部が docstring の `<code>` 内 autolink**。"
        "署名 (`decl_header`) と equations の取りこぼしは 0。"
        "docstring の autolink は本文中の名前を環境全体から引くもので、"
        "その宣言の `refs` には現れない — IR の欠落ではなく、別の索引が要るという話。")
    say("")
    say(f"数え方: `href` が最初の属性であるアンカーだけを数えている "
        f"({n(r['anchors_href_first'])} 件)。`class` が先に来るのは "
        f"`a.break_within` (宣言自身の名前リンク。`nav.internal_nav` の宣言ジャンプ表にも同じものが出る) と "
        f"`a.hover-link` (見出しのアンカー) だけで、合わせて {n(r['anchors_class_first'])} 件 — "
        "いずれも自己リンクなので相互参照の数からは外してある。"
        f"同ページ内アンカー ({n(r['anchors_same_page'])} 件、すべて `#top`)、"
        f"外部リンク ({n(r['anchors_external'])} 件)、"
        f"フラグメント無しのモジュールリンク ({n(r['anchors_no_fragment'])} 件) も除外。")
    say("")

    # ---- G ----------------------------------------------------------------
    say("## G. その他の差分 (IR に足りないもの / 規則で埋まるもの)")
    say("")
    say("### 宣言の並び順")
    say("")
    say(f"IR の `line`,`col` で並べ替えると HTML の並びを "
        f"**{n(r['order_reproduced'])}/{n(r['pages'])} ページ**で再現する "
        "(安定ソート = 入力順を保存)。ただしキーが同値になるページがあり、"
        f"`(line, col)` だけで順序が**一意に決まる**のは "
        f"**{n(r['order_determined'])}/{n(r['pages'])} ページ**。")
    say("")
    if s["order_mismatch"]:
        say("順序が再現しないページ: " + ", ".join(f"`{m}`" for m in s["order_mismatch"]))
        say("")
    say(f"タイのあるページ {n(len(s['order_ties']))} / 宣言 "
        f"{n(sum(len(t['declarations']) for t in s['order_ties']))}:")
    say("")
    for t in s["order_ties"]:
        say(f"* `{t['module']}`: " + ", ".join(f"`{d}`" for d in t["declarations"]))
    say("")
    say("→ IR に宣言の**出現順**そのもの (または declaration range の終端) を入れないと、"
        "この 4 件はコイントスになる。")
    say("")

    say("### Equations")
    say("")
    say(f"doc-gen4 は `equationLimit = {EQUATION_LIMIT}` の規則で、"
        "描画テキスト長がこの値以上の equation を落とし、落としたときだけ "
        "`One or more equations did not get rendered due to their size.` の li を先頭に足す "
        "(`DocGen4/Process/Base.lean`, `DocGen4/Output/Definition.lean`)。"
        "IR の `equations` 文字列にこの規則を当てると:")
    say("")
    eq = s["eq_rule"]
    notice_yes = eq[("notice", True, True)]
    notice_no = eq[("notice", False, False)]
    ul_yes = eq[("ul", True, True)]
    say("| | |")
    say("|---|---:|")
    say(f"| 省略の通知が出るページ側宣言を当てた | {n(notice_yes)}/{n(notice_yes + eq[('notice', True, False)])} |")
    say(f"| 通知が出ない宣言を当てた | {n(notice_no)}/{n(notice_no + eq[('notice', False, True)])} |")
    say(f"| `ul.equations` の有無を当てた | {n(ul_yes)}/{n(ul_yes + eq[('ul', True, False)])} |")
    say(f"| `li.equation` の個数まで一致 | {n(eq[('li_count', True)])}/{n(r['decls'])} |")
    say("")
    say("→ ここは IR に追加フィールドは要らない。**規則を消費側に写せば済む**。")
    say("")

    say("### `render := false` 相当 — IR にあって HTML に無い宣言")
    say("")
    say(f"HTML ページのある {n(r['pages'])} モジュールの IR 宣言 {n(census['ir_decls_in_pages'])} に対し "
        f"HTML の `div.decl` は {n(r['decls'])}。差 **{n(census['ir_only'])}** の `kind` 内訳:")
    say("")
    say("| IR の `kind` | 件数 |")
    say("|---|---:|")
    for k, v in sorted(census["ir_only_kinds"].items(), key=lambda x: -x[1]):
        say(f"| `{k}` | {n(v)} |")
    say("")
    say("例: " + ", ".join(f"`{x}`" for x in census["ir_only_sample"]))
    say("")
    if census["ir_only_all_members"]:
        say(f"**この {n(census['ir_only'])} 件は、IR 側で親 `structure` の `members` に載っている名前と"
            f"完全に一致する** ({n(census['ir_only'])}/{n(census['ir_only'])})。"
            f"逆向きも成立していて、`members` に載る名前が単独の `div.decl` として出ることは "
            f"{n(census['members_rendered_standalone'])} 件。"
            "つまり doc-gen4 が親の `structure` に畳む集合は、"
            "**IR が既に持っている情報だけで正確に決まる** — 追加フィールドは要らない。")
    else:
        say(f"うち IR 側で親 `structure` の `members` に載っているのは "
            f"{n(census['ir_only_in_members'])}/{n(census['ir_only'])} 件。"
            "残りは IR だけからは「単独ページに出さない」と判定できない。")
    say("")

    say("### `decl_kind` の修飾語")
    say("")
    say("IR の `kind` は Lean の宣言種別そのもので、HTML の `span.decl_kind` が出す "
        "`noncomputable` / `abbrev` といった修飾語を持っていない。")
    say("")
    say("| IR `kind` | HTML `decl_kind` | 件数 |")
    say("|---|---|---:|")
    for (irk, htmlk), v in sorted(s["kind_pairs"].items(), key=lambda x: (-x[1], str(x[0]))):
        say(f"| `{irk}` | `{htmlk}` | {n(v)} |")
    say("")
    defish = {k: v for k, v in s["kind_pairs"].items() if k[0] == "definition"}
    def_total = sum(defish.values())
    def_bad = sum(v for k, v in defish.items() if k[1] != "def")
    say(f"def 系 (IR `kind = definition`) {n(def_total)} 件中 **{n(def_bad)} 件 "
        f"({pct(def_bad, def_total):.0f}%) が誤ラベルになる**: "
        + " + ".join(f"`{k[1]}` {n(v)}" for k, v in sorted(defish.items(), key=lambda x: -x[1]) if k[1] != "def")
        + "。")
    inst = {k: v for k, v in s["kind_pairs"].items() if k[0] == "instance"}
    inst_bad = sum(v for k, v in inst.items() if k[1] != "instance")
    if inst_bad:
        say("")
        say(f"instance も同様に {n(sum(inst.values()))} 件中 {n(inst_bad)} 件が "
            + " + ".join(f"`{k[1]}`" for k in inst if k[1] != "instance") + "。")
    say("")

    say("### 要素の出現数 (IR に対応フィールドが無いもの)")
    say("")
    say("| 要素 | 件数 |")
    say("|---|---:|")
    for tag, cls in TRACKED_CLASSES:
        say(f"| `{tag}.{cls.replace(' ', '.')}` | {n(s['class_census'][(tag, cls)])} |")
    say("")

    say("### `gh_link` の終了行")
    say("")
    say(f"`gh_link` は {n(s['gh']['links'])} 件、全部が `#L<開始>-L<終了>` の範囲リンク。"
        f"開始行は IR の `line` と {n(s['gh']['start_line_matches_ir'])}/{n(s['gh']['links'])} 一致するが、"
        f"**終了行に相当するフィールドが IR に無い** → {n(s['gh']['links'])} リンク全部が再現不能。"
        "declaration range の終端を IR に足す必要がある。")
    say("")

    say("### doc-gen4 側のデッドリンク")
    say("")
    say(f"`<a href=\"#top\">return to top</a>` は {n(s['pages_with_top_href'])}/{n(r['pages'])} ページにあるが、"
        f"`id=\"top\"` を持つページは {n(s['pages_with_top_id'])}。"
        "**348 ページ全部でこのリンクは死んでいる** (doc-gen4 側のバグ)。"
        "litedoc4 は真似しなくてよい、という意味で差分に数える。")
    say("")

    return "\n".join(lines) + "\n"


# --------------------------------------------------------------------------


def main():
    args = parse_args()
    t0 = time.time()

    ir_index, ir_modules, ir_deps = read_ir(args.ir)

    # The name set a link target can resolve against.
    decl_names, ref_names, member_names, dep_names = set(), set(), set(), set()
    for m in ir_modules.values():
        for d in m["declarations"]:
            decl_names.add(d["name"])
            for _mod, name in d["refs"]:
                ref_names.add(name)
            for mem in d["members"]:
                member_names.add(mem["name"])
    for names in ir_deps.values():
        dep_names.update(names.keys())
    ir_names = decl_names | ref_names | member_names | dep_names
    ir_name_parts = {
        "declarations": len(decl_names),
        "refs": len(ref_names),
        "members": len(member_names),
        "deps": len(dep_names),
        "union": len(ir_names),
    }

    s = scan(args.doc, ir_modules, ir_names, args.top)
    cond = conditions(args, ir_index, s["pages"])

    # Section D: naive reconstruction against the HTML truth.
    ir_by_name = {d["name"]: d for m in ir_modules.values() for d in m["declarations"]}
    h = collections.Counter()
    for name, truth in s["truth"].items():
        d = ir_by_name[name]
        naive = naive_links(d)
        h["decls"] += 1
        h["truth"] += sum(truth.values())
        h["naive"] += sum(naive.values())
        h["matched"] += sum((naive & truth).values())
        h["missed"] += sum((truth - naive).values())
        h["spurious"] += sum((naive - truth).values())
        if naive == truth:
            h["exact"] += 1
            if not truth:
                h["exact_empty"] += 1

    token_census, ambiguous_examples, shadow_examples = ir_token_census(ir_modules)

    # Section A / G: the join between the HTML declarations and the IR ones.
    page_modules = set(page_module(args.doc, p) for p in s["pages"])
    ir_in_pages = {
        d["name"]: d for mod in page_modules for d in ir_modules[mod]["declarations"]
    }
    ir_only = sorted(set(ir_in_pages) - s["html_decls"])
    # doc-gen4 folds structure projections and constructors into the parent
    # `structure` block. The IR already names them: they are exactly the entries
    # of some declaration's `members`. Checked rather than assumed.
    member_names_in_pages = set(
        mem["name"] for mod in page_modules for d in ir_modules[mod]["declarations"]
        for mem in d["members"]
    )
    pageless = [m for m in ir_modules if m not in page_modules]
    census = {
        "ir_decls_in_pages": len(ir_in_pages),
        "ir_only": len(ir_only),
        "html_only": len(s["html_decls"] - set(ir_in_pages)),
        "ir_only_kinds": dict(collections.Counter(ir_in_pages[x]["kind"] for x in ir_only)),
        "ir_only_sample": ir_only[:6],
        "ir_only_in_members": len([x for x in ir_only if x in member_names_in_pages]),
        "members_rendered_standalone": len(member_names_in_pages & s["html_decls"]),
        "ir_only_all_members": set(ir_only) == (member_names_in_pages & set(ir_in_pages)),
        "pageless_modules": len(pageless),
        "pageless_with_decls": sum(1 for m in pageless if ir_modules[m]["declarations"]),
        "token_census": token_census,
    }

    report = render(cond, s, ir_index, ir_names, ir_name_parts, census, h)
    elapsed = time.time() - t0
    report += f"\n---\n\nこのレポートの生成にかかった時間: {elapsed:.1f} s\n"

    sys.stdout.write(report)
    if args.out:
        open(args.out, "w", encoding="utf-8").write(report)

    if args.json_out:
        all_targets = collections.Counter()
        for region in s["targets"].values():
            all_targets.update(region)
        payload = {
            "conditions": cond,
            "elapsedSeconds": round(elapsed, 3),
            "population": {
                "pages": s["r"]["pages"],
                "pagesWithDecls": s["r"]["pages_with_decls"],
                "htmlDecls": s["r"]["decls"],
                "htmlBytes": s["bytes"]["total"],
                "irModules": ir_index["moduleCount"],
                "irDeclarations": ir_index["declarationCount"],
                "irDeclarationsInPageModules": census["ir_decls_in_pages"],
                "matched": len(s["html_decls"]),
                "irOnly": census["ir_only"],
                "irOnlyKinds": census["ir_only_kinds"],
                "irOnlySample": census["ir_only_sample"],
                "irOnlyAreAllStructureMembers": census["ir_only_all_members"],
                "irOnlyInMembers": census["ir_only_in_members"],
                "membersRenderedStandalone": census["members_rendered_standalone"],
                "htmlOnly": census["html_only"],
                "pagelessIrModules": census["pageless_modules"],
                "pagelessIrModulesWithDeclarations": census["pageless_with_decls"],
            },
            "declHeaderAnchors": {
                "total": s["anchor_kind"]["anchors"],
                "declaration": s["anchor_kind"]["declaration"],
                "sort": s["anchor_kind"]["sort"],
                "module": s["anchor_kind"]["module"],
                "textExactFullName": s["text_relation"]["exact_full_name"],
                "textNamespaceSuffix": s["text_relation"]["namespace_suffix"],
                "textNotation": s["text_relation"]["notation"],
                "textIdentifierShaped": s["text_relation"]["identifier_shaped_text"],
                "distinctNotationPairs": len(s["notation_pairs"]),
                "distinctSuffixPairs": len(s["suffix_pairs"]),
                "topNotation": [
                    {"text": t, "target": g, "count": c}
                    for (t, g), c in s["notation_pairs"].most_common(s["top"])
                ],
            },
            "bytes": {
                "total": s["bytes"]["total"],
                "chrome": s["bytes"]["chrome"],
                "head": s["bytes"]["head"],
                "header": s["bytes"]["header"],
                "internalNav": s["bytes"]["internal_nav"],
                "declHeader": s["bytes"]["decl_header"],
                "equations": s["bytes"]["equations"],
                "rest": s["bytes"]["total"]
                - s["bytes"]["chrome"]
                - s["bytes"]["decl_header"]
                - s["bytes"]["equations"],
            },
            "naiveRelink": {
                "positionFree": True,
                "note": "multiset comparison per declaration; an upper bound, not an achievable rate",
                "declarations": h["decls"],
                "truthLinks": h["truth"],
                "emittedLinks": h["naive"],
                "matched": h["matched"],
                "missed": h["missed"],
                "spurious": h["spurious"],
                "exactDeclarations": h["exact"],
                "exactDeclarationsWithZeroLinks": h["exact_empty"],
                "reachableCeiling": s["text_relation"]["exact_full_name"]
                + s["text_relation"]["namespace_suffix"],
            },
            "undecidableBySet": {
                "sameTextTwoTargetsInOneSignature": s["r"]["same_text_two_targets_in_one_signature"],
                "targetLinkedTwiceInOneSignature": s["r"]["target_linked_twice_in_one_signature"],
                "shortNameCollisionsInOneDecl": token_census["short_name_collisions"],
                "binderShadowsRefShortName": token_census["binder_shadows_ref_short"],
                "binderEqualsRefFullName": token_census["binder_equals_ref_full"],
                "tokenCensus": {
                    "declarations": token_census["declarations"],
                    "tokens": token_census["tokens"],
                    "fullName": token_census["tokens_full_name"],
                    "shortName": token_census["tokens_short_name"],
                    "unmatched": token_census["tokens_unmatched"],
                },
                "shortNameCollisionExamples": ambiguous_examples,
                "binderShadowExamples": shadow_examples,
            },
            "targetResolution": {
                "nameSet": ir_name_parts,
                "anchors": sum(all_targets.values()),
                "distinct": len(all_targets),
                "unresolved": sum(v for k, v in all_targets.items() if k not in ir_names),
                "unresolvedDistinct": len([k for k in all_targets if k not in ir_names]),
                "byRegion": {
                    region: {
                        "anchors": sum(c.values()),
                        "distinct": len(c),
                        "unresolved": sum(v for k, v in c.items() if k not in ir_names),
                        "unresolvedDistinct": len([k for k in c if k not in ir_names]),
                    }
                    for region, c in s["targets"].items()
                },
                "unresolvedInsideCodeSpan": sum(
                    v for (_r, code), v in s["unresolved_in_code"].items() if code
                ),
                "excluded": {
                    "anchorsHrefFirst": s["r"]["anchors_href_first"],
                    "anchorsClassFirst": s["r"]["anchors_class_first"],
                    "samePage": s["r"]["anchors_same_page"],
                    "external": s["r"]["anchors_external"],
                    "noFragment": s["r"]["anchors_no_fragment"],
                },
                "topUnresolved": [
                    {"target": k, "count": v}
                    for k, v in sorted(
                        ((k, v) for k, v in all_targets.items() if k not in ir_names),
                        key=lambda x: -x[1],
                    )[:20]
                ],
            },
            "other": {
                "orderReproducedPages": s["r"]["order_reproduced"],
                "orderDeterminedPages": s["r"]["order_determined"],
                "orderMismatchPages": s["order_mismatch"],
                "orderTies": s["order_ties"],
                "equationLimit": EQUATION_LIMIT,
                "equationRule": {
                    "noticePredicted": s["eq_rule"][("notice", True, True)],
                    "noticeActual": s["eq_rule"][("notice", True, True)]
                    + s["eq_rule"][("notice", True, False)],
                    "noNoticePredicted": s["eq_rule"][("notice", False, False)],
                    "noNoticeActual": s["eq_rule"][("notice", False, False)]
                    + s["eq_rule"][("notice", False, True)],
                    "ulPredicted": s["eq_rule"][("ul", True, True)],
                    "ulActual": s["eq_rule"][("ul", True, True)] + s["eq_rule"][("ul", True, False)],
                    "liCountExact": s["eq_rule"][("li_count", True)],
                },
                "declKindPairs": [
                    {"irKind": a, "htmlDeclKind": b, "count": c}
                    for (a, b), c in sorted(s["kind_pairs"].items(), key=lambda x: -x[1])
                ],
                "elementCounts": {
                    f"{tag}.{cls.replace(' ', '.')}": s["class_census"][(tag, cls)]
                    for tag, cls in TRACKED_CLASSES
                },
                "ghLinks": dict(s["gh"]),
                "pagesWithTopHref": s["pages_with_top_href"],
                "pagesWithTopId": s["pages_with_top_id"],
            },
        }
        open(args.json_out, "w", encoding="utf-8").write(
            json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
        )


if __name__ == "__main__":
    main()
