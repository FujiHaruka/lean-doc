# stage 7e-rev — §8 の revision 行に残っていた未検証 2 点を閉じる

`approach.md` §8「revision をページのバイトに埋めるか」が「未検証」として残していた:

* **点 A** — `coverage.ts` の受け入れオラクルは**注入後**に回せば成立するか
* **点 B** — `jump-src.js` は実行時に `href` を読むのか、注入の順序制約は何か

数字とその条件は `benchmarks/results/stage7e-rev-summary.txt` (SoT)。
ここには**何をやったか**だけを書く。

## この段階が触っていないもの

`experiments/stage1`〜`stage7g` は **1 行も変えていない**。この段階は
`stage7d/render.ts` と `stage4c/coverage.ts` を**呼ぶだけ**で、写しも作らない
(比較対象が同一実装であることが点 A の前提なので、写して分岐する余地を作らない)。
**Lean は 1 度も走らせていない** — IR は段階 7d の `w7d/ir-j4` を再利用する。

## 中身

| ファイル | |
|---|---|
| `run.sh` | 点 A。同一 IR から (i) 具体 rev / (ii) `{{SOURCE_URL}}` の 2 木を描き、(ii) の写しに置換注入して (ii-injected) を作り、`/usr/bin/diff -r` で突き合わせ、3 つに `coverage.ts` をかける。注入は段階 6b V4/V6 と同じ実装で 5 回 |
| `jump-src-probe.ts` | 点 B。doc-gen4 の `static/jump-src.js` の**バイトそのもの**を、実在の生成ページの**本物のパース結果** (deno-dom) に対して評価し、`location.replace` に渡る文字列を捕まえる |

## `jump-src-probe.ts` が何ではないか

**ブラウザではない。** 本物なのは HTML のパースと `getElementById` /
`querySelector(".gh_link a")` / `getAttribute("href")`。
`document.location` / `window.location.replace` / DOMContentLoaded の発火は**シム**。

だからこれが答えるのは「属性から**どの文字列が読み出され、どこへ解決されるか**」まで。
「ブラウザで module script とリスナが**どの順で走るか**」は答えない。
summary はこの 2 つを別の節に分けてある (B-2 と B-3)。

## 再現手順

```sh
W=/private/tmp/lean-doc-relay/w7e \
IR=/private/tmp/lean-doc-relay/w7d/ir-j4 \
LIDX=/private/tmp/lean-doc-relay/w7c/linkindex/link-index.lidx \
  experiments/stage7e-rev/run.sh

deno run --allow-read --allow-net --allow-env experiments/stage7e-rev/jump-src-probe.ts \
  --js /Users/haruka/dev/lean-projects/.lake/packages/doc-gen4/static/jump-src.js \
  --page $W/pages-ph/InformationTheory/Asymptotic.html \
  --page-url https://fujiharuka.github.io/information-theory/InformationTheory/Asymptotic.html \
  --id InformationTheory.Asymptotic.DotEq
```

生ログ: `benchmarks/results/stage7e-rev-*`。
