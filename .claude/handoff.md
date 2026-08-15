# Handoff — 2026-08-15 (19)

## Relay control
- Mode: **DONE**
- Goal: **lean-doc v0.1 = 使える CLI**。`docs/implementation-plan.md` の M1→M6 を順に完遂する
- Leg: 7 / cap 16   # cap は 2026-08-15 にユーザーが 8 → 16 へ引き上げ。leg 7 で完遂した
- Predecessor: none
- Stop-on: **completion**
- Progress ledger:
  - r1〜r5: M0 / M1 / M2 / M3-a / M3-b / M3-c + docs 圧縮 (`26df736` → `5086353`)
  - r6: M3-d1 `ad9bad9` / M3-d2 `255b413` / M3-d2b `8ab4335` / plan 圧縮 `c879f69`
  - r7: **M3-d3 `7567e1e` / M3 完遂 `b4e7250` / M4 `9fd8ba1`+`ee9293c`+`f73ded5` /
    M5 `224842d`+`ce2d1d3` / M6 `5a2b8a9`** — **v0.1 到達**

## 到達点 — v0.1 のゲートは 2 つとも通っている【すべて実測 2026-08-15】

| ゲート | 結果 |
|---|---|
| **A: 移設** | `coverage.ts` で **99.5062% (21,919,956 / 22,028,728 B)**、byte 完全一致 304/348、不足 108,772 B は**全部 rev**、rev 以外の食い違い **0 文字**。**M1-d3 (2026-08-11) をバイト単位で再現** |
| **B: v0.1** | 第 2 の対象で **doc-gen4 の木ゼロで `lean-doc build` 一発** (19/19)、増分も from-scratch と 20/20 |
| 本物の移動 / 削除 (クローン) | **439/439** / **437/437** バイト一致 |
| 品質 | `cargo test --workspace --no-fail-fast` **295 passed / 0 failed**、clippy 警告 0、fmt 緑 |

**`experiments/` からの未移設はゼロ。** 抽出器は `extractor/` に移動済み。

## State

- Branch: main / **clean** / **push 済み** (`5a2b8a9` = `origin/main`)
- 計測対象 `/Users/haruka/dev/lean-projects` は**無傷** (`git status -uall` 0 行 / doc 参照木 **6,095 ファイル**健在)
- 常駐抽出器の残骸なし
- **クローン `/private/tmp/lean-doc-relay/clone`**: ベースライン復帰済み (HEAD `ca4fd931` / clean /
  `All targets up-to-date (3779 jobs)`)。**olean はクローンのパスで焼き直してある** (`rebuild-own.sh`、677 s)
- **第 2 の対象**: `tools/make-target2.sh` が再生成する (木は残さない方針)

## 次にやるなら — v0.1 の外側

1. **README の未検証項目 10 件**が次の作業リスト。特に **実在の公開パッケージでの実走**
   (ユーザー判断で v0.1 の範囲外にした)、**GitHub Actions 実走**、**静的資産を生成しない穴**
2. **`docs/implementation-plan.md` が 927 行** (閾値 600)。**畳んで縮む段階は過ぎている** —
   §7 が実質「結果のアーカイブ」に育ったので、**本当の直し方は §7 を別文書に切り出すこと**。
   ただし CLAUDE.md のリポジトリ構成表を書き換える話なので**ユーザー判断**
3. **`.lidx` のモジュール名は非エスケープ** (IR / `name-map.json` はエスケープ形)。
   href は同じパスに解決するので出力バイトには出ないが**ルックアップ鍵としては別物**

## Load-bearing context (次に触る人が踏む罠)

- **`/private/tmp` は揮発する** — このセッションでクローンの git 管理下ファイルが全消失した
  (`.lake` だけ残存)。**fixture の存在確認は「ディレクトリがあるか」ではなくファイル数で取る**
- **ゲート A は `--source-url` を参照木の rev `573793b243fb1343636088eb62d1789ab2b14cec` に固定して回す** —
  40 桁 hex なら何でもよいのではなく**参照木と同じ 40 桁**でないと 99.5 → 96.4% になる (差は rev だけ)
- **壁時計を実装差として読まない**。`--jobs 4` では「壁時計 ≒ CPU 時間なら warm」が使えない
  (user > wall が正常)。暖機は **user 時間の収束**で見る
- **「作る前に検査する」は 2 回破られた** (M4-b の相対パス / M6 の `mkdir -p`)。どちらも**計測対象に
  実際に書き込んだ**。新しい呼び手を書くたびに再発するので、パスは渡す前に絶対化し検査を mkdir の前に置く
- **統合前に必ず自分で回す**: fmt/clippy/test + `experiments/` と対象の無傷確認 +
  **成果物を自分で作り直してバイト比較** + 母数の独立再計算。この leg では報告のテスト本数 (160) を
  247 に訂正し、NUL の「欠陥」判定を**オラクルに否定されて取り消した**
- **subagent には「コミットするな」と指示する**。**同時に走らせるのは 1 体まで**
- **npm/node は壊れている**。JS は **deno**。`diff` はスクリプトでは `/usr/bin/diff`
- **`experiments/` は 1 バイトも変更しない**。**`git add -f` を使わない**

## Files to read first

- `README.md` (287) — v0.1 の顔。未検証項目 10 件はここ
- `docs/implementation-plan.md` (927) — §1 のゲート A/B と決定 1、§7 の M3-d4 / M4-d / M5-a / M5-b / M6
- `crates/lean-doc/src/build.rs` (985) — 1 コマンドの実体。台帳の書き戻しの規則
- `tools/{build-gate,clone-gate,target2-gate,ci-build}.sh` — ゲートの実行形
