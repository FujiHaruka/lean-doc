# Handoff — 2026-08-17 08:15 (G3 = 段 C 達成。段 D / E / F と CI の穴埋めまで完了 / leg 4)

## State

- Branch: main / **clean** / push 済み (HEAD `117e928`)。
  **tag `v0.1.0` を打って push 済み** (2026-08-17)
- leg 4 で **29 commit**。`cargo test --workspace` **346 passed / 0 failed / 21 ignored**、
  clippy 無警告、`tools/e2e-micro.sh` 緑、`tools/target2-gate.sh all` = **all checks passed**、
  CI (run `31978156708`) 緑
- 計測環境: 対象 `/Users/haruka/dev/lean-projects` は `c4f6af29`、**作業ツリーは元通り**
  (`?? docs/doc-gen-bench/` は元から)。`lake build InformationTheory` 済み。
  `extractor/build/extract` と `target/release/lean-doc` は両方 built
- **ディスクは 11 Gi 空き**。leg 4 で一度満杯にして事故を起こした (下の「踏んだ地雷」)

## Relay control

- Mode: DONE
- Goal: **G3「段 C — 依存写像を『安定な部分』と『動く部分』に割る」** → **達成**。
  ただし**割る必要は無かった** — 動く部分は誰も読んでいなかったので**書かないだけで済んだ**
- Leg: 4 / cap 8
- Predecessor: none (leg 3 は起動確認後に kill 済み)
- Stop-on: completion
- Progress ledger:
  - r1: ゴール設定のみ。実作業ゼロ
  - r2: **G1 完遂** — `af7fe18`〜`a39f7e8` (14 commit)
  - r3: **G2 を否定して G3 を出した** — `d5af924`〜`4590030` (6 commit)
  - r4: **G3 達成 + 段 D/E/F + CI の穴埋め** — `594ae01`〜`9a05d05` (29 commit)

## この leg で入ったもの

**1 モジュール変更 (対象 422 モジュール、warm) の姿が変わった:**

| | leg 開始時 | 現在 |
|---|---:|---:|
| 再描画ページ | **422 / 422** | **1 / 422** |
| 抽出器が走査した定数 | 490,287 | **0** (地図を再利用) |
| 抽出要求の合計 | 2.09 s | **0.022 s** |
| `detect` の olean ハッシュ | 0.463 s | **0.042 s** |
| `lake env` の呼び出し | 2 回 (約 1.90 s) | **1 回** (約 1.14 s) |
| 壁時計 | (内側 6.24 s) | **3.96〜6.22 s、中央値 4.35 s** |

4 段の中身 (SoT は `docs/plans/reextract-count.md` §6、数字は `docs/verification-log.md`):

- **段 C** `54239be` — `.lidx` から**自パッケージの宣言群を書かない** (`--link-index-omit`)。
  `@` 節は全部残す。**落としてもサイトは 429/429 バイト一致**という実測が根拠
- **段 D** `c8c0fae` — 地図が既に正しければ**書き直さない** (`--link-index-key` + `.key` sidecar)。
  判定は トークン / `@` 節 / `#lidx2` マーカーの 3 つ
- **段 E** `04fb36b` — 核の githash を `lake env` 抜きで聞く (`lean --githash`)。0.84 → 0.03 s
- **段 F** `1ab83b3` — 台帳の olean ハッシュを並列に。0.500 → 0.029 s、**台帳のバイトは不動**

**ゲート**: `tools/onemod-gate.sh` (e2e と CI が同じ 1 本を呼ぶ)。CI にも
「1 モジュール編集してもう一度回す」段を足した (**雛形は変えていない**)。

## 次にやるなら — **性能はもう床に当たっている**

残り 4.35 s の内訳【実測 → `benchmarks/results/g3-attribution-2026-08-17.txt`】:

```
Lean の環境ロード (ready)                  約 2.4 s   approach.md §3 が「床」と結論済み
Server::start の lake env + exec + 171 MB  約 1.14 s  LEAN_PATH を本当に要る。
                                                      自前で組むのは Lake のレイアウト知識の
                                                      再実装で、Lake が変えた瞬間に静かに壊れる
                                                      → **やらない**と結論済み
ownership                                  0.309 s    L3-1 の正しさの規則。逆索引を持てば
                                                      減るが過少報告の危険。ノイズ以下
残り                                       約 0.5 s
```

**この 2 つで残りの 8 割。どちらも「触らない」理由が書いてある。**
次の性能改善は構造変更 (常駐をプロセス跨ぎにする等) が要り、別の計画になる。

候補は 3 つ、**どれも自明ではないのでユーザー判断を仰ぐ側**:

1. ~~v0.1 を締めるか~~ → **締めた** (2026-08-17、tag **`v0.1.0`**、commit `117e928`)
   【決定、ユーザー判断】。根拠は**ゲート A / B の決着だけ**で、未検証項目は片付いていない
   (18 件のまま)。**GitHub Release は作らない**と決めた — バイナリを配布できない
   (抽出器は対象の toolchain に対して作るので使い回せない) ので実質ノートだけになり、
   未検証 18 件を抱えた状態で「ダウンロードして使える製品」と読まれるため。
   **判定の SoT は `docs/implementation-plan.md` §1 末尾**、この日の実測は
   `docs/milestone-log.md` の末尾節
2. **残る未検証項目** (README §未検証項目) — `push:` トリガと利用者リポジトリの checkout。
   検証用ワークフローが `workflow_dispatch` のみなのは**意図的** (push 毎に数 GB 落ちる)
   なので、**このリポジトリでは安く試せない**
3. **実在の公開パッケージでの実走** — v0.1 の範囲外と決めてある【決定 2026-08-15】

## Files to read first

- `docs/plans/reextract-count.md` §6 — 段 C〜F の設計と結果。**この線の SoT**
- `docs/verification-log.md` の 2026-08-17 の節 — **数字の SoT**
- `benchmarks/results/g3-*-2026-08-17.txt` (6 本) — 生ログ
- `tools/onemod-gate.sh` — 1 モジュール編集の判定器。e2e と CI が共有

## 踏んだ地雷 (次の自分へ)

- **ディスクを満杯にした。** `/private/tmp/lean-doc-relay` に 5 世代 24 GB 溜まっていた。
  満杯になると**中断された `lake build` が対象の olean を欠落させ**、さらに
  **シェルコマンド自体が動かなくなる** (ハーネスが出力ファイルを作れない) ので復旧手段も失う。
  → 規則を CLAUDE.md「ベンチマーク」に追加した。**計測が終わったら消す**
- **`git checkout <file>` で subagent の実装を吹き飛ばした。** 一時的に機能を無効化して
  ゲートを落とす実験をしたあと `git checkout` で戻したら、**HEAD にまだ無い変更ごと消えた**。
  無効化はスクラッチのコピーでやるか、`git stash` を使う
- **Python 3.9 の f-string に backslash / ネスト同種クォートが書けない**。集計スクリプトで 2 回踏んだ
- **「あるものを使う」は「今のソースのものを使う」ではない。** 今日 3 件出た
  (e2e の抽出器再利用 / `ci-build.sh` の抽出器と lean-doc)。**3 件とも出力は正常に見えていた**
- **壁時計はこの対象で 1.6 倍動く。** 同一バイナリ 6 回で 3.96〜6.22 s。
  判定に使うのは決定的な整数の方
- **`--timings` に出ない仕事がある。** `resolve_external_links` は `build` の clock の外。
  wall − 内側 = その差、という形で切り出せる
- **模擬と本物でシナリオが違う。** 「1 モジュール変更 3.00 パス」は `ledger touch` の値で、
  本物のソース編集は 4.00 パス。差は `ownership` の全走査 1 周
- **ssh (port 22) はこの機材から通らない。** push は HTTPS + `gh`:
  ```
  GIT_CONFIG_COUNT=2 \
  GIT_CONFIG_KEY_0=credential.helper GIT_CONFIG_VALUE_0='' \
  GIT_CONFIG_KEY_1=credential.helper GIT_CONFIG_VALUE_1='!gh auth git-credential' \
  git push https://github.com/FujiHaruka/lean-doc.git main:main
  ```
- **`diff` はこのシェルで `colordiff` に alias されていて存在しない。`/usr/bin/diff` を使う**
- **CI ワークフローは `workflow_dispatch` のみ**。`gh workflow run ci-template.yml --ref main`
  → 12〜15 分 (package の生成が長い)。結果は
  `gh run download <id> -n template-validation`
- **subagent には「コミットするな」と指示する。同時に走らせるのは 1 体まで**
- **新しいゲート / 計測器は必ず一度落としてから通す** — 今回 `onemod-gate.sh` は
  **2 回続けて本物を捕まえた**
