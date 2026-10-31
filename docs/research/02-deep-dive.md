# Deep dive (round 2): designs, schemas, and failure modes for an agent-maintained research KB

Compiled 2026-10-30. Builds on `lit-review.md` (round 1). I did not repeat its breadth.
Method: I shallow-cloned and read the source for qmd (HEAD 2026-10-14, v2.8.3), hister (HEAD 2026-10-30, CHANGELOG top = v0.21.0), Graphiti (2026-10-28), A-Mem (2025-12-12), HippoRAG (2026-10-06), Basic Memory (2026-10-30), RepoAgent (2024-12-23), and deepwiki-open. Clones are in `scratchpad/src/`. Papers and docs were read via their primary URLs. Anything I could not confirm against a primary source is marked **[unverified]**.

**Correction to round 1.** The first-round review said Karpathy's gist specifies `raw/` and `wiki/` folders. It does not. The gist names three *layers* ("Raw sources", "The wiki", "The schema") and two files (`index.md`, `log.md`). The only literal path in it is `raw/assets/`, given as an Obsidian attachment-folder tip. Everything else is "intentionally abstract".

---

## 1. Karpathy's LLM Wiki gist and qmd

### 1.1 The gist (https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f; read raw, ~1,960 words)
- **Layers.** *Raw sources* are immutable; the LLM reads them and never modifies them. *The wiki* is LLM-owned markdown ("You read it; the LLM writes it"). *The schema* is CLAUDE.md/AGENTS.md, which the human and LLM "co-evolve".
- **Operations.**
  - *Ingest* processes one source. The LLM reads it, discusses it, writes a summary page, updates `index.md`, updates the affected entity and concept pages, and appends to `log.md`. One source "might touch 10–15 wiki pages". Karpathy prefers ingesting one source at a time, supervised.
  - *Query* answers with citations, and "good answers can be filed back into the wiki as new pages".
  - *Lint* checks for contradictions, stale claims superseded by newer sources, orphans, concepts mentioned without their own page, missing cross-references, and gaps to fill with web search.
- **index.md** is a content-oriented catalog: one line per page with a summary and optional metadata, grouped by category. The gist says it works "at moderate scale (~100 sources, ~hundreds of pages)" without embeddings.
- **log.md** is append-only. Each entry starts with a greppable prefix such as `## [2026-04-02] ingest | Title`.
- **Search** is optional. qmd is recommended "as the wiki grows". The gist also suggests YAML frontmatter plus Dataview.
- **Not specified:** a page schema, frontmatter fields, provenance format, or a conflict representation. The gist delegates all of that to the agent.

### 1.2 Community critiques (secondary, but concrete)
- A digest of all 1,061 gist comments (https://omiid.me/notebook/66/i-read-all-1-061-comments-on-karpathy-s-llm-wiki-gist) finds four failure classes:
  1. **Garbage compounds.** "A bad wiki article becomes a prior that poisons future generations", and pages end up citing each other as evidence.
  2. **Notes rot against reality.** Facts are silently overwritten instead of superseded, the index drifts, pages are orphaned, and numbers carry no source. The worked example is a note claiming a dashboard needed admin access, while the code's permission check returned `true` for everyone. Lint missed it because "the contradiction wasn't between notes. It was between a note and a line of code."
  3. **Relevance is task-relative.**
  4. **The pattern assumes a single user.**
- Mitigations the commenters converged on:
  - a **promotion gate**: agent drafts cannot serve as evidence until a human promotes them;
  - a **source pointer plus a checked-on date** on every claim about the outside world, with lint re-verifying the oldest claims first;
  - **supersession callouts** instead of silent edits.
- Other critiques:
  - Self-reference and "knowledge-base poisoning": https://foundanand.medium.com/the-hidden-flaw-in-karpathys-llm-wiki-e3a86a94b459
  - Detail loss from repeatedly rewriting its own summaries: https://yu-wenhao.com/en/blog/karpathy-zettelkasten-comparison/
- All of these are blogs. I found no peer-reviewed evaluation of the pattern **[gap]**.

### 1.3 qmd internals (source: `src/store.ts`, `README.md`)
- **Storage:** one SQLite file.
  - `content(hash PK, doc, created_at)` is content-addressed.
  - `documents(collection, path, title, hash, created_at, modified_at, active, UNIQUE(collection,path))` maps paths to content.
  - `documents_fts` is FTS5 over `(filepath, title, body)` with the `porter unicode61` tokenizer.
  - `content_vectors(hash, seq, pos, …)` plus a sqlite-vec `vec0` table (`embedding float[d] distance_metric=cosine`, partitioned by collection).
  - Also `llm_cache`, `document_metadata`/`document_metadata_values` (frontmatter is queryable through a JSON filter AST), and `file_sync_state`.
  - Vectors are keyed by content hash, so renames do not trigger re-embedding.
- **Chunking:** 900 tokens with 15% overlap (`CHUNK_SIZE_TOKENS=900`). Break points are scored so that headings beat weaker breaks, and code fences are never split. Optional tree-sitter AST chunking for TS/JS/Python/Go/Rust.
- **Hybrid pipeline (`hybridQuery`):**
  1. A BM25 probe. If the top score is ≥ 0.85 and leads the runner-up by ≥ 0.15, LLM expansion is skipped. This bypass is disabled when an `intent` is supplied.
  2. Expansion into typed sub-queries (`lex`/`vec`/`hyde`) by a fine-tuned 1.7B GGUF model.
  3. Type-routed retrieval: `lex`→FTS, `vec`/`hyde`→vector.
  4. RRF with k=60. The original query's lists get weight 2.0. A top-rank bonus adds +0.05 for rank 1 and +0.02 for ranks 2–3.
  5. Slice to `RERANK_CANDIDATE_LIMIT = 40`. The README's diagram says "top 30"; the code says 40.
  6. Pick the best-matching chunk per document by keywords and rerank *chunks, not whole bodies* ("O(tokens) trap") with `qwen3-reranker-0.6b` (yes/no logprobs).
  7. Position-aware blend: RRF ranks 1–3 get 0.75 retrieval weight, ranks 4–10 get 0.60, ranks 11+ get 0.40.
  8. Dedupe by file.
- **Models:** embeddinggemma-300M (~300 MB), qwen3-reranker-0.6B (~640 MB), and the expansion model (~1.1 GB). They run via node-llama-cpp; `QMD_EMBED_MODEL` overrides the embedder.
- **Context tree:** `qmd context add qmd://notes/sub "…"` attaches descriptions to path prefixes. They are returned alongside matching documents, and the README calls this "the key feature".
- **Interfaces:**
  - CLI: `search` (BM25), `vsearch`, `query` (hybrid), `get`, `multi-get`.
  - MCP tools: `query` (1–10 typed `searches`; the first gets 2x weight), `get`, `multi_get`, `status`, `metadata`.
  - `qmd mcp --http` also serves plain `POST /query`, `POST /metadata`, `GET /health`. The README says these "endpoints are unauthenticated". Unknown MCP parameters are silently ignored; the README warns about this specifically for `collection` vs `collections`.
- **Bundled skill:** it tells the agent to write its own `intent:/lex:/vec:/hyde:` queries ("You are a better query expander than the built-in model") and never to answer from snippets.
- **Packaging:** not in nixpkgs, but the repo ships `flake.nix`, which wraps `bun src/cli/qmd.ts` and includes a Home Manager module. It is TypeScript/Bun, not Rust.

---

## 2. Memory architectures: what to copy

### 2.1 Graphiti / Zep (source: `graphiti_core/nodes.py`, `edges.py`, `prompts/dedupe_edges.py`, `utils/maintenance/edge_operations.py`)
- **Nodes**
  - `EpisodicNode{source ∈ {message, json, text, fact_triple}, source_description, content (raw), valid_at (when the original doc was created), entity_edges[]}`.
  - `EntityNode{name, name_embedding, summary ("regional summary of surrounding edges"), attributes}`.
  - `CommunityNode{summary}`.
  - `SagaNode` (episode-sequence summaries; newer than the paper).
  - All nodes carry `uuid, group_id (partition), labels, created_at`.
- **EntityEdge (the "fact")**
  - `name` (relation), `fact` (natural-language sentence), `fact_embedding`, `episodes[]` (provenance: which episodes asserted it).
  - Bi-temporal fields: `valid_at`/`invalid_at` are world time. `created_at`/`expired_at` are system time.
  - The docstrings encode subtle rules: future plans take the *episode reference time* as `valid_at`, and "soft deadlines are not invalid_at".
- **Contradiction pipeline**, for each newly extracted fact:
  1. Run a hybrid (RRF) search restricted to edges with the **same endpoint pair**. These are the duplicate candidates.
  2. Run an unrestricted hybrid search. These are the invalidation candidates.
  3. Ask one LLM call to return `duplicate_facts` and `contradicted_facts` indices. The prompt's guardrails: "NEVER mark facts as contradicted unless the new fact explicitly makes the old fact false". Do not infer contradiction "from recency alone"; activity facts do not contradict role or ownership facts. A different title for the same relation counts as a contradiction, not a duplicate.
  4. Resolve **deterministically on dates** (`resolve_edge_contradictions`). If the intervals do not overlap, nothing happens. If the old fact started earlier, set `old.invalid_at = new.valid_at` and `old.expired_at = now`. If a candidate starts *later* than the new fact, the new fact is the one that gets expired.
  - Nothing is deleted.
- **Lesson:** the LLM only judges "incompatible?". The temporal bookkeeping is plain code over explicit dates. Candidate retrieval is scoped by entity pair, which keeps the LLM's job local.

### 2.2 A-Mem (source: `agentic_memory/memory_system.py`; NeurIPS 2025, https://arxiv.org/abs/2502.12110)
- **MemoryNote:** `content, id, keywords[], links, context (default "General"), category, tags[], timestamp (YYYYMMDDHHMM), last_accessed, retrieval_count, evolution_history[]`.
- **Evolution step** (`process_memory`):
  1. Retrieve the k=5 nearest notes.
  2. One LLM call returns JSON `{should_evolve, actions ⊂ {strengthen, update_neighbor}, suggested_connections, tags_to_update, new_context_neighborhood[], new_tags_neighborhood[]}`.
  3. `strengthen` appends links and **replaces** the new note's tags. `update_neighbor` **overwrites the neighbors' `context` and `tags`**; their content is never changed.
  4. Every `evo_threshold=100` additions trigger a `consolidate_memories()` re-index.
- **Failure modes visible in the code:**
  - `evolution_history` is declared but never appended to, so neighbor rewrites leave no audit trail.
  - Neighbors are addressed by positional index into `list(self.memories.values())`, which is fragile.
  - Every exception is caught and logged, and the note is returned unevolved. This fails silently.
  - These match the round-1 note that later work found evolution fragile across model families *(secondary)*.
- **Lesson:** keep the useful idea (on write, look at nearest neighbors and propose links and retags), but make every change a reviewable diff (git) and never overwrite metadata without history.

### 2.3 HippoRAG 2 (ICML 2025, https://arxiv.org/abs/2502.14802; source `src/hipporag/HippoRAG.py`)
- **Graph:** phrase (entity) nodes from OpenIE triples, plus **passage nodes**. Edges are:
  - relation edges from the triples;
  - passage–phrase "contains" edges;
  - synonymy edges between phrases with embedding similarity ≥ 0.8 (kNN k=2047).
  - The graph is undirected by default.
- **Query time:**
  1. Embed the query and match it against *triples* (query-to-triple).
  2. An LLM "recognition memory" filter drops irrelevant triples.
  3. Each surviving triple's subject and object get the fact score, divided by the number of passages the phrase occurs in (an IDF-like term), averaged, and kept to the top `linking_top_k=5`.
  4. Every passage node gets its min-max-normalized dense score × `passage_node_weight=0.05`.
  5. Run PPR with damping 0.5 and rank passages by PPR mass.
- **Ablation** (avg Recall@5, MuSiQue/2Wiki/HotpotQA, Table 4):

  | Variant | Recall@5 |
  |---|---|
  | Full | 87.1 |
  | Without the LLM filter | 86.4 |
  | Without passage nodes | 81.0 |
  | NER-to-node linking instead of query-to-triple | 74.6 |
  | Query-to-node | 59.6 |

- **Cost:** indexing MuSiQue (11,656 passages) with Llama-3.3-70B took 9.2M input and 3.0M output tokens and 99.5 minutes.
- **Cheap replication over a markdown vault (my design inference, not tested):**
  - Passage nodes = notes or sections. Phrase nodes = wikilink targets and tags, which are already explicit, so no OpenIE is needed.
  - Seeds = BM25/hybrid note scores × 0.05 plus the notes the query's terms resolve to.
  - Run PPR with damping 0.5 using petgraph or a hand-written power iteration: ~20 lines and milliseconds on several hundred nodes.
  - The ablation says the LLM filter is the most dispensable part (−0.7). The biggest loss comes from replacing *triple*-level linking with node linking (−12.5). A wikilink-only graph is therefore closest to the weaker "NER-to-node" variant, so expect smaller gains than the paper reports **[speculative]**.

---

## 3. A small typing scheme for notes

**Primary sources**
- **Discourse graphs.** Base grammar from https://discoursegraphs.com/docs/roam/base-grammar: node types QUE (Question), CLM (Claim), EVD (Evidence), Source. Relations: EVD *informs* QUE, EVD *supports* CLM, EVD *opposes* CLM. Chan's own white paper says efficacy "remains unproven" (https://joelchan.me/assets/pdf/Discourse_Graphs_for_Augmented_Knowledge_Synthesis_What_and_Why.pdf). The main evaluation is a 3-year field deployment with Roam users, *Steps Towards an Infrastructure for Scholarly Synthesis* (venue **[unverified]**).
- **Revised Bloom, knowledge dimension** (Krathwohl 2002, *Theory Into Practice* 41(4), https://www.chemnet.edu.au/sites/default/files/files/Blooms_taxonomy_2002_Krathwohl.pdf):
  - **A. Factual:** terminology; specific details.
  - **B. Conceptual:** classifications; principles and generalizations; theories, models, structures.
  - **C. Procedural:** subject-specific skills and algorithms; techniques and methods; *criteria for when to use* procedures.
  - **D. Metacognitive:** strategic knowledge; knowledge of cognitive tasks; self-knowledge.
- **SKOS** (W3C Recommendation 18 Aug 2009, https://www.w3.org/TR/skos-reference/):
  - `skos:Concept`, `prefLabel/altLabel/hiddenLabel`, `broader/narrower/related`, `definition/scopeNote/example/changeNote/historyNote`, `exactMatch/closeMatch`.
  - `skos:broader` is **not transitive**; `broaderTransitive` is the transitive super-property.
- **Micropublications** (Clark, Ciccarese & Goble, *J Biomed Semantics* 5:28, 2014, https://doi.org/10.1186/2041-1480-5-28): the minimal unit is "a statement with its attribution". Claims are modelled as defeasible and backed by evidence.

**Recommended scheme.** Five `kind`s in frontmatter. Each maps to the sources above.

| kind | Maps to | Required fields | Key relations |
|---|---|---|---|
| `concept` | Bloom B; `skos:Concept` | `aliases` (altLabel, so BM25 hits notation variants such as "BFS"/"breadth-first search"), one-paragraph definition | `broader`, `related`, `prereq` |
| `technique` | Bloom C (Ca/Cb), plus a "when to use" section (Cc) | inputs/outputs, cost model, `applies_to` | `uses` → concept, `implemented_in` → code anchor |
| `result` | discourse CLM; micropublication | `claim` (one sentence), `as_of` date, `status ∈ {current, superseded, disputed}`, `superseded_by`, and **≥1 `source`** (the promotion gate) | `supports`/`opposes` ← evidence or source |
| `question` | discourse QUE | `opened` date, `status ∈ {open, answered, abandoned}` | `informed_by` ← result/source |
| `source` | discourse Source / Graphiti episode | citekey/DOI/URL/commit, `ingested` date, immutable body | substantiates → result |

- I left out a separate EVD type. For one researcher, evidence is a source excerpt (an anchor into a source note) attached to a `result`; that is closer to Graphiti's `episodes[]` provenance on a fact. Metacognitive knowledge (Bloom D) belongs in the learner model (§4), not in notes.
- Daily logs, course pages, and paper reading notes stay as views and sources. A paper note is `kind: source`, and the results extracted from it become `result` notes.

---

## 4. Estimating what the student knows

**Validated**
- Classic knowledge tracing needs graded responses (round 1).
- Adding *non-assessed* activity to KT does help: DMKT, EDM 2021 (https://educationaldatamining.org/EDM2021/virtual/static/pdf/EDM21_paper_233.pdf); multi-activity KT (https://arxiv.org/abs/2301.12916). But both models are still anchored on graded items. Assessed material moved knowledge more than watching videos, and repeated viewing showed diminishing gains.
- Search-as-learning: Yu et al., SIGIR 2018 (https://arxiv.org/abs/1805.00823) predicted knowledge *state* and *gain* from in-session query and browsing features. They used 468 search missions over 11 topics, with pre- and post-tests as ground truth. Query complexity and resource selection indicated knowledge state better than time-on-task (the latter claim is per a secondary summary).
- Negative result: behaviour-only clusters from EdNet "track engagement, not mastery" (preprint, https://arxiv.org/abs/2608.16963).

**Open learner models.** Bull & Kay's SMILI☺ (IJAIED 26(1), 2016, https://doi.org/10.1007/s40593-015-0090-8) is a framework for learner models that the learner can *inspect and contest*. That is the right posture for an LLM-maintained profile: show the estimate and its evidence, and let the student correct it. Negotiated/persuadable OLMs are part of this literature. I did not read those papers in depth, so treat the exact mechanisms as **[unverified]**.

**LLM prerequisite extraction**
- Zero-shot GPT-4 on LectureBankCD prerequisite links: accuracy ≈ 0.746, F1 ≈ 0.777; RAG did not help (Graphusion, https://arxiv.org/abs/2410.17600, Table 2 *(secondary read)*).
- Concept graph recovery: https://arxiv.org/abs/2402.14293 flags possible contamination.
- On ESCO skills, the best of 13 LLMs scored F1_BERT = 0.835 against expert prerequisites (https://arxiv.org/abs/2507.18479). That is a soft semantic metric, and ESCO is public, so memorization is plausible.
- Takeaway: LLM-proposed `prereq` edges are usable as *suggestions* with human confirmation. Recall and precision on niche technical topics are untested.

**Conclusion.** Implicit signals (notes authored, hister searches, pages read) are validated only as *engagement/exposure* signals and as weak priors for knowledge *state*. They are not mastery measures. Treat note authorship plus exposure as "has encountered". Promote to "knows" only on explicit recall (FSRS reviews, Q&A with graded answers). Repeated searches for the same concept are a plausible "not yet retained" signal **[speculative; no study found]**.

---

## 5. hister v0.21 (source read at HEAD; nixpkgs has 0.20.0)

- **Engine:** Bleve v2.6.1 (scorch) for full text. SQLite via GORM for app data. Optional semantic search through **sqlite-vec** (vendored C) or Postgres, with embeddings from any OpenAI-compatible endpoint (default `qwen3-embedding:8b` via Ollama, 512-token chunks with 64 overlap). Hybrid merging is a linear weight, `semantic_weight` (default 0.4; 0 means keyword only). It is not RRF. I did not trace the exact normalization **[unverified]**.
- **Local files: yes.** `indexer.directories: [{path, label, filetypes, patterns, excludes, user}]`. It scans at startup and then runs a **file watcher**. Markdown is indexed as rendered text, and the title is the first H1, falling back to the file path. PDF goes through a built-in reader or a configurable `pdf_parser.command` (stdin PDF → stdout UTF-8, e.g. `pdftotext`). The default `max_file_size_mb` is 1.
- **HTTP API** (`server/api.go` route registry):
  - `GET /search?q=…` returns JSON. It supports date bounds, `semantic`, `semantic_weight`, facets, and cursor pagination; with no `q` it upgrades to a WebSocket.
  - `POST /api/add {url, title, text | html, label, type, source_mtime}` pushes arbitrary documents. `POST /api/add_pdf` takes base64.
  - Also `/api/document`, `/api/update`, `/api/label`, `/api/delete`, `/api/reindex`, `/api/versions` (diffs for URLs matching versioning rules), and `/api/history`.
  - `file://` URLs are accepted only under configured directories (`ErrFileURLNotAllowed`).
- **MCP:** `POST /mcp` (Streamable HTTP) with tools `search`, `get_facets`, `get_preview`, `get_history`. Results sit under `structuredContent.untrusted_content` with `trust: "untrusted"`. Auth is via `app.access_token` or per-user tokens.
- **Nix:** the upstream flake exports a package plus NixOS, Home Manager, and darwin modules.
- **Verdict.** For phone search over the tailnet, **pointing hister's `indexer.directories` at the vault with `label: kb` needs no new code** and gives you the web UI, auth, query language, and MCP. Requirements:
  - every note has an H1 (that becomes the title);
  - big generated files stay under the size cap;
  - PDFs are pre-converted to markdown, so hister indexes the good text and not `pdftotext` output of equations.
- hister's ranking is single-field Bleve plus optional linear semantic merging. It does not do chunk-level reranking or frontmatter-aware filtering. **For the agent's own retrieval, a separate local index (qmd-style, or a small Rust FTS5/tantivy tool) is still worthwhile**, because it can parse frontmatter (`kind`, `status`, `as_of`) and return section-level hits.
- Pushing via `/api/add` is worth it only for content that does not live in the watched directory, such as generated views. Writing those views into the vault is simpler.

---

## 6. Code ↔ knowledge co-evolution

- **RepoAgent** (EMNLP'24 demo; source):
  - Python only. It uses `ast` for structure and `jedi` for references.
  - Each `DocItem` stores `obj_name, item_type, code_start_line, code_end_line, md_content[] (doc versions), reference_who[], who_reference_me[]` and an `item_status` from `{doc_up_to_date, doc_has_not_been_generated, code_changed, add_new_referencer, referencer_not_exist}`.
  - The pre-commit hook parses `git diff` of staged `.py` files and marks a structure changed if any changed line falls in `[start,end]` (`identify_changes_in_structure`; the code has a TODO saying it "may be wrong, needs unit tests").
  - The state lives in `.project_doc_record` JSON.
  - **Lesson:** the reference-based status values are the useful part. A note becomes suspect when its symbol changes *or when its callers change*.
- **DocPrism** (arXiv 2511.00215 → ISSTA, https://doi.org/10.1145/3832248):
  - Problem: naive LLM checking yields "unacceptably high false positive rates" because documentation is abstract by nature.
  - Fix: "Local Categorization, External Filtering", which splits the check into five local subtasks and reports only *incorrectness*.
  - Results: 1,991 pairs in four languages, 15% flag rate, precision 0.62 (v1 abstract). 48% of errors were incorrect reasoning.
  - **Lesson:** run LLM drift checks only on anchors that structural diffing has already flagged, and expect about 40% false alarms.
- **CodeWiki** (Findings of ACL 2026 per repo, https://arxiv.org/abs/2510.24428): dependency-graph decomposition into a module tree (`--max-depth` default 2), recursive leaf-first doc agents, and parent pages synthesized from children. Output: `overview.md`, a page per module, `module_tree.json`, `metadata.json`. It is a regenerate-from-scratch design with no incremental anchoring.
- **deepwiki-open:** an adalflow FAISS retriever over 350-word chunks (`api/config/embedder.json`). It is a RAG wiki generator; I found no drift mechanism.
- **Swimm:** "smart tokens/paths/snippets" verified on every PR. Small changes auto-sync and big ones fail CI. The algorithm is proprietary and patented, so **[internals unverified]**.
- **Concrete anchor format.** Use SCIP symbol strings (https://github.com/sourcegraph/scip/blob/main/scip.proto). The grammar is `<scheme> ' ' <manager> ' ' <package> ' ' <version> ' ' <descriptor>+`, with descriptors `name/` (namespace), `name#` (type), `name.` (term), `name(disamb).` (method). rust-analyzer emits SCIP via `rust-analyzer scip` (checked against the nixpkgs build's `--help`).
  - Suggested note frontmatter: `code: [{repo, path, symbol: "<SCIP symbol>", commit, sig_hash}]`, where `sig_hash` hashes the tree-sitter node text of the signature, or of the whole body.
  - Lint flow: resolve the symbol → missing means `broken`; signature hash changed means `suspect`; references changed means `suspect-low`.
  - Only `suspect` anchors go to a DocPrism-style LLM check.

---

## 7. PDF ingestion for math-heavy papers

| Tool | Equations | Speed / hardware | Licence | nixpkgs (this machine's registry) |
|---|---|---|---|---|
| **marker** (https://github.com/datalab-to/marker) | LaTeX in `$$` via a surya VLM. Inline math needs balanced mode or `--force_ocr`; best with `--redo_inline_math --use_llm` | Vendor figures on one B200 (olmocr-bench): balanced 2.9 pages/s, fast 7.4, no-OCR CPU 23.7. CPU runs via llama.cpp | Code Apache-2.0. Weights: modified OpenRAIL-M, free for research/personal/startups < $5M | **absent** |
| **MinerU** (https://github.com/opendatalab/MinerU, v4.x) | Formula recognition included (model not named on the README) | CPU supported for `basic` (ONNX, 2 GB RAM) and `standard/advanced` (llama.cpp, 8 GB). Torch tiers need ≥ 8 GB VRAM | "MinerU Open Source License" (Apache-2.0 plus conditions) | **absent** |
| **docling** | Optional `--enrich-formula` (CodeFormula model → LaTeX); off by default because it is slow. Has repetition-stopping logic | CPU OK | MIT (nixpkgs meta) | **present, 2.118.0** |
| **Nougat** (Blecher et al. 2023, https://arxiv.org/abs/2308.13418) | Trained end-to-end on arXiv/PMC → markdown + LaTeX | 19.5 s per batch of 6 pages on an A10G; 1.5% of test pages collapse into repetition loops, more out of domain; processes each page independently | Code MIT, **weights CC-BY-NC** | absent |
| **GROBID** | TEI XML. Good for header and references (36 PDF/s header-only; 4,000 full PDFs in 26 min on 8 threads). The docs page does not describe formula handling; it marks formula zones but does not produce LaTeX **[unverified]** | Java, Docker | Apache-2.0 | absent |

- **Benchmarks.**
  - OmniDocBench v1.6_full end-to-end (https://github.com/opendatalab/OmniDocBench), formula CDM: MinerU-pipeline 83.07, Marker 85.24, MinerU2.5-VLM 95.77, PaddleOCR-VL-1.6 97.53. **Caveat:** OmniDocBench comes from the same lab as MinerU, and Docling and Nougat are not scored in the end-to-end table.
  - Marker's own olmocr-bench (vendor): marker balanced 76.0, MinerU pipeline 72.7, docling 50.3.
- **Recommendation.** Use GROBID for metadata and citations (it feeds the `source` note), plus marker or MinerU for the body with LaTeX.
  - Both need packaging. Run them as an OCI container or `uv`-managed tool rather than writing Nix derivations for torch stacks.
  - Cache the output next to the PDF (`paper.md`, plus the tool+version in the frontmatter) so conversion is a one-time cost.
  - docling is the only one already in nixpkgs, but it scores lowest in both benchmarks above.
  - On the GPU box, run MinerU-VLM or marker balanced. On CPU, use marker fast plus `--use_llm` only for pages with math **[suggestion]**.

---

## 8. Existing agent-maintained markdown KBs: lessons

- **Basic Memory** (https://github.com/basicmachines-co/basic-memory):
  - Format: each file is an Entity with frontmatter `title, type, permalink, tags`. Observations are `- [category] text #tag (context)`; relations are `- relation_type [[Target]]`. Bare links index as `links_to`.
  - SQLite (or Postgres) index with FTS plus FastEmbed vectors and an optional cross-encoder rerank. MCP tools `bm_write/read/edit/move/delete/search/list/find`, plus `bm_schema validate|infer|diff`.
  - Open issues in Oct 2026 are mostly about the **derived index**, not the files:
    - #1771: duplicate rows in `search_index`, with no uniqueness guard and a non-atomic reindex.
    - #1773: `doctor` does not surface orphans or inverted timestamps.
    - #1275: NFC/NFD filename variants create duplicate entities (macOS + Syncthing).
    - #1796: case-only twin folders.
    - #1653: frontmatter keys lost on overwrite.
  - **Lesson:** the index must be disposable and rebuildable (`rm index.db && reindex` gives an identical result), keyed by normalized path plus content hash, with schema-preserving frontmatter writes and a doctor that fails loudly.
- **Smart Connections** (Obsidian): long-running user issues about re-embedding loops (#1035), freezes (#356, #473), and #1371, "embedding model silently reverts … and deletes all existing vectors". **Lesson:** store the embedding model id with the vectors, refuse to mix models, and key vectors by content hash (as qmd does).
- **Reor:** a local-first AI notes app (LanceDB, automatic similarity links). **Archived** 2025-05-13 (GitHub `isArchived: true`). Automatic similarity-based linking was its core feature; the README does not give a reason for archiving.
- **Letta:** the V1 server repo is retired. Active work is **Letta Code**, where "all context (including memory blocks) is tracked via git" (MemFS), with skills and `/sleeptime` "dreaming". This independently converges on git-tracked files plus skills plus background consolidation.
- **Khoj** (37.9k★, active) and **Cognee** (graph+vector "cognify" pipeline) are still active. I did not read their internals for this round **[not reviewed in depth]**.
- **Karpathy-pattern repos** on GitHub are small templates (top: balukosuri/llm-wiki-karpathy, 220★). None has a validated schema worth copying.

---

## Design-relevant conclusions

1. **Copy Graphiti's split:** the LLM decides *whether* two facts are incompatible; deterministic code applies `as_of`/`superseded_by`/`status` from explicit dates. Never delete. Find candidates by shared concept links, not over the whole vault.
2. **Put a promotion gate on `result` notes:** no `source` means draft, and drafts cannot be cited. This targets the most reported LLM-wiki failure (compounding self-citation).
3. **Five note kinds** (concept, technique, result, question, source), mapped to SKOS, Bloom, discourse graphs, and micropublications. `aliases` are mandatory for concepts.
4. **Search:** hister covers phone and tailnet search with zero code via `indexer.directories`. The agent gets its own disposable SQLite FTS5 (+ optional vectors) index that parses frontmatter, modelled on qmd's schema, content hashing, and chunk-level rerank. Do not depend on qmd itself unless Bun/node-llama-cpp is acceptable.
5. **Optional graph retrieval:** PPR (damping 0.5) over notes plus wikilinks/tags, seeded by hybrid scores × 0.05. It is cheap to add later, and expected gains are modest without triples.
6. **Code anchors:** SCIP symbol + commit + signature hash. Structural diff first, LLM check second, about 40% false positives expected.
7. **Knowledge estimates:** encountered ≠ known. Only graded recall promotes, and the profile is open/contestable (Bull & Kay).
8. **PDFs:** GROBID for metadata; marker or MinerU (containerised) for LaTeX bodies; cache the markdown beside the PDF together with the tool version.
