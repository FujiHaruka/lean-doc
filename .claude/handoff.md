# Handoff — 2026-08-09 (4)

## State

- Branch: main / Uncommitted: clean / push 済み
- Active phase: 段階 3 の**増分 3 (URL 生成と正しさの照合)** に入るところ。増分 1・2 は完了。
- 計測環境: 対象リポジトリ `/Users/haruka/dev/lean-projects` の doc-gen4 (v4.31.0) に
  計装パッチが当たったまま (`benchmarks/tools/apply-instrumentation.sh --check`)。
  olean 暖機済み・対象リポジトリ clean。`.lake/build/doc` は**読むだけ**。

## Relay control

- Mode: ON
- Goal: `docs/plans/three-axes.md` を完遂する。初回・CI・増分の 3 軸それぞれに実測を 1 つ入れる。
  段階 3 → IR 永続化 → 段階 4 / cold 節 / 段階 5 の順。**判断基準を満たさなければそこで停止し、
  否定を `docs/verification-log.md` に記録して `approach.md` の見直し案を書いて終了 — それも完遂。**
- Leg: 2 / cap 10
- Predecessor: three-axes-r1
- Stop-on: completion | user-decision | no-progress×2 | leg-cap
- Progress ledger:
  - r1: 段階 3 増分 1・2 完了。`experiments/stage3/` + `compare-links.ts` + `map-size.ts`。
    `7380856` / `b0da5a0` / `da91c89`

## Tasks

- #6 [pending] 写像の実サイズを 3 通り実測 — **実質完了**。増分 2 で出たので閉じてよい

## Where we are

増分 1: 署名+equation+structure 親型から参照定数を回収し、ユニーク 1,446 (自 913 / 依存 533)。
doc-gen4 が HTML に出したリンク 1,154 件を**取りこぼし 0** で包含 (ただし HTML は 348/432 モジュール)。
増分 2: 依存側 533 のうち 530 が `declaration-data.bmp` にある (未ヒットは再帰子 3 件)。
写像は上限 34.3 MB / **下限 53 KB**、ロードは環境ロードの床の 8%、`kind` はリンクに不要。
**判断基準 (b) は達成。(a) は「取りこぼし 0」までで、URL の一致はまだ見ていない。**

## Next step

`docs/plans/stage3.md` の**増分 3**。モジュール名 → URL の規則を実装して、生成した URL が
doc-gen4 の `<a href>` と**同じ場所を指すか**を照合する。今あるのは「名前の集合が一致する」
ところまでで、**href 文字列の一致は未検証**。`benchmarks/tools/compare-links.ts` を
「名前の集合の比較」から「(宣言, 参照名) → href の比較」に広げるのが素直。
IR には絶対的な識別子 (モジュール名 + 宣言名) を持ち、相対化は出力時 (§5.6 の境界)。
その後**増分 4 = 判断**: `verification-log.md` に段階 3 の総括、`approach.md` §5.3 / §6.4(b) を結果に合わせる。

## Files to read first

- `docs/plans/stage3.md` — 増分 3 の中身。**落とし穴が増分 1 の実地知見で更新済み**
- `docs/verification-log.md` の「段階 3 — 増分 1 / 増分 2」 — 数字の SoT
- `benchmarks/tools/compare-links.ts` — 突き合わせツール。増分 3 で拡張する本体
- `experiments/stage3/README.md` — `--refs` / `--dump-refs` の意味と実測値
- `docs/plans/three-axes.md` §6 — **事前決定の表**。蒸し返すと relay が PAUSED で止まる

## Load-bearing context

- **突き合わせ先の HTML は 432 モジュール中 348 しかない** (フルビルド 42% 打ち切りの影響)。
  一致率はこの範囲でしか出せない。**「何モジュール分か」を必ず併記する。**
- **署名ブロックの切り出しは 2 段階の除外が要る**: `div.decl_header` / `ul.equations` /
  `li.structure_field` に限定し、さらに**自己リンク** (`span.decl_name` と
  `div.structure_field_info` 内の先頭 `<a>` = フィールド名) を外す。これで初めて取りこぼし 0 になった。
- **structure のフィールドは親のブロック内にレンダリングされる。** 「宣言 → HTML ブロック」の
  対応は親子関係込みで解く必要がある。
- **doc-gen4 は dead link を出す** (`Eq.rec` など、環境の全定数を見てリンクするため)。
  一致率が 100% にならないのは lean-doc 側の欠陥とは限らない。**不一致は必ず原因ごとに分類する。**
- **`refUs` の計時は遅延評価で 0 になる罠がある** (`experiments/stage3/README.md` に記録)。
  Lean 側で新しい計時を足すときは、値が出現数と一緒に動くかを確認すること。
- 未検証で残っているもの: **上流の公開サイトに `declaration-data.bmp` 相当があるか**
  (ネットワーク不使用のため未確認、`approach.md` §8 に記録済み)。
- 段階 3 の成果物は**バイト数と一致率であって秒ではない**。`11.82 秒` を完成品の値として引用しない。
