# doc-gen4 ボトルネック計測レポート

**対象**: doc-gen4 `v4.31.0` (rev `0bc516c`) / Lean `v4.31.0` / Mathlib `v4.31.0`
**測定対象プロジェクト**: `InformationTheory` (431 モジュール, Mathlib 全体に依存)
**測定日**: 2026-08-09
**目的**: 新規 Lean ドキュメントジェネレーター構想の Step 1「現状の時間を分解して測る」

> 計測そのものは対象の `InformationTheory` リポジトリ上で実施した。
> 本リポジトリにはレポート・計装パッチ・ツール・生ログを移してあり、
> 文中のパスは移動後の配置 (`benchmarks/…`) に書き換えてある。
> 再現するには、対象の Lean プロジェクト側でパッチを当てて実行する。

---

## 0. 結論サマリ

計測の結果、現行 doc-gen4 のコストは **3 つの層**に分かれ、それぞれ別の原因で高価であることが確定した。

| 層 | 何が起きるか | 実測での支配率 | 原因 |
|---|---|---|---|
| **A. 環境ロード** | モジュールごとに 1 プロセスを起動し、その import closure 全体を `importModules` する | Mathlib 依存プロジェクトの実ビルドで **85.0%** (単一モジュール単独では 89.6〜97%) | プロセス境界ごとに closure 全体を再ロードする設計 |
| **B. 意味解析** | `DocInfo.ofConstant` を宣言ごとに実行 (型の pretty print + equation lemma 生成) | 環境が共有されている場合の主コスト。Lean core 抽出では **93.9%** | `getEqnsFor?` が実際に equational lemma を elaborate する |
| **C. HTML 生成** | `fromDb` が DB からモジュールを読み直して HTML 化 | HTML 生成 CPU の **66〜78%** が DB 読み出し | 宣言ごとに個別 SQL を発行する N+1 構造 |

実ビルドの数字で言うと、Mathlib 依存プロジェクトのドキュメント化では
**8.9 億回の定数走査に対して実際に処理されるのは 0.032%** であり、
「該当しない定数を素通りするだけ」のコスト (1,195 秒) が
本来の意味解析 (631 秒) の **約 2 倍**を占める (§4.6.1)。

そして構想の中心仮説は**支持された**。同一の 432 モジュールを、

- **方式A** (現行 doc-gen4 = モジュールごとに 1 プロセス): 2.51 秒/モジュール → 432 モジュール換算 **約 1,084 秒**
- **方式B** (1 プロセスでまとめて抽出): **47.4 秒**

**約 23 倍**の差。うち方式A の 89.6% が `importModules` であり、これは「自パッケージの宣言だけを見たいのに、依存ライブラリ全体を毎回ロードし直している」コストそのものである。

さらに 2 つ、計測して初めて見えた事実がある。

1. **HTML 生成には増分性が一切ない** (§4.6.3)。`fromDb` を DB も出力も無変更で 2 回続けて
   実行しても 46.25 秒 → 43.91 秒と変わらない。増分性は Lake のマーカーファイル層にしかなく、
   `fromDb` の内部には存在しない。
2. **並列度はメモリで律速される** (§4.8)。8 並列時の RSS 合計は 21.7 GB (物理 16 GB) に達し、
   スワップ 16.2 GB を使い切った。プロセス並列で closure ロードを繰り返す設計は、
   CPU コア数を増やしても頭打ちになる。

---

## 1. 測定環境

| 項目 | 値 |
|---|---|
| マシン | Apple M1, 8 コア, 16 GB RAM |
| OS | macOS (Darwin 25.6.0) |
| ディスク | APFS, 空き 17 GB (測定開始時) |
| Mathlib olean | 5.7 GB / 8,229 モジュール |
| InformationTheory olean | 600 MB / 431 モジュール |

計測はいずれも olean が既にビルド済み・ページキャッシュが暖まった状態で行った (cold cache では import がさらに遅い。実測で同一モジュールが 7.35s → 3.32s と 2 倍以上変動する)。

### 1.1 本計測の限界 (先に明示しておく)

- **フルビルドは完走していない。** Mathlib + InformationTheory 8,600 モジュールのうち
  3,590 モジュール (42%) を抽出した時点で打ち切った (理由 → §4.6, §4.8)。
  抽出フェーズは 3,590 モジュールの実測、HTML 生成フェーズは DB に入った
  6,072 モジュールでの単独実測に分けて計測している。全体の wall clock は測れていない。
- **§4.7 の 2 本 (equations 比較) はフルビルドと並行して測った**ため絶対値が膨らんでいる。
  同一条件下の相対比較としてのみ有効。
- 方式A (§4.3) は 432 モジュール中 112 モジュールの実測で、残りは平均値からの換算。
- 単一マシン (M1 / 8 コア / 16 GB) の結果であり、特に §4.8 のメモリ律速は
  RAM 量に強く依存する。

---

## 2. 対象パイプラインの構造

doc-gen4 は v4.31 で **SQLite ベース**に再設計されている。Lake の facet として次の順に走る。

```text
bibPrepass                     参考文献の前処理 (1 回)
   │
genCore  Init / Std / Lake / Lean    ← 4 プロセス。core 宣言を api-docs.db へ
   │
single <module>                ← モジュール 1 個につき 1 プロセス。 ★ここが N 回
   │                              (依存モジュールの docInfo に再帰的に依存)
   ▼
api-docs.db  (SQLite)
   │
fromDb  <root modules>         ← 1 プロセス。DB から全対象モジュールの HTML を生成
   │                              + 検索インデックス + navbar
   ▼
.lake/build/doc/**.html
```

`single` の実体は次の 2 段だけである (`Main.lean`)。

```lean
let doc ← load <| .analyzeConcreteModules relevantModules   -- 環境ロード + 意味解析
updateModuleDb builtinDocstringValues doc buildDir dbFile (some sourceUri)  -- DB 書き込み
```

`load` の中身 (`DocGen4/Load.lean`) は `importModules` して `Process.process` を呼ぶだけであり、
**`analyzeConcreteModules` は既に `Array Name` を受け取れる**。1 モジュールに固定しているのは
`Main.lean` の CLI 定義だけで、バッチ抽出はアーキテクチャ上すでに可能である (→ §4.3)。

### 2.1 見落としやすい構造的な性質

コードを読んで確認した、計測前には分からない性質:

1. **`process` は環境の全定数を走査する** (`Process/Analyze.lean:186`)

   ```lean
   for (name, cinfo) in env.constants do
     let some modidx := env.getModuleIdxFor? name | unreachable!
     if !relevantModules.contains moduleName then continue
   ```

   1 モジュールを抽出する場合でも、Mathlib 依存プロジェクトでは **49 万定数**を走査して
   数十件だけ処理する。走査自体は 0.18 秒と安いが、コストは closure サイズに比例して増える。

2. **`collectTactics` がモジュールごとに全タクティクを列挙する** (`Analyze.lean:133`)

   `getAllModuleDocs` は対象モジュールごとに `Elab.Tactic.Doc.allTacticDocs` を呼ぶ。
   単発では 10〜40 ms で見えないが、バッチ化すると O(モジュール数 × 全タクティク数) として
   顕在化する (→ §4.3 で 16.37 秒、バッチ総時間の 36%)。

3. **`fromDb` の後段はモジュール数に比例した固定コストを毎回払う**

   - `htmlOutputIndex` は `declarations/` 配下の **全 `.bmp` をディスクから読み直す** (`Output.lean:205`)
   - `collectBackrefs` は全 `backrefs-*.json` を読む
   - `updateNavbarFromDisk` は `doc/` 配下の **全 HTML を再帰スキャン**する
   - `headerData` も全 `.bmp` を読む

   つまり「1 モジュールだけ変更」しても、この 4 つは全モジュール規模で走る設計になっている。

---

## 3. 計測方法

doc-gen4 に計装を入れた。環境変数 `DOCGEN_TIMING=<path>` が設定されているときだけ、
各フェーズの実測時間を JSONL で追記する (無効時はゼロコスト)。

- 追加: `DocGen4/Timing.lean` (`emit` / `timed` / `timed'`)
- 計装点: `Main.lean` (single / genCore / fromDb / headerData の各段)、
  `DocGen4/Load.lean` (initSearchPath / importModules / process)、
  `DocGen4/Process/Analyze.lean` (getAllModuleDocs / 定数ループ / ソート、
  走査件数・該当件数・`ofConstant` 累積時間つき)、
  `DocGen4/Output.lean` (HTML タスクごとの DB ロード / レンダ / 書き込み、インデックス各段)
- 追加コマンド: `doc-gen4 batch` — `single` と同じ処理を**複数モジュール 1 プロセス**で行う (方式B の測定用)

並列プロセスが同一ファイルに追記するため、各レコードは PID を持つ。
集計は `benchmarks/tools/analyze.ts`。

パッチ内容は `benchmarks/doc-gen4-instrumentation.patch` に保存してある。
計装は `.lake/packages/doc-gen4/` (別 git リポジトリ) に当ててあるため、
元に戻すには次を実行する (`lake update` でも消える)。

```bash
git -C .lake/packages/doc-gen4 checkout . && rm -f .lake/packages/doc-gen4/DocGen4/Timing.lean
lake build doc-gen4
```

再適用は `git -C .lake/packages/doc-gen4 apply <パッチの絶対パス>`。

---

## 4. 実測結果

### 4.1 最小ケース: Lean core のみ (end-to-end)

`InformationTheory.Meta.EntryPoint` は `import Lean` だけの 1 モジュール。
これを `lake build InformationTheory.Meta.EntryPoint:docs` でドキュメント化した。
= 構想文書でいう「A. Lean core だけの小規模プロジェクト」。

**wall 265 秒 / ピーク RSS 2.23 GB**

| フェーズ | プロセス | wall | 内訳 |
|---|---|---|---|
| `genCore Lean` | 1 | **239.7s** | import 3.0s / modDocs 9.6s / **定数ループ 225.1s** / DB 1.85s |
| `genCore Std` | 1 | 71.1s | import 0.88s / modDocs 2.13s / 定数ループ 65.5s / DB 2.38s |
| `genCore Init` | 1 | 30.1s | import 0.23s / modDocs 2.26s / 定数ループ 25.7s / DB 1.86s |
| `genCore Lake` | 1 | 7.9s | import 0.40s / modDocs 0.64s / 定数ループ 6.6s / DB 0.19s |
| `single` (対象の 1 モジュール) | 1 | 3.2s | **import 3.15s (97%)** / 解析 0.08s / DB 0.003s |
| `fromDb` | 1 | 18.1s | → §4.4 |

4 つの `genCore` は並列に走るので wall は最長の `Lean` に律速される。

**定数ループの内訳** (= `DocInfo.ofConstant` の累積):

| | 走査した定数 | 該当した定数 | ループ時間 | うち `ofConstant` |
|---|---|---|---|---|
| Lean | 203,906 | 88,115 | 225.1s | 224.4s (**99.7%**) |
| Std | 112,076 | 50,600 | 65.5s | 65.3s |
| Init | 65,197 | 65,197 | 25.7s | 25.6s |

1 宣言あたり **2.55 ms**。この中身は `Info.ofConstantVal` (型の pretty print) と
`computeEquations?` である。後者は `getEqnsFor?` を呼び、**equational lemma を実際に生成する**
(`Process/DefinitionInfo.lean:30`)。ビルド中に

```
WARNING: Failed to calculate equational lemmata for Lean.Meta.Tactic.TryThis.addRewriteSuggestion:
  (deterministic) timeout at `isDefEq`, maximum number of heartbeats (200000) has been reached
```

が出るのは、ここで本物の elaboration が走っている証拠である。

**出力サイズ** (Lean core 2,394 モジュール分):

| | サイズ |
|---|---|
| `api-docs.db` | 129 MB |
| `doc/` (HTML 2,402 ファイル) | 223 MB |
| `doc-data/` | 198 MB |
| 合計 | **550 MB** |

### 4.2 `single` のコスト構造 — import closure サイズへの依存

代表モジュールを直接 `doc-gen4 single` で処理した (逐次、キャッシュ暖機済み)。

| モジュール | closure | total | import | modDocs | 定数ループ | 走査/該当 | DB |
|---|---:|---:|---:|---:|---:|---|---:|
| `Mathlib.Init` | 1,161 | 0.71s | **0.66s** | 0.00s | 0.04s | 123,283 / 6 | 0.00s |
| `Mathlib.Order.Basic` | 1,285 | 1.01s | **0.79s** | 0.01s | 0.21s | 132,336 / 431 | 0.01s |
| `Mathlib.Analysis.SpecialFunctions.Log.Basic` | 3,503 | 7.64s | **7.35s** | 0.02s | 0.26s | 339,540 / 179 | 0.01s |
| `Mathlib.MeasureTheory.Integral.Bochner.Basic` | 4,066 | 3.81s | **3.32s** | 0.03s | 0.45s | 385,778 / 202 | 0.01s |
| `InformationTheory.Shannon.BirkhoffErgodic` | 4,777 | 4.02s | **3.75s** | 0.03s | 0.24s | 429,468 / 65 | 0.01s |
| `InformationTheory` (root) | 6,019 | 5.48s | **5.24s** | 0.04s | 0.18s | 490,037 / 0 | 0.00s |

**import が総時間の 90〜97% を占める。**実際の意味解析は 0.04〜0.45 秒に過ぎない。

`InformationTheory` root に至っては、49 万定数を走査して**該当 0 件** — つまり
5.48 秒かけて何も抽出していない。root モジュールは `import` しかないので当然だが、
現行設計ではこの空振りにも closure 全体のロードが課される。

ピーク RSS も closure に比例する: 720 MB (`Mathlib.Init`) → 3.26 GB (`InformationTheory` root)。
16 GB のマシンでは、8 並列で走らせると 8 × 2〜3 GB = 16〜24 GB となりスワップ域に入る
(→ §4.8 で実測)。

#### 構想 Step 1 の 3 プロジェクト分類との対応

構想文書は「小規模 / Mathlib の一部 / import Mathlib」の 3 つを測るよう求めている。
別プロジェクトを用意しなくても、本リポジトリ内のモジュールが closure サイズで
ちょうどその 3 段になっていたので、それを使った。

| 構想の分類 | 対応するモジュール | closure | `single` 総時間 | うち import |
|---|---|---:|---:|---:|
| **A. Lean core だけ** | `InformationTheory.Meta.EntryPoint` | 2,259 | 0.87s | 0.75s (86%) |
| **B. Mathlib の一部を import** | `InformationTheory.Polymatroid.Basic` | 3,033 | 1.37s | 1.22s (89%) |
| **C. import Mathlib 相当** | `InformationTheory` (root) | 6,019 | 5.48s | 5.24s (96%) |

(いずれも `doc-gen4 single` の直接実行・キャッシュ暖機済み。
A を Lake 経由・cold で測ると 3.23s / import 3.15s になる。)

**コストは closure サイズで決まり、パッケージの区別とは無関係**である。
「Mathlib に依存しているかどうか」ではなく「何モジュール読み込むか」だけが効く。
C の root モジュールに至っては該当宣言 0 件で 5.48 秒を消費している (§4.2 の表)。

### 4.3 方式A vs 方式B — 1 プロセスで複数モジュールを抽出する

構想文書の Step 3。`doc-gen4 batch` を追加して同一の 432 モジュール
(`InformationTheory` 全体 + root) を両方式で処理した。

| | プロセス数 | 実測 | 432 モジュール換算 |
|---|---:|---:|---:|
| **方式A**: モジュールごとに 1 プロセス | 112 (実測) | 281.06s | **約 1,084s** |
| **方式B**: 1 プロセスでまとめて | 1 | **47.4s** | **47.4s** |

**約 23 倍。** 方式A の内訳は 281.06 秒中 **251.92 秒 (89.6%) が `importModules`**。

方式B の内訳:

| フェーズ | 時間 | 比率 |
|---|---:|---:|
| `importModules` (1 回だけ) | 12.91s | 28% |
| `getAllModuleDocs` | **16.37s** | **36%** |
| 定数ループ (走査 490,171 / 該当 8,824) | 15.99s | 35% |
| `updateModuleDb` | 0.64s | 1.4% |
| 合計 | 45.93s | |

注目すべきは、バッチ化すると **`getAllModuleDocs` が最大項に浮上する**こと。
これは §2.1 で述べた `collectTactics` の O(モジュール数 × 全タクティク数) が原因で、
1 モジュールずつ処理している限り 10〜40 ms に埋もれて見えない。
新設計では「全タクティクを 1 回列挙してモジュール別に振り分ける」だけで消える。

つまりバッチ化の利得は 23 倍で頭打ちではなく、**この O(n×m) を潰せばさらに縮む**。

### 4.4 `fromDb` (HTML 生成) の内訳

Lean core 2,394 モジュール、wall 18.07 秒。

| フェーズ | 時間 | 比率 |
|---|---:|---:|
| `walCheckpoint` | 0.44s | 2.4% |
| `loadLinkingContext` | 0.32s | 1.8% |
| `getTransitiveImports` | 0.02s | 0.1% |
| **`htmlOutputResultsParallel`** | **16.77s** | **92.8%** |
| `loadAllTactics` | 0.001s | ~0% |
| `htmlOutputIndex` | 0.43s | 2.4% |
| `updateNavbarFromDisk` | 0.02s | 0.1% |

`htmlOutputResultsParallel` は 20 タスクに分割される。全タスクの累積 CPU 78.05 秒の内訳:

| | 累積 CPU | 比率 |
|---|---:|---:|
| DB 接続オープン | 0.22s | 0.3% |
| **モジュールを DB から読む** | **61.23s** | **78.4%** |
| HTML レンダリング | 7.16s | 9.2% |
| HTML 書き込み | 4.05s | 5.2% |
| 検索用 JSON 生成 | 3.32s | 4.3% |
| 検索用 JSON 書き込み | 1.71s | 2.2% |

**HTML 生成の実コストは DB 読み出しであって、レンダリングではない** (9.2%)。
原因は `DB/Read.lean` の `loadModule` が N+1 クエリ構造になっていること: モジュールの
メンバー一覧を 1 クエリで取った後、宣言ごとに `loadDocInfo` を呼び、その中でさらに
引数・属性・宣言範囲・equation を個別のプリペアドステートメントで引いている。
加えて `getContainedNames` は `name_info` × `declaration_ranges` の 4 way 自己結合である。

1 モジュールあたり 32.6 ms CPU。新設計で IR をモジュール単位のファイル (またはブロブ 1 行) に
まとめれば、この 78% はほぼ消える性質のコストである。

### 4.5 増分ビルド

| シナリオ | wall | doc-gen4 の起動 |
|---|---:|---|
| 1 回目 (cold) | 265s | genCore ×4 + single ×1 + fromDb |
| **2 回目 (無変更)** | **2.33s** | **なし** (Lake が全 job を replay) |
| **3 回目 (1 モジュール変更)** | **5.21s** | `single` ×1 のみ (0.86s)。`fromDb` は replay |

Lake のトレース機構は正しく効いており、**無変更なら doc-gen4 は 1 プロセスも起動しない**。
ここは現行実装の明確な強みで、新設計でも同等以上を保証する必要がある。

ただし 3 回目で `fromDb` が再実行されなかった点は注意が必要である。lakefile は
「空のマーカーファイルのトレース変化で再ビルドを誘発する」設計になっているが、
今回はモジュール内容を変更しても HTML 再生成が走らなかった。
**変更が HTML に伝播しない (stale page が残る) 可能性**があり、増分性の正しさは
速度とは別に検証すべき項目である。

なお、仮に `fromDb` が正しく再実行されたとしても、それは §4.6.3 で示すとおり
**全モジュールの再生成**を意味する。現行の増分性は
「取りこぼすか、全部やり直すか」の二択になっている。

### 4.6 フルビルド (InformationTheory + Mathlib 全体)

`lake build InformationTheory:docs` を実行した。これは Mathlib 8,169 + InformationTheory 431 の
全モジュールについて `single` を走らせ、最後に `fromDb` で HTML を生成する。

**このビルドは完走させていない。** 3,590 モジュール (全体の 42%) を抽出した時点で打ち切った。
理由は §4.8 に述べるメモリ律速で、残りを既定の並列度で流すと 2.5 時間、
並列度を落とすと 11 時間かかる見込みだったためである。
代わりに、**抽出フェーズは 3,590 モジュールの実測**、**HTML 生成フェーズは
DB に入った 6,072 モジュールでの単独実測** (§4.6.2) に分けて計測した。
両フェーズは DB を介して疎結合なので、この分割で失われる情報はコストの相互作用のみである。

#### 4.6.1 抽出フェーズ (3,590 モジュールの実測)

| フェーズ | CPU 合計 | 比率 |
|---|---:|---:|
| **`importModules`** | **11,566s** | **85.0%** |
| 定数ループ | 1,827s | 13.4% |
| └ うち `DocInfo.ofConstant` | 631s | 4.6% |
| └ **うち「該当しない定数を素通りするだけ」** | **1,195s** | **8.8%** |
| `getAllModuleDocs` | 143s | 1.0% |
| DB 書き込み | 68s | 0.5% |
| 合計 | 13,611s | 99.9% |

| 指標 | 値 |
|---|---:|
| 1 モジュールあたり平均 | **3.79 秒** |
| 延べ closure ロード | **9,306,404** (平均 2,592 モジュール/プロセス) |
| 走査した定数 (延べ) | **888,462,895** |
| 該当した定数 (延べ) | 282,852 (**走査の 0.032%**) |
| import 単価 | 1,243 µs / module-load |

**8.9 億回の定数走査に対し、実際に処理されたのは 28 万件 = 0.032%。**
しかもその走査のオーバーヘッド (1,195 秒) は、本来の意味解析 (`ofConstant` 631 秒) の
**約 2 倍**にあたる。1 モジュール分のドキュメントを作るために依存ライブラリ全体を
毎回舐め直す構造が、実測でそのまま出ている。

3,590 モジュールの実測から全体 (8,600 モジュール) を外挿すると、
抽出フェーズだけで **約 32,600 秒 = 9.1 時間の CPU 時間**になる。

#### 4.6.2 HTML 生成フェーズ (6,072 モジュールの実測)

DB に入った 6,072 モジュール (Lean core 2,394 + Mathlib 3,678) に対して `fromDb` を単独実行した。

| | Lean core のみ (2,394 モジュール) | 6,072 モジュール | 比 |
|---|---:|---:|---:|
| wall | 18.07s | **44.52s** | 2.46× |
| モジュール数 | 2,394 | 6,072 | 2.54× |

**モジュール数にほぼ線形。**内訳:

| フェーズ | 時間 | 比率 |
|---|---:|---:|
| **`htmlOutputResultsParallel`** | **40.89s** | **91.8%** |
| └ うち DB からのモジュール読み出し (累積 CPU) | 131.6s / 199.5s | **66%** |
| `loadLinkingContext` | 2.20s | 4.9% |
| `htmlOutputIndex` | 1.07s | 2.4% |
| `updateNavbarFromDisk` | 0.07s | 0.2% |

ピーク RSS 1.2 GB。出力は HTML 6,080 ファイル / `doc` 798 MB / `doc-data` 745 MB。

#### 4.6.3 HTML 生成には増分性がない

**同じ `fromDb` をもう一度、DB も出力も一切変えずに実行した。**

| | wall |
|---|---:|
| 1 回目 | 46.25s |
| **2 回目 (完全に無変更)** | **43.91s** |

**ほぼ同じ。**`fromDb` は呼ばれたら常に全対象モジュールの HTML を再生成する。
増分性は Lake のマーカーファイル層にしか存在せず、`fromDb` の内部には一切ない。

これは構想の要件「変更されたモジュールだけを確実に再処理できる」に対する
現行実装の最も大きな穴である。Mathlib 規模 (11,000 モジュール) では、
1 モジュールの変更が `fromDb` の再実行に繋がった瞬間に **80 秒程度の全再生成**が走る。

#### 4.6.4 出力サイズ

| 対象 | `api-docs.db` | `doc/` | `doc-data/` | 合計 |
|---|---:|---:|---:|---:|
| Lean core のみ (2,394) | 129 MB | 223 MB | 198 MB | 550 MB |
| + Mathlib 一部 (6,072) | 433 MB | 798 MB | 745 MB | **1.98 GB** |

モジュールあたり約 340 KB。8,600 + 2,394 = 約 11,000 モジュールへ外挿すると **3.6 GB 前後**。
`doc-data/` (検索用 `.bmp` + `backrefs-*.json`) が `doc/` とほぼ同量あることに注意。
これは中間生成物であり、新設計では IR に統合できる。

### 4.7 `DISABLE_EQUATIONS` の効果

doc-gen4 には equation lemma の生成を切るスイッチがある (`Analyze.lean:162`)。
方式B (432 モジュールのバッチ抽出) で有無を比較した。

> 注: この 2 本はフルビルドと並行して測ったため、絶対値は §4.3 の単独測定 (47.4s) より
> 2 倍以上遅い。**両者が同一条件下にあるので相対比較のみ有効。**

| | equations あり | `DISABLE_EQUATIONS=1` | 差 |
|---|---:|---:|---:|
| `ofConstant` 累積 | 22.72s | 21.11s | **−7.1%** |
| 定数ループ全体 | 27.51s | 29.80s | (ノイズ) |
| バッチ総時間 | 98.07s | 96.38s | −1.7% |

**InformationTheory では equation の寄与は小さい (意味解析部分で 7%)。**
理由は明快で、このプロジェクトの宣言はほぼ `theorem` であり、
equation lemma が生成されるのは `definition` だけだからである
(`DefinitionInfo.ofDefinitionVal` からしか `computeEquations?` は呼ばれない)。

逆に Lean core (`Init` / `Std`) は definition が支配的で、そこでは
「equational lemmata の計算がヒートビート上限に達した」警告が実際に出る (§4.1)。
**equation のコストは「対象パッケージが定義中心か定理中心か」で大きく変わる**ため、
新設計では常時オフではなく**オプトイン/オプトアウト可能な段階**として設計すべきである。

### 4.8 並列度はメモリで律速される

フルビルド中に観測した、計測前には想定していなかった制約。

`single` のピーク RSS は import closure に比例する (§4.2: 720 MB 〜 3.26 GB)。
Lake は既定で CPU 数 (このマシンでは 8) だけプロセスを並列に走らせるため、
Mathlib の深い部分に入ると次の状態になった。

| 指標 | 実測 |
|---|---:|
| 同時実行 `single` プロセス | 8 |
| その RSS 合計 | **21.7 GB** (物理 16 GB) |
| スワップ使用量 | **16.2 GB / 17.4 GB** |

ビルドが進むにつれて 1 モジュールあたりの `importModules` は 1.34 秒 → 6.88 秒へ悪化した。
ただし**この悪化の主因はスワップではない**: 同区間で closure 平均も 1,042 → 4,602 と
4.4 倍に増えており、**1 module-load あたりの単価は 1.27 ms → 1.45 ms (14% 増) にとどまる**。

- **主因**: Mathlib の深部ほど import closure が大きい (コストは closure に線形)
- **副因**: メモリ圧による単価の 14% 悪化

しかし実運用上の帰結は深刻で、`LEAN_NUM_THREADS=4` に落として並列度を半減させたところ、
スワップが解放されないまま処理レートは 34 モジュール/分 → 7.3 モジュール/分まで落ちた
(このためフルビルドを打ち切った)。

**新設計への含意**: プロセス並列で closure ロードを繰り返す設計は、
CPU コア数を増やしても**メモリが実効並列度の上限を決める**。
1 プロセスに環境を 1 つだけ持つバッチ方式なら、この制約自体が消える
(方式B のピーク RSS は 3.2 GB、432 モジュールを処理して単一プロセス分のみ)。

---

## 5. import closure の分布 (なぜ `single` 方式が破綻するか)

`.lean` ヘッダを解析して、パッケージ内の推移的 import closure サイズを求めた
(`benchmarks/tools/closure-sizes.ts`。Lean 4.31 の `module` / `public import` 構文に対応)。

| パッケージ | モジュール数 | closure 合計 | 平均 | p50 | p90 | 最大 |
|---|---:|---:|---:|---:|---:|---:|
| Mathlib | 8,169 | **7,375,495** | 903 | 751 | 1,999 | 3,280 |
| Mathlib + InformationTheory | 8,600 | **8,485,717** | 987 | 813 | 2,194 | 3,280 |

これは**パッケージ内**の closure であり、実際にはこれに Lean core (最大 2,259 モジュール) が乗る。
実測 `loadedModules` は 4,777〜6,021 だった。

`single` 方式では、この closure 合計がそのまま「延べモジュールロード回数」になる。

**予測と実測の照合** (3,590 モジュールを実際に処理した §4.6.1 のデータ):

| | 予測 (ヘッダ解析) | 実測 |
|---|---:|---:|
| 延べ closure ロード | — | **9,306,404** (3,590 モジュールで) |
| 1 プロセスあたり平均 | 987 (パッケージ内のみ) | 2,592 (Lean core 込み) |
| import 単価 | — | **1,243 µs / module-load** |

実測の単価を全体 8,600 モジュールに掛けると、**環境ロードだけで約 9.1 時間の CPU 時間**になる
(§4.6.1 の外挿)。

一方バッチ方式なら、この項は **1 回分の closure ロード (実測 12.9 秒)** に潰れる。
**これが構想の中心仮説「依存ライブラリの規模から切り離せる」の定量的な根拠である。**

---

## 6. 構想文書「9. 最初に答えるべき技術的質問」への回答

**Q1. Environment のロードは全体時間の何割か**
→ **Mathlib 依存プロジェクトの実ビルドで 85.0%** (3,590 モジュールの実測, §4.6.1)。
単一モジュールを単独で測ると 89.6〜97%。1 プロセスにまとめると 28% まで下がる (§4.2, §4.3)。
ただし Lean core の `genCore` だけは例外で、そこでは環境ロードは 1.3% にすぎず、意味解析が 94% を占める (§4.1)。
つまり「環境ロードが支配的」なのは *downstream パッケージ*の話であり、
*自パッケージの宣言が多い*ケースでは意味解析が支配的になる。**両方に手を入れる必要がある。**

**Q2. 1 プロセスで複数モジュールを処理するとどれだけ短縮できるか**
→ **同一 432 モジュールで 1,084 秒 → 47.4 秒、約 23 倍** (§4.3)。
仮説は支持された。さらに `collectTactics` の O(n×m) を直せば 47.4 秒のうち 16.4 秒が消える。

**Q3. `DocInfo` 相当の意味解析で最も高価な処理は何か**
→ **`DocInfo.ofConstant`、1 宣言あたり 2.55 ms。**内訳は型の pretty print と
`computeEquations?` (`getEqnsFor?` による equational lemma の生成)。
後者はヒートビート上限に達して警告を出すほど重い (§4.1)。

**Q4. equation 情報を省くとどれだけ速くなるか**
→ **対象パッケージの性質に強く依存する。**InformationTheory (定理中心) では
意味解析部分で 7%、全体で 2% しか効かなかった (§4.7)。equation lemma は `definition` にしか
生成されないため、定理が大半のライブラリでは削っても効果が薄い。
一方 Lean core のような定義中心のコードでは、equation 計算がヒートビート上限に達するほど重い。
**「全体で何割」という単一の数字は存在しない**ので、切り替え可能な段階として設計するのが正しい。

**Q5. 外部リンク解決に必要な依存側データは何か**
→ `fromDb` が使うのは `LinkingContext` の 3 つだけ (`DB.lean:475`):
`moduleNames` / `sourceUrls` (モジュール → ソース URL) / `name2ModIdx` (宣言名 → モジュール)。
Lean core 2,394 モジュールで `name2ModIdx` は **100,535 エントリ**、構築に 0.32 秒。
**依存パッケージについて必要なのは「宣言名 → モジュール名 → URL」の写像だけ**であり、
型・docstring・equation は一切要らない。これは構想 3.1 の「依存は外部参照にする」を
そのまま裏づける (Mathlib の完全な IR を持つ必要がない)。

**Q6. `.olean` hash だけで安全にキャッシュを無効化できるか**
→ **できない。**現行 Lake は olean トレースで無変更をよく検出できている (2 回目 2.33 秒, §4.5) が、
olean hash だけでは不足で、少なくとも pretty printer の出力に影響する
**Lean バージョン**と**抽出器バージョン**が必要 (構想 3.4 の設計で正しい)。

より重要なのは、現行実装が増分性について**両方向に壊れている**ことである。

- **取りこぼし**: モジュールを変更しても `fromDb` が再実行されず、HTML が更新されなかった (§4.5)。
  lakefile は「空のマーカーファイルのトレース変化で再ビルドを誘発する」設計だが、
  空ファイルの内容ハッシュは変わらない。
- **無駄**: 逆に `fromDb` が呼ばれた場合は、変更の有無に関わらず**全モジュールを再生成**する (§4.6.3)。

新設計では、キャッシュキーの設計 (無効化しすぎない) と、
**出力側の増分性 (変更ページだけ書き直す)** を別々に設計する必要がある。
現行はどちらも「マーカーファイル」という 1 つの仕組みに載せようとして両方失敗している。

**Q7. Lean version と extractor version の変更をどう検出するか**
→ 現行 doc-gen4 はこれをキャッシュキーに入れていない (空マーカーファイル方式)。
新設計では IR のヘッダに埋めるべき項目として確定 (構想 3.4 の方針で妥当)。

**Q8. exported name, alias, generated declaration をどの段階で IR へ入れるか**
→ 現行は書き込み時に解決している。`updateModuleDb` が `saveRecursors` で
`.rec` / `.casesOn` / `.recOn` / `.brecOn` を `internal_names` に登録し、
構造体の射影関数も同様に登録する (`DB.lean:598`)。
`Eq.ndrec` / `HEq.ndrec` は名指しの特別扱い。
**この解決を抽出時に済ませておかないと、レンダリング側が環境を必要としてしまう**ため、
IR には解決済みで入れるのが正しい。

**Q9. source location を安定して取得できるか**
→ 取得できている。`findDeclarationRanges?` の結果を `declaration_ranges` テーブルに保存し、
ソース URI は Lake の facet (`srcUri`) が git remote + commit から組み立てて
`single` に引数で渡す。範囲を持たない宣言は `isBlackListed` で除外される
(`Process/DocInfo.lean:155`)。

**Q10. IR を JSON から SQLite やバイナリへ変える必要が出る規模はどこか**
→ 現行 doc-gen4 は既に SQLite であり、そこが**遅い側**になっている (§4.4 で HTML 生成 CPU の 78.4%)。
教訓は「規模が来たら SQLite」ではなく、**アクセスパターンに合わない粒度で SQLite を使うと逆効果**ということ。
doc-gen4 は宣言ごとに行を分割したため、レンダリング時に N+1 クエリで再構築する羽目になっている。
モジュール単位で読んでモジュール単位で書くなら、**モジュール 1 個 = 1 ブロブ**が正しい粒度で、
索引だけを別に持てばよい。

---

## 7. 新設計への設計指針 (計測から導かれるもの)

効果の大きい順。括弧内は本レポートでの根拠。

1. **抽出は 1 プロセス 1 環境のバッチにする** (§4.3, §4.8)
   最大の一手。実測 23 倍。副次的にメモリ律速も消える。
   `analyzeConcreteModules` は既に `Array Name` を受けるので、doc-gen4 の中でも
   CLI を変えるだけで実現できる (本レポートの `batch` コマンドがその実証)。

2. **環境の全定数を走査しない** (§4.6.1)
   走査コスト 1,195 秒 > 意味解析 631 秒。該当率 0.032%。
   `env.constants` を舐めるのではなく、`getModuleIdx?` から対象モジュールの宣言だけを引くべき。
   これは 1 と独立に効く (バッチ化しても走査は 1 回残るため)。

3. **依存パッケージからは「宣言名 → モジュール → URL」だけを取る** (§6 Q5)
   `fromDb` が実際に必要としているのは `LinkingContext` の 3 要素だけで、
   型も docstring も equation も要らない。構想 3.1 の外部参照方式はそのまま成立する。

4. **IR の粒度はモジュール単位のブロブにする** (§4.4, §4.6.2, §6 Q10)
   現行は宣言ごとに行を分割したため、レンダリング時に N+1 クエリで再構築しており、
   HTML 生成 CPU の 66〜78% がその再構築に消えている。
   読みも書きもモジュール単位なら、モジュール 1 個 = 1 ブロブ + 索引が正しい粒度。

5. **出力側の増分性を独立に設計する** (§4.6.3, §6 Q6)
   現行の `fromDb` は毎回全ページを再生成する。IR のモジュール単位ハッシュを持ち、
   変わったモジュールのページだけ書き直す設計にする。
   同時に「変更が伝播しない」取りこぼし (§4.5) も、マーカーファイルではなく
   内容ハッシュで判定すれば両方解決する。

6. **全モジュール横断の後処理をインクリメンタルにする** (§2.1)
   検索インデックス生成・navbar 生成・backrefs 収集・header-data 生成は、
   いずれも「ディスク上の全モジュール分を毎回読み直す」実装になっている。
   今回の規模では 1〜2 秒に収まっているが、モジュール数に線形なので
   構想の「ローカル開発では短い待ち時間でプレビュー」には障害になる。

7. **タクティク収集を O(モジュール数 × タクティク数) にしない** (§4.3)
   バッチ化すると即座に最大項 (36%) に浮上する。
   全タクティクを 1 回列挙してモジュール別に振り分ければ消える。

8. **equation 生成は切り替え可能にする** (§4.7)
   定理中心のライブラリでは 7% しか効かないが、定義中心のコードでは
   ヒートビート上限に達するほど重い。固定の判断をせず段階にする。

**あえて優先しなくてよいもの**: HTML のレンダリング自体は生成 CPU の 9.2% に過ぎない (§4.4)。
テンプレートエンジンの高速化や Rust 化は、ここでは効果が薄い。
構想 5 節が「言語選択より責務分離と計測を優先する」としているのは、この計測結果と整合する。

## 8. 再現手順

```bash
# 計装を有効にしてビルド (パッチ適用済みの doc-gen4 が前提)
lake build doc-gen4

# end-to-end (Lean core のみ)
DOCGEN_TIMING=$PWD/benchmarks/results/smoke.jsonl \
  lake build InformationTheory.Meta.EntryPoint:docs

# 単一モジュールのコスト構造
DOCGEN_TIMING=... lake env .lake/packages/doc-gen4/.lake/build/bin/doc-gen4 \
  single --build .lake/build <Module> bench.db "file:///tmp/x"

# 方式B (バッチ)
DOCGEN_TIMING=... lake env .lake/packages/doc-gen4/.lake/build/bin/doc-gen4 \
  batch --build .lake/build bench.db "file:///tmp/x" $(cat benchmarks/results/it-modules.txt)

# 方式A (逐次 1 プロセス/モジュール)
benchmarks/tools/run-serial.sh

# フルビルド (第1引数 = LEAN_NUM_THREADS, 0 で既定 / 第2引数 = ログ名)
benchmarks/tools/run-full.sh 0 full-build

# HTML 生成だけを単独で回す (既存の DB を使う。root 省略で DB の全モジュール)
DOCGEN_TIMING=... lake env .lake/packages/doc-gen4/.lake/build/bin/doc-gen4 \
  fromDb --build .lake/build --manifest /tmp/m.json .lake/build/api-docs.db

# 集計
deno run -A benchmarks/tools/analyze.ts <file.jsonl> [--by-module]
deno run -A benchmarks/tools/closure-sizes.ts .lake/packages/mathlib/Mathlib Mathlib
```
