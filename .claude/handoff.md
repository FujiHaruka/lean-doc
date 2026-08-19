# Handoff — 2026-08-19 (サイト側 JS の TypeScript 化)

## State

- Branch: main / clean / **push 済み** (`5636968` まで。`bf9aa9f` `da57609` は未 push)
- Active phase: `docs/plans/assets-typescript.md` の **P0〜P4 完了**。CI 検証中
- 計測環境: 触っていない (計装・olean 暖機とも前回のまま)

## Relay control

- Mode: DONE
- Goal: `app.js` (917 行の生 JS) を TypeScript 化 — strict / モジュール分割 / vitest /
  biome (スペースインデント) / vite + minify、完遂まで自走
- Leg: 1 / cap 8
- Predecessor: none
- Stop-on: completion
- Progress ledger:
  - r1: 計画 `dabc17c` / TS 化 `82b31e9` / テスト `e4724d1` / ゲート+CI `26b5995`
    `797072a` / docs `5636968` / P4 `bf9aa9f` / 決定 5 決着 `da57609`。
    **CI はブランチ `ts-assets` で 1 度フル緑** (run 32202975792)、最新は 32203827489

## Where we are

`crates/litedoc4-render/assets/app.js` は**リポジトリから消えた**。ソースは
`crates/litedoc4-render/web/src/` の TS 20 本、`build.rs` が vite を回して
`OUT_DIR` に `app.js` と `theme-boot.js` を焼き、`assets.rs` / `frame.rs` が
`include_str!` で拾う。**`cargo build` は node を要る**ようになり、cargo を回す
ワークフロー 8 本と `action.yml` に `actions/setup-node` を入れた。
配るバイトは **32,173 → 15,109 B (gzip 10,508 → 4,908)**。

## Next step

**残作業は無い。** 次にこの領域を触るなら、まず `bf9aa9f` / `da57609` の push と
CI (run 32203827489) の結果確認から。それが緑なら完了。

新しく JS を足すときの手順だけ覚えておく:
`crates/litedoc4-render/web/` で `mise exec -- npm run …`、確認は
`mise exec -- tools/assets-gate.sh`。**`mise exec` を外すと PATH 上の壊れた node に当たる**。

## Files to read first

- `docs/plans/assets-typescript.md` — 決定 1〜6 と §9 結果。この作業の SoT
- `crates/litedoc4-render/build.rs` — node がビルド依存になった理由と、フォールバックが無い理由
- `tools/assets-gate.sh` — 4 段ゲート。node 版の一致検査もここ
- `crates/litedoc4-render/web/src/index-format.ts` — 索引デコーダ。`!` の方針がヘッダにある
- `CLAUDE.md` の「この機材の罠」 — node / biome.json の 2 件が今回追加

## Load-bearing context

- **`biome.json` にコメントを書くと設定ごと黙って捨てられ、終了コードは 0。**
  `biome.jsonc` にすること。これで一度タブ整形されている
- **`git checkout -- <path>` を無効化実験に使って未コミットの CI 編集を消した。**
  CLAUDE.md が警告している通り。バックアップコピーを使う
- **ローカル緑 ≠ CI 緑**: `mktemp -t <prefix>` は BSD で通り GNU で落ちる。
  ゲートを書いたら CI で 1 度回す (`gh workflow run ci.yml --ref <branch>`)
- 決定 5 (biome を Deno の `tools/*.ts` に広げるか) は**測って却下**。
  再検討の条件は「それらのどれかがゲートに昇格したとき」
