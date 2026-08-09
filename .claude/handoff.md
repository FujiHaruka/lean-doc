# Handoff — 2026-08-10 (4)

## State

- Branch: main / Uncommitted: clean / push 済み (`d3f1963`)
- Active phase: `docs/plans/three-axes.md` の **leg 8 (【判断点 3】= stale が残らないことの検証)**。
  **3 軸すべてに実測が入った** — 初回 14.53s (warm) / CI cold 25.76→19.11s (先読みあり) /
  増分 3.30–4.39s。残るのは stale の証明 (leg 8) と全体整合 (leg 9)。
- 計測環境: 対象リポジトリ `/Users/haruka/dev/lean-projects` の doc-gen4 (v4.31.0) に計装パッチが
  当たったまま (`benchmarks/tools/apply-instrumentation.sh --check`)。対象リポジトリ clean、
  `.lake/build/doc` の 348 ページは**正解データなので読むだけ**。`fromDb` を再実行すると壊れる。
- ディスク空きは **15 GiB しかない**。scratch を使ったら消す。

## Relay control

- Mode: ON
- Goal: `docs/plans/three-axes.md` を完遂する。初回・CI・増分の 3 軸それぞれに実測を 1 つ入れる。
  **判断基準を満たさなければそこで停止し、否定を `docs/verification-log.md` に記録して
  `approach.md` の見直し案を書いて終了 — それも完遂。**
- Leg: 6 / cap 10
- Predecessor: three-axes-r5
- Stop-on: completion | user-decision | no-progress×2 | leg-cap
- Progress ledger:
  - r1: 段階 3 増分 1・2。`7380856` / `b0da5a0` / `da91c89`
  - r2: 段階 3 増分 3・4 (完結) + IR 永続化。`6c5a845` / `bcc4851` / `4aa119f`
  - r3: 段階 4 増分 1・2 (位置つきタグ + 再計測)。`70aeba0` / `f623de3` / `88c1e8a`
  - r4: **段階 4 完結 =【判断点 2】通過**。`454c77a` / `291d5e6` / `e7bcac0` / `506ee0a` / `555a754`
  - r5: **leg 6 (CI 軸) 完結 + leg 7 (段階 5 増分 1)**。`1fdd5d6` / `8ee72be` / `82db52a` / `d3f1963`

## Tasks

- #1 [pending] leg 8【判断点 3】— stale ページが残らないことの検証
- #2 [pending] leg 9 全体整合 — approach.md §6 の全面更新 + 703 行の圧縮 + 431/432 の決着

## Where we are

**3 軸すべてに実測が入った。** r5 で埋めた 3 つ:

1. **cold の内訳を実測に置換** (`1fdd5d6`) — 「約 3 GB の olean」はラベルの落ちた数字だった。
   `mincore(2)` で実測すると **2,588,347,936 B (2.59 GB)**、出所と思われる peak RSS は 1.26 倍の過大。
   読み出しの 59.9% は **`.olean.private`**。cold 12.7s は 132,343 回の同期 major fault の
   直列待ちで、**ディスクは詰まっていない** (8 並列 read で 1,841 MB/s)。
2. **CI 軸 = leg 6 完結** (`8ee72be` / `82db52a`) — 打ち手 1「`cache get` 直後なら warm」は
   **この機材では偽**、打ち手 2 **先読みは成立** (importModules 13.573→6.923s、
   major fault 121,386→26,329、cold 合計 25.76→19.11s)。
3. **増分 = 段階 5 増分 1** (`d3f1963`) — 1 モジュール変更で **3.30–4.39s (warm 実測)**。
   **理論値 0.03s は 110 倍外れて否定**。律速は**環境の取得で 81.8%**
   (importModules 50.7% + `lake env` 0.96s 25.1%)、本来の仕事は 1.4%。

## Next step

`docs/plans/three-axes.md` の **leg 8 =【判断点 3】「stale ページが残らないことの検証」**。

leg 7 は判断基準 3 つのうち **#3 (影響ページ集合が正しく閉じている) を「満たした」と書かなかった**。
必要条件は満たすが、**olean ハッシュを素通りする経路が 2 つ**残っている:

1. **印字の経路** — `notation` / `delab` / `export` は M の環境拡張にあり、
   **N の olean を触らずに N の署名の印字を変える**。対象では `notation` 6 個・2 モジュールのみで
   全部 `scoped`、unexpander/delab/export は 0 (実測)。だが**これは対象の性質であって定理ではない**。
2. **所有モジュールの経路** — IR は参照を `(定義モジュール, 名前)` で持つので、
   **宣言を別モジュールに移すと参照側の IR が古くなるのに参照側の olean は変わらない**。

さらに前 leg から持ち越しの実例がある (下記 Load-bearing context の「43 ページ」)。
**leg 8 はこの 2 経路 + 実例を検証項目に落とし、ハッシュ連鎖が捕まえられるかを実測で決める。**
捕まえられないなら**それが判断点 3 の否定**で、`approach.md` の §5.5 (モジュール単位ハッシュが
単一の真実) を見直す案を書いて終わる — 計画 §3 の (B) も完遂。

その後 leg 9 (全体整合)。

## Files to read first

- `docs/plans/three-axes.md` — 状態行と §3 到達点の表 (どこが埋まってどこが残っているか)
- `docs/verification-log.md` の「段階 5 — 増分生成」 — leg 7 の判断基準と、#3 を保留にした理由
- `experiments/stage5/README.md` — ハッシュ機構の設計判断 (何をハッシュするか / 依存伝播)
- `benchmarks/results/stage5-incremental-summary.txt` — 直近の計測ハーネスの書式 (真似する)

## Load-bearing context

### 新しく手に入った道具 (r5)

- **`benchmarks/tools/olean-evict.c`** — `msync(MS_INVALIDATE)` で**指定ファイルだけ** page cache
  から落とす。**sudo 不要**。これで **cold が反復可能になった** (計画 §6 事前決定 #6 の
  「cold はセッション最初の 1 回」という制約が外れた)。独立反復の差 0.45%。
- **`benchmarks/tools/olean-residency.c`** — mincore で常駐ページを数える。JSONL 出力。
- **`benchmarks/tools/olean-prefetch.c`** — 先読み。**`madvise(MADV_WILLNEED)` 版が `read` 版より良い**
  (同じ所要時間で常駐が高く下流の fault が少ない)。macOS の `MADV_WILLNEED` は**同期**
  (検証済み)。**Linux では非同期なので方式の選択は移送できない。**
- バイナリは .gitignore 済み。`cc -O2` で再ビルドが要る。

### 計測の落とし穴 (r5 で踏んだもの)

- **計測の順序が後続の数字を汚す。** stage4b を 16 run 回したら wired メモリが 2.08→4.09 GiB に
  上がって**戻らず**、以後の baseline が 13.6→15.7s に悪化した。
  **ブロック内 baseline を必ず取り直す。重い系列は最後に置く。**
- **交互配置が使えない場面がある。** full の作業集合 (olean 6,021 + 匿名 2.8 GB) が
  1 モジュール側を追い出し、6 round 回しても major fault 72,000。
  そのときは系列ごとの連続実行にして、前後でドリフトを確認する。
- **warm 判定に `壁時計 ≒ user+sys` を使えない相手がいる** (Deno は (user+sys)/壁 が 1.3)。
  **major page fault を主指標にする。**
- **載せられる page cache の上限を `vm_stat` から計算しない。** 実測では読み側でも 1.5 GiB 止まりで、
  inactive を足した 3.9–4.2 GiB とは 2.5 倍以上ずれた。**実測で取る。**

### 未解決 / leg 8・9 に持ち込むもの

- **43 ページの stale の実例**: ディスク上の doc は同一ビルドなのに **git rev が 2 つ混在**
  (`573793b…` 305 ページ / `5e38aec…` 43 ページ・540 リンク、独立に検算済み)。
  43 ページは**署名は現行 IR と一致していてリンクだけが古い** — コンテンツは新しく
  メタデータだけ古い。**ハッシュ差分では原理的に捕まらない stale。leg 8 の検証項目。**
- **`docs/approach.md` が 703 行** — CLAUDE.md の `/compact-plan` 閾値 600 行を超えた。
  **leg 9 で圧縮する。** ただし「決着した経緯は消してよいが、**仮説と、それを否定する条件は残す**」。
- **431 と 432 の混在**: doc-gen4 側の計測は 431 モジュール、抽出器側は 432。
  README と approach.md で両方使っている。**leg 9 で決着させる。**
- **Linux は 1 バイトも測っていない。** CI 軸の結論は macOS のみ。
  `approach.md` §3 の「CI は常に cold」は**仮定に格下げ済み**で、検証項目
  「`ubuntu-latest` で `cache get` 直後の常駐率を測る」が未着手として記録されている。
  実 CI で測るには対象 (public) に workflow を足すことになり、**これは要ユーザー判断**
  (relay では PAUSED 終端になるので勝手にやらない)。
- **IR の既知の欠落 (schema 3 候補)**: 定数タグの*トリム前*の範囲。無いので `splitWhitespaces` の
  空白復元ができず、772/3,477 宣言が byte 不一致 (差は `\n`→` ` が 1,765 文字、平文の 0.115%、
  **アンカーへの影響 0**)。推測で埋めると 407 直して 100 壊すことを実測済み。**今は入れない。**
- **完全版の抽出器 (stage4b) の cold 合計は実測できていない** — peak RSS 3.33 GB の作業集合が
  この機材に収まらず thrash する。cold 合計 19.11s は stage1 からの**外挿**。
- `benchmarks/tools/read-ir.ts` の `DEFAULT_IR` が**古い scratchpad**を指したまま。`--ir` を渡すこと。
- `docs/verification-log.md` は 2,066 行。**数字の SoT なので圧縮しない。**
  600 行ルールは `docs/plans/*.md` と `approach.md` に対して適用。

### 対象リポジトリの制約 (毎 leg 効く)

- **書き換えない・コミットしない。** `lake exe cache get` / `lake update` / `fromDb` を**実行しない**。
- そのため**「ソースを編集して `lake build` し直す」増分実験ができない。**
  leg 7 は「M が変更された」という事実だけをハッシュ台帳に注入して測った。
  **`lake build` の時間は lean-doc の外だが増分の臨界パス上にある** — 測るなら対象のコピーが要る。
