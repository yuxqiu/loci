# ICU4X word segmentation as a CJK alternative

Question: is there a library that segments CJK text consistently for queries and
documents, avoiding the per-run language routing of m0 §9.3 (see research/07 §4,
D74, D100)?

## Candidates

- **charabia** (Meilisearch, MIT) has loci's design: jieba for Chinese, lindera
  IPADIC for Japanese, ko-dic for Korean. It picks the language with `whatlang`,
  which calls text Mandarin unless about 5% of it is kana, so kanji-only text such
  as `東京` is treated as Chinese. Meilisearch's fix (v1.10) is explicit language
  declarations per field (`localizedAttributes`) and per query (`locales`). Its docs
  say Chinese and Japanese are hard to tell apart automatically and should not
  share a field. Sources: charabia README; Meilisearch docs "Handling multilingual
  datasets"; meilisearch issues #3565 and #4828; PR #3347; release v1.10.0.
- **ICU4X `icu_segmenter`** (Unicode-3.0) has one word segmenter for all scripts.
  `WordSegmenter::new_auto` uses its dictionary model for Chinese and Japanese
  (docs.rs, `WordSegmenter`). That model is ICU's `cjdict`: one shared
  Chinese/Japanese word list with frequencies, used because the two scripts share
  characters. ICU's LICENSE says it is built from Libtabe (BSD-style) and IPADIC
  (permissive), with no CC-CEDICT, so it can ship in an MIT binary given those
  notices. ICU4X's own LICENSE (Unicode-3.0) does not reproduce them, so loci would
  ship them in `THIRD_PARTY_NOTICES`. Sources: unicode-org/icu `LICENSE`;
  unicode-org/icu4x `LICENSE`. ICU4X issue #4808 notes that a dictionary falls
  behind as new words appear.
- **Character bigrams** (Lucene's CJK analyzer; tantivy's n-gram tokenizer) are
  consistent by construction. D74 decided against a separate bigram index.

## Measurement (2026-10-31)

`icu_segmenter` 2.x, `new_auto`, compiled data, release build. Each pair below is a
query and a document. The first verdict is whether every query word appears among
the document's words. The second is whether the query shares a token with the
document under loci's tokenizer (zh, ja and ko features) plus the D100 IPADIC
alternatives.

| Query | Document | ICU4X | loci + D100 |
|---|---|---|---|
| 勉強 | 東京大学でアルゴリズムを勉強しています。 | yes | yes |
| 東京 | 東京に住んでいます | yes | yes |
| 日本語 | 日本語を勉強する | yes | yes |
| 機械学習 | 機械学習の勾配降下法 | yes | yes |
| トランスフォーマー | トランスフォーマーモデルを使う | no | no |
| 梯度下降 | 机器学习中的梯度下降算法 | yes | yes |
| 数组 | 在数组中查找元素 | yes (`数`/`组` on both sides) | yes |
| 二分查找 | 二分查找的时间复杂度 | yes | yes |

Without D100, loci misses the first four.

Observations:

- ICU4X segments queries and documents with one model, so kanji-only words never
  depend on a routing guess.
- Some of its splits are coarse or odd (`住`/`んで`/`いま`/`す`, `中的`, `数`/`组`),
  so ranking quality on real notes is untested.
- Neither approach matches a katakana word inside a longer compound. Only bigrams
  would.
- Size: the whole probe binary, with ICU4X's segmentation data, is 2.9 MB.
  research/07 measured loci's three dictionaries at about +136 MB.

Korean (same probe): ICU4X splits Hangul only at spaces, so `공부` matches none of
`도서관에서 공부했습니다`, `한국어를 공부하는 학생` or `저는한국어를공부합니다` (the
`cjk` fixture), and `학습` does not match `기계학습은`. ko-dic splits these today.

## Assessment

ICU4X could replace jieba + IPADIC (Chinese and Japanese) with one small dependency
and no routing between them, at an unmeasured cost in segmentation quality. It
cannot replace ko-dic: Korean words carry attached particles and endings, which
ICU4X does not split. Before switching, loci needs a retrieval comparison on real
notes (`loci bench` cases in Chinese and Japanese).

## Outcome

Adopted for Chinese and Japanese (D101): Han and kana runs go to ICU4X. Korean was
first kept on ko-dic (Nix binary 100 MB, it was 146 MB), then moved to syllable
bigrams (D102), which needs no dictionary: the release binary is 19 MB (101 MB with
ko-dic).
ICU4X keeps Korean words whole by design: UAX #29 treats Hangul as letters, and an
optional extension only breaks between Hangul and other scripts.
