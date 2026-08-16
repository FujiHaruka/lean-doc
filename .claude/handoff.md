# Handoff — 2026-08-17 01:55 (G2 は否定で決着 → G3 = 段 C / leg 3 → leg 4)

## State

- Branch: main / **clean** / push 済み (HEAD `4590030`)
- leg 3 で **6 commit**。**G2「宣言単位の再解析キャッシュ」は 2 経路の実測で否定**され、
  代わりに **IR キャッシュの CI 配線が入って実走で緑**になった
- 開始からの経過: **朝 10:00 JST が自走の終了目安**。いま 01:55 JST なので約 8 時間ある
- 計測環境: 対象 `/Users/haruka/dev/lean-projects` は `c4f6af29`、**作業ツリーは元通り**
  (leg 3 で 2 度ソースを触ったが両方 revert + `lake build` 済み。`?? docs/doc-gen-bench/` は元から)。
  `.lake` 健在、`extractor/build/extract` と `target/release/lean-doc` は両方 built

## Relay control

- Mode: ON
- Goal: **G3「段 C — 依存写像 (`link-index.lidx`) を『安定な部分』と『動く部分』に割る」**。
  設計は `docs/plans/reextract-count.md` §6。**C1 と C2 は同時にやるか、どちらもやらないか**
  (C1 単独は取り分 9% で撤退ライン未満)。達成後、**朝 10:00 JST 前ならゴールを自分で
  再設定して自走を続ける**【ユーザー指示】
- Leg: 4 / cap 8
- Predecessor: `decl-cache-r3`   # leg 4 が走り出しを確認してから kill する
- Stop-on: completion | user-decision | no-progress×2 | leg-cap
- Progress ledger:
  - r1: ゴール設定のみ。実作業ゼロ
  - r2: **G1 完遂** — `af7fe18`〜`a39f7e8` (14 commit)。CI 実測 3 run、docs 7 ファイル更新
  - r3: **G2 を否定して G3 を出した** — `d5af924`〜`4590030` (6 commit)。段 A/B/C を実測、
    CI テンプレに IR キャッシュを配線して 2 run 実走、`approach.md` を 586 行に圧縮

## Where we are

**G2 は 2 つの独立な経路で否定された**(→ `docs/verification-log.md` の 2 節、2026-08-17):

1. **母数から** — 1 モジュール変更で再解析される宣言は median 8 本 / **17.8 ms**。
   増分の総時間 11.7〜18.1 s に対して 0.15%。
2. **内訳から** — 1 モジュール変更 7.3118 s のうち **`analyze` は 0.0072 s (0.1%)**。
   支配しているのは**環境ロード 3.329 s** と**依存写像の書き直し 1.461 s**。

**代わりに入ったもの**: `ci-template.yml` / `workflow-templates/lean-doc-docs.yml` に
**4 つ目のキャッシュ `docs-out` (IR + 台帳 + state + site)**。同一 commit で 2 run 実走し、
**cache-hit true / 再抽出 13 → 0 / 再描画 13 → 0 / 抽出器プロセス 1 → 0**。
run 2 でも `lake build` は実走しているので、**Linux ランナー上でも olean は byte 一致した**。

**残った問題 (= G3)**: 依存写像は 1 宣言追加で **267,504 行中 2 行**しか動かないのに、
(a) 10,448,303 B をまるごと書き直し (1.461 s)、(b) その**全体ダイジェスト**が `renderKey` に
入っているので **422/422 ページが再描画**される。全域写像の差分 (L3-2) は正しく 0 ページと
答えている — 過剰描画の原因は L3-2 ではなく `linkIndex` の鍵の粒度。

## Next step

1. **`docs/plans/reextract-count.md` §6 を読む** — 設計 (依存側 98% は `extractKey` 側、
   自パッケージ側は L2/L3 側)、オラクル、撤退ラインが全部そこにある。
2. **オラクルを先に決めて先に書く** — 変更後の site が**フル生成と byte 一致**すること。
   **再描画ページ数だけを見て緑にしない** (過少描画は静かに壊れる)。
   ゲートは決定的な整数で: 再抽出モジュール数 / 再描画ページ数 / IR 全読み回数。
3. **C2 (抽出器側) が本体** — `extractor/Extract.lean` の `writeLinkIndex` (line 401 付近)。
   いま 490,287 定数を走査して 255,810 宣言を 1 ファイルに書いている。
   **自パッケージぶんと依存側を別ファイルにする**のが筋。読む側は
   `crates/lean-doc-render` と `lean-doc-incr/src/ledger.rs` の `link_index_digest`。
4. **C1 (Rust 側)** — `renderKey` から `linkIndex` の全体ダイジェストを外し、
   **動いた名前の集合**から再描画集合を導く。**L3-2 が全域写像に対してやっている形をなぞる**
   (`lean-doc global --delta-json` → pipeline が union)。
5. **撤退ライン**: C1+C2 を入れても 1 モジュール変更が **6.5 s を切らない**なら閉じて否定として記録。

## Files to read first

- `docs/plans/reextract-count.md` — **G3 の SoT**。§6 が設計、§1〜§5 が否定された経緯
- `docs/verification-log.md` 末尾 2 節 (2026-08-17) — **数字の SoT**
- `extractor/Extract.lean` `writeLinkIndex` (401 付近) / `serve` (2927 付近の但し書き)
- `crates/lean-doc-incr/src/ledger.rs` `render_key` / `link_index_digest` (412〜460)
- `crates/lean-doc/src/build.rs` (マーカー / plan の分岐 / `write_marker` は絶対 root を書く)

## Load-bearing context

- **A/B の再現手順** (同一セッション warm、`--jobs 4`、対象 422 モジュール):
  ```
  ./target/release/lean-doc build --root /Users/haruka/dev/lean-projects \
    --out <scratch>/docs-out --lib InformationTheory \
    --extractor-bin extractor/build/extract --jobs 4
  ```
  同じ `--out` を使い回すと increment。フル生成 **9.8848 s** / 無変更 **0.313 s** /
  1 モジュール変更 **7.3118 s**。**`ready` を見て cold/warm を判定する** (warm は約 3.0 s、
  cold は約 16 s)。**cold と warm を混ぜない**
- **抽出器の位相ログは `<out>/work/extract-timings-1-events.jsonl` と `<out>/work/serve.out`**。
  ここに `linkIndex` / `analyze` の内訳が出る。**ビルドごとに上書きされるので先に退避する**
- **外側の計時器と内側の計時器で範囲が違う** — `served N module(s) ... in X` は
  `Resident::extract` の計時で、**遅延起動のサーバ起動 (= 環境ロード) と `fold_timings` を含む**
  (`crates/lean-doc/src/resident.rs:244`)。要求単体はサーバが返す `ok 0 <ns>` のほう。
  **leg 3 でこれを取り違えて誤った結論を出しかけた**【記録済】
- **`diff` はこのシェルで `colordiff` に alias されていて存在しない。`/usr/bin/diff` を使う**
- **olean は再ビルドで byte 一致する** (9 モジュール、3.6 KB〜1.2 MB、陽性対照つき)。
  計測器は `benchmarks/tools/olean-determinism.sh` (負のテスト 3 通りで落として通した)。
  **対象リポジトリのソースを触ったら必ず revert + `lake build` して戻す**
- **CI ワークフローは `workflow_dispatch` のみ**。`gh workflow run ci-template.yml --ref main`
  → 2〜3 分。結果は `gh run download <id> -n template-validation` の `template-run.txt`
- **`benchmarks/tools/measure-ledger.sh` は起動時に tracked な生ログを切り詰める**【実測】。
  動作確認のつもりで実走しない
- **ssh (port 22) はこの機材から通らない。** push は HTTPS + `gh`:
  ```
  GIT_CONFIG_COUNT=2 \
  GIT_CONFIG_KEY_0=credential.helper GIT_CONFIG_VALUE_0='' \
  GIT_CONFIG_KEY_1=credential.helper GIT_CONFIG_VALUE_1='!gh auth git-credential' \
  git push https://github.com/FujiHaruka/lean-doc.git main:main
  ```
- **subagent には「コミットするな」と指示する。同時に走らせるのは 1 体まで**
- **新しいゲート / 計測器は必ず一度落としてから通す**
