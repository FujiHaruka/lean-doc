# Handoff — 2026-08-10 (5)

## State

- Branch: main / Uncommitted: clean / push 済み
- **`docs/plans/three-axes.md` は完遂した (leg 1〜9)。** 3 軸すべてに実測が入り、
  判断点 3 は**否定**として記録され、docs は整合し、`approach.md` は 599 行に収まっている。
- 計測環境: 対象リポジトリ `/Users/haruka/dev/lean-projects` の doc-gen4 (v4.31.0) に計装パッチが
  当たったまま (`benchmarks/tools/apply-instrumentation.sh --check`)。対象リポジトリ clean、
  `.lake/build/doc` の 348 ページは**正解データなので読むだけ**。`fromDb` を再実行すると壊れる。
- ディスク空きは **16 GiB しかない**。scratch を使ったら消す。

## Relay control

- Mode: DONE
- Goal: `docs/plans/three-axes.md` を完遂する。初回・CI・増分の 3 軸それぞれに実測を 1 つ入れる。
  判断基準を満たさなければ否定を記録して終了 — それも完遂。
- Leg: 6 / cap 10 (**leg 6 で完遂。cap には達していない**)
- Predecessor: three-axes-r5 (kill 済み)
- Stop-on: completion
- Progress ledger:
  - r1: 段階 3 増分 1・2。`7380856` / `b0da5a0` / `da91c89`
  - r2: 段階 3 増分 3・4 (完結) + IR 永続化。`6c5a845` / `bcc4851` / `4aa119f`
  - r3: 段階 4 増分 1・2 (位置つきタグ + 再計測)。`70aeba0` / `f623de3` / `88c1e8a`
  - r4: **段階 4 完結 =【判断点 2】通過**。`454c77a` / `291d5e6` / `e7bcac0` / `506ee0a` / `555a754`
  - r5: **leg 6 (CI 軸) 完結 + leg 7 (段階 5 増分 1)**。`1fdd5d6` / `8ee72be` / `82db52a` / `d3f1963`
  - r6: **leg 8 (判断点 3 = 否定) + leg 9 (全体整合) — ゴール完遂**。
    `d8f95df` (基準を計測前に宣言) / `5afcf89` / `01f7279` / `1ec4562` / `58e3e67` / `b58c29b`

## Tasks

なし (ゴール完遂)。次に着手するなら下記「次にやるなら」から選ぶ。

## Where we are

**leg 8 =【判断点 3】は否定で決着した。** 「stale ページが残らない」を 5 命題 (S1〜S5) に
分解し、**否定条件と事前予測を計測前に commit** (`d8f95df`) してから測って、**5 つとも否定**された。

| # | 命題 | 決め手 (すべて実測) |
|---|---|---|
| S1 | 入力はすべて台帳が見ている | `--source-url` の rev を変えると **432/432 ページ**が変わるのに `ledger check` は **0 changed** |
| S2 | 影響集合は変わるページを全部含む | 宣言 1 個の追加で 142 ページ変化。最も広い `importers` でも **stale 3〜141** |
| S3 | 単独再抽出はフル抽出と一致する (全構成で) | `--open` 下で **12,694 対 12,961 B** で不一致 |
| S4 | 消えたモジュールは何も残さない | `ledger check` が**例外で exit 1**、ページは残り、生きた 4 ページがそこを指し続ける |
| S5 | 保守する成果物がサイト全体 | 非モジュール成果物 **23/23** 未生成、うち 5 個 (38.6 MB) が全モジュールの関数 |

**否定されたのは「モジュール単位ハッシュだけで増分が閉じる」という構造であって、
増分生成そのものではない** — 速度 (3.30〜4.39s 対 17.07s) は生きている。
`approach.md` §5.5 を **3 層 (L1 グローバル鍵 / L2 モジュールハッシュ / L3 導出の依存)** に見直した。

**leg 9 (全体整合) で出た大きい訂正が 2 つ**:

1. **`33,034s = 9.2 時間` は出所を辿れなかった。** 引用元 (`doc-gen4-report.md` §4.6.1) は
   **約 32,600 秒 = 9.1 時間**で、生ログを再集計してもそちらが正しい。
   **自分に有利な側に 1.3% ずれていた。** → 約 2,240× に訂正 (README も同時に直した)。
2. **431 はルートモジュールの数え落とし。** 432 = `InformationTheory/**.lean` 431 + ルート 1。
   計測はすべて 432 で行われていた。docs 全体を 432 に統一 (CLAUDE.md も直した)。
   **348 / 391 は揃えてはいけない** — どちらも 42% 打ち切りビルドの産物で意味が違う。

## 次にやるなら (優先順・すべて未着手)

1. **参照側モジュールの olean が実際に変わるか** — 増分 1・2 を通じて**仮定のまま**。
   対象リポジトリの**複製**を作って `lake build` を回さないと決着しない。
   ここが「変わる」なら L3-1 (名前の所有権) の打ち手が不要になるので、**設計に一番効く**。
   同じ複製で `lake build` の時間 (増分の臨界パス上、未測定) も測れる。
2. **段階 6 (常駐)** — 消えるのは 3.1 秒、残るのは 0.46 秒 (実測)。この 2 つを見てから決める。
3. **全域索引 (38.6 MB / 5 ファイル) の増分更新** — 規模は実測、時間は未測定。
   doc-gen4 に実装の前例がある (per-module `.bmp` を読み直して統合、`Output.lean:236-243`)。
4. **`ledger.ts` の実装の穴 3 つ** — `envChanged` が `changed` に効いていない
   (`ledger.ts:222-224` 対 `231-234`)、`incremental.sh:75-76` が `--ir` を渡さない、
   モジュール一覧が `build` 時に凍結される。**どれも数行で直るが、直すと数字が動く**ので
   直したら測り直すこと。

## Files to read first

- `docs/plans/three-axes.md` — 完遂した計画。何がどこで決着したかの索引
- `benchmarks/results/stage5b-stale-summary.txt` — 判断点 3 の全数字 (398 行)
- `experiments/stage5b/README.md` — 判断基準 S1〜S5 の宣言と、実験の再現方法
- `docs/approach.md` §5.5 — 3 層の見直し案 (**未検証**。ここから実装に入ることになる)

## Load-bearing context

### 判断点 3 で分かった「誤りだった記述」(再発防止)

- **「健全な上界は逆向きの推移的 import 閉包」は誤り** (増分 1 README §4)。
  Lean の elaboration については正しいが、**ページについては偽** — docstring の autolink は
  名前 → 定義モジュールの**全域写像**を引くので import で閉じない。
  doc-gen4 の `name2ModIdx` も同じ構造なので、**doc-gen4 の出力にも同じ経路の逆方向依存が
  9 本実在する** (実測。`「the two do not depend on each other」`と書いてある文章の中にリンクがある)。
- **「43 ページの stale はハッシュ差分では原理的に捕まらない」も誤り**だった。
  正体は「グローバル入力 (git rev) が trace に入っていない」で、原因は
  `doc-gen4/lakefile.lean:277-283` が `srcUri` を trace に入れずに
  `buildFileUnlessUpToDate'` の内側で fetch していること。**ビルド中に commit が入った**
  ことによる時間的分割 (マーカーの mtime が 2 群に完全分離、実測)。
- **`391 − 348 = 43` が「43 ページ stale」と一致するのは偶然** (leg 9 で実測、重なり 0)。
  bmp 側の 43 は `.doc` マーカーを持たない = 打ち切りで HTML まで到達しなかったモジュール。

### 道具 (前 leg から引き継ぎ、健在)

- `benchmarks/tools/olean-{evict,residency,prefetch}.c` — sudo 不要で cold を反復可能にする。
  バイナリは .gitignore 済みなので `cc -O2` で再ビルドが要る。
  **macOS の `MADV_WILLNEED` は同期、Linux は非同期。方式の選択は移送できない。**
- `experiments/stage5b/` — 判断点 3 の実験一式。`run-all.sh` (E1/E2/E4/E5、Lean 不要) /
  `run-e3.sh` (Lean を起動する) / `run-e6.sh`。schema 2 の IR ツリーと scratch を渡す。
- **`compare-pages.py` は存在しないディレクトリを渡すと黙って「0 changed」を返す。**
  stale を数えるツールの既定値が「問題なし」側に倒れている。使うときはパスを確認する。
- IR (schema 2 / 432 モジュール / 16 MB) は session scratchpad にしか無い。
  **消えたらフル抽出 16 秒で作り直せる** (`experiments/stage5/extract-once.sh`)。

### 計測の落とし穴 (効き続けるもの)

- **計測の順序が後続の数字を汚す。** 重い系列を回すと wired メモリが上がって戻らない。
  **ブロック内 baseline を必ず取り直す。重い系列は最後に置く。**
- **warm 判定に `壁時計 ≒ user+sys` を使えない相手がいる** (Deno は 1.3)。
  **major page fault を主指標にする。**
- **交互配置が使えない場面がある** — 作業集合が 3 倍違う変種は「どれも warm にならない」。
  系列ごとの連続実行にして、前後でドリフトを確認する。

### 対象リポジトリの制約 (毎回効く)

- **書き換えない・コミットしない。** `lake exe cache get` / `lake update` / `fromDb` を実行しない。
  `lake env <cmd>` は環境を借りるだけなので可。
- そのため**「ソースを編集して `lake build` し直す」増分実験ができない。**
  判断点 3 も IR レベルの注入で代替した (何を偽装したかは summary の §0 に全部書いてある)。
  **これを外すには対象の複製が要る** — 上の「次にやるなら 1」。
