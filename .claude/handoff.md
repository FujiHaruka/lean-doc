# Handoff — 2026-08-22

## State

- Branch: main / clean / `9f36f32` まで push 済み
- Active phase: `docs/plans/feature-sweep.md` の **束 A・B 完了、束 C から**
- CI: `9f36f32` CI 緑 / `e591562` lake package 緑 (版を 0.2.0 に上げた修正が効いた)
- 計測環境: 計装は触っていない。olean は暖まっている。`/private/tmp/lean-doc-relay` は 5.5 MB

## Relay control

- Mode: ON
- Goal: `docs/plans/feature-sweep.md` の 8 項目を束 A → B → C で完遂。各項目は
  「ゲート/テスト・撤退ライン・実測ログ」の 3 点が揃うまで終わりにしない。
  新しいゲートは必ず一度落としてから通す。完了条件は計画 §5 の 6 項目すべて。
- Leg: 1 / cap 8
- Predecessor: none
- Stop-on: completion | user-decision | no-progress×2 | leg-cap
- Progress ledger:
  - r1: 計画 `561db4c` / **束 A** = A-1 `baab197`+`7ef9488`、A-2 `19ffb9f` /
    **B-0** `419061c` / **束 B** = B-1 `8318e6c`、B-2 `4e79978`、B-3 `82c049a` /
    版 0.2.0 `e591562`。**束 C だけが残っている。**

## Where we are

束 A (依存リンク検証 / watch) と束 B (sorry / 属性 / 生成宣言、IR schema 4 → 5) が入り、
**描画は 1 バイトも動いていない** — 凍結フィクスチャは全部通ったまま。
束 C は**描画が動く唯一の束**で、最後に再凍結を 1 回だけ行う。

## Next step

**束 C を C-1 (a 数式 MathML) から始める。** 計画 §4 C-1。
`pulldown-latex` v0.8.0 (依存 `bumpalo` 1 本) をビルド時に走らせて MathML を焼く。
撤退ラインは「対象の docstring で落ちる/崩れる → クライアント側 KaTeX」。
**先に測る**: 対象 422 モジュールで数式を含む docstring の件数と変換成功率。

順序は C-1 → C-2 (d 逆引き) → C-3 (i 設定) → **C-4 再凍結**。
C-4 では `LITEDOC4_BLESS=1` の再生成手段を HEAD に置き、差分を全件レビューし、
PROVENANCE.md を書き換え、**`MIN_SCHEMA_VERSION` を 5 に上げる** (B-1 が保留した)。

## Files to read first

- `docs/plans/feature-sweep.md` — この作業の SoT。§3 Approach / §4 C-1〜C-4 / §6 決定 / §9 版
- `docs/plans/b0-generated-decls.md` — 生成宣言の判定。B-3 が §11 を反証訂正している
- `crates/litedoc4-render/tests/data/PROVENANCE.md` — C-4 が何を守るか。**JSON を手で編集しない**
- `crates/litedoc4-md/src/` — docstring の描画経路。C-1 が触る
- `benchmarks/results/` の `deps-docs` / `watch` / `sorry` / `attrs` / `generated-decls` — 数字の出所

## Load-bearing context

- **計画が 676 行**で 600 の閾値超え。だが `/compact-plan` を**まだかけていない**のは、
  この文書が【実測/外挿/仮定】ラベルと反証条件で埋まっていて、要約が最初に落とすのがそこだから
  (CLAUDE.md「要約と分割は別の手段」)。**畳むなら束 C 完了後に、A・B の実装メモだけを**。
- **IR を変えたら `target/release/litedoc4` を建て直す。** 古いバイナリが新しい IR を読むと
  名前を出して落ちる (`benchmarks/results/attrs-2026-08-21.txt` §4)。1 度これで診断を空振りした。
- **版番号は extractor とバイナリの互換トークン** — IR schema を動かしたら同じコミットで版も動かす。
  `release.yml` がタグとの不一致を拒否するようになった。
- **`tools/assets-gate.sh` は `mise exec --` 越しに呼ぶ** (素だと exit 137、assets の話は何も出ない)。
- **zsh には `PIPESTATUS` が無く、`nomatch` はコマンド列ごと落とす** — 存在確認をグロブでやらない。
- **長命プロセス (`litedoc4 watch`) はセッションが落ちても生き残り**、ゲートを嘘の失敗にする。
  `pgrep -f 'litedoc4 watch'` を先に見る。
- 束 C の各項目は**描画を動かす**ので、`*-expected.json` が落ちる。**C-4 まで落ちたままにしない** —
  各項目のうちに落ちる理由を 1 行で言えるようにしておき、C-4 でまとめて再凍結する。
