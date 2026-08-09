#!/usr/bin/env python3
#
# Ground truth for the stage-4 HTML generator's acceptance test.
#
# The generator (next increment) has to re-emit `div.decl_header` from the IR.
# To score it we need doc-gen4's own answer in a form that can be diffed
# *positionally*. The previous increment scored a naive re-linker by comparing
# per-declaration *multisets* of (text, target); that scored precision 100.0%
# while ignoring where on the page each link sits, i.e. it measured an upper
# bound, not a reproduction. This tool removes that escape hatch: for every
# declaration it emits the ordered sequence of anchors, each carrying its start
# offset in the header's plaintext.
#
# usage:
#   decl-header-truth.py --doc-root <dir> --out <path.jsonl> [--summary <path.txt>]
#
#   --doc-root  doc-gen4's HTML for the package, default $TARGET_REPO (else
#               /Users/haruka/dev/lean-projects) + .lake/build/doc/InformationTheory
#   --out       JSONL, one declaration per line (required)
#   --summary   also write the conditions block + the cross-check table here
#
# EXTRACTION (the definition the generator will be held to)
#   1. A declaration is a `<div class="decl" id="...">` (also `class="decl
#      sorried"`); the `id` is its fully qualified name.
#   2. Its header is the `<div class="decl_header">` inside it. `div.gh_link`
#      is a *sibling* that precedes the header inside `div.<kind>`, so the
#      `source` link is not part of this and is not counted here.
#   3. The header's plaintext is the concatenation of its descendant text nodes
#      in document order, with HTML character references decoded. Tags
#      contribute nothing. Whitespace and newlines are kept exactly as written.
#   4. Each `<a>` inside the header, in document order, yields
#        start  offset of its first character in the plaintext, in UTF-16 code
#               units (the unit the IR's spans use). The corpus contains
#               non-BMP characters (𝓧 𝓨 𝕜 …), so this differs from Python's
#               codepoint indexing and the difference is measured below.
#        text   the anchor's own plaintext
#        href   the href attribute, decoded, left relative
#        cls    the class attribute, or null
#   5. Per declaration we also keep `kind_text` (`span.decl_kind`),
#      `header_text`, and `impl_arg_count` (`span.impl_arg`).
#
# WHAT IS NOT MEASURED
#   * Nothing is timed. This is a content extraction.
#   * Anything outside `div.decl_header`: docstrings, `ul.equations`,
#     `nav.internal_nav`, `div.gh_link`, instances-for lists.
#   * Pages outside the given --doc-root.
#
# The doc tree is opened read-only; the measurement target is never written to.

import argparse
import collections
import html
import html.parser
import json
import os
import platform
import re
import subprocess
import sys
import time

DEFAULT_TARGET_REPO = "/Users/haruka/dev/lean-projects"

# Patterns for the independent cross-parse (see cross_parse below).
_X_DECL = re.compile(r'<div class="decl(?: sorried)?" id="([^"]*)">')
_X_ANCHOR = re.compile(r"<a\b[^>]*>(.*?)</a>", re.S)
_X_HREF = re.compile(r'href="([^"]*)"')
_X_TAG = re.compile(r"<[^>]+>")

# HTML void elements: they never close, so they must not enter the tag stack.
VOID = frozenset(
    "area base br col embed hr img input link meta param source track wbr".split()
)

# Expected values, quoted from benchmarks/results/stage4-html-inventory.txt
# (sections A and B) as produced by benchmarks/tools/html-inventory.py. The
# cross-check is the whole point of this tool: if a row fails, this tool is
# wrong until proven otherwise.
EXPECTED = [
    ("pages", "HTML pages", 348),
    ("pages_with_decls", "pages with >=1 div.decl", 329),
    ("decls", "div.decl", 3477),
    ("anchors_excl_break_within", "decl_header anchors, excluding a.break_within", 78065),
    ("anchors_declaration", "... links to a declaration (href has '#')", 72421),
    ("anchors_sort", "... sort links (foundational_types.html)", 5638),
    ("anchors_module", "... module links (no fragment)", 6),
]


def u16len(s):
    """Length of `s` in UTF-16 code units (not codepoints)."""
    return len(s.encode("utf-16-le")) // 2


class DeclHeaderParser(html.parser.HTMLParser):
    """Streaming extractor for `div.decl_header` contents.

    convert_charrefs=True makes html.parser decode character references in text
    for us; attribute values are unescaped by html.parser unconditionally.
    """

    def __init__(self, module, page):
        super().__init__(convert_charrefs=True)
        self.module = module
        self.page = page
        self.decls = []
        self.stats = collections.Counter()
        self.anchor_classes = collections.Counter()

        self._stack = []           # open element tags, void elements excluded
        self._decl_depth = None    # len(stack) at which the current div.decl opened
        self._decl = None
        self._header_depth = None
        self._kind_depth = None
        self._buf = None           # header plaintext chunks
        self._u16 = 0              # running plaintext length in UTF-16 units
        self._kind = None
        self._open_anchors = []    # (record, [text chunks]) for anchors still open

    # -- helpers ----------------------------------------------------------
    def _in_header(self):
        return self._header_depth is not None

    def handle_starttag(self, tag, attrs):
        a = dict(attrs)
        cls = a.get("class")
        if tag not in VOID:
            self._stack.append(tag)
            depth = len(self._stack)
        else:
            depth = None

        if tag == "div" and cls in ("decl", "decl sorried"):
            if self._decl is not None:
                self.stats["nested_decl"] += 1
            self._decl_depth = depth
            self._decl = {
                "module": self.module,
                "name": a.get("id", ""),
                "page": self.page,
                "kind_text": "",
                "header_text": "",
                "impl_arg_count": 0,
                "anchors": [],
            }
            return

        if self._decl is None:
            return

        if tag == "div" and cls == "decl_header":
            self._header_depth = depth
            self._buf = []
            self._u16 = 0
            self._open_anchors = []
            return

        if not self._in_header():
            return

        if tag == "span" and cls == "decl_kind":
            self._kind_depth = depth
            self._kind = []
        elif tag == "span" and cls == "impl_arg":
            self._decl["impl_arg_count"] += 1
        elif tag == "a":
            if self._open_anchors:
                self.stats["nested_anchor"] += 1
            self.anchor_classes[cls] += 1
            if "href" not in a:
                self.stats["anchor_without_href"] += 1
            # html-inventory.py selects anchors by `href` being the *first*
            # attribute. Split that count by our own rule so the two exclusion
            # rules can be shown to pick the same set instead of assumed to.
            if not attrs or attrs[0][0] != "href":
                if cls is not None and "break_within" in cls.split():
                    self.stats["href_not_first_and_break_within"] += 1
                else:
                    self.stats["href_not_first_but_not_break_within"] += 1
            rec = {
                "start": self._u16,
                "text": "",
                "href": a.get("href"),
                "cls": cls,
            }
            self._decl["anchors"].append(rec)
            self._open_anchors.append((rec, []))
            if tag in VOID:  # defensive; <a> is never void
                self._close_anchor()

    def handle_startendtag(self, tag, attrs):
        # `<x ... />` opens and closes at once: run the start handler, then
        # immediately undo the stack push it may have made.
        self.handle_starttag(tag, attrs)
        if tag not in VOID:
            self.handle_endtag(tag)

    def _close_anchor(self):
        rec, chunks = self._open_anchors.pop()
        rec["text"] = "".join(chunks)

    def handle_endtag(self, tag):
        if tag in VOID:
            return
        depth = len(self._stack)
        if self._stack and self._stack[-1] == tag:
            self._stack.pop()
        else:  # unbalanced markup; resync as best we can
            self.stats["unbalanced_end_tag"] += 1
            if tag in self._stack:
                while self._stack and self._stack.pop() != tag:
                    pass
            depth = len(self._stack) + 1

        if self._decl is None:
            return
        if self._in_header():
            if tag == "a" and self._open_anchors:
                self._close_anchor()
            if self._kind_depth is not None and depth == self._kind_depth:
                self._decl["kind_text"] = "".join(self._kind)
                self._kind_depth = None
                self._kind = None
            if depth == self._header_depth:
                self._decl["header_text"] = "".join(self._buf)
                self._header_depth = None
                self._buf = None
                if self._open_anchors:
                    self.stats["anchor_unclosed_at_header_end"] += len(self._open_anchors)
                    while self._open_anchors:
                        self._close_anchor()
        if self._decl_depth is not None and depth == self._decl_depth:
            self.decls.append(self._decl)
            self._decl = None
            self._decl_depth = None

    def handle_data(self, data):
        if not self._in_header() or not data:
            return
        self._buf.append(data)
        self._u16 += u16len(data)
        for _rec, chunks in self._open_anchors:
            chunks.append(data)
        if self._kind is not None:
            self._kind.append(data)


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

    return (
        run("rev-parse", "HEAD"),
        run("describe", "--tags", "--always"),
        bool(run("status", "--porcelain")),
    )


def utc(ts):
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(ts))


def conditions(doc_root, pages):
    repo = os.path.abspath(os.path.join(doc_root, "..", "..", "..", ".."))
    rev, tag, dirty = git_describe(os.path.join(repo, ".lake/packages/doc-gen4"))
    try:
        toolchain = open(
            os.path.join(repo, "lean-toolchain"), encoding="utf-8"
        ).read().strip()
    except OSError:
        toolchain = "?"
    mtimes = [os.path.getmtime(p) for p in pages] or [0]
    return [
        ("run at (UTC)", utc(time.time())),
        ("HTML source", doc_root),
        ("HTML generated (oldest page)", utc(min(mtimes))),
        ("HTML generated (newest page)", utc(max(mtimes))),
        ("target repo", repo),
        ("doc-gen4 rev", rev or "?"),
        ("doc-gen4 describe", tag or "?"),
        ("doc-gen4 working tree dirty (instrumentation patch)", str(dirty)),
        ("lean-toolchain (target)", toolchain),
        ("Python", sys.version.split()[0]),
        ("platform", platform.platform()),
    ]


# --------------------------------------------------------------------------


def cross_parse(text):
    """Second, independent extraction path, used only to check the first.

    Deliberately built the way `html-inventory.py` builds things -- tag-depth
    walking plus regex strip-tags -- so that a shared bug in html.parser's
    handling of this markup cannot make both agree. Returns
    {name: (plaintext, [(href, text), ...])}. Offsets are not produced here;
    those are checked by the slice self-test instead.
    """
    out = {}
    for m in _X_DECL.finditer(text):
        end = _x_block_end(text, m.start(), "div")
        seg = text[m.start() : end]
        hi = seg.index('<div class="decl_header">')
        hj = _x_block_end(seg, hi, "div")
        header = seg[hi:hj]
        inner = header[header.index(">") + 1 : header.rindex("</div>")]
        anchors = []
        for a in _X_ANCHOR.finditer(inner):
            href = _X_HREF.search(a.group(0))
            anchors.append(
                (
                    html.unescape(href.group(1)) if href else None,
                    html.unescape(_X_TAG.sub("", a.group(1))),
                )
            )
        out[m.group(1)] = (html.unescape(_X_TAG.sub("", inner)), anchors)
    return out


def _x_block_end(text, start, tag):
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


def page_module(doc_root, path):
    """`<root>/Fano/Core.html` -> `InformationTheory.Fano.Core` for root=…/InformationTheory."""
    rel = os.path.relpath(path, doc_root)
    stem = rel[: -len(".html")]
    return os.path.basename(os.path.normpath(doc_root)) + "." + stem.replace(os.sep, ".")


def main():
    p = argparse.ArgumentParser(
        prog="decl-header-truth.py",
        description="Extract doc-gen4's div.decl_header anchors as positioned "
        "sequences (ground truth for the HTML generator's acceptance test).",
    )
    target = os.environ.get("TARGET_REPO", DEFAULT_TARGET_REPO)
    p.add_argument(
        "--doc-root",
        default=os.path.join(target, ".lake/build/doc/InformationTheory"),
    )
    p.add_argument("--out", required=True)
    p.add_argument("--summary", default="")
    p.add_argument(
        "--no-cross-parse",
        action="store_true",
        help="skip the independent second extraction path (it costs ~2x runtime)",
    )
    args = p.parse_args()

    t0 = time.time()
    doc_root = os.path.abspath(args.doc_root)
    pages = sorted(
        os.path.join(dp, fn)
        for dp, _dirs, files in os.walk(doc_root)
        for fn in files
        if fn.endswith(".html")
    )

    c = collections.Counter()
    anchor_classes = collections.Counter()
    parser_stats = collections.Counter()
    break_within_per_decl = collections.Counter()
    plaintext_chars = 0
    plaintext_u16 = 0
    non_monotone_decls = []
    slice_mismatch = []
    empty_headers = []
    hrefless = []
    non_bmp_chars = collections.Counter()
    cross_bad = []

    c["pages"] = len(pages)
    with open(args.out, "w", encoding="utf-8") as out:
        for path in pages:
            text = open(path, encoding="utf-8").read()
            module = page_module(doc_root, path)
            rel = os.path.relpath(path, doc_root)
            parser = DeclHeaderParser(module, rel)
            parser.feed(text)
            parser.close()
            parser_stats.update(parser.stats)
            anchor_classes.update(parser.anchor_classes)
            if parser.decls:
                c["pages_with_decls"] += 1

            if not args.no_cross_parse:
                other = cross_parse(text)
                if set(other) != set(d["name"] for d in parser.decls):
                    c["cross_parse_name_mismatch"] += 1
                for d in parser.decls:
                    ref = other.get(d["name"])
                    if ref is None:
                        continue
                    c["cross_parse_checked"] += 1
                    if ref[0] != d["header_text"]:
                        c["cross_parse_text_mismatch"] += 1
                        cross_bad.append(d["name"])
                    if ref[1] != [(a["href"], a["text"]) for a in d["anchors"]]:
                        c["cross_parse_anchor_mismatch"] += 1
                        cross_bad.append(d["name"])
            for d in parser.decls:
                c["decls"] += 1
                header = d["header_text"]
                plaintext_chars += len(header)
                plaintext_u16 += u16len(header)
                if header == "":
                    empty_headers.append(d["name"])
                enc = header.encode("utf-16-le")
                skew = len(enc) // 2 - len(header)
                if skew:
                    c["decls_with_non_bmp"] += 1
                    c["max_offset_skew"] = max(c["max_offset_skew"], skew)
                    non_bmp_chars.update(ch for ch in header if ord(ch) > 0xFFFF)

                prev = -1
                n_break = 0
                for anc in d["anchors"]:
                    cls = anc["cls"]
                    is_break = cls is not None and "break_within" in cls.split()
                    if is_break:
                        n_break += 1
                    else:
                        c["anchors_excl_break_within"] += 1
                        href = anc["href"] or ""
                        if "#" in href:
                            c["anchors_declaration"] += 1
                        elif href.endswith("foundational_types.html"):
                            c["anchors_sort"] += 1
                        else:
                            c["anchors_module"] += 1
                    if anc["href"] is None:
                        hrefless.append((d["name"], anc["text"]))
                    if anc["start"] < prev:
                        non_monotone_decls.append(d["name"])
                    prev = anc["start"]
                    # self-consistency: the anchor text must be exactly the
                    # plaintext slice its offset points at.
                    s = anc["start"] * 2
                    e = s + u16len(anc["text"]) * 2
                    if enc[s:e].decode("utf-16-le") != anc["text"]:
                        slice_mismatch.append((d["name"], anc["start"]))
                c["anchors_total"] += len(d["anchors"])
                c["impl_arg"] += d["impl_arg_count"]
                c["break_within"] += n_break
                break_within_per_decl[n_break] += 1
                out.write(json.dumps(d, ensure_ascii=False, sort_keys=True) + "\n")

    elapsed = time.time() - t0
    out_bytes = os.path.getsize(args.out)
    non_bmp = plaintext_u16 - plaintext_chars  # one extra unit per surrogate pair

    lines = []
    say = lines.append
    n = lambda x: f"{x:,}"

    say("decl-header-truth — doc-gen4 の decl_header アンカー列 (正解データ)")
    say("=" * 72)
    say("")
    say("benchmarks/tools/decl-header-truth.py の出力。すべて実測")
    say("(このツールを走らせれば再現できる。数字の出所はこのファイル自身)。")
    say("")
    say("[measurement conditions]")
    for k, v in conditions(doc_root, pages):
        say(f"  {k:<52} {v}")
    say(f"  {'wall clock for this extraction':<52} {elapsed:.1f} s")
    say(f"  {'JSONL output':<52} {args.out}")
    say(f"  {'JSONL bytes':<52} {n(out_bytes)}")
    say("")

    say("[cross-check vs benchmarks/results/stage4-html-inventory.txt §A/§B]")
    say("  期待値は既存の実測レポート (html-inventory.py の出力) から引いたもの。")
    say("  食い違ったらこのツールが疑わしい — 数字を合わせに行かないこと。")
    say("")
    say(f"  {'item':<48}{'expected':>10}{'actual':>10}  result")
    say("  " + "-" * 76)
    all_pass = True
    for key, label, exp in EXPECTED:
        act = c[key]
        ok = act == exp
        all_pass = all_pass and ok
        say(f"  {label:<48}{exp:>10,}{act:>10,}  {'PASS' if ok else 'FAIL'}")
    say("  " + "-" * 76)
    say(f"  overall: {'ALL PASS' if all_pass else 'MISMATCH — fix the tool'}")
    say("")
    say("  数え方: decl_header 内のアンカーのうち class に break_within を含むもの")
    say("  (= 宣言自身の名前リンク) を除いた列が上の 78,065。残りを href で分類し、")
    say("  '#' を含むものを宣言リンク、foundational_types.html で終わるものを sort")
    say("  リンク、それ以外をモジュールリンクとした (html-inventory.py と同じ規則)。")
    say("")

    say("[newly measured here — 既存値の無い新規の実測]")
    say("")
    say(f"  a.break_within (decl_header 内)                  {n(c['break_within'])}")
    say(f"  decl_header 内アンカー総数 (break_within 込み)     {n(c['anchors_total'])}")
    say("  宣言あたりの break_within の個数:")
    for k in sorted(break_within_per_decl):
        say(f"    {k} 個 : {n(break_within_per_decl[k])} 宣言"
            + ("   <- 期待どおり 1 宣言 1 個" if k == 1 else "   <- 想定外"))
    say("")
    say(f"  入れ子アンカー (<a> の中の <a>)                    {n(parser_stats['nested_anchor'])}"
        "   (期待値 0)")
    say(f"  start が単調非減少でない宣言                       {n(len(non_monotone_decls))}"
        "   (期待値 0)")
    say("    ※ このツールはストリーミング解析なので start は構造上非減少になる。")
    say("       独立した証拠ではなく回帰ガードとして記録する。")
    say(f"  anchor.text != header_text[start:start+len] の件数 {n(len(slice_mismatch))}"
        "   (期待値 0)")
    say("    ※ こちらは独立な検算 — オフセットの単位を間違えると落ちる。")
    say("")
    if args.no_cross_parse:
        say("  独立第二経路との突合: --no-cross-parse で省略")
    else:
        say("  独立第二経路 (タグ深さ走査 + 正規表現 strip-tags、html-inventory.py と")
        say("  同じ作り) との突合 — html.parser 側の解釈ミスを共倒れさせないための検算:")
        say(f"    突合した宣言                                    {n(c['cross_parse_checked'])}")
        say(f"    平文が食い違った宣言                            {n(c['cross_parse_text_mismatch'])}"
            "   (期待値 0)")
        say(f"    (href, text) 列が食い違った宣言                  {n(c['cross_parse_anchor_mismatch'])}"
            "   (期待値 0)")
        say(f"    ページ内の宣言名集合が食い違ったページ            {n(c['cross_parse_name_mismatch'])}"
            "   (期待値 0)")
        if cross_bad:
            say("    例: " + ", ".join(cross_bad[:5]))
    say("")
    say(f"  decl_header 平文の合計長 (UTF-16 code units)      {n(plaintext_u16)}")
    say(f"  decl_header 平文の合計長 (Unicode codepoints)     {n(plaintext_chars)}")
    say(f"  差 = 非 BMP 文字 (サロゲートペア) の個数           {n(non_bmp)}")
    say(f"  非 BMP 文字を含む decl_header                     {n(c['decls_with_non_bmp'])}")
    if non_bmp_chars:
        say("    内訳: " + ", ".join(
            f"{ch} (U+{ord(ch):04X}) x{cnt}"
            for ch, cnt in sorted(non_bmp_chars.items(), key=lambda x: -x[1])))
        say(f"    → 数は少ないが、1 宣言内で codepoint 添字と UTF-16 添字は最大")
        say(f"       {c['max_offset_skew']} 単位ずれる (末尾のアンカーほど大きくずれる)。単位を")
        say("       取り違えると『ほぼ合っている』出力になり、集計では気づけない。")
    say(f"  平文が空の decl_header                            {n(len(empty_headers))}")
    if empty_headers[:5]:
        say("    例: " + ", ".join(empty_headers[:5]))
    say(f"  span.impl_arg の合計 (decl_header 内)              {n(c['impl_arg'])}")
    say("")

    say("[decl_header 内アンカーの class 内訳]")
    for cls, cnt in sorted(anchor_classes.items(), key=lambda x: -x[1]):
        say(f"  {str(cls):<24} {n(cnt)}")
    say("")

    say("[parser sanity counters — すべて 0 が期待値]")
    for k in (
        "nested_decl",
        "nested_anchor",
        "unbalanced_end_tag",
        "anchor_unclosed_at_header_end",
        "anchor_without_href",
        "href_not_first_but_not_break_within",
    ):
        say(f"  {k:<38} {n(parser_stats[k])}")
    say("")
    say("[除外規則が既存ツールと同じ集合を選んでいることの確認]")
    say("  html-inventory.py は「href が最初の属性であるアンカー」だけを数え、")
    say("  このツールは「class に break_within を含まないアンカー」を数える。")
    say(f"  href が先頭でない かつ break_within         "
        f"{n(parser_stats['href_not_first_and_break_within'])}")
    say(f"  href が先頭でない が break_within でない     "
        f"{n(parser_stats['href_not_first_but_not_break_within'])}   (0 なら両規則は同一集合)")
    if hrefless:
        say("  href の無いアンカー例: " + ", ".join(f"{a}/{b!r}" for a, b in hrefless[:5]))
    say("")
    say("[JSONL schema] 1 行 1 宣言、キーはソート済み")
    say("  module, name, page, kind_text, header_text, impl_arg_count,")
    say("  anchors: [{start (UTF-16 code units), text, href, cls}, ...] 文書順")
    say("")

    invariants_ok = (
        parser_stats["nested_anchor"] == 0
        and parser_stats["nested_decl"] == 0
        and parser_stats["unbalanced_end_tag"] == 0
        and parser_stats["anchor_unclosed_at_header_end"] == 0
        and parser_stats["href_not_first_but_not_break_within"] == 0
        and not non_monotone_decls
        and not slice_mismatch
        and c["cross_parse_text_mismatch"] == 0
        and c["cross_parse_anchor_mismatch"] == 0
        and c["cross_parse_name_mismatch"] == 0
    )
    say("[exit status]")
    say(f"  cross-check vs stage4-html-inventory.txt : {'PASS' if all_pass else 'FAIL'}")
    say(f"  invariants (期待値 0 の項目すべて)        : {'PASS' if invariants_ok else 'FAIL'}")
    say("")

    report = "\n".join(lines) + "\n"
    sys.stdout.write(report)
    if args.summary:
        open(args.summary, "w", encoding="utf-8").write(report)
    return 0 if (all_pass and invariants_ok) else 1


if __name__ == "__main__":
    sys.exit(main())
