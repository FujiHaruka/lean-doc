# Handoff — 2026-08-17 (未検証項目 4 件を潰す relay の起点 / leg 0 → r1)

## State

- Branch: main / **clean** / push 済み (HEAD `a3aeb31`)。tag `v0.1.0` は `117e928`
- 直前の仕事は **未検証項目の棚卸し** (`a3aeb31`) — README の 18 件を **15 件**に整理した。
  **減ったのは分類と鮮度であって、検証が進んだからではない**:
  1 件は古く (実ブラウザ = `browser-gate.sh` が既に CI で 9 検査緑)、
  3 件は「持たないと決めた」ものなので別節へ分けた。**実作業はここから**
- `cargo test --workspace` **0 failed**、`tools/provenance-gate.sh` ok (27 claims)
- **ディスクは 57 Gi 空き** (`/private/tmp/lean-doc-relay` の残骸は無い)。
  **計測が終わったら消すこと** — leg 4 で満杯にして対象の olean を壊した実績がある

## Relay control

- Mode: ON
- Goal: **README「未検証項目」15 件のうち 4 件を、下の順で潰す**
- Leg: 1 / cap 8
- Predecessor: none (leg 0 = ユーザーの元セッション。tmux 名を持たないので kill しない)
- Stop-on: completion
- Progress ledger:
  - r0: **棚卸しのみ** (`a3aeb31`)。4 段の実作業はゼロ

## ゴール — 4 段。**この順を変えない**

順序の根拠は「**安い恒久ゲートを先に敷いてから、高コストの一回性実験を撃つ**」。
段 4 を先にやると、そこで出た問題を受け止める網 (段 1〜3) がまだ無く、
**実走で何かが出ても「それが唯一の問題か」を言えない**。

| 段 | 項目 | 完了条件 |
|---|---|---|
| **1** | **#15 md4c FFI の fuzz + `cargo-deny`** | commit 済 seed corpus を全通しするゲートが CI で緑。`cargo-deny` が CI に乗る |
| **2** | **#10 + #4 を 1 本で** | `e2e/micro` に **path 依存のサブパッケージ**を足し、相対リンクフォールバックと `«…»` の `.lidx` 綴りが**実経路**を通る |
| **3** | **#13 等幅フォントの字形** | `browser-gate.sh` が字幅/字形を見て、**CI の `ubuntu-latest`** で判定する |
| **4** | **#1 実在の公開パッケージでの実走** | **Mathlib に依存していない**実在の公開 Lean パッケージで `lean-doc build` が通る |

### 段ごとの要点

**段 1 — #15 fuzz + cargo-deny**
- 設計は決定済で、**時間ではなく corpus で回す** (→ `docs/plans/quality-gates.md` §4 決定 5)。
  「N 秒回して落ちなければ緑」は非決定的なので**やらない**。commit 済 seed corpus を全通しする形
- 既知の 2 入力 (fenced code 中の NUL / 本文行の無い GFM テーブル) は**既に回帰テストにある**。
  seed に入れること自体は目的ではない — **探索は手元で回し、新しい入力が見つかったら corpus に足す**
- `cargo-deny` は Q9 の残り。数分で入る
- **これが唯一の「安全性」項目** — vendor した C を FFI で呼んでいる

**段 2 — #10 + #4 を 1 本で**
- **なぜ今まで触れなかったか (棚卸しで判明した構造)**: フィクスチャが 2 つとも依存の形を持たない。
  `e2e/micro` は `lake-manifest.json` が `packages: []` で**依存 0**、
  第 2 の対象 (`tools/make-target2.sh`) は**対象 1 の manifest をコピー**するので
  常に同じ 15 パッケージ (全部 GitHub + 40 桁 rev)。**依存の形を変える機材が無い**
- **path 依存** (url 無し / rev が 40 桁 hex でない) を 1 本足せば #10 の
  相対リンクフォールバックが実経路を通る。**そのモジュール名にギュメを入れれば #4 も同じ 1 本**
- ネットワーク不要・Mathlib 不要。`e2e/micro` は Mathlib に依存しないフィクスチャ (→ `e2e/README.md`)
- **`micro/` の既存の宣言を消さない** — 1 つ 1 つが「対象が持たない形」を担当している

**段 3 — #13 字形**
- 「**macOS 以外では見ていない**」と README に書き続けていたが、**CI ランナーは `ubuntu-latest`**。
  Linux 環境は既にタダで手に入っている。**持っている機材を使っていなかった**型
- 本文に出る**非 ASCII 178 種**【実測】。添字 (₁ ᵢ ᵐ ⁿ) と double-struck (ℝ ℕ ℤ) を
  持たない等幅フォントでは字幅が崩れる。崩れるなら JuliaMono をサブセットして vendor する
- `browser-gate.sh` → `benchmarks/tools/check-site-browser.ts` に足す。
  今そこにある検査は 7 種 (幅ループで 9): コンソールエラー / ツリー / 検索 ×2 / instances /
  テーマ (`data-theme` が動くことだけ) / 横スクロール / JS 無効

**段 4 — #1 実在の公開パッケージ**
- **【ユーザー指定・最重要】Mathlib はサイズが大きいので、Mathlib に依存していない
  実在の公開 Lean パッケージを対象にすること。** 対象 1 / 第 2 の対象は両方 Mathlib 依存なので、
  これは**新しい軸**であって置き換えではない — **既存の数字は残す** (CLAUDE.md「ベンチマーク」)
- 候補になりうるのは `batteries` / `Cli` / `MD4Lean` / `UnicodeBasic` / `LSpec` など、
  **対象の依存に実在していて Mathlib を要らないもの**。選定理由を docs に書くこと
- これは**一回性**で恒久ゲートにならない (rev が動く)。だから最後
- **#5 (同名宣言) / #9 (乖離 3 件) / #6 (より大きい対象での cold) はこの段に付随して当たりうる** —
  当たったら記録する。**当たらなかったことも記録する**

## 共通の規律 (CLAUDE.md より、外すと事故る)

- **ゲートは必ず一度落としてから通す。** 作った当日に「何をしても通るゲート」を 2 件作っている
- **skip で緑を返さない。** 入力が無いテストは `#[ignore]` + `tools/corpus-tests.txt`
- **ゲートは「走った本数」を数える。** cargo は 0 件マッチでも exit 0
- **性能を壁時計でゲートしない。** 決定的な整数を使う
- **数字には 4 ラベル** (実測 / 外挿 / 仮定 / 理論値)。**出所を辿れること**
- **docs を同じコミットで直す** — README「未検証項目」/ `implementation-plan.md` §1 末尾 /
  `milestone-log.md` / `verification-log.md`。**件数 (15 件) を動かしたら引用元も全部**
  (`CLAUDE.md` 冒頭にも書いてある)
- **各段は独立にコミットする。** メッセージは 1 行、日本語、短く

## Files to read first

- `README.md` §未検証項目 — **15 件の現況。棚卸し直後なので正確**
- `docs/plans/quality-gates.md` §4 決定 5 / §5 の Q6・Q9・Q10 — **段 1 の設計はここにある**
- `e2e/README.md` — フィクスチャが「対象が持たない形」を担当する設計。**段 2 で読む**
- `benchmarks/tools/check-site-browser.ts` — **段 3 で足す先** (341 行)
- `docs/implementation-plan.md` §1 末尾 — v0.1 を締めた根拠と残したもの

## 踏んだ地雷 (次の自分へ)

- **ディスクを満杯にした。** `/private/tmp/lean-doc-relay` に 5 世代 24 GB。満杯になると
  **中断された `lake build` が対象の olean を欠落させ**、**シェルコマンド自体が動かなくなる**。
  → **計測が終わったら消す**。1 回のサイトは約 60 MB、`make-target2.sh` の package は数 GB
- **`git checkout <file>` で subagent の実装を吹き飛ばした。** 無効化実験はスクラッチのコピーか `git stash` で
- **「あるものを使う」は「今のソースのものを使う」ではない。** 1 日で 3 件出た。**3 件とも出力は正常に見えていた**
- **壁時計はこの対象で 1.6 倍動く** (同一バイナリ 6 回で 3.96〜6.22 s)。判定は決定的な整数で
- **Python 3.9 の f-string に backslash / ネスト同種クォートが書けない**
- **`diff` はこのシェルで `colordiff` に alias されていて存在しない。`/usr/bin/diff` を使う**
- **`rg` の `-r` は `--replace`。`-rn` のように束ねない** (後続フラグが置換文字列に食われる)
- **ssh (port 22) はこの機材から通らない。** push は HTTPS + `gh`:
  ```
  GIT_CONFIG_COUNT=2 \
  GIT_CONFIG_KEY_0=credential.helper GIT_CONFIG_VALUE_0='' \
  GIT_CONFIG_KEY_1=credential.helper GIT_CONFIG_VALUE_1='!gh auth git-credential' \
  git push https://github.com/FujiHaruka/lean-doc.git main:main
  ```
- **CI ワークフローは検証用が `workflow_dispatch` のみ** (push 毎に数 GB 落ちるので意図的)。
  `ci.yml` だけが push / PR で走る。`gh workflow run ci-template.yml --ref main` → 12〜15 分
- **subagent には「コミットするな」と指示する。同時に走らせるのは 1 体まで**
