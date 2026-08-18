# Handoff — 2026-08-19 (検索の索引形式 v2)

## Relay control
- Mode: PAUSED
- Goal: `docs/plans/search-v2.md` の P0〜P2 を実装して完遂まで自走
- Leg: 1 / cap 8
- Predecessor: none
- Stop-on: user-decision (**撤退ラインに当たった**)
- Progress ledger:
  - r1: P0 `bfaf537` / P1+P2 `b44937c` / 撤退ラインの実測 `b0c5d64` / fmt `af85f84`

## State

- Branch: main / clean / push 済み。**CI は `af85f84` で回している最中** (それ以前の
  2 本は `cargo fmt --check` だけで赤。`af85f84` がその修正)
- **P0 / P1 / P2 は全部入っていて、テストもゲートも緑**
- **ユーザー判断待ちが 1 件**: 下の「Next step」

## Where we are

`docs/plans/search-v2.md` の §15 が結果の SoT。要点だけ:

| | |
|---|---|
| **P0** | `search-index.json` から `modules` 節 (12.8%) を落とし、instances を別ファイルへ。成果物 7 → 8 本 |
| **P1** | `search-index.bin` (形式 v2) + `app.js` のバイト走査。実測 **405,402 → 108,500 B / gzip 47,959 → 39,763 B** |
| **P2** | 打鍵ごとの絞り込みキャッシュ (`NARROW_MAX = 512`) |
| **新ゲート 2 本** | `check-site-closure.py` に Python の独立デコーダと `search-index == name-map`、`check-site-browser.ts` に**凍結した旧採点器との突き合わせ** (11 クエリ、うち 3 つは 1 文字ずつ打つ)。**どちらも一度落としてから通した** |
| **拾った欠陥** | `utf16Length` が astral を 1 と数えていた (JS の `.length` は 2)。**ブラウザゲートが捕まえた** — 設計時の計測ツールも同じ間違いをしていて、公開コーパスに BMP 外の名前が無いので気づけなかった |

## Next step — **ユーザー判断が要る**

**§12 の撤退ラインに当たった。** 「ブラウザ実測でピークが 5 倍以上下がらなければ P1 は
入れない」に対し、実測は **4.2〜4.9 倍** (読み方による。生ログ
`benchmarks/results/search-v2-browser-2026-08-19.txt`)。

- 保持 (GC 後) 769 → 156 KiB = 4.9×
- GC 前 (ピーク近似) 1,030 → 245 KiB = 4.2×
- 保持 + ファイル実体 769 → 262 KiB = 2.9×
- **差はどの取り方でも 613 KiB で一定。倍率だけが分母で動く**

Deno/V8 の 8.2× / 15.7× が再現しなかった理由も実測済み: **`JSHeapUsedSize` は TypedArray の
実体を数えない**ので、新側の 156 KiB にファイル自身 (106 KiB) が入っていない。

選択肢は 2 つ:

- **A: P1 を残す** (ラインを引き直す)。根拠は 613 KiB / 転送 −17.1% / 生 3.7× と、
  **ラインを引いたときの 5 倍が「別のものを測った数字」だったこと**
- **B: P1 を戻す** (`b44937c` と `b0c5d64` の app.js 部分と `search_index.rs` を revert)。
  **P0 と P2 は形式を触らないので残せる** — 計画が最初からそう分けてある

**判断が出るまで手を動かさない。** どちらでも、CI が `af85f84` で緑になっていることを先に確認する。

## Files to read first

1. `docs/plans/search-v2.md` §15 — 実装の結果と撤退ラインの判定
2. `benchmarks/results/search-v2-browser-2026-08-19.txt` — ブラウザ実測の生ログ
3. `crates/litedoc4-global/src/search_index.rs` — 形式の仕様はここのモジュールコメント
4. `crates/litedoc4-render/assets/app.js` — リーダと採点 (`readIndex` / `search` / `searchNarrowed`)

## 手元の状態

- 実対象規模のサイトは `benchmarks/tools/synth-ir.ts` で**いつでも作り直せる** (公開サイトの
  `modules.json` + `search-index.json` を curl → IR 合成 → `litedoc4 global`)。
  scratch に置いた実体は捨ててよい
- ブラウザゲートの後に puppeteer が残ることがある → `pkill -f check-site-browser.ts`
