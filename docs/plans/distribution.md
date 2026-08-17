# 配布 — composite action / Rust バイナリの Release / extractor プリビルドの可否

**状態**: **完遂 (2026-08-18)**。D1〜D4 すべて決着 — D2/D3 は公開して実走で緑、
D4 は「配れると確認した上で配らない」判定。起票時点では v0.1.0 に「ソースから建てる」しか
配布経路が無く、README がユーザーに `checkout` 2 回とキャッシュ鍵 3 つを書かせていた。

| 段 | 結果 |
|---|---|
| **D1** 検証 | X1/X3/X4/X5/X6 すべて実測。extractor は **toolchain だけで決まり、可搬で、不一致では静かに壊れない** |
| **D2** Release | **`v0.1.3`** — Linux musl (静的) / macOS arm。書庫に `LICENSE` + `NOTICE` |
| **D3** action | **`FujiHaruka/lean-doc@v0.1.3`** — 利用側は 5 行。self-test 5 ジョブ |
| **D4** extractor | **配らない**。226 MB vs 16 s のビルド、しかも CI はキャッシュする |

**この作業で見つけた欠陥は 7 件** (うち product 0 件 / 配布経路と検査 7 件) — 内訳は各段の
結果ブロックに。**最初から緑だったものは 1 つも無い。**

---

## 1. Context — 配るものは 3 つで、2 つの性質は真逆

| 配るもの | サイズ | ビルド | 依存 |
|---|---:|---:|---|
| `lean-doc` (Rust) | **2.5 MB**【実測 2026-08-17】 | **~24 s**【実測 CI, run 31955883894】 | `serde_json` + vendored md4c。**自己完結** |
| `extract` (Lean) | **171 MB** / gzip **45.9 MB**【実測 2026-08-18】 | **~16 s**【実測 2026-08-15】 | **対象の toolchain**。`lake env` で借りる |
| 使い方 (CI) | — | — | README の 30 行 YAML をコピペ + `checkout` 2 回 |

- `strip -S` は **8 バイトしか減らない**【実測 2026-08-18】 — 171 MB はデバッグシンボルではなく
  Lean ランタイムの**実体**で、削る余地が無い。
- したがって **Rust 側はプリビルド配布が圧倒的に得** (2.5 MB のダウンロードが 24 s のビルドと
  数百 MB の cargo キャッシュを消す) が、**Lean 側は得かどうか自明でない**
  (45.9 MB のダウンロード vs 16 s のビルド、しかも toolchain × OS で掛け算になる)。

`.github/workflow-templates/lean-doc-docs.yml` は**配布経路として機能していない** — GitHub の
starter workflow は org の `.github` リポジトリ専用で、個人リポジトリに置いても他人からは
見えない。ファイル自身も "inert where it sits" と書いている。

---

## 2. Approach

> **検証を先に置き、配布物は「隠す → 消す → Lean も消す」の順で強い主張に上げる。**

**なぜこの順か。** 段 3 (extractor が toolchain だけで決まるか) の答えが、段 1 の action が
extractor を「ビルドする」のか「ダウンロードする」のかを決める。逆順で作ると action を
2 回書き直すことになるので、**X 検証 → Rust Release → action** の順で進める。

**検証に新しい機材を用意しない。** 同一 toolchain (`leanprover/lean4:v4.31.0`) で依存セットが
極端に違うパッケージが既に 3 つある — `e2e/micro` (Lean core のみ) / `e2e/micro-dep` /
`lean-projects` (Mathlib 全体)。`ci-build.sh` が持っている「別パッケージで byte 一致」の実測は
**2 つの依存セットが互いのコピー**だったので証拠として弱い、と自分で注記している。この 3 つなら
その穴が塞がる。**ディスク追加ゼロ**で、Linux 側は既存の CI ジョブ `e2e-micro` が同じことを
毎 push でやっているので、そこに足すだけで済む。

**各段は独立して価値がある。** 段 3 が「否」でも段 1・段 2 は残る。段 2 が倒れても段 1 は残る
(action の中で `cargo build` にフォールバックする形を最初から持つ)。**新しい単一障害点を
作らない**のがこの設計の要で、ダウンロードは常に「取れなければ建てる」。

**新しい配布経路は新しい未検証項目**なので、CLAUDE.md の規律をそのまま適用する — 作った
ゲートは**必ず一度落として**から通す、走った本数を数える、`skip` で緑を返さない。

---

## 3. 段の内訳

### D1 — extractor の一意性を測る (先行)

**問い**: `extract` は「対象の toolchain だけ」で決まるのか、それとも対象の依存セットにも
依るのか。答えが「toolchain だけ」なら、toolchain ごとに 1 つプリビルドを配れる。

| | 検証 | 機材 | 状態 |
|---|---|---|---|
| **X1** | 同一 toolchain・**異なる依存セット**でビルドした `extract` が byte 一致するか (`e2e/micro` / `micro-dep` / `lean-projects`) | 手元 (macOS arm64) | **真**【実測 2026-08-18】 |
| **X2** | ~~交差実走 — IR が byte 一致するか~~ | — | **不要になった** (下記) |
| **X3** | X1 を Linux で再現 | CI (`workflow_dispatch`) | **真**【実測 2026-08-18】 |
| **X4** | **可搬性** — job A でビルドした `extract` を artifact 経由で job B (別 runner) に渡し、動くか | CI | **真**。ただし runner は全部 `$HOME=/home/runner` → X6 が要った |
| **X5** | **不一致の壊れ方** — 別 toolchain の環境で走らせると何が起きるか (静かに壊れるか、落ちるか) | CI | **明確に落ちる**【実測】 |
| **X6** | `ELAN_HOME` を別の場所に置き、`~/.elan` が無い環境で動くか (X4 で埋め込みパス 17 件が残った) | CI | **真**【実測】 |

**X1 の結果**【実測 2026-08-18 → [`../../benchmarks/results/extractor-uniqueness-2026-08-18.txt`](../../benchmarks/results/extractor-uniqueness-2026-08-18.txt)】:
依存パッケージ **0 個 / path 依存 / 15 個 (Mathlib 全体)** の 3 環境と、**昨日別セッションで
ビルドしたもの**の計 4 本が **byte 一致** (`226d49ab…`)。生成される C も一致。

**X2 が不要になった理由**: X1 が真なので「A でビルドしたものを B で走らせる」は
「**同じバイナリ**を B で走らせる」と同義になった。残る問いは出力の同一性ではなく**可搬性**
(X4) — そちらは `otool -L` が答えを出した: 動的リンクは `libc++` / `libSystem` の 2 つだけで
**Lean ランタイムは静的リンク**、`LC_RPATH` 無し、埋め込みの絶対パスは実行に使われない
`lean.h` の 1 件のみ。**macOS ではバイナリは可搬**。

**残る問いは Linux に移った** — `leanc` の既定が違えば `libleanshared` に動的リンクする
可能性があり、そこが X3 / X4 / X5 の実体。**手元に別 toolchain を入れない** (ディスク) ので
CI で測る。

**X1 が偽なら D4 は無い** (プリビルドは配れない)。**X4 が偽でも D4 は無い** (byte が同じでも
持ち運べないなら配れない)。**X5 が「静かに壊れる」なら D4 は危険** — 配る前に版検査が要る。

X2 で比べるのは「バイナリの byte」ではなく「**出てくる IR の byte**」。同じ入力に同じ出力を
返さないバイナリは、byte 一致していても配る意味が無い (逆に、byte が違っても IR が同じなら
配れる可能性が残る)。

### D2 — Rust バイナリを GitHub Releases に置く

- **`.github/workflows/release.yml`** — tag push (`v*`) で走り、Release にアセットを付ける。
- ターゲット (優先順):
  1. `x86_64-unknown-linux-musl` — **最優先**。静的リンクで glibc の版問題が消える
  2. `aarch64-apple-darwin` — ローカル開発の主戦場
  3. `aarch64-unknown-linux-gnu` — arm runner 向け。余裕があれば
- **`checksums.txt`** を同梱。
- **書庫の中身は `lean-doc` + `LICENSE` + `NOTICE`**【ライセンス上の要件、2026-08-18 に判定 →
  `provenance.md` §4】。バイナリ配布は Apache-2.0 §4 の **Object form** に当たるので (a) が
  発動し、**md4c の MIT は「参照」では運べない** — `NOTICE` は全文を `vendor/` に指していただけで、
  書庫に vendor/ は入らない。**同日 `NOTICE` に md4c の MIT 全文を入れて塞いだ**。
- **検証**: musl 静的リンク + **vendored md4c (C)** が通るかは未検証。`.tar.gz` を作って
  終わりにせず、**そのバイナリで e2e を 1 本回す** (`--version` が出ることは動く証拠ではない)。

> **結果 — 完遂**【2026-08-18】。`v0.1.1` / `v0.1.2` を公開。
> - **musl は通った**。静的リンクは実測で確認 (`ldd` が共有オブジェクトを 1 つも解決しない)。
>   **この検査自体が最初に落ちた** — `ldd` の文言 (`not a dynamic executable` /
>   `statically linked`) で判定していたため。いまは `=>` の有無で見る
> - 書庫: **Linux musl 1,146,144 B / macOS arm 961,119 B**【実測】。中身は
>   `lean-doc` + `LICENSE` + `NOTICE` (ライセンス上の要件、上記)
> - **smoke が本物**: 書庫を展開して `tools/e2e-micro.sh` を完走 (one command /
>   idempotence / determinism)。さらに **README の手順を手元でそのまま実行**して
>   `latest/download` → 展開 → `--version` まで確認
> - **Intel macOS は落とした** — `macos-13` runner が 15 分以上 queued のまま起動せず、
>   arm からのクロスビルドは「**誰も実行していないバイナリを配る**」ことになる。
>   `aarch64-unknown-linux-*` も同じ理由 (テストする arm Linux runner が無い)
> - **publish は 1 度落ちた**: `cd $RUNNER_TEMP/dl` してから `gh` を叩いたので
>   git 木の外になった。`--repo` を渡し、**アセットが実際に付いたかを問い直す段**を足した

### D3 — composite action を出す

- **ルートに `action.yml`** (Marketplace 掲載にはルート必須。掲載自体は後で良く、
  `uses: FujiHaruka/lean-doc@v0.1.1` は掲載と無関係に動く)。
- 中身は `tools/ci-build.sh` を呼ぶだけ。キャッシュ鍵 3 つを内側に隠す。
- **Pages への publish は入れない** — 責務が別。`site` を output で返す。
- **ハマりどころ (設計に織り込む)**:
  - **`hashFiles()` は workspace の外を読めない**。action 自身の `Cargo.lock` は
    `$GITHUB_ACTION_PATH` 配下 = workspace 外なので、**自前で `sha256sum` して
    `$GITHUB_OUTPUT` に出す**。`github.action_ref` は `@main` 参照で鍵が動かなくなるので使わない
  - `actions/cache` の `path:` は絶対パスなら workspace 外を指せる (`${{ github.action_path }}/target`)
  - composite の中から `uses:` は呼べる。その post ステップ (cache の save) も走る
  - **`lake build` を含めるかは input で選ばせる** (`lake-build`, 既定 `true`)。
    `leanprover/lean-action` を既に使っている利用者と組み合わせられるように
- **バージョン運用**: `v0.1.3` を固定用。可動 `v0` はまだ作っていない (作るなら
  「最新の 0.x」として付け替える)。

> **結果 — 完遂**【2026-08-18】。`action.yml` (ルート) + `.github/workflows/ci-action.yml`
> の 5 ジョブ。**`hashFiles()` が workspace 外を読めない**件は `keys` 段で自前に
> `sha256sum` して回避 (`github.action_ref` は `@main` で鍵が凍るので使わない)。
> - **自己テストが実欠陥を 1 件出した**: 同じ job で 2 回呼ぶと、**2 回目の
>   `actions/cache` restore が 1 回目の出力を古い commit の状態で上書き**していた
>   (2 回目が全 6 ページ再描画。product は無傷で、手元の 2 回は 0)。
>   sentinel で **初回だけ restore する**形に直し、4 つの cache すべてに同じガードを入れた
>   (一般形に引き上げ — CLAUDE.md「欠陥を直すとき」)
> - **わざと落とす経路**が緑: 存在しない `root` で action は失敗する
> - **`uses: FujiHaruka/lean-doc@main`** が緑 = `_actions/` への展開という実配布経路が動く。
>   これで README の未検証項目「利用者リポジトリの checkout」も実質潰れた
> - **Release からバイナリを取る経路も実走した** — `uses:@v0.1.3` で
>   **`binary-source = release`**【実測】。`binary-source` output を足したのはこのため
>   (ログを読むのではなく assert するため)。**ここで欠陥が 2 つ出た**:
>   (a) **`GITHUB_ACTION_REF` は composite action の `run` 段では空**
>   (`ref=''` と印字された。context 式 `github.action_ref` には来るので env で渡す)、
>   (b) ref が tag かどうかだけで判定すると **`@main` が古い Release のバイナリを掴む** —
>   いまは **ref がツリーの `Cargo.toml` の版と一致するときだけ**取る。
>   あわせて「`target/release/lean-doc` があれば使う」も**やめた**: そのファイルは
>   cargo キャッシュ由来かもしれず、鍵は `Cargo.lock` なので**ソースが動いても動かない**
>   (`ci-build.sh` が一度払った失敗)
> - **self-test 自身も 1 度直した**: 「1 回目は必ず描画する」が**状態キャッシュの中身に
>   依存**していた (前の run の状態が復元されると 0 描画が正しい)。`full: true` で決定化

### D4 — extractor のプリビルド配布 → **配れる。しかし配らない**【決定 2026-08-18】

D1 の 5 検証はすべて通った。**配布の前提条件は 3 つとも満たしている**:
toolchain だけで決まり (X1/X3)、可搬で (X4/X6)、不一致は静かに壊れず落ちる (X5)。

**それでも配らない**。理由は数字で、好みではない:

| | |
|---|---|
| ビルド | **15〜16 s**【実測】。しかも CI ではキャッシュが効くので**初回だけ** |
| 配布物 | **Linux 226 MB / macOS 171 MB** (gzip 45.9 MB)【実測】。**strip で 8 バイトしか減らない** |
| マトリクス | toolchain × OS。**Lean が版を出すたびに増え、追随しないところに穴が開く** |

16 秒を 45〜70 MB のダウンロードに置き換えても勝たない。**維持コストが利得を上回る**、が結論。

**将来やるときの手順は揃っている** — `.github/workflows/ci-extractor-portability.yml` が
X3〜X6 を回すので、`release.yml` にアセットを足し、action の取得順を
「Release → 無ければ建てる」にするだけ (Rust 側と同じ形)。**前提が崩れる条件**:
Lean が extract を桁で小さくしたとき、または環境ロードが重くなってビルドが 16 秒で
済まなくなったとき。→ 数字は
[`../../benchmarks/results/extractor-uniqueness-2026-08-18.txt`](../../benchmarks/results/extractor-uniqueness-2026-08-18.txt) §5。

---

## 4. ディスクの規律【ユーザーからの明示指示 2026-08-18】

CLAUDE.md の事故 (24 GB でディスクが満杯 → 対象の olean が 1 つ欠落) を繰り返さない。

- 起点の空きは **52 Gi**【実測 2026-08-18】。手元の大物は `target` 4.3 GB / `extractor/build` 176 MB /
  `fuzz` 約 140 MB。
- **X1 / X2 で作る `extract` のコピーは 171 MB × 3**。作業領域は
  `/private/tmp/lean-doc-relay/dist/` に集約し、**各検証の終わりに消す**。掃除の主体はこの計画。
- **新しい Lean パッケージを clone しない・新しい toolchain を手元に入れない**。
  X3〜X5 は CI に置く (runner のディスクは使い捨て)。
- 各 leg の終わりに `df -h /` を記録する。**10 Gi を切ったら作業を止めて掃除する**。

> **結果**【実測 2026-08-18】: 起点 **52 Gi** → 終了 **51 Gi**。ピークは一時的に
> +684 MB (`extract` 4 コピー) で、X1 が終わった時点で消した。**X3〜X6 を CI に寄せた
> ので、手元に toolchain も Lean パッケージも追加していない** — 別 toolchain
> (v4.30.0) は runner の使い捨てディスクに入っている。生成したサイトと IR は各実験の
> 直後に削除。**予定より使わずに済んだ**。

---

## 5. やらないこと

- **crates.io publish** — path 依存で 6 crates 全部の publish が要り、
  root `Cargo.toml` の `publish = false` (「誰も import しない境界」) を壊す。
- **Homebrew / apt / Docker イメージ** — 利用者は Lean 開発者で elan を既に持っている。
  Docker は Mathlib の olean キャッシュと二重の配布経路になり、メンテコストに見合わない。
- **Windows** — README で未対応を宣言済み。
- **Marketplace 掲載** — `uses:` は掲載なしで動く。掲載は配布経路が実走で緑になってから。

---

## 6. 撤退ライン

- **X1 が偽** (依存セットで byte が動く) → D4 を捨て、action は「建てる + キャッシュ」のまま。
  この文書に数字を残して終わり。
- **musl で md4c が通らない** → `x86_64-unknown-linux-gnu` に落とす。glibc の版問題は
  ubuntu-latest でビルドして「ubuntu-22.04 以降」と明記する形で受ける。
- **action の実利用テストが緑にできない** → `action.yml` を出さない。半分動く配布経路を
  README に書くのは、今の「コピペ」より悪い。
