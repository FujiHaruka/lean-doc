# Handoff — 2026-08-15 (18)

## Relay control
- Mode: ON
- Goal: **lean-doc v0.1 = 使える CLI**。`docs/implementation-plan.md` が実装の SoT で、
  M1→M2→M3→M4→M5→M6 を順に完遂する
- Leg: 7 / cap 8
- Predecessor: none
- Stop-on: completion | user-decision | no-progress×2 | leg-cap
- Progress ledger:
  - r1: 実装計画 + 未決 3 件 + M0 + **M1-a** + **M1-b** + バイト差分オラクル。`26df736` → `4fdc8d4`
  - r2: **M1-c 完遂** (md4c FFI + AST / AST → HTML / autolink)。`a605256` → `2a8cebe`
  - r3: **M1 完遂** (d1/d2/d3) + docs 圧縮 + **M2-a**。`418938d` → `5965298`
  - r4: **M2 完遂** + **M3-a**。`f811724` → `0e90382`
  - r5: plan 圧縮 (617→449) + **M3-b** + **M3-c**。`3e1e239` / `adf0380` / `5086353`
  - r6: **M3-d1** `ad9bad9` / **M3-d2** `255b413` / **M3-d2b** `8ab4335` / plan 圧縮 629→572 `c879f69`

## State

- Branch: main / **clean** / push 済み (`c879f69` が HEAD、`origin/main` と一致)
- `cargo test --workspace --no-fail-fast` **247 本** / clippy 警告 0 / fmt 緑
- **M3 は a/b/c/d1/d2/d2b 通過。残りは M3-d3 と M3-d4 のみ**
- 計測対象 `/Users/haruka/dev/lean-projects` は clean、**参照木 736 MB 健在**。
  この leg では `lake build` を 1 度だけ回したが **no-op (5.76 s / 3,779 jobs)** で何も変わっていない
- **クローン `/private/tmp/lean-doc-relay/clone` 健在** — git clean、`lake build` も no-op (3,779 jobs)、
  432 モジュール、`.lake/build/{lib,ir,doc}` 完備。**ゲートはここで回す** (下記)
- 揮発 fixture 健在: `w7h/{base-ir,base-state,base-ledger.json}` / `m1/ref-pages` /
  `m2/{ref-global,gate/ref-site}` / `w7c/linkindex/link-index.lidx` / `m3b*` / `m3c*`
- 抽出器バイナリ `experiments/stage7d/build/extract` (171 MB、2026-08-11 ビルド) **健在**

## Tasks

- #1 [in_progress] **M3** — 残りは **M3-d3** (差分ハーネス) と **M3-d4** (クローンでの本ゲート)
- #2〜#4 [pending] M4〜M6

## Next step

**M3-d3 = 差分ハーネス** から。`lean-doc incremental` と `experiments/stage7h/incremental.sh` に
**同じシナリオを食わせて「同じ答えを計算するか」を判定**する (M3-a/b/c と同じ「問いを 1 箇所で定義して
両実装に回す」形。`tools/{ledger,merge,impact}-{reference,compare}.sh` が作法)。

- 抽出器は**両側とも `experiments/stage7g/extract-once.sh`** (製品側は `--extractor` にそのまま渡せる —
  必須引数が `--modules` / `--ir-dir` / `--timings` で一致している)
- 対象は**計測対象**でよい (`ledger touch` で変化を注入する形。`lake build` は不要)
- プロトタイプ側は `--global new --state <dir>` で回す (製品はキャッシュ版しか持たないので)

その後 **M3-d4 = クローンでの本ゲート**。詳細は下の「ゲートの実行形」。

## ゲートの実行形 — この leg で決めた (M3-d4 の設計)

**計測対象ではなくクローンで回す。** `incremental.sh` のヘッダが protocol を明記している —
「計測対象では `.lean` を編集せず `lake build` も回さない (対象を変更してはならないため)。
**クローンに対しては何も偽装しない — ソースを本当に編集し `lake build` を本当に回す**」(stages 5e/6a/7g)。

- **本物の移動**: `experiments/stage5e/setup-clone.sh move <clone> <A> minimal` が既にある。
  `A` の中身を新モジュール `X = A ++ "Core"` に移し、`A` を 1 行 shim にして `lake build` まで回す。
  **`A` は referrer を持つものを選ぶこと** (`lean-doc impact --census` で選ぶ。docstring のバッククォートしか
  参照が無いモジュールを選ぶと移動が IR の `refs` から観測できず実験が無駄になる — setup-clone.sh:18-25 に前例)。
  `run.sh` の既定は `InformationTheory.Shannon.Huffman.Length`
- **後始末**: `setup-clone.sh reset <clone>` が `git clean -fd` + `git checkout` + `lake build` で戻す
- **本物の削除のシナリオは存在しない** — 自分で作る。**`experiments/` ではなく `tools/` に置くこと**
- **`.lake/build/doc` (736 MB / 約 9 時間) には触れない。クローンにコミットしない**

### ゲートで必ず踏む 2 つの罠

1. **モジュール一覧の順序**【実測 2026-08-15】 — 抽出器は**渡されたリスト順をそのまま `index.json` に書く**。
   `lean-doc modules` は code point 順、プロトタイプの `find … | sort` はロケール順で、
   **同じ 432 件・同じ集合だが 163 行が別の位置**に来る。この順序は**台帳のバイト**と
   **`index.json` のバイト**の両方を作る (サイトのバイトには届かない)。
   → **増分側と from-scratch 側に必ず同じリストを渡すこと。** 違うリストだとパイプラインと無関係に落ちる
2. **`--link-index`**【実測、決定 4】 — プロトタイプの step 7 (`incremental.sh:371-373`) は渡していない。
   渡さないと 432 ページ中 **150 ページのバイトが変わる**。
   → **ページのバイトをプロトタイプの増分実行と比較してはいけない。**
   比較は「集合と IR」をプロトタイプと、「ページ」を**同じ最終 IR からの `lean-doc site`** と行う

## Files to read first

- `docs/implementation-plan.md` (572 行) — §5 の穴、§6 の 6 制約、§7 の U1/U2 表・
  **M3-d が引き取る負債 8 件**・M3-d2/d2b の結果、§8 の V1〜V6
- `crates/lean-doc/src/pipeline.rs` (1,136) — `lean-doc incremental` の実体
- `crates/lean-doc/tests/incremental.rs` (2,472) — 7 状態オラクル (偽抽出器)
- `experiments/stage7h/incremental.sh` (441) — 差分ハーネスの相手
- `experiments/stage7h/run.sh:97-215` — クローン protocol の呼び手 (setup / variant)
- `experiments/stage5e/setup-clone.sh` (109) — move / reset
- `tools/impact-{reference,compare}.sh` — 「問いを 1 箇所で定義して両実装に回す」形

## Load-bearing context

- **統合前に必ず自分で回す**: fmt/clippy/test + `experiments/` と `lean-projects` の無傷確認 +
  **成果物を自分で作り直してバイト比較** + **mutation を 1 件スポット再現** + **母数を独立に再計算**。
  r6 では検算で**報告の 2 件を訂正した** (「22 行移動」→ 実測 163/432、母数 57 → 55)
- **`diff` のハンク数を移動量として使わない** — ブロック移動を 1 と数えるので大幅に過小評価する。
  分母は「同じ添字で食い違う行数 / 総数」で取る
- **`--source-url` をシェル変数で渡すとき引用符を剥がす** — `"..."` ごと渡すと 432 ページ全部が差分になる
- **`w7h/base-pages` は参照に使えない** — ページの参照は `m1/ref-pages`、サイト全体は `m2/gate/ref-site` (438)
- **「全件バイト一致」は分岐被覆の証明ではない**。すり抜ける mutation は各段にある
  (M3-d2 は 14 中 9 が逃げ、M3-d2b は 6 中 4)。**分岐発火回数を先に測ってから**一致を主張する
- **M3-d2 の宿題**: ①連続実行のあいだ**誰が台帳を書き戻すか未決** (`incremental` は読むだけ。M4)、
  ②**cold start の増分は何も出さない** (前回 run が state と name-map を残した前提。文書化のみで未強制)
- **移設元のコメントを根拠にしない** — `merge-ir.ts:44` の「Lean の外では再現できない」は事実として誤りだった
- **`cargo test --workspace` は `--no-fail-fast` を付ける**
- **subagent には「コミットするな」と指示する**。**同時に走らせるのは 1 体まで**
- **npm/node は壊れている**。JS は **deno**。`diff` は zsh で `colordiff` エイリアス (未インストール) なので
  スクリプトでは `/usr/bin/diff`
- **`experiments/` は 1 バイトも変更しない**。**`git add -f` を使わない**
- **`serde_json` の `preserve_order` と `sha2` の `features = ["asm"]` は必須依存**
- **`Utf16Text` に `Deref<Target=str>` は敢えて実装していない**。この防御を緩めない
