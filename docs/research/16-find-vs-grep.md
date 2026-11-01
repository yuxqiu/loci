# 16 — `loci find` versus grep: what a search command adds for an agent

Status: research, 2026-11-01. Builds on research 07 (CJK tokenisation), 14
(progressive disclosure, tool search) and 15 (digests versus search). Their findings
are cited, not repeated: Cherny's "agentic search … outperformed everything" quote,
Cursor's +12.5% for semantic search, the LocAgent and CoSIL ablations, SWE-agent's
`search_dir` format and Agentless Table 2 are in research 15 §1.

Question: agents search well with grep/rg in a terminal. What is `loci find` for? Do
people who give agents BM25 search find it useful? How does each approach scale? If
loci keeps a search command, what is its value?

What `find` is today (checked in `crates/loci-core/src/search/mod.rs` at 668f5f9): an
in-RAM tantivy index built on every call over the formed notes (concepts, areas,
views, journal entries; `--kind` replaces the scope); fields title ×3, path ×1, body
×1, frontmatter excluded; the loci tokenizer (NFC, ICU word segmentation for Han and
kana, Hangul bigrams, lowercased letter/digit runs, no stemming); one
`BoostQuery(TermQuery)` per field and query token, all `Occur::Should` (OR); BM25 with
ties by path; top `-k` (10); `path:line: snippet` for the first body line holding a
query token. The framework rules (`share/loci/rules.md`) use it in "Adding a
concept" step 1 (search under every name) and step 4 (link it from the notes that
mention it), and in "Finding things". DESIGN §8 says skills depend only on
`loci find --json`.

Nothing here changes DESIGN, the specs, the rules or the skills. Conventions as in
research 12 to 15: `[unverified]` means I did not confirm the claim in a primary
source (the reason is given); `[design inference]` is my extrapolation; `[gap]` means
I looked and found nothing. **(summary)** marks a table number read through a
fetch tool that summarises the page rather than from the raw text; spot-check
before quoting it elsewhere. Every 2026 arXiv paper cited is a preprint.

## TL;DR

| Question | Finding | Strength |
|---|---|---|
| 1. What do production agents use? | **grep/rg first, everywhere.** Claude Code: glob/grep "just-in-time", "bypassing the issues of stale indexing" (Anthropic, Sept 2025); on Linux it now drops its own Grep/Glob tools and runs `grep`/`find` through Bash (embedded ugrep/bfs). Codex CLI's prompt: "prefer using `rg`"; no search tool. Cognition built a faster grep subagent (SWE-grep), not an index. **Ranked search is added, not substituted**: Cursor keeps grep and adds embeddings (+12.5% offline; +2.6% code retention online on repos of ≥1,000 files). Sourcegraph dropped embeddings for BM25-ranked search, at 100,000-repository scale. Nobody ships per-call BM25 to an agent as its main tool | Strong on practice; vendor numbers |
| 2. Does a ranked retriever beat iterative grep? | **For a frontier model on literal text, no; BM25 is the weakest option measured.** BrowseComp-Plus subset (GPT-5.4 nano): BM25 tool 32%, embeddings 45%, read+grep 61%, bash 73% (Li et al. 2026, Table 5). LongMemEval: grep beat vector retrieval in 10 of 10 harness–model pairs with inline results (Sen et al. 2026, Table 1). **Ranked retrieval wins** on vocabulary-gap queries with a dense retriever (BrowseComp-Plus: +10 to +21 points over BM25 per model), for small models that loop under grep (+9 to +13 points, 8B-class), and on broad recall (embeddings 56.7 vs grep 28.0 gold-evidence recall). BM25 itself gains most from an LLM rewriting the query (BRIGHT 14.5 → 27.0), which is what an agent does anyway | Several 2025–26 studies, mostly preprints; none on notes |
| 3. Scalability | **rg scans; `find` rebuilds.** Measured here (synthetic vault, 2.4 KB notes): rg 10 / 21 / 67 ms at 1k / 10k / 100k notes; `find` 133 ms / 0.95 s / 9.1 s with 1.2 GB RSS at 100k. Per-call rebuild is fine to ~10k notes and breaks before 100k; rg is fine past 100k (Cursor saw >15 s only on "very large monorepos"). **Output volume is grep's real cost**: a word in ~12% of notes prints 68 KB / 667 KB / 6.8 MB of lines; `rg -l` 4 / 43 / 446 KB of paths; `find` ~2 KB at any size | Measured; one machine |
| 4. CJK | **Segmentation buys precision and Korean inflection; grep buys substring recall.** Probe: rg `京都` also hits `東京都`, rg `言模` hits across the word boundary of `语言模型`; `find` does neither. `find 공부하다` hits `공부했다` through bigrams; rg does not. Where segmentation of query and note differ, rg still hits and `find` misses (research 07 §4) | Hand probe; no study `[gap]` |
| 5. What can only `find` do? | **Top-k ranking (context economy) and CJK word matching.** Kind scoping, title search, frontmatter exclusion and line snippets are each one rg flag or path. Exhaustive tasks (rules step 4: every note that mentions X) are a **grep** task; top-10 truncation is wrong for them. Stemming and "did you mean" have no agent evidence `[gap]`. The output format of a search tool matters as much as its engine (SWE-agent summarised vs iterative: 18.0 vs 12.0, research 14) | Mixed |
| 6. Today's defects | Reproduced: on a 1k-note vault, "breadth first search" ranked **none** of the 22 notes containing "breadth-first" in the top 10 (first at rank 19), behind notes whose *title* holds "first" or "search". Zero hits print nothing. Snippets of title matches repeat the H1 (the body includes it) | Measured |

**Recommendation (option f below):** make rg the agent's primary tool in the rules,
with concrete patterns for each loci task, and assign `find` the two things rg cannot
do: a ranked top-k over a large vault and CJK word matching. Fix the defects that make
`find` worse than rg at those (coverage-first ranking instead of plain OR, a loud
zero-hit result, no H1 snippets). Then run the `loci bench --agent` comparison
below, grep-only against find-available. **If find-available does not beat grep-only
on any case class by more than run-to-run noise, delete `find` (option a)**,
removing tantivy, ICU and the tokenizer, and keep `loci bench` at agent level only.
Reject adding embeddings now (option e): no model dependency, no evidence on notes,
and the vault sizes in scope do not need it.

## 1. What production agents use, and why

### Anthropic / Claude Code

- **"Effective context engineering for AI agents"** (Anthropic Applied AI, 29 Sept
  2025, §"Context retrieval and agentic search",
  https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents):
  "Claude Code is an agent that employs this hybrid model: CLAUDE.md files are naively
  dropped into context up front, while primitives like glob and grep allow it to
  navigate its environment and retrieve files just-in-time, effectively bypassing the
  issues of stale indexing and complex syntax trees." The trade-off is stated:
  "runtime exploration is slower than retrieving pre-computed data", and "the hybrid
  strategy might be better suited for contexts with less dynamic content".
- **"Building agents with the Claude Agent SDK"** (29 Sept 2025, now at
  https://claude.com/blog/building-agents-with-the-claude-agent-sdk, §"Gather context
  › Semantic search"): "Semantic search is usually faster than agentic search, but less
  accurate, more difficult to maintain, and less transparent. … we suggest starting with
  agentic search, and only adding semantic search if you need faster results or more
  variations." No numbers.
- **Current Claude Code tools** (https://code.claude.com/docs/en/tools-reference,
  §"Glob tool behavior", read 2026-11-01): "On macOS, Linux, and WSL, Claude Code leaves
  Glob and Grep out of the default tool set, and Claude searches with `find` and `grep`
  through the Bash tool instead. In Claude's shell those two commands run embedded
  versions of `bfs` and `ugrep`." The Grep tool, where present, "is built on ripgrep";
  Glob results are "capped at 100 files". So the reference agent for loci's users
  reaches a vault through shell grep, and `loci find` competes with a command it
  already knows.
- The "mostly vibes" account of dropping RAG is in research 15 §1. Both Anthropic
  statements compare grep with an **embedding** index; neither mentions BM25.

### Cursor

- **"Improving agent with semantic search"** (6 Nov 2025, https://cursor.com/blog/semsearch,
  §"Online A/B tests"; the offline +12.5% is in research 15): "one group's agent had
  access to semantic search while the other relied solely on traditional search tools
  like grep. … agent code retention increases by 0.3% when semantic search is
  available. This effect increases to 2.6% on large codebases with 1,000 files or more";
  "a 2.2% increase in dissatisfied follow-up user requests when semantic search was not
  available". Conclusion: "the combination of these two leads to the best outcomes".
  The gain grows with corpus size.
- **"Fast regex search: indexing text for agent tools"** (Vicent Marti, 23 Mar 2026,
  https://cursor.com/blog/fast-regex-search): "the agents just love to use grep";
  "there are specific queries which the model can only resolve by searching with
  regular expressions"; "Most Agent harnesses, including ours, default to using
  ripgrep"; "We routinely see rg invocations that take more than 15 seconds" in "very
  large monorepos"; "grep is one of the few Agent operations whose latency scales with
  the size and complexity of the code". The fix is a local trigram/sparse-n-gram index
  that **keeps regex semantics**, and it must be "very fresh, particularly when it comes
  to the model reading its own writes". The specific timings (16.8 s → 13 ms) are in an
  interactive widget I could not read `[unverified]`.

### Sourcegraph / Cody, Augment, Codex CLI, Cognition, Aider

- **Sourcegraph Cody** dropped embeddings (Isken, "How Cody understands your
  codebase", 15 Feb 2024, §"So what happened to embeddings?",
  https://sourcegraph.com/blog/how-cody-understands-your-codebase): code "has to be sent
  to a 3rd party (OpenAI)"; "creating embeddings and keeping them up-to-date introduces
  complexity"; "searching vector databases for codebases with >100,000 repositories is
  complex and resource-intensive". The replacement "incorporates an adapted form of the
  BM25 ranking function alongside other signals" and "requires zero additional config".
  This is the one production case of BM25 for LLM context, and it runs on a persistent
  server index (Zoekt: "fast substring and regexp matching … based on trigram
  indexing", https://github.com/sourcegraph/zoekt), not per call. No agent ablation was
  published `[gap]`.
- **Augment** ("Augment's Context Engine is now available for any AI coding agent", 6
  Feb 2026, https://www.augmentcode.com/blog/context-engine-mcp-now-live): "improved
  agent performance by 70%+ across Claude Code, Cursor, and Codex" on "300
  Elasticsearch PRs, each with 3 different prompts". Vendor-run, one repository,
  scoring method not described `[unverified]` as a measure of retrieval.
- **OpenAI Codex CLI** (github.com/openai/codex at cb1bc0a,
  `codex-rs/protocol/src/prompts/base_instructions/default.md:264`): "When searching for
  text or files, prefer using `rg` or `rg --files` respectively because `rg` is much
  faster than alternatives like `grep`." There is no content-search tool; `file-search`
  is fuzzy matching over paths.
- **Cognition SWE-grep** (16 Oct 2025, https://cognition.ai/blog/swe-grep): on
  embeddings, "The embeddings can even be counterproductive, as the agent can give too
  much weight to irrelevant information"; on agentic search, "Both Claude Code and
  Cline have noted that this works well for them, and up until today, we agreed", but it
  is "generally slow, requiring dozens of sequential roundtrips". Their answer is an
  RL-trained subagent using "grep, read, glob" with "up to 8 parallel tool calls per turn
  in a maximum of 4 turns". DeepWiki/Devin search not checked `[gap]`.
- **Aider** has a repo map and no content search index (research 15 §1).

### Notes and memory, not code

- **Letta** ("Benchmarking AI Agent Memory: Is a Filesystem All You Need?", 12 Aug 2025,
  https://www.letta.com/blog/benchmarking-ai-agent-memory): 74.0% on LoCoMo with files
  versus Mem0's reported 68.5%. Not grep-only: the agent has `search_files` (semantic)
  and grep, and "must start by calling search_files".
- **LlamaIndex** ("Did Filesystem Tools Kill Vector Search?", 13 Jan 2026,
  https://www.llamaindex.ai/blog/did-filesystem-tools-kill-vector-search): a filesystem
  agent beat RAG on correctness (8.4 vs 6.4, LLM judge) but was slower (11.17 s vs 7.36
  s) on 5 tasks; at 100–1,000 abstracts with BM25, "RAG outperformed the fs-explorer
  approach in terms of speed (substantially) and of correctness (slightly)". n = 5.
- **qmd** (tobi/qmd, README at 86d2365, https://github.com/tobi/qmd): a local markdown
  search CLI for agents, "BM25 full-text search, vector semantic search, and LLM
  re-ranking"; its fixture shows "bm25 ~0.50, vector ~0.70, and hybrid/full ~1.00" on 10
  queries over 6 files, with no grep baseline. Evidence that people build this, not that
  it helps.

**What 1 means for loci** `[design inference]`. Every agent loci targets already
greps. Vendors add a second, *ranked* tool only when it brings something grep cannot:
semantic matching across vocabulary (Cursor), or scale grep cannot reach (Sourcegraph,
Cursor's regex index). Per-call BM25 brings neither of the two big ones; whatever it
adds must come from ranking, field weights and tokenisation.

## 2. Evidence from agent benchmarks

### Grep against ranked retrieval inside an agent loop

- **Li et al., "Beyond Semantic Similarity: Rethinking Retrieval for Agentic Search via
  Direct Corpus Interaction"**, arXiv 2605.05242 (May 2026; Jimmy Lin, Wenhu Chen et al.).
  Checked in the HTML. Full BrowseComp-Plus, Claude Sonnet 4.6: Qwen3-Embedding-8B
  retrieval 69.0% against terminal tools only (grep, read, shell) 80.0% (§4.2). Subset
  of 100, GPT-5.4 nano (Table 5): BM25 32%, Qwen3-Embed-8B 45%, read+grep 61%, open bash
  73%, at 19 / 18 / 19 / 35 tool calls. Retrieval keeps one advantage: mean gold-evidence
  recall 56.7 against 28.0 (Table 4); grep wins localisation (48.4 vs 21.7). And grep
  scales badly with corpus size: from 100K to 200K documents tool calls rose from 38.5 to
  86.9 and accuracy fell 13.6 points; at 400K accuracy was 37.5% with 20 runs at the tool
  budget (§4.3 RQ4).
- **Sen et al., "Is Grep All You Need? How Agent Harnesses Reshape Agentic Search"**,
  arXiv 2605.15184 (May 2026). Checked in the HTML. LongMemEval-S, 116 questions,
  Table 1: with inline results grep beat vector retrieval in all 10 harness–model
  pairs (Opus 4.6 / Claude Code 76.7 vs 75.0; GPT-5.4 / Codex CLI 93.1 vs 75.9; Gemini
  3.1 Flash-Lite / Chronos 86.2 vs 62.9); when results were written to a file, vector
  won 5 of 10. The authors: "We do not claim that grep 'beats' vector in general", and
  domains where evidence is "rarely literal" may differ. Adding distractor sessions
  degraded neither method much (Fig. 1).
- **BrowseComp-Plus** (Chen et al., arXiv 2508.06600, Table 1, **summary**), same agent
  with a BM25 versus a Qwen3-Embedding-8B search tool (accuracy): GPT-5 55.9 vs 70.1; o3
  49.3 vs 63.5; Opus 4 15.5 vs 36.1; GPT-4.1 14.6 vs 35.4. "Qwen3-Embedding-8B reduces
  search calls by approximately 1–3 compared to BM25" (§4.7). Queries are multi-hop
  paraphrased clues, the strongest vocabulary-gap setting.
- **Bellam, "How Do Small Coding Agents Use Code Graphs and Embeddings?"**, arXiv
  2610.33958 (Oct 2026, **summary**), SWE-bench Verified, function Acc@10, grep-only vs
  grep+embeddings: Qwen3-8B 12.8 vs 26.3; Gemma-4-E4B 9.6 vs 21.8; Qwen3-32B 26.9 vs
  32.1; Qwen3-4B 24.4 vs 23.1. Grep-only Qwen3-8B "collapses into search loops". The
  abstract: the gain "falls to 4-5 points at 32B".
- **Bhola et al., "Code Isn't Memory"**, arXiv 2606.22417 (Jun 2026, **summary**): a
  structural index with Opus 4.7 versus an agentic-grep harness, resolve 50.4 vs 45.3
  (p = 0.087), localisation Acc@5 84.5 vs 75.3 (p = 0.080), fewer turns; the index
  "does not duplicate behavior that competent agentic grep already reaches". Structural,
  not BM25.
- **LSP versus grep**, arXiv 2608.13568 (Jun 2026, abstract only): "models default to
  grep on localization (0-6% semantic use)"; "grep solves multi-file renames perfectly".
  Agents given a richer tool often do not use it.

### Retriever and query form

- **BRIGHT** (Su et al., arXiv 2407.12883, ICLR 2025, **summary**): average nDCG@10 BM25
  14.5 against the best dense model 22.5 (Table 2); with GPT-4 reasoning as the query,
  BM25 27.0, the best of all (Table 38); "BM25 improves the most, possibly because BM25
  can adapt to different queries" (§4.2). An agent that rewrites its own queries recovers
  most of BM25's vocabulary gap, and does the same for grep.
- **CodeRAG-Bench** (Wang et al., arXiv 2406.14497, Table 3, **summary**), NDCG@10 BM25 vs
  best dense: RepoEval 93.2 vs 94.3, MBPP 98.6 vs 99.0, but DS-1000 5.2 vs 33.1 and ODEX
  6.7 vs 37.1 (natural-language query, documentation target). **RepoCoder** (Zhang et
  al., EMNLP 2023, arXiv 2303.12570, App. B): "the performance of RepoCoder using the
  dense retriever is comparable to that using the sparse retriever". Lexical matching
  ties dense where query and target share words, and loses badly where they do not.
- **Term mismatch.** Zhao & Callan, "Term necessity prediction", CIKM 2010 (§4.3, read as
  PDF): average P(t | relevant) is 0.55 on TREC 3 titles and 0.59 on TREC 9
  descriptions, so a query term is missing from about 41–45% of relevant documents.
  Requiring every term (AND, or one grep for a phrase) multiplies recall by roughly that
  factor per added term; OR keeps recall and pays in precision. Disjunctive retrieval is
  "generally known to achieve better retrieval quality than a purely conjunctive
  approach" (Siedlaczek, Mallia & Suel, WSDM 2022, background statement). No measured
  study of AND against OR by query length for agents `[gap]`.

**What 2 means for loci** `[design inference]`. Split by query kind:

| Query kind | Example (neutral) | Best evidence | Implication |
|---|---|---|---|
| Known item, title words | "Dijkstra" | lexical ties dense; grep agents match or beat a BM25 tool | rg on `^# ` is enough; `find`'s title boost does the same |
| Known item, body words | "relaxation step" | grep agent 61 vs BM25 tool 32 (Li et al.) | rg, with the agent reformulating |
| Vocabulary gap | "shortest route" for a note titled "Dijkstra's algorithm" | only **dense** retrieval beats grep (BrowseComp-Plus, Cursor); BM25 is as blind as grep | `find` adds nothing; an embedding tool would |
| Multi-word topic | "breadth first search" | phrase/regex beats OR (§5, measured below) | rg `breadth.first` is precise; `find`'s OR is not |
| Exhaustive ("every note that mentions X") | rules step 4 | grep "solves multi-file renames perfectly"; top-k truncates | rg, never top-k |
| Broad survey over a big vault | "what do I know about graphs" at 10k notes | ranked retrieval has higher recall per call; grep degrades with size (Li et al. §4.3) | the one place top-k helps |
| Small model | any | grep-only loops; ranked tools +9 to +13 points | unknown for BM25 specifically `[gap]` |

## 3. Scalability

### Measured here

Setup: loci built from 668f5f9 (release), ripgrep 15.2.0, hyperfine, x86_64 Linux, 16
threads, warm page cache. Three generated vaults of 1,000, 10,000 and 100,000 concepts in
20 areas; each note has an H1 of three words and five 70-word paragraphs drawn from the
word stream of loci's own docs (about 2.4 KB; 3 / 24 / 239 MB of notes); 5% of
paragraphs carry a CJK word. Generator: a 30-line script in the session scratchpad; not
committed. Medians of 5–10 runs (100k `find`: 3 runs).

| Notes | rg -i heap | rg -il heap | `loci find heap` | `loci status` | `find` max RSS |
|---|---|---|---|---|---|
| 1,000 | 10.6 ms | 10.6 ms | 133 ms | 33 ms | — |
| 10,000 | 20.8 ms | 18.0 ms | 953 ms | 91 ms | 212 MB |
| 100,000 | 66.8 ms | 65.8 ms | 9.1 s | 690 ms | 1.23 GB |

(rg's RSS at 10k: 10 MB.) `status` approximates the cost of opening and parsing the
vault, so building the index is roughly 0.1 s, 0.86 s and 8.4 s. This agrees with D132
(about 290 ms on a 3,023-note vault). ripgrep's own benchmark searched a ~1 GB file in
0.27 s and a Linux kernel checkout in 0.16–0.33 s (Gallant, "ripgrep is faster than
{grep, ag, git grep, ucg, pt, sift}", 23 Sept 2016, https://burntsushi.net/ripgrep/).

**Output volume**, query "heap" (in about 12–13% of notes, one paragraph per line):

| Notes | rg lines | rg -l paths | `find` (k = 10) |
|---|---|---|---|
| 1,000 | 134 lines, 68 KB | 126 files, 4 KB | 2.3 KB |
| 10,000 | 1,324 lines, 667 KB | 1,252 files, 43 KB | 2.0 KB |
| 100,000 | 13,487 lines, 6.8 MB | 12,752 files, 446 KB | 0.5 KB |

A rare term ("supersession", 0.6% of notes) prints 3.5 KB / 33 KB / 332 KB of lines.
At about 4 characters per token, a common word at 10k notes costs ~170k tokens as raw
lines and ~11k as paths. Agents limit this themselves (`-l`, `-c`, `| head`, narrower
patterns), and harnesses cap it (Claude Code's Glob at 100 files; SWE-agent at 50
results, research 14), but each cap is a truncation in path order, not by relevance.

### Where each breaks for a personal vault `[design inference]`

- **1k notes:** everything works. rg prints at most tens of KB; `find` is 0.13 s. Ranking
  saves little context.
- **10k notes:** rg is still instant, but an unscoped common-word search prints a
  context window's worth; the agent must use `-l`, a title pattern or a narrower regex.
  Per-call `find` takes about a second, which is acceptable for an agent call but
  repeated on every query (an agent issues several per task, BrowseComp-Plus: 7–26
  calls per question).
- **100k notes:** rg is still well under a second; `find` per call is 9 s and 1.2 GB, so
  per-call rebuild has broken, and a persistent index (in `.loci/`, refreshed from git)
  would be needed: the Sourcegraph and Cursor designs, with Cursor's warning that an
  index for agents must be "very fresh … reading its own writes". Grep-agent accuracy
  also falls with corpus size in Li et al. (200K–400K documents), the regime where a
  ranked tool earns its keep. A personal vault at 100k notes is far beyond anything the pilot
  has seen (D132's generated test vault had 3,023 notes) `[design inference]`.

## 4. CJK and tokenisation

Probe (same build; six one-line notes; `find` against `rg -l`, `-i` for Latin):

| Query | Notes holding it | `find` | rg | Who is right |
|---|---|---|---|---|
| `京都` (Kyoto) | `京都は古都である`; `東京都の人口` (Tokyo-to) | Kyoto only | both | `find`: `東京都` contains `京都` as a substring |
| `言模` | `大型语言模型` | none | hit | `find`: not a word, spans `语言|模型` |
| `语言`, `模型`, `勾配`, `降下` | inside longer runs | hit | hit | both |
| `공부하다` (dictionary form) | `공부했다` (past tense) | hit (bigram `공부`) | none | `find` |
| `Hashing` / `hashes` | "Hashing is fast" | hit / none | hit / none | tie; rg `-i 'hash'` hits both by prefix |
| `都` | both notes | Tokyo only | both | depends on intent |

- Grep needs no segmentation and never misses a literal substring. Its CJK failure is
  precision: short Han queries match inside unrelated longer words (`京都` in `東京都`),
  and nothing is ranked.
- Segmentation's failure is recall when the query and the note are segmented
  differently. Research 07 §4 measured this with dictionary segmenters (a kanji-only
  query `勾配` against IPADIC's `勾配降下法` segmentation; `トランスフォーマー` against
  `トランスフォーマーモデル`) and proposed a parallel bigram field as the fix. The ICU
  segmenter adopted since (research 09; m0 §9.3 uses one word list for Chinese and
  Japanese) passed the kanji-in-Japanese case above, but no measured case set covers
  ICU's compound splitting `[gap]`.
- Korean is the clearest gain: the bigram field matches a stem inside any inflected
  form, which grep can only do if the agent strips the ending itself.
- For Latin script, `find` without stemming is *worse* than grep at morphology: grep's
  substring match acts as prefix stemming (`hash` hits hash, hashing, hashed, hashes);
  `find` matches whole tokens only.
- No study compares grep with BM25 for CJK retrieval by agents `[gap]`.

## 5. What `find` could offer that rg cannot

| Feature | rg equivalent | Agent evidence | Verdict `[design inference]` |
|---|---|---|---|
| Ranking, top-k | none (path order; truncation by `head`) | summarised, capped output beat iterating through matches (SWE-agent 18.0 vs 12.0); ranked retrieval has higher recall per call (Li et al. Table 4) | **the main value**, grows with vault size |
| Field weighting (titles) | `rg -i '^# .*term' concepts/` | weighting an author-written title gave under 1.5% (Robertson et al. 2004, research 15); a title pattern is a filter, not a weight | small; rg covers known items |
| Scope by kind | path arguments: `concepts/ views/ journal/` (areas are `concepts/*/AREA.md`) | none | rg covers it with one rule |
| Excluding frontmatter | frontmatter is only `id` and `prereqs`; a ULID or `[[slug]]` match is rare, and a prereq match is arguably wanted | none | negligible |
| Stemming / normalisation | prefix or alternation regex; NFC is the one gap (rg does not normalise; lint enforces NFC paths, not text) | none `[gap]` | add only if bench shows morphological misses |
| Snippet with line | `rg -n` (line number and the line) | — | rg covers it; `find`'s snippet repeats the H1 for title matches |
| "Did you mean" over titles | `ls concepts/*/` or `rg -l` | tool-output guidance ("narrow your query") helps (SWE-agent) | a zero-hit message is cheap and useful; fuzzy suggestions untested |
| CJK word matching | none (substring only) | none `[gap]` | **real value** for zh/ja precision and ko inflection |
| A benchmarkable surface | `bench --agent` measures any tool, rg included | — | tool-level bench only measures `find` against itself |
| Exhaustive mention search (rules step 4) | `rg -il 'term|other name' concepts/ views/` | grep "solves multi-file renames perfectly" | **rg is right**, `find`'s top-10 is wrong |

### The dogfood false positive, reproduced

On the 1k vault, "breadth first search" ranked notes whose title contains "first" or
"search" above every note containing "breadth-first": none of the 22 such notes was in
the top 10 (ranks 19, 22, 23, 27, …), while `rg -il 'breadth.first'` lists exactly the
22. The cause is in the query (§9.3): every token in every field is an independent
`Should` clause, the title boost is ×3, and the sum of separately scored fields can let
one title match on a common word outweigh a body that holds all three words (the
summed-field problem in Robertson et al. 2004, research 15 §2). A real note *titled*
"Breadth-first search" would still rank first; the failure hits every note where the
phrase is in the body.

## 6. Options for loci

### (a) Drop `find`; agents use rg under written rules

- **Code:** remove `loci-core/src/search/` (tantivy, `icu_segmenter`, the tokenizer,
  `unicode-normalization` if unused elsewhere), the `find` command and its tests, the
  `cjk` fixture's `find` assertions, `search.index`. `loci bench` keeps agent level
  only; tool level (recall@k, MRR, latency) has nothing to measure. Binary and build
  shrink. DESIGN §8 ("Skills depend only on `loci find --json`") and m0 §9.3 change.
- **Rules and skills:** "Finding things" and "Adding a concept" steps 1 and 4 name rg
  patterns, for example: titles `rg -i '^# .*dijkstra' concepts/`; everywhere in notes
  `rg -il 'dijkstra|shortest path' concepts/ views/ journal/`; count first on a common
  word `rg -ic … | sort -t: -k2 -nr | head`; Chinese/Japanese use the shortest
  unambiguous word; Korean search the stem without its ending.
- **Workflow:** one tool the agent already knows; exhaustive and exact searches done
  right.
- **Scalability:** fine past 100k notes; context cost is managed by the agent.
- **For:** every production agent greps (§1); a grep agent beat a BM25 tool by 29 points
  (Li et al.); least code; no per-call index cost. **Against:** loses CJK word precision
  and Korean inflection matching (§4); loses ranking, which matters at 10k+ notes; small
  models loop under grep (Bellam); one fewer thing `bench` can measure without an LLM.

### (b) Keep `find` as is

- **Code, rules:** unchanged. **For:** nothing to do. **Against:** the measured false
  positive (§5), silent zero hits, H1 snippets, no stemming, 9 s per call at 100k, and
  the rules send exhaustive step 4 to a top-10 tool. Reject.

### (c) AND by default, zero-hit message, title suggestions

- **Code:** `Occur::Must` per query token (across fields), or a minimum-should-match;
  a zero-hit result prints a message (human) and an empty list with a hint (JSON); on
  zero hits list titles sharing a token or within edit distance. Tests change.
- **For:** fixes the "first"/"search" false positive; a loud zero hit stops the agent
  concluding a note does not exist. **Against:** pure AND loses recall fast with no
  stemming and with term mismatch (each added term keeps about 55% of relevant notes,
  Zhao & Callan): "breadth first search algorithms" misses a note that says "BFS
  algorithm". A softer form, **coverage first** (rank by the number of distinct query
  tokens matched, then BM25), fixes the false positive without the recall cliff.
  Fuzzy title suggestions have no agent evidence `[gap]`.

### (d) A thin wrapper over rg with kind scoping and title-first ordering

- **Code:** replace tantivy and ICU with a call to rg (a new runtime dependency) or the
  `grep` crates; order files with title matches first, then by match count. `bench`
  tool level measures this ranking.
- **For:** substring semantics agents know, rg speed. **Against:** everything it adds is
  one rg flag or one rule (§5); it loses CJK word matching; it adds a dependency or
  reimplements rg. A wrapper that adds nothing grep lacks is the "duplicate tool" that
  agents ignore when the native one is free (2608.13568: 0–6% use of the richer tool).
  Reject.

### (e) Add semantic (embedding) search

- **Code:** an embedding model (local, tens to hundreds of MB, or an API), a persistent
  vector store in `.loci/`, freshness handling. `loci` has no model dependency today (research 15 §5
  rejected read-time LLM calls partly on that ground).
- **For:** the only kind of ranked retrieval that beats grep on vocabulary gaps (Cursor
  +12.5%; BrowseComp-Plus +10 to +21 points over BM25; Bellam for small models); the
  benefit grows with corpus size (Cursor: 2.6% on ≥1,000 files). **Against:** model
  and index maintenance, staleness, privacy ("more difficult to maintain, and less
  transparent", Anthropic; Sourcegraph's reasons for leaving embeddings); no evidence on
  personal notes; frontier grep agents already beat embeddings on literal text (Sen et
  al.; Li et al.). A user who wants it can point the agent at an external tool (qmd)
  without loci owning it. Not now.

### (f) rg-first rules, a narrower and fixed `find`, decided by measurement

1. **Rules and skills** (the main session's files; not edited here): rg is the default
   for exact, multi-word and exhaustive searches, with the patterns in (a). "Adding a
   concept" step 4 (every note that mentions it) uses rg. `find` is named for two jobs:
   a ranked first look when a term is common or the vault is large, and Chinese,
   Japanese or Korean words.
2. **`find` fixes**, small and each testable: coverage-first ranking (§(c)); an
   explicit zero-hit line in human output naming the scope searched (and the hint to try
   other names or rg); snippets from the first matching line after the H1. Optionally a
   phrase boost for adjacent query tokens (positions are already indexed).
3. **Measure** with the plan below. Delete `find` (a) if find-available does not beat
   grep-only on any case class beyond noise; keep the fixed `find` if it wins on CJK or
   on large-vault surveys. Revisit a persistent index only if a real vault passes ~20k
   notes, and embeddings (e) only if vocabulary-gap cases dominate the misses.

- **Code effect:** a few dozen lines in `search/mod.rs` and its tests; nothing removed
  until the measurement decides. **Scalability:** unchanged (fine to ~10k per call).
- **For:** puts each query kind on the tool the evidence favours (§2 table); keeps the
  two features rg lacks; fixes the reproduced defect; ends in a decision, not a
  standing feature with no reader. **Against:** two tools to describe in the rules;
  `find` survives a while longer on unproven value.

## Recommendation

Adopt **(f)**. The answers:

- **What is the point of `find` when agents grep well?** Less than it seemed. For exact,
  multi-word and exhaustive searches grep is as good or better, and the strongest
  2026 evidence has frontier agents with grep beating a BM25 tool by a wide margin (Li
  et al.). What only `find` gives is a short ranked list when grep would print
  thousands of lines, and CJK word matching (no `京都` in `東京都`; Korean stems across
  endings).
- **Do people who give agents BM25 search find it useful?** Production agents use grep
  and add *embeddings*, not BM25, when they add anything (Cursor). The one large BM25
  deployment (Sourcegraph) is a persistent server index at enterprise scale. No one
  publishes a gain from adding BM25 to a grep agent `[gap]`; BM25 single-shot is the
  weakest retriever in every agent comparison found.
- **How does each scale?** rg stays under 70 ms at 100k notes; its cost is output
  volume, which the agent controls. Per-call `find` is fine to about 10k notes and
  breaks before 100k (9 s, 1.2 GB).
- **If loci keeps a search command, its value is** ranking for context economy at
  large vault sizes, CJK word matching, and nothing else measured. It should be kept
  only if the benchmark below shows it.

## What to measure: `loci bench --agent`, grep-only against find-available

The decisive evidence is missing `[gap]`: no study runs a grep-only agent against one
that also has BM25 on a markdown note collection, and none covers CJK.

**Vaults.** A neutral sample vault (150–400 concepts, 8–12 areas; research 13's can be
reused) with about 15% of notes in Chinese, Japanese or Korean; and the same notes
padded with generated filler notes to 1k and 10k (as in §3), so that scale changes and
the targets do not.

**Agent arms** (same model, prompt and harness; only tool access differs):
- **G** grep-only: the rules rewritten as in (a); `loci find` denied by the harness
  (for Claude Code, a permission deny rule on the `loci find` command).
- **F** find-available: the rules as in (f), rg also allowed.
- **F0** (optional): today's `find` and rules, to measure the fixes.
- One frontier and one small model, since the small-model evidence differs (Bellam).

**Cases** (`bench/retrieval.jsonl`, `expect` = slug or path):
- **Known item, title words** ("What is Dijkstra's algorithm in my notes?").
- **Known item, body words only** (key words absent from slug and H1).
- **Multi-word topic with distractors** (notes titled with one of the words, as in §5).
- **Vocabulary gap** (another name or a paraphrase absent from the note).
- **CJK:** a Chinese word inside a longer run; a kanji-only query for a Japanese note;
  a Korean dictionary form against an inflected form; a short Han query whose
  substring occurs inside an unrelated word (precision trap).
- **Exhaustive:** "List every note that mentions X" with 3–8 expected notes. bench
  passes a case when *any* expected item is mentioned (m1-m4 §3), so score these by
  hand (or add an all-of mode later if they matter).
- **Absent item:** "Is there a note on X? Reply with its wikilink or `none`." bench
  cannot score `none`; score by hand, as research 15 suggests. This is where a silent
  zero hit hurts.

**Runs.** `loci bench --agent <cmd>` per arm × vault size × model, at least three
repeats to estimate noise. bench reads pass rate and median time only, so take tool
calls and output characters read from the harness transcript (per arm: number of
`rg`/`grep` and `loci find` calls, total characters of tool output). Also run tool-level
`loci bench` for F and F0 to see whether the ranking fix moves recall@10 and MRR.

**Decision rules** (fix before running):
- Delete `find` (a) if F is within noise of G on every case class at 1k and 10k, for the
  frontier model.
- Keep `find` if F beats G beyond noise on CJK or on 10k-vault cases, or uses clearly
  less tool output for the same pass rate; scope the rules to those cases.
- If F beats G only for the small model, keep it only if small models are a supported
  use (an open question for the user).
- If vocabulary-gap cases are the dominant miss in both arms, that is the case for
  embeddings (e), measured as a separate arm.
- Report F0 against F regardless: it says whether the coverage-first ranking and the
  zero-hit message were worth making.

## Sources

Production agents:
- Anthropic, "Effective context engineering for AI agents" (29 Sept 2025): https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents
- Anthropic, "Building agents with the Claude Agent SDK" (29 Sept 2025): https://claude.com/blog/building-agents-with-the-claude-agent-sdk
- Claude Code tools reference (read 2026-11-01): https://code.claude.com/docs/en/tools-reference
- Cursor, "Improving agent with semantic search" (6 Nov 2025): https://cursor.com/blog/semsearch
- Cursor, "Fast regex search: indexing text for agent tools" (23 Mar 2026): https://cursor.com/blog/fast-regex-search
- Sourcegraph, "How Cody understands your codebase" (15 Feb 2024): https://sourcegraph.com/blog/how-cody-understands-your-codebase ; Cody FAQ: https://sourcegraph.com/docs/cody/faq ; Zoekt: https://github.com/sourcegraph/zoekt
- Augment, "Context Engine … for any AI coding agent" (6 Feb 2026): https://www.augmentcode.com/blog/context-engine-mcp-now-live
- OpenAI Codex CLI, `codex-rs/protocol/src/prompts/base_instructions/default.md` line 264 at cb1bc0a: https://github.com/openai/codex
- Cognition, "Introducing SWE-grep and SWE-grep-mini" (16 Oct 2025): https://cognition.ai/blog/swe-grep
- Letta, "Benchmarking AI Agent Memory: Is a Filesystem All You Need?" (12 Aug 2025): https://www.letta.com/blog/benchmarking-ai-agent-memory
- LlamaIndex, "Did Filesystem Tools Kill Vector Search?" (13 Jan 2026): https://www.llamaindex.ai/blog/did-filesystem-tools-kill-vector-search
- qmd README: https://github.com/tobi/qmd

Benchmarks and papers:
- Li et al., "Beyond Semantic Similarity: Rethinking Retrieval for Agentic Search via Direct Corpus Interaction" (2026): https://arxiv.org/abs/2605.05242
- Sen et al., "Is Grep All You Need? How Agent Harnesses Reshape Agentic Search" (2026): https://arxiv.org/abs/2605.15184
- Chen et al., BrowseComp-Plus (2025): https://arxiv.org/abs/2508.06600
- Bellam, "How Do Small Coding Agents Use Code Graphs and Embeddings?" (2026): https://arxiv.org/abs/2610.33958
- Bhola et al., "Code Isn't Memory" (2026): https://arxiv.org/abs/2606.22417
- LSP versus grep for coding agents (2026): https://arxiv.org/abs/2608.13568
- Su et al., BRIGHT, ICLR 2025: https://arxiv.org/abs/2407.12883
- Wang et al., CodeRAG-Bench (2024): https://arxiv.org/abs/2406.14497
- Zhang et al., RepoCoder, EMNLP 2023: https://arxiv.org/abs/2303.12570
- Zhao & Callan, "Term necessity prediction", CIKM 2010: https://doi.org/10.1145/1871437.1871474
- Siedlaczek, Mallia & Suel, WSDM 2022: https://doi.org/10.1145/3488560.3498489
- Yang et al., SWE-agent, NeurIPS 2024 (Table 3, via research 14): https://arxiv.org/abs/2405.15793
- LocAgent, CoSIL, Agentless, Robertson et al. 2004: see research 15

Scalability:
- Gallant, "ripgrep is faster than {grep, ag, git grep, ucg, pt, sift}" (23 Sept 2016): https://burntsushi.net/ripgrep/
- Measurements in §3 and §4: this note (loci 668f5f9, ripgrep 15.2.0), generated vaults in the session scratchpad, not committed
