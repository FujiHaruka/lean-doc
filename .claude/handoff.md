# Handoff — 2026-08-10 (2)

## State

- Branch: main / Uncommitted: clean / push 済み (`88c1e8a`)
- Active phase: `docs/plans/three-axes.md` の **leg 5 (段階 4 = 判断点 2)** の**増分 3**。
  増分 1 (出力範囲の確定 + 「集合では足りない」の判定) と増分 2 (位置つきタグ + 再計測) は完了。
- 計測環境: 対象リポジトリ `/Users/haruka/dev/lean-projects` の doc-gen4 (v4.31.0) に
  計装パッチが当たったまま (`benchmarks/tools/apply-instrumentation.sh --check`)。
  olean 暖機済み・対象リポジトリ clean。`.lake/build/doc` は**読むだけ**。
  IR は scratchpad にあり、`experiments/stage4b/run.sh` で再生成できる:
  - schema 2 (タグ付き、15.13 MiB) `/private/tmp/claude-502/-Users-haruka-dev-lean-doc/2dbcb565-edbc-4bd9-846b-574772a9c30c/scratchpad/ir-tagged`
  - schema 1 (対照、8.34 MiB) 同ディレクトリの `ir-notag`

## Relay control

- Mode: ON
- Goal: `docs/plans/three-axes.md` を完遂する。初回・CI・増分の 3 軸それぞれに実測を 1 つ入れる。
  **判断基準を満たさなければそこで停止し、否定を `docs/verification-log.md` に記録して
  `approach.md` の見直し案を書いて終了 — それも完遂。**
- Leg: 4 / cap 10
- Predecessor: three-axes-r3
- Stop-on: completion | user-decision | no-progress×2 | leg-cap
- Progress ledger:
  - r1: 段階 3 増分 1・2。`7380856` / `b0da5a0` / `da91c89`
  - r2: 段階 3 増分 3・4 (完結) + IR 永続化。`6c5a845` / `bcc4851` / `4aa119f`
  - r3: 段階 4 増分 1 (判定) + 増分 2 (位置つきタグ + 再計測)。`70aeba0` / `f623de3` / `88c1e8a`

## Tasks

- #1 [in_progress] leg 5【判断点 2】段階 4 — 残るは増分 3 (HTML 生成器) と増分 4 (時間の実測)
- #2 [pending] leg 6 CI 節 / #3 [pending] leg 7・8 段階 5 / #4 [pending] leg 9 全体整合

## Where we are

増分 1 で「参照集合では doc-gen4 の署名内リンクを再構成できない」と実測で判定した
(アンカー 72,421 件の 44.0% は印字トークンと定数名に字面の関係が無く、素朴照合の recall は
天井 56% / 実測 51.7%)。増分 2 で `experiments/stage4b/` に位置つきスパンを足し、
baseline identity 4 項目 PASS、抽出 12.53 → **13.71s**、書き込み 0.198 → **0.549s**、
読み出し 0.056 → **0.100s** (環境ロードの床の 4.0%)。倍率は 85× → 78× に下がり全 docs に反映済み。
**HTML 生成器はまだ 1 行も書いていない。§6.2 の「1.28s = 仮定」が唯一残った仮定。**

## Next step

`docs/plans/three-axes.md` の **leg 5.3** — Deno/TypeScript で IR から HTML を生成する
(事前決定 #1)。順序は「先に正しさ、後で時間」(§7 の必須チェック):

1. `ir-tagged` を入力に、`benchmarks/results/stage4-html-inventory.txt` の
   「出す / 出さない」範囲でモジュールページを生成する
2. **受け入れテストを先に作る** — `div.decl_header` 内アンカーの
   **(位置, テキスト, href) 三つ組の列**を doc-gen4 の 348 ページと比較。
   **集合比較にしない** (増分 1 の素朴照合は集合比較だと precision 100% に見えた)
3. 通ってから増分 4 で warm 6 回の時間を測り、§6.2 の 1.28s 仮定を置き換える

## Files to read first

- `docs/plans/three-axes.md` — leg 5.3 / 5.4 の行、§6 の事前決定 (#1 言語、#4 `--open` OFF)
- `experiments/stage4b/README.md` — schema 2 のスパン形式・オフセット単位・既知の欠落
- `benchmarks/results/stage4-html-inventory.txt` — 出す要素 / 出さない要素の一覧と母数
- `docs/verification-log.md` の「段階 4 — 増分 1」「増分 2」 — 数字の SoT
- `benchmarks/tools/read-ir.ts` — schema 2 対応済みの読み出し + 計測ハーネスの型

## Load-bearing context

- **定数スパン 142,181 個のうち 81,055 個 (57%) が別の定数スパンの内側に入れ子。**
  doc-gen4 は内側が `<a>` を出したとき**外側のアンカーを落とす**。
  スパン列を平坦に走査する実装は過半数で不正な入れ子 `<a>` を作る。**pre-order の入れ子を尊重する。**
- **オフセットは UTF-16 コード単位** (TS の `String.slice` に合わせた)。
  対象には `𝓧` `𝓨` `𝕜` があり非 BMP で単位が実際に食い違う。`experiments/stage4b/check-spans.ts` で検算できる。
- **位置の無い定数出現が 940 件 (0.66%)** — 構造体メンバ署名の binder 部分。
  `Member` が `sig.type` しか持たないため。**レコードの形の問題**で、直すなら増分 2 の再計測になる。
- **`splitWhitespaces`**: 定数タグが改行をまたぐと doc-gen4 は `"\n   "` を `"    "` に潰す。
  文字数は同じなのでオフセットは動かないが、**バイト単位の HTML diff では差として出る**。
- 規則で埋まると実測で確認済み (追加フィールド不要): 宣言の並び (`line`,`col` の安定ソートで 348/348)、
  equations の省略 (**1 本ごとの描画テキスト長 200 文字以上**が閾値。本数ではない)、
  `render := false` の 186 件 (**親 structure の `members` の名前と 186/186 一致**)。
- **突き合わせ先の HTML は 432 モジュール中 348 しかない** (フルビルド 42% 打ち切り)。
  IR 側の 84 モジュールには対応ページが無く、うち 77 は宣言を持つ。**母数を必ず併記。**
- **doc-gen4 は dead link を出す** (`#top` が 348/348 ページで死んでいる、`Eq.rec`)。
  不一致は必ず原因ごとに分類する。**真似しない差分**として数える。
- **タイマーが 0 を返す罠を 4 回踏みかけている。** 純粋な `let` の消費者がタイマー外にあると計算が沈み、
  **毎回都合の良い側に外れた。** 新しい計時は「値が入力量と一緒に動くか」を必ず確認する。
- **§6.2 にはもう 1 つ宿題がある** — doc-gen4 側の HTML 生成 44.52s は
  page cache 状態を記録していない計測。lean-doc 側と比べるなら warm で取り直す必要がある。
- `benchmarks/tools/read-ir.ts` の `DEFAULT_IR` が**古いセッションの scratchpad**
  (`3db6b213…`) を指したまま。そこが消えると「壊れたツール」に見える。`--ir` か `$IR_DIR` を渡すこと。
- `docs/verification-log.md` が 1,203 行。**これは計画文書ではなく数字の SoT なので圧縮しない。**
  600 行ルールは `docs/plans/*.md` に対して適用する (approach.md 518 / three-axes.md 144)。
