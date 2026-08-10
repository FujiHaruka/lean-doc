# stage 7a — IR schema 3: `splitWhitespaces` の幅を IR に載せる

段階 7 の 1 点だけを扱う使い捨て実験。**目的は 1 つ**: doc-gen4 の
`splitWhitespaces` が書き換える空白を IR から再現できるようにして、
byte 再現率のうち 32.919 ポイントを占めていた原因を閉じる。

数字とその条件は `benchmarks/results/stage7a-summary.txt` (SoT)。
ここには**何をなぜ変えたか**だけを書く。

## なぜ新しいディレクトリなのか

stage4b / stage4c を書き換えず、両方の写しをここに置いた。CLAUDE.md の
「新しい段階は既存を壊さず新ディレクトリを足す (数字の再現性のため)」に従う。
具体的には:

* `experiments/stage4b` が書く IR は **schema 2** で、その 15.13 MiB /
  `writeIR` 0.549s / 読み出し 0.100s / 抽出 13.71s は `docs/approach.md` と
  `docs/verification-log.md` が引用している実測値。抽出器を直接書き換えると
  この列がもう再現できない。
* `experiments/stage4c/render.ts` の 63.587% と増分 3・4 のタイミングも同様。
  さらに **`coverage.ts` は受け入れオラクル**なので、生成側と一緒に触れる場所に
  置いておきたくない。stage4c は採点側ごとそのまま残し、生成側だけを写した。

したがって `experiments/stage4c/coverage.ts` は**この段階でも 1 行も変えていない**。
採点は前と同じプログラムで行っている。

## 中身

| ファイル | stage4b/4c との差 |
|---|---|
| `Extract.lean` | stage4b の写し。`Span` に `front` / `back`、`spanToJson` に 6 要素形、`irSchemaVersion` を 3 に、size report に `ws widths` 行 |
| `render.ts` | stage4c の写し。`applyWsHeuristic` を `applyWsWidths` に置換、`--ws-heuristic` を削除、schema ゲートを 3 に |
| `check-spans.ts` | stage4b の写し。schema 3 の幅の不変条件を追加 |
| `build.sh` / `run.sh` | stage4b の写し。出力名だけ差し替え |

## 機構 — 何が食い違っていたのか

`renderTagged` (`DocGen4/RenderedCode.lean:249-256`) は `.const` タグの本体が
素のテキストのとき `splitWhitespaces` (`DocGen4/Output/Base.lean:281-288`) を通す。
これは前後の空白を切り落として**同じ長さのスペース列として貼り直す**
(`"".pushn ' ' n`)。つまりタグ本文が `" =\n  "` なら出力は `" =   "` になる:

```
mine  (schema 2):  <a …#Eq>=</a>\n  <span class="fn">…
theirs (doc-gen4): <a …#Eq>=</a>   <span class="fn">…
```

長さは変わらないのでオフセットは 1 つも動かない。違うのは文字だけ。
schema 2 は `wsTrim` が計算した `(front, total, back)` のうち**狭めた後の span
しか書いていなかった**ので、消費側には「どこまでがタグの中だったか」が残らない。
schema 3 はその 2 つの幅を一緒に書く。それだけ。

`front` / `back` は `kind 1` のうち少なくとも一方が非ゼロのものにしか付かない
(29,248 / 142,181 span)。単位は UTF-16 code unit — 空白はすべて ASCII なので
文字数と一致するが、`start` / `stop` と同じ単位で読むことに意味がある。

## 推測では閉じない、の実測

stage4c の `--ws-heuristic` は「childless な `kind 1` span に隣接する空白の run を
丸ごとスペースにする」推測で、407 直して 100 壊した。理由は `check-spans.ts` が
数える通り:

* 幅を持つ run のうち maximal でないものは **0** — run 自体はタグの端で切れている。
* しかし **run を持たない** const span の辺のうち 135,316 か所に空白が隣接していて、
  そこを丸ごと塗ると **8,403 単位**を誤って書き換える。正しく書き換えるべきなのは
  3,417 単位しかない。

タグの外の空白とタグの中の空白は、テキストだけ見ても区別が付かない。
**幅を IR に載せる以外に閉じる方法はない** — これが schema 3 の存在理由。

## 落とし穴 (stage4c README の「Pitfalls」はすべてそのまま有効)

`render.ts` を触ったので 3 点検査を再実行済み: `stage7a-pitfall-recheck.txt`。

追加で 1 つ。**`applyWsWidths` は範囲が重なったら throw する。** 幅と span が
食い違った IR を読んだときに、それらしいバイトを黙って出すのが一番まずい。
