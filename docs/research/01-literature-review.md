# Literature review: an LLM-agent-managed personal knowledge base

Compiled 2026-10-30. Scope: a vault of several hundred markdown notes plus PDFs, maintained by an agent, for a graduate-level user in a technical field.
Verification: every item below was located by web search, and the venue/year comes from the venue page, the ACL Anthology, arXiv, or the publisher. Some numbers come from secondary summaries; those are marked *(secondary)*. Items I could not confirm against a primary page are marked **[unverified]**. Industry blog claims are vendor self-reports unless noted otherwise.

---

## 1. Index structure: tree, graph, hybrid, or agentic/no index

| Source | Venue / year | Finding |
|---|---|---|
| RAPTOR: Recursive Abstractive Processing for Tree-Organized Retrieval (Sarthi et al.) | ICLR 2024 — https://arxiv.org/abs/2401.18059 | Recursively clusters chunks and summarizes the clusters into a tree. With GPT-4 it gains +20 points absolute on QuALITY. Ablation *(secondary)*: clustering beat simply grouping adjacent chunks by only 56.6 vs 55.8, so the gain comes mostly from having multi-level summaries, not from the clustering. |
| From Local to Global: A GraphRAG Approach to Query-Focused Summarization (Edge et al., MSR) | arXiv 2024 — https://arxiv.org/abs/2404.16130 | Builds an entity graph with an LLM, then writes community summaries. On "global sensemaking" questions over about 1M-token corpora it has roughly 72–83% win rates on comprehensiveness against vector RAG *(LLM-judged)*. Indexing is expensive. |
| LazyGraphRAG (MSR Blog, Edge, Trinh, Larson) | Nov 2024 — https://www.microsoft.com/en-us/research/blog/lazygraphrag-setting-a-new-standard-for-quality-and-cost/ | Uses noun-phrase co-occurrence instead of LLM extraction. Indexing costs about the same as vector RAG, which is 0.1% of GraphRAG's cost, and quality on global queries is comparable. Vendor self-evaluation on a single news dataset. |
| HippoRAG (Gutiérrez et al.) / HippoRAG 2 "From RAG to Memory" | NeurIPS 2024 / ICML 2025 — https://arxiv.org/abs/2502.14802 | Retrieves by running Personalized PageRank over a knowledge graph linked to passages. HippoRAG 2 reports that earlier structured RAGs *fell below* plain dense RAG on simple factual QA. HippoRAG 2 recovers that ground and gains about 7 points on associative (multi-hop) tasks over NV-Embed-v2. |
| LightRAG (Guo et al.) | Findings of EMNLP 2025 — https://aclanthology.org/2025.findings-emnlp.568/ | Graph-plus-vector dual-level retrieval with incremental graph updates, a cheaper GraphRAG variant. |
| RAG vs. GraphRAG: A Systematic Evaluation (Han et al.) | arXiv Feb 2025 — https://arxiv.org/abs/2502.11371 | A controlled comparison (same LLM and embeddings). Plain RAG wins single-hop QA: NQ F1 64.8 vs 34.3 for triplet-only KG-GraphRAG *(secondary)*. GraphRAG helps on temporal/multi-hop queries and gives more diverse summaries. The two are complementary, and the paper proposes selection/integration hybrids. |
| When to use Graphs in RAG (GraphRAG-Bench, Xiang et al.) | ICLR 2026 (per repo) — https://arxiv.org/abs/2506.05690 | Graphs help on complex reasoning and summarization. Reranked vanilla RAG often ties or wins on fact retrieval (e.g., Medical summarization: RAG 65.8 vs HippoRAG2 63.1). The paper recommends better graphs over bigger ones and tight control of context size. |
| Agentic search in Claude Code (Boris Cherny, HN comment / Latent Space podcast, 2025) | Industry, informal — e.g. https://vadim.blog/claude-code-no-indexing/ (secondary) | Early Claude Code used Voyage embeddings in a local vector DB. The team found grep/glob agentic search "generally works better", and it has no index staleness or privacy issues. Cherny later said the evidence was "just vibes" (no published benchmark). **[primary HN comment not directly fetched]** |
| Beyond Semantic Similarity: Rethinking Retrieval for Agentic Search via Direct Corpus Interaction (Li et al., incl. J. Lin, Y. Choi) | arXiv May 2026 — https://arxiv.org/abs/2605.05242 | Agents that use only grep, file reads, and shell "substantially outperform" sparse, dense, and reranking baselines on several BRIGHT/BEIR sets, and do well on BrowseComp-Plus with no semantic retriever. Preprint. |
| Is Progressive Disclosure All You Need for Long-Context Agents? (He et al.) | ACL REALM workshop 2026 — https://arxiv.org/abs/2607.17598 | On ∞Bench, a strong agent grepping a single book needs no index. As the corpus grows to many books, raw navigation collapses and a *one-level* (flat) skill/index layer degrades much more slowly. Deep hierarchical routing costs context and adds nothing. Results depend on the harness: all approaches tie under Codex. |

**What the evidence says.** No single structure wins. Plain chunk retrieval or direct grep matches or beats graph methods on single-fact lookup. Graphs and trees pay off on multi-hop, temporal, and corpus-wide "what are the themes" questions, and only when graph quality is high. A strong agent with filesystem tools does well on small corpora. A thin, flat index (descriptions plus pointers) becomes necessary once the corpus is too large to skim. several hundred notes is near the point where a flat index starts to pay off, but well below the point where a full LLM-built knowledge graph would.

---

## 2. Agent memory systems

| Source | Venue / year | Finding |
|---|---|---|
| Generative Agents (Park et al.) | UIST 2023 — https://arxiv.org/abs/2304.03442 | Retrieval score = recency + importance + relevance, each min-max normalized, with weights set to 1. Recency decays exponentially at 0.995 per game-hour since last access. Importance is an LLM rating from 1 to 10. Reflections fire when cumulative importance passes 150. The weights were not ablated. |
| MemGPT: Towards LLMs as Operating Systems (Packer et al.) → Letta | arXiv 2023 — https://arxiv.org/abs/2310.08560 | An OS-style memory hierarchy: in-context "core" memory plus archival and recall storage that the LLM pages through with function calls it issues itself. |
| Sleep-time Compute (Lin, Snell, …, Packer, Gonzalez; Letta/Berkeley) | arXiv Apr 2025 — https://arxiv.org/abs/2504.13171 | Precomputing offline over a context lowers test-time compute about 5x at equal accuracy. The benefit tracks how predictable the queries are. This is the argument for background "consolidation" jobs. |
| Letta: "Benchmarking AI Agent Memory: Is a Filesystem All You Need?" | Letta blog, Aug 2025 — https://www.letta.com/blog/benchmarking-ai-agent-memory/ | A plain agent with file tools scored 74.0% on LoCoMo (gpt-4o-mini), above Mem0-graph's reported 68.5%. Specialized memory libraries showed no clear advantage. Vendor benchmark, and the numbers are disputed among Mem0, Zep, and Letta. |
| A-Mem: Agentic Memory for LLM Agents (Xu et al.) | NeurIPS 2025 — https://arxiv.org/abs/2502.12110 | Zettelkasten-style notes (keywords, tags, context, embedding, links). New notes trigger LLM "memory evolution" that rewrites linked older notes. Improves LoCoMo results with fewer tokens. Later work found the evolution step fragile across model families *(secondary)*. |
| Mem0 (Chhikara et al.) | arXiv Apr 2025 — https://arxiv.org/abs/2504.19413 | Extracts facts, then an LLM chooses ADD/UPDATE/DELETE/NOOP against the top-10 similar memories. About 91% lower p95 latency than full context. The graph variant (Mem0g) helps on temporal questions, not multi-hop. |
| Zep: A Temporal Knowledge Graph Architecture for Agent Memory (Rasmussen et al.) / Graphiti | arXiv Jan 2025 — https://arxiv.org/abs/2501.13956 | Bi-temporal edges: event time T and ingestion time T′. Contradicted facts get `invalid_at` and are kept, not deleted. Episode, entity, and community subgraphs. Reports up to +18.5% on LongMemEval (vendor). |
| LongMemEval (Wu et al.) | ICLR 2025 — https://arxiv.org/abs/2410.10813 | Five abilities: extraction, multi-session reasoning, temporal reasoning, knowledge updates, abstention. Long-context models drop about 30%. Three design wins: round-level granularity, fact-augmented index keys, and time-aware indexing/query expansion. |
| Memory in the Age of AI Agents: A Survey (Liu et al.) | arXiv Dec 2025 — https://arxiv.org/abs/2512.13564 | Taxonomy by form (token / parametric / latent), function (factual / experiential / working), and dynamics (formation / evolution / retrieval). |
| Harness the Memory: Holistic Evaluation of Memory Substrates (Huang et al.) | arXiv Aug 2026 — https://arxiv.org/abs/2608.15008 | Across 3 backbones, 4 suites, and 26 metrics, "no single substrate consistently dominates". Over-retrieval hurts sequential decision tasks. The authors propose routing to a substrate by regime. Preprint. |

**What the evidence says.** Temporal validity matters more than recency decay. LongMemEval and Zep both show that *knowledge updates* and *temporal reasoning* are where systems fail. The working mechanisms are explicit timestamps, `valid_from/invalid_at` instead of overwriting, and time-aware query expansion. Recency/importance weighting (Generative Agents) is a heuristic that was never ablated. Simple file-based memory run by a capable agent is competitive with specialized memory stacks.

---

## 3. Tools vs instructions; future-proofness

| Source | Venue / year | Finding |
|---|---|---|
| CodeAct: Executable Code Actions Elicit Better LLM Agents (Wang et al.) | ICML 2024 — https://arxiv.org/abs/2402.01030 | Across 17 LLMs, code as the action space beats JSON/text tool calls by up to 20% success with fewer turns, because of control flow and data flow. Gains are larger for open models. |
| Model Context Protocol (Anthropic) | Nov 2024 — https://www.anthropic.com/news/model-context-protocol | An open standard for exposing tools and resources to agents. |
| Code execution with MCP (Anthropic Engineering) | Nov 4 2025 — https://www.anthropic.com/engineering/code-execution-with-mcp | Present tools as a *filesystem of code* the agent reads on demand, and do data processing in a sandbox. In one example workflow tokens drop from 150k to 2k (−98.7%). Saved code plus a SKILL.md becomes reusable skills. A single example, not a benchmark. |
| Equipping agents for the real world with Agent Skills (Anthropic) | Oct 16 2025 — https://www.anthropic.com/engineering/equipping-agents-for-the-real-world-with-agent-skills | Progressive disclosure in 3 levels: name+description always loaded → SKILL.md body on activation → bundled files on reference. Skills complement MCP: they teach workflows, while MCP provides connectivity. |
| Effective context engineering for AI agents (Anthropic) | Sep 2025 — https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents | Recommends "just-in-time" context: keep lightweight identifiers (paths, queries) and load at runtime with tools; CLAUDE.md is loaded up front, grep/glob for the rest. Structured note-taking (NOTES.md) for persistence. Claims are anecdotal. |
| Context Engineering for AI Agents: Lessons from Building Manus (Peak Ji) | Manus blog 2025 — https://manus.im/blog/Context-Engineering-for-AI-Agents-Lessons-from-Building-Manus | Explicitly "bitter lesson" framed: bet on context engineering, not custom models, so the product rides model progress. Treat the filesystem as external memory. Rebuilt the harness 4–5 times. |
| The Bitter Lesson (Sutton) | Essay 2019 — http://www.incompleteideas.net/IncIdeas/BitterLesson.html | General methods that scale with compute beat hand-engineered knowledge. This is the basis for "thin harness" arguments. |

**What the evidence says.** There is good evidence (CodeAct) that general-purpose code/shell actions beat fixed narrow schemas, and that the gap grows as models improve. Industry practice (Anthropic, Manus) is converging on a small set of general tools (files, shell, code) plus *instructions loaded on demand* (skills), rather than many bespoke APIs. Bespoke tools are worth keeping where they enforce invariants (validation, atomic writes) or cut tokens. Direct evidence that thin designs are future-proof is mostly practitioner anecdote: Manus refactors, and harness code deleted as models improved. No controlled study exists **[gap]**.

---

## 4. Learner modeling, knowledge estimation, review

| Source | Venue / year | Finding |
|---|---|---|
| Deep Knowledge Tracing (Piech et al.) | NeurIPS 2015 — https://papers.nips.cc/paper/5654-deep-knowledge-tracing | An RNN over sequences of question attempts predicts the probability of the next correct answer. This is the base of the KT literature, and it needs graded interaction logs. |
| Exploring Knowledge Tracing in Tutor-Student Dialogues using LLMs (Scarlatos, Baker, Lan) | LAK 2025 — https://learninganalytics.upenn.edu/ryanbaker/Dialogue_KT_LAK_25-2.pdf | An LLM labels knowledge components in free-form dialogue, and a fine-tuned Llama-3 ("LLMKT") outperforms classic KT at predicting correctness. Shows KT can run on unstructured interaction, not just item logs. |
| CIKT: Collaborative and Iterative KT with LLMs | EMNLP 2025 — https://aclanthology.org/2025.emnlp-main.975/ | An "Analyst" LLM maintains an explicit natural-language profile of each student's knowledge state, which a predictor uses. Gives interpretable, editable learner models. |
| SINKT (Fu et al.) | CIKM 2024 (venue from citing papers; no URL checked) **[unverified]** | Uses an LLM to build a concept–question structure graph for inductive KT. |
| Automated Concept Map Extraction from Text (Galletti et al.) | LDK 2025 — https://aclanthology.org/2025.ldk-1.10/ | A pipeline of summarize → rank importance → extract entities and relations. Prior concept-map extraction had F1 < 20%, and LLM pipelines improve on it but the task is still hard. |
| A Stochastic Shortest Path Algorithm for Optimizing Spaced Repetition Scheduling (Ye, Su, Cao) | KDD 2022 — https://doi.org/10.1145/3534678.3539081 | A memory model (difficulty/stability/retrievability) fit on 220M review logs, with optimal scheduling via SSP. It is the research basis of FSRS (the open-source scheduler now in Anki). |
| Memory Machines: Can LLMs create lasting flashcards from readers' highlights? (Kirkby & Matuschak) | Report 2026 — https://memory-machines.com/report | About 1,500 rated prompts. The best generator (GPT-5.2) is still about 36% unusable. LLM judges top out below 70% accuracy. Models target well but *construct* prompts poorly. Readers agree on only 39% of interests, so humans should pick the targets. |
| Matuschak, "GPT-4 can often generate usable SR prompts … with guidance" | Working notes 2023 — https://notes.andymatuschak.org/zJRDpsHzhrx87XxabV5jqXg | Works for declarative facts when a human points at targets. Fails on conceptual material. Writing prompts is itself elaboration, so automating it has an understanding cost. |
| SmartFlash: Balancing AI Automation and Agency for Self-Regulated Learning | arXiv 2026 — https://arxiv.org/abs/2602.14431 | Reviews prior work (e.g., AI and human flashcards of comparable quality in MOOCs, Bachiri et al. 2023 **[unverified]**) and argues for keeping learner agency in card creation. |

**What the evidence says.** Classic KT requires graded attempt logs, which a notes vault does not naturally produce. LLM-based KT (LAK'25, EMNLP'25) shows you can infer knowledge states from unstructured interaction and keep them as *editable natural-language profiles*. I found **no peer-reviewed work that estimates a person's knowledge from their personal notes alone** **[gap]**. Note existence is a weak proxy; graded recall attempts are the real signal. For review, use FSRS for scheduling and LLMs for drafting cards from human-chosen targets, with human screening of what the model builds.

---

## 5. PKM and HCI

| Source | Venue / year | Finding |
|---|---|---|
| Matuschak, Evergreen notes (concept-oriented, atomic, densely linked; "prefer associative ontologies to hierarchical taxonomies") | Working notes — https://notes.andymatuschak.org/z5E5QawiXCMbtNtupvxeoEX ; https://notes.andymatuschak.org/Evergreen_notes_should_be_concept-oriented | Factor notes by *concept*, not by source, project, or date, so links build up across domains. Fitting a new note into the whole takes effort, and that effort is the point. |
| Sensecape (Suh, Min, Palani, Xia) | UIST 2023 — https://doi.org/10.1145/3586183.3606756 | LLM sensemaking with multiple levels of abstraction (canvas plus hierarchy views). Users explored more broadly when they could switch between levels. |
| Graphologue (Jiang, Rayan, Dow, Xia) | UIST 2023 — https://doi.org/10.1145/3586183.3606737 | Turns LLM text into interactive node-link diagrams, supporting non-linear exploration. |
| NoTeeline (Huq et al.) | IUI 2025 — https://arxiv.org/abs/2409.16493 | The user jots micronotes, and the LLM expands them in the user's style. Less effort and time (n=12). Keeps the user's selection while automating the prose. |
| The Impact of Generative AI on Critical Thinking (Lee et al., MSR/CMU) | CHI 2025 — https://doi.org/10.1145/3706598.3713778 | Survey of n=319. Higher confidence in GenAI correlates with less critical thinking, and work shifts toward verifying and integrating output. Self-reported and correlational. |
| Steering LLM Summarization with Visual Workspaces for Sensemaking | arXiv 2024 — https://arxiv.org/abs/2409.17289 **[venue unverified]** | User-arranged spatial workspaces steer LLM summaries. Supports human-owned structure with LLM-generated content. |

**What the evidence says.** Peer-reviewed evaluations of Zettelkasten, PARA, or MOCs against folders are essentially absent. The guidance is practitioner lore **[gap]**. The HCI evidence supports (a) *multiple views at different levels of abstraction* over one canonical hierarchy (Sensecape), and (b) keeping the human in the selecting and judging role while the LLM does the prose and bookkeeping (NoTeeline, Matuschak, CHI'25 critical thinking). This resolves the topic-vs-chronology tension: keep atomic concept notes as the source of truth, and *generate* chronological, project, and course views (indexes, MOCs, logs) as derived artifacts. Karpathy's "LLM wiki" pattern does essentially this: three layers (immutable raw sources, an LLM-owned wiki, a schema file such as CLAUDE.md/AGENTS.md), plus an `index.md` and an append-only `log.md` (correction from the deep dive: the gist names no `raw/`/`wiki/` folders; its only literal path is `raw/assets/` in an Obsidian tip), there are ingest/query/lint operations, and the store is a git repo of markdown (https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f, Apr 2026). His motivation is that "bookkeeping" (cross-references, deduplication, conflict detection) is the expensive part, and that is the part to delegate.

---

## 6. Code knowledge

| Source | Venue / year | Finding |
|---|---|---|
| RepoGraph (Ouyang et al.) | ICLR 2025 — https://arxiv.org/abs/2410.14684 | A line-level definition/reference graph (tree-sitter + networkx) used as a plug-in. Improved all 4 frameworks it was added to on SWE-bench-Lite. In SWE-agent it is just one extra `search_repograph` action. |
| CodeRAG: Supportive Code Retrieval on Bigraph (Li et al.) | arXiv Apr 2025 — https://arxiv.org/abs/2504.10046 | A requirement graph linked to a code dependency graph, plus agentic traversal. Note the name collisions: other "CodeRAG" papers exist (arXiv 2509.16112, 2309.14509). |
| RepoAgent (Luo et al.) | EMNLP 2024 Demo — https://aclanthology.org/2024.emnlp-demo.46/ | Repo-level doc generation from a global structure analysis. A **pre-commit hook regenerates docs for changed code**, so docs and code stay in sync. |
| DeepWiki (Cognition) | Product, Apr/May 2025 — https://cognition.ai/blog/deepwiki **[blog page not fetched]** | Auto-generated wiki plus Q&A for public repos, exposed via an MCP server. Shows the "generated, regenerable docs" pattern at scale. |
| Detecting Code Comment Inconsistencies using LLM and Program Analysis | FSE 2024 companion — https://dl.acm.org/doi/10.1145/3663529.3664458 | GPT-4 combined with program analysis found 160 inconsistencies in 13 projects, and developers fixed 23. Related work (DocPrism) found that naive "ask the LLM" flags more than 90% of functions, so drift detection needs structure, not just an open-ended LLM judgment. **[DocPrism venue unverified]** |

**What the evidence says.** For code, structural indices (symbol/definition graphs) are cheap, deterministic, and help agents. They are best exposed as one extra tool, not as a replacement for grep. Keeping docs and code in sync works when generation is *triggered by diffs* (pre-commit or CI) and when drift checks compare against concrete code facts (signatures, references). For notes about the student's own repos, link notes to file paths and commit hashes and re-verify them on change.

---

## 7. Search

| Source | Venue / year | Finding |
|---|---|---|
| BEIR (Thakur et al.) | NeurIPS 2021 Datasets & Benchmarks — https://arxiv.org/abs/2104.08663 | BM25 is a robust zero-shot baseline. Many dense retrievers fall below it out of domain. Late-interaction and reranking models generalize best. |
| ColBERTv2 (Santhanam et al.) | NAACL 2022 — https://aclanthology.org/2022.naacl-main.272/ | Late interaction with residual compression. Best on 22 of 28 out-of-domain tests. PLAID (CIKM 2022) makes it 7x faster on GPU and 45x faster on CPU. |
| Introducing Contextual Retrieval (Anthropic) | Sep 2024 — https://www.anthropic.com/news/contextual-retrieval | Adding a chunk-specific context prefix before embedding and BM25 cut top-20 retrieval failure by 35% (embeddings only), 49% (+BM25 hybrid), and 67% (+reranking). Vendor evaluation. |
| BrowseComp-Plus (Chen et al.) | arXiv Aug 2025 — https://arxiv.org/abs/2508.06600 | With a fixed corpus, swapping BM25 for Qwen3-Embedding-8B raised GPT-5 from 55.9 to 70.1% accuracy, with fewer search calls. |
| SAGE: Benchmarking Retrieval for Deep Research Agents | arXiv Feb 2026 — https://arxiv.org/abs/2602.05975 | On longer documents, BM25 *beats* LLM dense retrievers for agents. It attributes BrowseComp-Plus's result to dense indexing that was truncated to 4096 tokens and to agents writing natural-language queries. Stronger agents depend less on the retriever. |
| qmd (Tobi Lütke) | Open source — https://github.com/tobi/qmd | Local SQLite FTS5 BM25 + sqlite-vec + query expansion + RRF + a small local reranker, exposed via CLI/MCP and chunked on markdown structure. The search layer Karpathy's gist recommends. |
| Context Rot (Chroma Research) / Lost in the Middle (Liu et al.) | 2025 report — https://research.trychroma.com/context-rot ; TACL 2024 — https://arxiv.org/abs/2307.03172 | Accuracy degrades as input grows (18 models), and irrelevant context hurts more than none. Mid-context content is used less well. This argues against "just stuff the whole vault in". |

**What the evidence says.** At 1k–100k documents, local hybrid search (BM25 + small embedding model + RRF, optionally with a reranker) is cheap, runs in SQLite, and is the strong default. BM25 alone is a better baseline than many dense models, especially for technical vocabulary (domain jargon, identifiers). Whether dense or BM25 wins for agents depends on document length and query style. Agents that issue keyword queries do well with BM25. ColBERT-class late interaction is worth adding only if hybrid recall measurably fails. Retrieval quality matters less as agent strength rises.

---

## Design implications

1. **Make filesystem + grep the primary retrieval path, and add a thin hybrid search tool next to it.** several hundred notes, a strong agent navigating files is competitive (Claude Code, the Letta filesystem result, Direct Corpus Interaction). Add a local BM25+vector tool (qmd-style, SQLite) for vocabulary-mismatch queries. Do not build a vector-only RAG pipeline.
2. **Keep one flat, LLM-maintained index (one line per note: title, description, tags, path), not a deep hierarchy.** The progressive-disclosure study found that one-level routing scales and deep routing wastes context. This is Karpathy's `index.md`.
3. **Treat a knowledge graph as derived and optional.** Graph RAG beats plain RAG only on multi-hop, temporal, and global sensemaking questions, and loses on fact lookup. Explicit `[[wikilinks]]` already give a free graph. Use LazyGraphRAG-style cheap co-occurrence or periodic "theme" summaries if global questions come up. Avoid LLM triple extraction over the whole vault.
4. **Use multi-level summaries (RAPTOR/MOC-style) only for long PDFs and large topic clusters,** and regenerate them, never edit them by hand. Most of the gain comes from summary levels, not clever clustering.
5. **Model time explicitly.** Give every claim or note `created`/`updated`/`last_verified` dates. When something is superseded, mark it (`superseded_by`, `invalid_after`) instead of silently overwriting (Zep/Graphiti, LongMemEval knowledge-update failures). Use recency only as a tie-breaker. Its weighting has never been ablated.
6. **Prefer a few general tools (read/write/grep/shell/code) plus on-demand skills (instructions) over many bespoke APIs** (CodeAct, Anthropic skills/MCP, Manus). Keep bespoke code only where it enforces invariants: frontmatter schema validation, link checking, atomic moves. That code should fail loudly.
7. **Make chronology and organization views, not storage.** Atomic concept notes are the source of truth. Daily logs, project pages, course pages, and MOCs are generated or linked views. Multiple views at different abstraction levels are what HCI work supports (Sensecape), and they avoid the topic-vs-folder fight.
8. **Run bookkeeping as background "sleep-time" passes** (lint: broken links, duplicates, contradictions, stale claims, orphan notes) instead of during interactive queries. Precomputation pays off when future queries are predictable (Sleep-time Compute).
9. **Keep the human choosing what to learn; let the LLM do construction drafts with screening.** Even the best model leaves about 36% of generated prompts unusable, and writing prompts is itself elaboration (Matuschak/Kirkby). Schedule with FSRS, not a homemade scheduler.
10. **Estimate knowledge from recall attempts, not from note existence.** Keep a per-concept, editable natural-language "knowledge state" profile (CIKT-style) updated from review outcomes and Q&A sessions. Treat note-derived estimates as a weak prior. No validated method exists for estimating knowledge from personal notes.
11. **For code-linked notes, store path + symbol + commit and re-verify them on diff** (RepoAgent pre-commit pattern). Use structural checks (symbol still exists, signature unchanged), not open-ended "is this doc stale?" LLM judgments, which flag far too many functions.
12. **Keep loaded context small and design for verification.** Context rot and lost-in-the-middle argue against loading the whole vault. The CHI'25 critical-thinking result argues that the agent should show sources and diffs for every edit (git as the audit log) so the student stays the verifier rather than a passive consumer.

### Known gaps / caveats
- No peer-reviewed comparison of PKM methods (Zettelkasten/PARA/MOC vs folders) exists.
- No validated method for estimating knowledge from a person's notes was found.
- Agentic-search vs RAG evidence is mostly industry anecdote plus 2026 preprints. Memory benchmark numbers from Mem0, Zep, and Letta are vendor-reported and mutually disputed.
- Secondary-source numbers are marked; check them against the paper tables before quoting them in a publication.
