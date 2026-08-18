# Handoff — 2026-08-19 (検索の索引形式 v2)

## Relay control
- Mode: DONE
- Goal: `docs/plans/search-v2.md` の P0〜P2 を実装して完遂まで自走
- Leg: 1 / cap 8
- Predecessor: none
- Stop-on: completion (撤退ラインに当たり、**ユーザー判断で線を引き直して完了**)
- Progress ledger:
  - r1: P0 `bfaf537` / P1+P2 `b44937c` / 撤退ラインの実測 `b0c5d64` / fmt `af85f84` /
    ラインの引き直し + 完了。**CI は `54ac02d` で緑**

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

## 撤退ラインの決着

**§12 の撤退ラインに当たり、線の方を引き直した**【決定 2026-08-19、ユーザー判断】。
「ピークが 5 倍以上下がらなければ P1 は入れない」に対し実測は **4.2〜4.9 倍**
(生ログ `benchmarks/results/search-v2-browser-2026-08-19.txt`)。

- 保持 (GC 後) 769 → 156 KiB = 4.9× / GC 前 1,030 → 245 KiB = 4.2× /
  保持 + ファイル実体 769 → 262 KiB = 2.9×
- **差はどの取り方でも 613 KiB で一定。倍率だけが分母で動く。**
- 5 という数字は、**新側の作業用構造を数えていない Deno の比較**から引かれていた。

**「5 倍を満たした」と書かない。満たしていない。**

## Next step

**この作業は完了。** 次にやるなら:

1. **エンジン差** — Firefox / Safari で同じ A/B を取る (`benchmarks/tools/measure-index-memory.ts`
   は Chrome 決め打ち)。**§13-1 の残り**
2. **実対象で本物のサイトを建てる** — いまの数字は合成 IR 経由。索引については本物だが、
   ページを含む全体は測っていない
3. **依存込み (258,760 宣言) の成立性** — §10 は設計を書いただけで 1 バイトも測っていない
4. 前の handoff から持ち越し: `failures` を誰も判定に使っていない / 残り 3 件の未検証 /
   可動 `v0` tag / Mathlib 依存パッケージを v4.32 / v4.33 で建てる

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
