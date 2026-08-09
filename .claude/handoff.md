# Handoff — 2026-08-10 (1)

## State

- Branch: main / Uncommitted: clean / push 済み (`4aa119f`)
- Active phase: `docs/plans/three-axes.md` の **leg 5 =【判断点 2】段階 4** に入るところ。
  leg 1〜4 完了 (段階 3 完結 + IR 永続化の実測)。
- 計測環境: 対象リポジトリ `/Users/haruka/dev/lean-projects` の doc-gen4 (v4.31.0) に
  計装パッチが当たったまま (`benchmarks/tools/apply-instrumentation.sh --check`)。
  olean 暖機済み・対象リポジトリ clean。`.lake/build/doc` は**読むだけ**。
  IR の実体は scratchpad の `ir/` (8.34 MiB、コミットしていない。`experiments/stage4/run.sh` で再生成)。

## Relay control

- Mode: ON
- Goal: `docs/plans/three-axes.md` を完遂する。初回・CI・増分の 3 軸それぞれに実測を 1 つ入れる。
  段階 3 → IR 永続化 → 段階 4 / cold 節 / 段階 5 の順。**判断基準を満たさなければそこで停止し、
  否定を `docs/verification-log.md` に記録して `approach.md` の見直し案を書いて終了 — それも完遂。**
- Leg: 3 / cap 10
- Predecessor: three-axes-r2
- Stop-on: completion | user-decision | no-progress×2 | leg-cap
- Progress ledger:
  - r1: 段階 3 増分 1・2 完了。`7380856` / `b0da5a0` / `da91c89`
  - r2: 段階 3 増分 3・4 完了 (段階 3 完結) + leg 4 IR 永続化。
    `6c5a845` / `bcc4851` / `4aa119f`

## Tasks

- leg 5【判断点 2】段階 4 — IR から HTML を作り、**Lean を起動せずに**再生成できるか
- leg 6 CI 節 (cold の打ち手) / leg 7・8 段階 5 (増分) / leg 9 全体整合

## Where we are

段階 3 は完結: 写像 (bmp) だけで doc-gen4 の href と 99.992% 一致、取りこぼし 0、
撤退ライン 4 本中 3 本を否定 (348/432 モジュール範囲)。leg 4 で IR 永続化を実測 —
書き込み `stage4.writeIR` 0.198s / 読み出し (Deno、Lean 非起動) 0.056s、
§6.1 の「書き込み 0.68s = 仮定」は消えて合計 12.73s が全行実測になった。
HTML 生成の 1.28s だけが仮定として残っている。

## Next step

`docs/plans/three-axes.md` の **leg 5**。`experiments/stage4/` の IR を入力に
**Deno/TypeScript で HTML を生成**し (事前決定 #1)、次を出す:

1. **Lean を起動せずに HTML が作れる**こと (段階 4 の判断基準そのもの)
2. HTML 生成時間の**実測** — §6.2 の「N+1 解消後 1.28s」は仮定。warm 6 回・中央値
3. doc-gen4 の HTML と**同じ仕事をしているか**の確認を先に (§7 の必須チェック)。
   完全一致は目標にしない — **どの要素を出したか / 出していないか**を明示して範囲を固定する

## Files to read first

- `docs/plans/three-axes.md` — leg 5 の行、§6 の事前決定 (#1 言語、#4 `--open` OFF)
- `experiments/stage4/README.md` — IR の形 (`index.json` / `modules/*.json` / `deps/*.json`)、再生成コマンド
- `docs/verification-log.md` の「段階 4 準備 — IR 永続化」 — 数字の SoT と計測条件
- `docs/approach.md` §6.2 / §5.6 — 置き換える対象の仮定と、Lean と出力の境界
- `benchmarks/tools/read-ir.ts` — IR の読み出し側。HTML 生成器はここから育てるのが素直

## Load-bearing context

- **タイマーが 0 を返す罠を 3 回踏んでいる** (段階 2 `--tactics-probe`、段階 3 `refUs`、
  leg 4 のハッシュ)。純粋な `let` の消費者がタイマー外にあると計算が沈む。
  **3 回とも都合の良い側に外れた。** 新しい計時を足したら、値が入力量と一緒に動くかを必ず確認する。
- **`stage4.total` の差から書き込みコストは読めない。** `--write-ir` が触れないはずの
  `importModules` に最大 1.0s の揺れが出る (436 ファイルの削除が効く)。
  フェーズ値 (`stage4.writeIR`) を直接読むこと。交互配置で測り直した 5 系列が検証ログにある。
- **IR は参照の「位置」を持っていない** (集合のみ)。doc-gen4 は位置つきタグを保持している。
  **leg 5 が「集合で足りるか」の判定点** — 足りなければ IR を太らせて leg 4 の再計測になる。
  これは退行ではない (段階 3 の設計判断が leg 5 で検証されるということ)。
- **突き合わせ先の HTML は 432 モジュール中 348 しかない** (フルビルド 42% 打ち切り)。
  一致率はこの範囲でしか出せない。**「何モジュール分か」を必ず併記する。**
- **doc-gen4 の HTML は同じ論理要素でも分岐ごとに属性順が違う。** 正規表現でのブロック照合は
  「一致した件数」ではなく**「母集団に対する割合」で検算**する (増分 1 で 153 本中 4 本しか
  拾えていなかった実例あり)。
- **doc-gen4 は dead link を出す** (`Eq.rec` など)。一致率が 100% にならないのは
  lean-doc の欠陥とは限らない。**不一致は必ず原因ごとに分類する。**
- **`_private.` 名は素朴に URL を出すと dead** (実測 8 名)。leg 5 で HTML を出すなら踏む。
- `docs/verification-log.md` が 910 行。**これは計画文書ではなく数字の SoT なので圧縮しない**
  (ラベルを落とすのが最悪の劣化)。600 行ルールは `docs/plans/*.md` に対して適用する。
- 未検証で残っているもの: 上流の公開サイトに `declaration-data.bmp` 相当があるか
  (ネットワーク不使用のため未確認、`approach.md` §8)。
