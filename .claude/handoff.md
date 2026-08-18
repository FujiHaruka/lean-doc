# Handoff — 2026-08-18 (2)

## State

- Branch: main / **clean** / 未検証項目の棚卸しと 5 件の実走が完遂
- Active phase: **なし。ユーザーの指示待ち**
- CI は **7 ワークフロー** — `ci.yml` / `ci-action.yml` / `ci-lake.yml` / `release.yml` /
  `ci-extractor-portability.yml` / **`ci-browser-windows.yml` (新規)** /
  **`ci-leak.yml` (新規)**。後ろ 3 つは `workflow_dispatch` のみ
- 手元: `/private/tmp/lean-doc-relay/unverified` は**削除済み**。elan に
  **v4.32.2 と v4.33.0 を入れた** (既定は v4.31.0 のまま。U5 の再測に使える)

## Relay control

- Mode: DONE
- Goal: 未検証項目のうち推奨 5 件 (U1 依存 root の外部リンク / U2 同名宣言のページ配置 /
  U3 Windows フォント + ダークの色 / U4 LSan / U5 新しい Lean) を完遂まで自走
- Leg: 1 / cap 8
- Predecessor: none
- Stop-on: completion
- Progress ledger:
  - r1: **U1〜U5 すべて実測して決着**。`9aeadd6` 計画 → `b696112` U1 → `6d09d7d` U2 →
    `55e4f03`/`15df4e7` U3/U4/U5 → `29734f9` docs → `4223bd0` 件数の SoT →
    `95a8ba3` 参照の腐り → フレーキー修正。**未検証項目 13 → 3 件、既知の欠陥が 1 件増えた**

## Where we are

**未検証項目は 13 → 3 件**。判定と結果の SoT は **`docs/plans/unverified-sweep.md`**
(冒頭 §0 が結果表)。**一覧は復元しない**【決定、ユーザー判断】 — 番号参照は一覧より先に
腐っていた。旧一覧は `git show e744f79^:README.md` / `git show 117e928:README.md`。

| | 出たもの |
|---|---|
| **U1** | マップの **21 root / 32 URL がすべて実在**。ただし **422 ページが実際に書く外部リンクは 4 root・3 リポジトリだけ** |
| **U2** | 同名宣言は **21 件実在するが出力に出ない**。落としているのは **blacklist** であって、2 本ある所有規則が一致しているからではない |
| **U3** | **Windows に Consolas はあった** — 欠け 0、ただし等幅で描けたのは **105/178**、送り幅の崩れ **73 種で 3 OS 中最悪**。ダークの色は検査 5b で判定するようにした |
| **U4** | md4c は corpus 12 件で **leak 0**。canary が **24 byte leaked** で落ちることを先に確認 |
| **U5** | **Lean v4.33.0 で extractor が建たない**。v4.32.2 は建ち IR も byte 一致 |

**残っている 3 件**: 他人のリポジトリから `uses:` される / ギュメ付きモジュールを持つ
「版固定できる依存」/ より大きいパッケージ・RAM の小さいランナー。
**どれも「作れば測れる」もので、機材が無いのではない。**

## Next step

**ユーザーの指示待ち。** 手を動かすなら:

1. **Lean v4.33.0 対応** — **U5 が見つけた既知の欠陥**。壊れているのは 1 箇所
   (`extractor/Extract.lean` の `getCustomAttrs`、`ReducibilityStatus.instanceReducible`)。
   **`_ => pure ()` で塞がない** — 新しいコンストラクタを黙って捨てる形はこの木の規律に反する。
   `instanceReducible` を何と表示するか (doc-gen4 の 4.33 対応を見る) の判断が要る。
   **古い版でも建つ必要がある**ので、単に足すだけでは v4.31/v4.32 が壊れる
2. **残り 3 件のどれか** — 一番効くのは「他人のリポジトリから使われる」で、
   **別リポジトリの作成が要る【ユーザー判断】**
3. **可動 `v0` tag** — `docs/plans/distribution.md` §152、まだ作っていない
4. **`docs/approach.md` が 613 行** — `/compact-plan` の閾値超え。前 relay から持ち越し

## Files to read first

- **`docs/plans/unverified-sweep.md` — 今回の SoT。§0 が結果表、§1 が 13 件の分岐**
- `benchmarks/results/lean-version-2026-08-18.txt` — **U5。v4.33 の障害が 1 箇所であることの実測**
- `benchmarks/results/browser-windows-2026-08-18.txt` — 3 OS のフォント表とコントラスト比
- `benchmarks/results/duplicate-owners-2026-08-18.txt` — **所有規則が 2 本あることと、
  どちらを消しても出力が動かないこと**
- `benchmarks/results/external-links-2026-08-18.txt` — U1

## Load-bearing context

- **「機材が無い」と書いてある項目を疑う**【実測 2026-08-18】 — 5 件のうち **2 件は
  持っている機材を使っていなかった** (Windows の Consolas、Linux の LSan)。
  Q8 が前日に同じ失敗を記録している。**CLAUDE.md「計測の誠実性」に追記済み**
- **条件を 2 つ変えて 1 つの結論を出しかけたのが、この日 3 回**【実測】 —
  (1) link index の所有判定を測るとき `--link-index-omit` を渡し忘れた、
  (2) IR の版差を測るとき e2e ゲートの探針が入った木と比べた、
  (3) 探針バイナリの抽出で 3 つの複製が 1 つの `micro-dep` を共有していた。
  **「数字が動いた」は「その変更で動いた」ではない**
- **`litedoc4 links` を足した** (読み取り専用サブコマンド)。マップは `build` のログの
  1 行以外どこからも観測できず、**その 1 行では何も叩けなかった**。
  URL は `ExternalLinks::url_for` が返したものをそのまま出す
- **常駐テストが CI で 8 回に 1 回 exit 126 で落ちていた**【実測、run 32133544132】。
  ETXTBSY と読んで `rename` 方式に直したが、**診断は測っていない**。
  緑が続いても証明にはならない (コードのコメントにその旨を書いてある)
- **`--link-index-omit` を渡さないと自パッケージの宣言が link index に入る**【実測】。
  手で `extract` を叩くときの落とし穴
