# 07 — CJK tokenization: dictionaries, licenses, binary size, tantivy 0.27 fit

Status: research + hands-on probe, 2026-10-30. Answers the `[unverified]` items in
DESIGN §7 (Search) and m0 spec §8.3 (binary-size cost, API fit) and the **[open]**
dictionary choice. Nothing here changes those documents; it informs a decision.

Probe setup: scratch crate (outside the repo) with `tantivy = "0.27.0"`,
`lindera = "6.2.0"` (`default-features = false` + one `embed-*` feature at a time),
`jieba-rs = "0.11.0"`, `tantivy-jieba = "0.21.0"`. rustc/cargo 1.101-nightly,
x86_64 Linux, `--release` with `strip = true`. The tokenizer under test follows
§8.3: split into script runs; Hangul → ko, kana → ja, Han-only → zh, others →
lowercase alphanumeric words; one position per token.

## TL;DR

| Language | Recommended default | Why | Binary cost |
|---|---|---|---|
| Chinese | **`jieba-rs` (bundled jieba `dict.txt`), `cut_for_search`, HMM off** | MIT-only data; best or tied-best recall in the probe; +2.8 MB vs +52 MB / +24 MB for lindera's jieba / CC-CEDICT; both lindera Chinese dictionaries carry CC BY-SA data | +2.8 MB |
| Japanese | **lindera `embed-ipadic`** | Permissive (NAIST/ICOT notice, shipped by Apache Lucene); 48 MB vs 200 MB for UniDic; NEologd/SudachiDict are 590–735 MB | +48 MB |
| Korean | lindera `embed-ko-dic` (only option) | Apache-2.0; also shipped by Lucene Nori | +86 MB, ~75 ms load |

- **API fit: works.** lindera 6.2 and jieba-rs 0.11 compile next to tantivy 0.27
  with no conflicts; a ~60-line loci-owned `Tokenizer`/`TokenStream` indexed and
  searched CJK + mixed text in a RAM index; **0 byte-offset mismatches** over all
  dictionaries. `tantivy-jieba` 0.21 also works with tantivy 0.27 (it depends only on
  `tantivy-tokenizer-api` 0.7, the same version tantivy 0.27 uses). `lindera-tantivy`
  5.0.1 pulls lindera 5 + tantivy 0.26 — not usable.
- **Size is the real cost.** zh+ja+ko with the recommended set: 6.7 MB → 143 MB
  (+136 MB). With lindera's jieba instead of jieba-rs: 192 MB.
- **Two recall problems no dictionary choice fixes** (see §4): Han-only Japanese
  queries are routed to the Chinese segmenter, and query/document segmentations of
  the same string can differ. Both suggest adding a bigram field alongside the
  dictionary field.

## 1. Licenses (primary sources)

| Dictionary | License | Redistribute in an MIT binary? | Source |
|---|---|---|---|
| IPADIC 2.7.0 (lindera `embed-ipadic`) | NAIST notice + ICOT Free Software terms: use/copy/distribute allowed; copyright notice and the NO WARRANTY section must accompany every copy | Yes, with notice in the distribution | [mecab-ipadic COPYING](https://github.com/taku910/mecab/blob/master/mecab-ipadic/COPYING); also in lindera's `NOTICE.txt`; shipped by Apache Lucene Kuromoji ([Lucene NOTICE.txt](https://github.com/apache/lucene/blob/main/NOTICE.txt)) |
| UniDic (lindera ships unidic-mecab **2.1.2**) | GPL-2.0 / LGPL-2.1 / BSD-3 triple license; choose BSD | Yes (BSD option), reproduce notice | [UniDic site](https://clrd.ninjal.ac.jp/unidic/en/); lindera `NOTICE.txt` |
| mecab-ipadic-NEologd | Apache-2.0 (+ IPADIC terms for the base) | Yes | [neologd repo](https://github.com/neologd/mecab-ipadic-neologd) |
| SudachiDict (lindera `embed-sudachidict`) | Apache-2.0 | Yes | [sudachi.rs](https://github.com/WorksApplications/sudachi.rs) (GitHub license metadata) |
| mecab-ko-dic 2.1.1 | Apache-2.0 | Yes, keep NOTICE | lindera `NOTICE.txt`; shipped by Lucene Nori ([Lucene NOTICE.txt](https://github.com/apache/lucene/blob/main/NOTICE.txt)) |
| CC-CEDICT (lindera `embed-cc-cedict`) | **CC BY-SA 4.0** | Allowed, but the embedded dictionary data (and any changes to it) must stay CC BY-SA with attribution. Whether that reaches the surrounding code is a legal question — not legal advice; CC does not recommend BY-SA for software | [cc-cedict.org wiki](https://cc-cedict.org/wiki/) ("licensed under a Creative Commons Attribution-Share Alike 4.0 License") |
| lindera `embed-jieba` (mecab-jieba 0.1.1) | Build scripts MIT; data = jieba `dict.txt.big` (MIT) **enriched with CC-CEDICT** pinyin/forms/definitions for ~22% of entries | **Same BY-SA obligations as CC-CEDICT.** Verified: the probe binary contains CC-CEDICT English glosses (e.g. `gradient/slope`) | [lindera/mecab-jieba](https://github.com/lindera/mecab-jieba); `lindera-jieba-6.2.0/NOTICE.txt` lists a CC-CEDICT CC BY-SA 4.0 notice |
| jieba-rs bundled `dict.txt` | MIT (jieba, © 2013 Sun Junyi) | Yes | [jieba-rs](https://github.com/messense/jieba-rs) (MIT); file diffed against [fxsjy/jieba `dict.txt`](https://github.com/fxsjy/jieba/blob/master/jieba/dict.txt): identical except one dropped line |

Consequence: **both lindera Chinese options bring CC BY-SA data into the binary.**
`jieba-rs` is the only measured Chinese option with purely permissive data.

## 2. Measured binary size and cold start

Release, stripped. Baseline = same probe binary with no dictionary (tantivy only).
Load = `load_dictionary("embedded://…")` + `Segmenter::new` (or `Jieba::new()`),
median of 5 process starts (warm page cache).

| Build | Binary | Δ vs baseline | zstd -19 | Load (ms) | Build time (dict, cached download) |
|---|---|---|---|---|---|
| baseline (tantivy 0.27) | 6.74 MB | — | 1.95 MB | — | 28 s (full) |
| + jieba-rs default dict | 9.51 MB | **+2.77 MB** | 4.27 MB | 65–175 (noisy; builds the prefix dict at runtime) | 4 s |
| + lindera cc-cedict | 30.40 MB | +23.7 MB | 7.20 MB | 1.4 | 8 s |
| + lindera ipadic | 54.76 MB | **+48.0 MB** | 10.6 MB | 0.6 | 9 s |
| + lindera jieba | 58.65 MB | +51.9 MB | 12.6 MB | 1.5 | 15 s |
| + lindera ko-dic | 92.83 MB | **+86.1 MB** | 17.8 MB | **~75** | 23 s |
| + lindera unidic (2.1.2) | 206.5 MB | +199.8 MB | 36.3 MB | 1.9 | 22 s |
| + lindera sudachidict | 599.8 MB | +593 MB | — | 1.5 | 49 s |
| + lindera ipadic-neologd | 742.2 MB | +735 MB | — | 2.0 | 58 s |
| ko-dic + ipadic + jieba-rs | 143.1 MB | +136.4 MB | — | ~75 total | — |
| ko-dic + ipadic + lindera jieba | 191.8 MB | +185.0 MB | — | ~75 total | — |
| ko-dic + unidic + cc-cedict | 315.3 MB | +308.6 MB | — | — | — |

Notes:
- lindera embeds dictionaries **uncompressed** (zero-copy load, hence ~1 ms), and
  includes all MeCab detail fields (POS, readings, glosses). `dict.words` (the
  details) is 50–60% of each dictionary (ipadic 32.7 of ~50 MB; ko-dic 39.5 of
  ~86 MB). loci uses only the surface form. A surface-only rebuild of a dictionary
  could roughly halve the cost — `[unverified]`, lindera 6.2 exposes no feature
  for it. jieba-rs stores its dict zstd-compressed (`include-flate`).
- ko-dic's ~75 ms load (vs ~1 ms for the others) is paid on every `loci find` if
  loaded eagerly; load it lazily, only when a Hangul run appears.
- Linking lindera with no `embed-*` feature adds nothing (dead code eliminated).
- **Nix build:** each `embed-*` build script downloads its tarball from
  `lindera.dev` at build time. In a network-less Nix sandbox, prefetch the
  tarballs (file names + md5 are in each `lindera-*/build.rs`) into
  `$LINDERA_BUILD_DICTIONARY_CACHE_DIR/6.2.0-fmt2/`; the build script skips the
  download when a valid tarball exists there (`lindera-dictionary-6.2.0/src/assets.rs`).
  Verified with a populated cache dir; not yet tried inside a Nix sandbox.

## 3. Segmentation quality for search recall

Probe corpus: 13 short docs (CS/math notes in zh/ja/ko, mixed with English, one
vocabulary list) and 47 queries (whole terms, sub-terms, OOV technical terms).
Metric: expected doc ranked first. **Illustrative only — tiny hand-written set,
not a benchmark.** ja/ko numbers below are from combined builds, since Han-only
queries route to the Chinese segmenter (§4).

| Combined build | Mode | zh (23) | ja (17) | ko (7) |
|---|---|---|---|---|
| no dictionary (whole run = 1 token) | — | 3 | 0 | 0 |
| ko + ipadic + lindera jieba | Normal | 19 | 12 | 7 |
| ko + ipadic + lindera jieba | Decompose | 21 | 12 | 7 |
| ko + ipadic + jieba-rs (HMM on) | `cut` / `cut_for_search` | 18 / 22 | 14 / 15 | 7 |
| ko + ipadic + jieba-rs (HMM off) | `cut` / `cut_for_search` | 19 / **23** | 10 / 11 | 7 |
| ko + unidic + cc-cedict | Normal / Decompose | 21 / **23** | 15 / **16** | 7 |

Observations:
- **Search mode helps recall.** `cut_for_search` emits sub-words plus the whole
  word (`神经网络` → `神经 | 网络 | 神经网 | 神经网络`), so both `神经网络` and
  `网络` match. The same holds for lindera `Mode::Decompose` on Chinese. loci uses
  OR'ed `TermQuery`s only (no phrase queries), so overlapping tokens cost nothing
  in correctness. Use search mode at index time; at query time either mode works.
- **jieba HMM hurts recall on technical text.** It joins OOV characters across word
  boundaries (`哈希表和红黑树` → `哈希 | 表和红 | 黑树`). With HMM off, OOV terms
  fall back to single characters, which still match per character.
- **OOV technical terms:** jieba `dict.txt` contains `卷积 倒排 张量 微调 神经网络
  注意力`, but not `哈希表 红黑树 区块链 梯度下降`. CC-CEDICT splits most OOV
  compounds into characters (`倒|排`, `卷|积`). This helps recall (it behaves like
  unigrams) but hurts precision. Both lindera (`load_user_dictionary_from_csv`) and
  jieba-rs (`add_word`) accept user words if needed later.
- **UniDic vs IPADIC:** UniDic short-unit words (`トランスフォーマー|モデル`,
  `量子|化`, `形態|素`) gave better sub-term recall than IPADIC
  (`トランスフォーマーモデル` as one token). At +200 MB, and with lindera on the
  2013-era 2.1.2 (current UniDic is 3.1.x), it is not worth being the default.
  NEologd merges into long tokens (`形態素解析`, `東京都`, `ハッシュテーブル`), which is
  worse for sub-term recall and is unmaintained (dictionary data 2020-08; last
  commit 2020-09-14).
- **Korean:** ko-dic splits particles and compounds well (`하강법|과`,
  `탐색|트리`); 7/7.

## 4. Issues not fixed by the dictionary choice

1. **Han-only Japanese queries go to the Chinese segmenter.** §8.3 routes a Han-only
   run to zh. A Japanese document run with kana (`機械学習の勾配降下法`) is
   segmented by IPADIC (`勾配`), but the query `勾配` is segmented by Chinese
   (`勾|配`), so it misses. The same thing happens with `日本語` → `日本|語` (jieba-rs)
   and `機械学習` → single characters (jieba-rs, HMM off; `機`/`習` are not in the
   simplified-Chinese dictionary). This is the main source of ja misses above.
2. **Query/document segmentation mismatch.** `トランスフォーマー` (query) vs
   `トランスフォーマーモデル` (document, IPADIC) never share a term.

Options (for a decision, not implemented): (a) a second field indexed with
character bigrams (the existing `segmenter = "bigram"` code path) and OR'ed at a lower
boost, which covers both issues at no dictionary cost; (b) for Han-only runs with
both zh and ja enabled, emit the union of both segmentations. (a) is simpler and
also covers OOV terms.

## 5. API fit with tantivy 0.27

Verified by compiling and running:
- `lindera::segmenter::Segmenter` is `Send + Sync`, and `segment(&self, Cow<str>)`
  takes `&self`, so `Arc<Segmenter>` inside a `Clone + Send + Sync + 'static`
  tantivy `Tokenizer` works.
- lindera `Token` has `byte_start`/`byte_end` relative to the input; with
  per-run segmentation, add the run's byte offset. jieba-rs 0.11 `Token` also has
  `byte_start`/`byte_end` (`start`/`end` are char offsets — do not use them for
  `offset_from`).
- Tokens borrow the segmenter (`Token<'a>`), so the simplest stream collects owned
  `tantivy::tokenizer::Token`s into a `Vec` and iterates over it. Probe pattern:

```rust
#[derive(Clone)]
struct LociTokenizer { zh: Option<Seg>, ja: Option<Seg>, ko: Option<Seg> }
type Seg = Arc<dyn Fn(&str) -> Vec<(usize, usize)> + Send + Sync>; // byte ranges in run
impl Tokenizer for LociTokenizer {
    type TokenStream<'a> = VecStream;               // Vec<tantivy Token> + cursor
    fn token_stream<'a>(&'a mut self, text: &'a str) -> VecStream { /* runs → segs */ }
}
```

- `segment` returns `LinderaResult`, but `TokenStream` has no error channel. The
  probe saw no errors. loci must decide whether to panic or degrade (the project
  prefers failing loudly).
- `lindera-tantivy` 5.0.1 depends on `lindera 5.0.1` + `tantivy 0.26.1`, so it
  cannot be used. `tantivy-jieba` 0.21 is compatible with 0.27, but it is
  Chinese-only, so loci's own run-splitting tokenizer is still needed.

## 6. Alternatives considered

| Crate | Notes |
|---|---|
| `jieba-rs` 0.11 (2026-09) | Recommended for zh. MIT, maintained, 2.8 MB, `cut_for_search`, byte offsets. |
| `tantivy-jieba` 0.21 | Works with tantivy 0.27 (tokenizer-api 0.7), but does only Chinese; not needed if loci wraps jieba-rs itself. |
| `vibrato` (Apache-2.0, maintained) | Fast MeCab-compatible Viterbi tokenizer, but no embedded dictionaries. loci would compile and ship a dictionary itself; no size advantage for the same IPADIC data `[unverified]`. |
| `sudachi.rs` (Apache-2.0) | Needs SudachiDict files at runtime. Via lindera `embed-sudachidict` it costs +593 MB. |
| lindera 6.2 (2026-09-26, MIT, active) | One API for ja/ko(/zh). Uncompressed embedding drives the size. |

## 7. Recommendation

- **zh:** jieba-rs bundled dict, `cut_for_search`, `hmm = false`. Not lindera
  jieba/CC-CEDICT: they are 9–19× larger and include CC BY-SA data.
- **ja:** lindera `embed-ipadic`. Revisit UniDic only if a surface-only build gets it
  under ~50 MB.
- **ko:** lindera `embed-ko-dic`, loaded lazily (75 ms).
- Default Nix binary with all three is ~143 MB (vs 6.7 MB without dictionaries).
  If that is too large, ship zh+ja by default (~58 MB), make ko a feature, and
  fall back to the bigram segmenter only when it is explicitly configured.
- Consider a parallel bigram field (§4) to fix the routing and mismatch misses.
- Include the IPADIC, ko-dic and jieba license notices in the distribution
  (`THIRD_PARTY_NOTICES`).
