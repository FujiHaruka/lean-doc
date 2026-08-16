# Handoff — 2026-08-17 01:30 (G1 完遂 → G2 開始 / leg 2 → leg 3)

## State

- Branch: main / **clean** / push 済み (HEAD `a39f7e8`)
- **G1「CI 軸を実測で閉じる」は完遂**。8 段すべて済み、leg 2 で 14 commit
- 開始からの経過: **朝 10:00 JST が自走の終了目安**。いま 01:30 JST なので約 8.5 時間ある
- 計測環境: 対象 `/Users/haruka/dev/lean-projects` は `c4f6af29`。`.lake` 健在、
  抽出器 `extractor/build/extract` と `target/release/lean-doc` は両方 built。
  **doc-gen4 の計装は今回触っていない**

## Relay control

- Mode: ON
- Goal: **G2「宣言単位の再解析キャッシュ — 意味解析を*速く*するのではなく*回数*を減らす」**。
  達成後、**朝 10:00 JST 前ならゴールを自分で再設定して自走を続ける**【ユーザー指示】
- Leg: 3 / cap 8
- Predecessor: `ci-placement-r2`   # leg 3 が走り出しを確認してから kill する
- Stop-on: completion | user-decision | no-progress×2 | leg-cap
- Progress ledger:
  - r1: ゴール設定のみ。実作業ゼロ
  - r2: **G1 完遂** — `af7fe18`〜`a39f7e8` (14 commit)。CI 実測 3 run、docs 7 ファイル更新

## Where we are

G1 の結論は**予測と食い違った**。`docs/verification-log.md`「CI 軸 (2026-08-16)」が SoT:
**「別ジョブは 8〜34 倍」は再現せず split/same は 1.08〜1.58 倍** (製品で n=5)、
**陽性対照 (`drop_caches`) だけが 5.2〜11.9 倍**。壊れたのは機構ではなく「別ジョブは cold で
始まる」という前提で、原因は**ランナーが 2 コア/7.75 GiB → 4 コア/15.6 GiB に変わっていた**こと。
テンプレも実走して緑になり `THIS FILE HAS NEVER BEEN RUN` を消した。引用元 7 ファイルは同じ
コミット群で直してある。

**G2 を選んだ理由**: 今回の実測で**床が I/O から CPU に移った**。ランナー上の
`lean-doc build` は 11.5〜20.7 s で、**環境ロードは 1.6〜2.7 s しかない**一方
**extract が 10〜18 s**。`approach.md` §6.1 末尾が名指ししている「変更のない宣言を
再解析しない」が、そのまま今の律速に当たる。

## Next step

1. **(先に片付ける) `docs/approach.md` が 648 行で 600 行を超えた** — leg 2 の G1 更新で増えた。
   `/compact-plan docs/approach.md`。**§3 の 2 段階の実測 (2026-08-10 / 2026-08-16) は
   両方残す** — 「仮説とそれを否定した条件」そのものなので畳んではいけない。
2. G2 の調査に入る: `docs/approach.md` §6.1 / §5.5 と `crates/lean-doc-incr/src/ledger.rs` を読み、
   **いま増分が何を単位に再抽出しているか (モジュール単位のはず) と、宣言単位に落とすと
   何が必要か**を洗う。**先に計画ファイルを書く** (グローバル規則: 計画には Approach 節を入れる)。
3. 数字を取る前に**母数を決める**: 1 モジュール変更時に再解析される宣言数 / そのうち
   実際に変わる宣言数。**これを測らずに実装に入らない** (「約 4 回」を誤って書いた前例が
   `eb96608` にある)。

## Files to read first

- `docs/verification-log.md`「CI 軸 (2026-08-16)」(末尾付近) — G1 の結論。**数字の SoT**
- `docs/approach.md` §6.1 末尾 / §5.5 — 「変更のない宣言を再解析しない」の名指しと、増分の 3 層設計
- `crates/lean-doc-incr/src/ledger.rs` — 増分の台帳。再抽出の単位がここで決まる
- `extractor/Extract.lean` の `analyze` 周辺 — 意味解析の本体 (段階 7d で並列化済み)
- `docs/implementation-plan.md` §4 決定 5 — 宣言の所有モジュールと `_private.`

## Load-bearing context

- **測ったのは 1 ワークロード。** G1 の「配置は 1.1 倍」は 422 モジュール / peak RSS 約 4.0 GB /
  15.6 GiB ランナーでの話。**より大きいパッケージや RAM の小さいランナーでは cold 側に戻る**
- **`lean-doc build` は環境ロード時間を `ready <ns>` 行で出す** (常駐抽出器の 1 行目)。
  phase ログの `importModules` は常駐経路では 0 になるので**そちらを読むと嘘をつかまされる**
- **計測ツールは揃っている** — `benchmarks/tools/run-placement-arm.sh` (ARM=same|split|cold、
  DROP / APPEND)、`check-placement.sh` (在庫と報告本数の突き合わせ + site の byte 一致)、
  `summarize-placement.sh`、`record-runner.sh` (CPU モデル / readahead / CPU 較正)
- **`set -e` + パイプ末尾の `grep` は静かに死ぬ。** 検査器と集計器で 2 回踏んだ。
  `ns="$( (grep … || true) | … )"` の形にする
- **新しいゲート / 計測器は必ず一度落としてから通す。** `check-placement.sh` は 7 通りで
  落として直した (うち 1 件は上記の静かな死)
- **「差が無い」を報告するときは陽性対照を置く。** 対照が無ければ、計測器が鈍いのか
  主張が偽なのか分けられない
- **ssh (port 22) はこの機材から通らない。** push は HTTPS + `gh`:
  ```
  GIT_CONFIG_COUNT=2 \
  GIT_CONFIG_KEY_0=credential.helper GIT_CONFIG_VALUE_0='' \
  GIT_CONFIG_KEY_1=credential.helper GIT_CONFIG_VALUE_1='!gh auth git-credential' \
  git push https://github.com/FujiHaruka/lean-doc.git main:main
  ```
- **Actions を回すのは lean-doc 自身のリポジトリだけ。外部への新規リポジトリ作成はしない**
  【ユーザーに宣言済み】。計測ワークフローは `workflow_dispatch` のみ
- **subagent には「コミットするな」と指示する。同時に走らせるのは 1 体まで**
- **`benchmarks/tools/measure-ledger.sh` は起動時に tracked な生ログを切り詰める**【実測】。
  動作確認のつもりで実走しない
