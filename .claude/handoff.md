# Handoff — 2026-08-11 (12)

## State

- Branch: main / **clean** / 全部コミット済み (`dd5c886` が HEAD)
- Active phase: **検証は全部終わった。次は実装フェーズの入口 = 実装計画を書くところ**
- 計測環境: 対象 `/Users/haruka/dev/lean-projects` は **clean**、doc-gen4 の計装は APPLIED のまま。
  この leg では **Lean を 1 度も走らせていない** (段階 8 はブラウザだけ)
- `docs/approach.md` は **636 行 = 閾値 600 をまだ超えている** (圧縮したが未達 → Tasks #3)

## Tasks

- #3 [in_progress] **approach.md を 600 行以下にする** — 1st/2nd pass で 654 → 636 行
  (バイトは 61.4 → 54.3 KB = −11.6%)。**行数が減らないのは長い表セルでできている文書だから**。
  残り 36 行を削るには節の解体が要る (候補: §2 の実験節と §6.4(a) の統合、§6.3 を §6.1 に畳む)。
  **数字とラベルは削らない**規則があるので、削るなら説明の散文
- #4 [pending] **製品版の実装計画を書く (Rust 移設)** — 移設対象 約 3,900 行
  (レンダラ 2,227 + 全域成果物 492 + 増分 4 本 1,179) + パイプラインのシェルスクリプト

## Where we are

**段階 8 が閉じ、`approach.md` §7 の検証段階は 1〜8 + CI 軸がすべて完了した。**
段階 8 で分かったのは、JS 注入版の順序制約は**タグの位置ではなくリスナ登録順**で決まること
(満たし方 3 通り、外すのは「リスナ内で注入し登録が `jump-src.js` より後」のときだけ)。
**配信時置換版と JS 注入版のどちらを製品で採るかは未決** — 段階 8 は後者が成立することを
示しただけ。**§8 の未解決 4 つ**(所有モジュール / `_private.` / 依存写像の配布 /
Lean バージョン差)も実装計画で扱う対象として残っている。

## Next step

**`docs/implementation-plan.md` を新規に書く**(グローバル規則: 計画には **Approach 節**を
context の後・ファイル別内訳の前に置く)。最低限これらを決める:

1. **完了条件** = 「Rust 版が 439/439 byte 一致」。`coverage.ts` は **Deno のまま製品外に残す**
2. **残る IR 全読み 5 回の `contentHash` キャッシュを最初から構造に入れるか** —
   後から足すより安い。Mathlib 規模だと 5 回 = 約 15.9 秒【実測】
3. **rev 注入の方式**(配信時置換 / JS 注入)。JS 注入なら**配信に鮮度ヘッダが要る**
4. §8 の未解決 4 つを「実装中に決める」「先に決める」に振り分ける

先に #3 (approach.md を 600 行以下) を片付けてから書き始めてもよい。

## Files to read first

- `docs/approach.md` — 計画の SoT。§5 の柱と §8 の未解決だけ読めば実装計画は書ける
- `docs/verification-log.md` の**冒頭の状態表**と**段階 8 の節** (末尾) — 数字の SoT
- `experiments/stage8/README.md` — 予測と結果が同じファイルに入っている
- `CLAUDE.md`「計測の誠実性」— 実装計画にも数字を書くなら 4 ラベルを付ける

## Load-bearing context

- **`coverage.ts:512` の revless 正規化は `/blob/[0-9a-f]{40}/` のハードコード。**
  `--source-url` にタグ名やブランチ名を渡す実装が出たら**採点が静かに下がる**
- **`render.ts` は `--only` が無いと全モジュールを描く。** rev を外した今、空の再生成集合は
  **常時通る経路**。**Rust で再実装するとき同じ穴を空けないこと** (→ §5.5)
- **段階 8 の探索中に一度だけ github.com へ実リクエストが出ている** (遮断コードを書く前に
  `continueRequest` で試した回)。**「外部に出さない」は遮断を書き終える前が穴になる**
- **`location.replace` は JS で包めない** (own property / `configurable=false`)。
  ブラウザ計測で遷移先を捕まえるなら **CDP の `Page.frameRequestedNavigation`**
- **npm/node は壊れている** (署名不正で SIGKILL)。JS が要るときは **deno**。
  `diff` は zsh で `colordiff` にエイリアス (未インストール) なので**スクリプトでは `/usr/bin/diff`**
- **`git add -f` を使わない** — `.gitignore` を無効化して 163 MB を push しかけた
- **subagent には「コミットするな」と指示する** — 統合と commit はオーケストレータが持つ
