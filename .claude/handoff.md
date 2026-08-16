# Handoff — 2026-08-16 (20)

## Relay control
- Mode: **ON**
- Goal: **UI 完全刷新 (M8)** — doc-gen4 模倣をやめ、**IR はそのまま**で HTML / CSS / JS を自前にし、
  **information-theory の GitHub Pages を新 UI で再ホストする**まで。計画は `docs/plans/ui-redesign.md`
- Leg: 1 / cap 8
- Predecessor: none
- Stop-on: **completion**
- Progress ledger:
  - r1: (進行中) 計画 `docs/plans/ui-redesign.md` を起こした
- 前ゴール (完了): **M7** — 計画 `fb05282` / オラクル `5d5dbbc` / M7-b `0db2e19` / M7-a `d339d6f` /
  M7-d 第 1 段 `72d682d` / M7-c `992ad35` / docs `b110245`〜`b058628`

## 到達点 — M7 は 4 段とも通っている【すべて実測 2026-08-16】

| 段 | 結果 |
|---|---|
| **M7-a** `.lidx` に行範囲 | **255,975 / 255,975 (100%)** に範囲。doc-gen4 の URL と一致 **235,185 / 食い違い 0** |
| **M7-b** パッケージ解決 | 15/15 + core。参照木にページのある **12 root すべてで一致**。**全部オフライン** |
| **M7-c** レンダラ切り替え | 依存リンク **96,331 本**が移り**食い違い 0**。**空マップは M7 前とバイト一致**、自パッケージの href は **0 本**動かず |
| **M7-d** 生存の実測 | 自サイトが emit した URL の**全数**で **484/484 が 200**、**アンカー 738/738 が有効** |

**この変更でゲート A (doc-gen4 との byte 再現) は保留に入った**【ユーザー判断 2026-08-15】 —
**doc-gen4 互換はもう追わない** (最終的にスタイルも変える方針)。**ゲートの再定義は未了で、
ユーザーが後日行う。** 99.5062% は **M6 時点の到達点**として書き換えずに残してある。

## State

- Branch: main / **clean** / **push 済み** (`origin/main`)
- 品質: `cargo test --workspace` **328 passed / 0 failed**、clippy 0、fmt 緑
- 計測対象 `/Users/haruka/dev/lean-projects` は**無傷** (`git status -uall` 0 行 / doc 参照木 **6,080 ページ**健在)
- `experiments/` は 1 バイトも動いていない (`git diff fb05282^ HEAD -- experiments/` が空)
- 最終バイナリでサイトを作り直して **438 ファイルが `diff -r` 0 行**で再現する

## 次にやるなら

1. **ゲート A の再定義** (ユーザーが持っている宿題)。材料は消していない —
   `coverage.ts` と `tools/*-gate.sh` は残してあり、各シェルゲートの冒頭に
   「M7 が依存リンクを動かしたので差分は想定内」と注記済み
2. **README 未検証 11〜13** が M7 が新しく開けた穴 — GitHub 以外 / rev が 40 桁で取れない依存の
   フォールバック (実物で踏んでいない)、オラクルの届かない 27 root、`--root` 無しの `ledger check`
3. **静的資産を生成しない穴** (README 未検証)。v0.1 から残っている一番大きい穴

## Load-bearing context (次に触る人が踏む罠)

- **`/private/tmp` は揮発する。** 今回 `m1/ref-pages` が**ディレクトリだけ残って中身 0** になっていて、
  `is_dir()` で存在判定していた `impact.rs` が**環境要因で赤くなっていた**。
  **fixture の存在確認はファイル数で取る** (`file_count`、`impact.rs` に実装済み)
- **`.lidx` は `#lidx2`** — 宣言行が `\t<name>\t<line>\t<endLine>`。`#lidx1` も読める
- **依存/自パッケージの判定は述語ではなく写像の中身**。`ExternalLinks` に自パッケージの root を
  入れないので `url_for` が `None` を返す。**2 つ目の判定を足さないこと**
- **`--root` 無しの `site`/`render`/`ledger` は空マップ**=M7 前のバイトを出す。
  `ledger check` を手で回すときは `--root` を渡さないと `build` と違う key になる
- **オラクル (`benchmarks/tools/extract-decl-source-urls.sh`) はカバレッジを主張しない** —
  親の div の中に描かれた宣言 (16,038) とページに載らなかった宣言 (2) には沈黙する
- **壁時計を実装差として読まない**。`--jobs 4` では「壁時計 ≒ CPU 時間なら warm」は使えない
- **統合前に必ず自分で回す**: fmt/clippy/test + `experiments/` と対象の無傷確認 +
  **成果物を自分で作り直してバイト比較** + 母数の独立再計算。この leg では
  「オラクルの全件に行アンカーがある」という自分の要約を**カバレッジではない**と訂正した
- **subagent には「コミットするな」と指示する**。**同時に走らせるのは 1 体まで**
- **npm/node は壊れている**。JS は **deno**。`diff` はスクリプトでは `/usr/bin/diff`
  (対話シェルでは `colordiff` に alias されていて存在しない)
- **`experiments/` は 1 バイトも変更しない**。**`git add -f` を使わない**

## Files to read first

- `docs/implementation-plan.md` (433) — §1 のゲート A 保留の枠、§5 の M7 節 (Approach と 4 段のゲート)
- `docs/milestone-log.md` (683) — M7-a/b/c/d の結果と母数
- `crates/lean-doc-render/src/external.rs` — `ExternalLinks`。M7 の判断が 1 箇所に閉じている
- `crates/lean-doc/src/packages.rs` — オフラインの解決器 (manifest + `lean --githash`)
- `benchmarks/tools/{extract-decl-source-urls,check-site-links,measure-blob-url-liveness}.sh|py` — 再現手順
