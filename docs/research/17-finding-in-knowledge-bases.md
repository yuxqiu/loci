# 17 — Finding things in a knowledge base: agents, search tools and links

Status: research, 2026-11-01. Builds on research 16 (`find` versus grep), 15
(digests versus search), 04 (relations) and 01/02 (GraphRAG, HippoRAG, LongMemEval).
Their findings are cited, not repeated: the production-agent survey, Li et al.'s
BrowseComp-Plus tool comparison, Sen et al.'s grep-versus-vector Table 1, the
scalability and CJK probes, and the "breadth first search" ranking defect are in
research 16.

Context: the user has decided to drop `loci find`; agents will search the vault with
ripgrep (`rg`). Research 16 left one gap: no study of agents searching **personal
notes or wikis**. This note goes after that gap and three related questions: what a
search tool's design changes for an agent, whether links help find related notes,
and what loci should teach, expose and measure.

Nothing here changes DESIGN, the specs, the rules or the skills. Conventions as in
research 12 to 16: `[unverified]` means I did not confirm the claim in a primary
source (the reason is given); `[design inference]` is my extrapolation; `[gap]` means
I looked and found nothing. **(summary)** marks a number read through a fetch tool
that summarises the page rather than from the raw text or PDF; spot-check before
quoting it elsewhere. Every 2026 arXiv paper cited is a preprint.

## TL;DR

| Question | Finding | Strength |
|---|---|---|
| 1. Agents finding things in notes, wikis, knowledge bases | **No peer-reviewed study of agents in a personal-notes vault exists** `[gap]`; the nearest evidence points two ways. *Search wins on scale and recall*: on a 500k-document enterprise corpus a single-shot BM25 top-10 beat a grep/bash agent on correctness (68.8 vs 60.6) and recall (68.4 vs 55.8), while the agent had the best completeness and 4.5× fewer wrong documents (EnterpriseRAG-Bench). *Navigation wins on small, structured corpora*: an agent navigating a 3-level summary hierarchy beat BM25, dense and agentic retrieval on enterprise QA (F1 0.456 vs 0.345–0.398, Corpus2Skill). Personal-memory benchmarks favour finer index keys (adding extracted facts to keys: +9.4% recall, LongMemEval) and BM25 holds its own on small personal corpora (PerLTQA R@1 0.705 vs DPR 0.602). People refinding their own information use keyword search for only 39% of searches (Teevan et al. 2004) and navigate by preference (Bergman et al. 2008) | Several benchmarks, none on personal notes |
| 2. Tool design | **Output volume and delivery matter as much as the engine.** Summarised search output (files and match counts, capped, with "narrow your search") beat iterating through matches 18.0 vs 12.0 and beat no search (15.7) (SWE-agent). A result written to a file instead of inline dropped a grep agent from 93.1 to 55.2 (Sen et al., summary); Claude Code's Bash tool does exactly that past ~30,000 characters. Agents already compose rg well: `rg … \| head` is 56% of their shell calls, chained `rg \| rg` 21%, `wc` 8% (Li et al. App. B.1). Better tool descriptions: +5.85 points median, but 67% more steps and regressions in 1 of 6 cases (MCP smells study). Swapping retrieval tools in or out of a frontier agent moves accuracy only 1–5 points (A-RAG) | Mixed; most from code or web, a few ablations |
| 3. Links as a retrieval signal | **Links carry real but secondary signal, and naive use hurts.** TREC-8: "little benefit" from links and none from PageRank on ad hoc search. Wikipedia "related article" judgements: text similarity beat co-citation on MAP (0.13 vs 0.07), co-citation won on clicks and on well-linked articles; text won on sparsely linked, niche ones (Schwarzer et al. 2016). Common-neighbour scores predict new links 16–55× better than chance; degree alone 5–15× (Liben-Nowell & Kleinberg). In HippoRAG, adding a seed's 1-hop neighbours *lowered* recall@2 (e.g. 2Wiki 59.1 → 53.4) while Personalized PageRank raised it (70.7). On multi-hop QA, graph backends keep a large lead over dense retrieval even under agentic search (RAGSearch, summary) | Strong classic IR; modern evidence on Wikipedia-style multi-hop |
| 4. Do agents use links when exposed? | **Yes when links are the action space, rarely when they are an optional tool.** Showing the page's outgoing links lifted GPT-4.1 on hard WikiGame tasks from 20% to 90% and cut invalid-link rates from 80% to 10%; an 8B model gained nothing. Small coding agents chained graph expansions in only 12–14% of episodes; models used an LSP 0–6% of the time on localisation | Several 2025–26 studies |

**Recommendation.** Teach rg with a small set of task-shaped patterns that keep output
inline and short (`-l`, `-c`, title patterns, phrase regexes, CJK caveats), and say
what empty output means. Keep `loci links` as the link-aware view (outgoing,
incoming, unlinked mentions) and name it in "Finding things" as the step after a
search hit, because the strongest link evidence is that exposed links stop agents
inventing them. Do **not** add hubs. Do not add a "related by shared links" command
yet: the evidence that it helps on a sparse personal vault is weak (text similarity
wins where links are few), so propose it only if `loci bench --agent` shows agents
missing related notes that a co-citation ranking would find. Measure output volume,
file spills, invalid links written and `loci links` use alongside pass rate.

## 1. Agents finding information in notes, wikis and knowledge bases

### 1.1 People refinding their own information

Agents in loci act on the user's behalf in the user's own vault, so how people refind
personal information is the closest human baseline.

- **Teevan, Alvarado, Ackerman & Karger, "The perfect search engine is not enough: a
  study of orienteering behavior in directed search"**, CHI 2004, pp. 415–422,
  doi:10.1145/985692.985745 (read as PDF). Diary study of 15 graduate students
  searching email, files and the web. "Our participants used keyword search in only
  39% of their searches, despite almost always knowing their information need up
  front"; for specific information "only 34 of the 81 searches … (42%) involved
  keyword search". Keyword search is defined to include "the UNIX grep or find
  commands". People *orienteer*: take small steps through known structure, using
  search as one tactic inside that strategy. Reasons given: less need to articulate
  the target, a "sense of location", and context that tells them they found the right
  thing ("I wouldn't have known that that was the file I wanted until I saw all of the
  other names in the same directory").
- **Bergman, Beyth-Marom, Nachmias, Gradovitch & Whittaker, "Improved search engines
  and navigation preference in personal information management"**, ACM TOIS 26(4),
  2008, doi:10.1145/1402256.1402259 (abstract via the Open University of Israel
  record, **summary**): users "estimated that they used navigation for 56 - 68% of file
  retrieval events but searched for only 4 - 15% of events", and better desktop search
  did not raise search use. Self-reported estimates; their 2010 paper quotes the same
  study as 4–13% `[unverified]` which range is right.

**For loci** `[design inference]`: the human evidence says people find known items
in their own collections by navigating structure as much as by searching. An agent is
not a person (it has no memory of where it filed things), but loci gives it the same
structure to orienteer through: area scope notes (`AREA.md`), sub-folders, views and
links. Rules that teach only rg would leave that structure unused.

### 1.2 Wiki-navigation agents

- **Margiotta, Croce & Basili, "Evaluating Large Language Models on Wikipedia Graph
  Navigation: Insights from the WikiGame"**, CLiC-it 2025, CEUR-WS vol. 4112 (read as
  PDF). 120 start–target pairs binned by human success rate. Three conditions: *blind*
  (names only), *chain-of-thought*, *link-aware* (the current page's outgoing links
  shown at each step). Table 1, success on *Hard* games: GPT-4.1 20.0% blind → 90.0%
  link-aware; GPT-4o-mini 10.0% → 86.7%; GPT-4.1-nano 10.0% → 20.0%; Llama 3.1 8B
  10.0% → 3.3%; humans 34.4%. Table 2, invalid-link rate on Hard games for GPT-4.1:
  80.0% blind, 10.0% link-aware. "Invalid links represent the dominant failure mode"
  when links are not shown; chain-of-thought "only marginally reduces" them.
- **Hickey, Knight & Bishop, "Can Language Models Play the Wikipedia Game?"**, arXiv
  2610.13948 (Oct 2026, **summary**). Agents see only the current page's link titles.
  "No model significantly outperformed humans"; many sentence-embedding agents
  (greedy: pick the link whose summary is closest to the target's) matched the largest
  open non-reasoning LLMs, and never hallucinated a link.
- **West & Leskovec, "Automatic versus Human Navigation in Information Networks"**,
  ICWSM 2012 (read as PDF), on 30,000+ human Wikispeedia paths: simple agents using
  TF-IDF similarity to the target found shorter paths than humans (humans need "twice
  the optimal number of clicks on average", the best agent 1.5×). A degree-only agent
  ranked "far off". Humans use hubs to get away from the start and similarity to home in
  on the target: "degree to get away from s, similarity to home in on t".
- **WebWalker** (Wu et al., arXiv 2501.07572, **summary**): agents traversing a
  website's subpages by links. Best overall accuracy 37.5% (GPT-4o); accuracy falls
  with depth (multi-source: 47.5 easy → 15.8 hard). Commercial search-RAG systems
  scored 16.8–40.7%.
- **The Amazing Agent Race** (Kim et al., arXiv 2604.10261, **summary**): Wikipedia
  tasks with tool chains; best 37.2%. Navigation errors 27–52% of trials, tool-use
  errors under 17%. Failed trials issued 56% more web searches (8.1 vs 5.2) and more
  tool calls (21.7 vs 16.5): more searching was a symptom of being lost, not a cure.
- **ReAct** (Yao et al., ICLR 2023, arXiv 2210.03629, §3.1, **summary**) gave the agent
  a three-action Wikipedia interface: `search[entity]` returns the first 5 sentences of
  the page "or else suggests top-5 similar entities", `lookup[string]` returns "the
  next sentence in the page containing string" (in-page grep), `finish`. Uninformative
  search results caused 23% of ReAct's errors and "derail[ed] the model reasoning".
- **WebArena** (Zhou et al., ICLR 2024) hosts an offline Wikipedia as a knowledge
  resource but reports no wiki-specific results (**summary**). Mind2Web wiki subsets:
  not checked `[gap]`.

These are pathfinding or browsing tasks over Wikipedia, not "find my note on X". What
transfers: (a) when an agent must name a link target, showing it the real targets
removes most invented ones; (b) similarity to the goal, not node degree, guides
navigation; (c) small models gain little from structure they must reason over.

### 1.3 Personal-memory benchmarks

- **LongMemEval** (Wu et al., ICLR 2025, arXiv 2410.10813, **summary**). §5.3 Table 3:
  adding extracted facts to the index keys ("K = V + fact") raised recall@5 from 0.706
  to 0.732 (session values) and 0.582 to 0.644 (round values), "an average gain of 9.4%
  in recall@k and 5.4% in final accuracy"; summaries or keyphrases alone as keys did not
  help. App. E.2 Table 9 (K = V, session): recall@5 BM25 0.634, Contriever 0.723,
  Stella 1.5B 0.720. Time-aware query expansion: +6.8% to +11.3% recall on temporal
  questions, but only with a strong model doing the expansion (§5.4).
- **LoCoMo** (Maharana et al., ACL 2024, arXiv 2402.17753, Table 3, **summary**): with
  GPT-3.5, retrieving *observations* (short assertions extracted from dialogue) at top-5
  gave F1 41.4 against 31.7 for raw dialogue turns and 32.5 for session summaries; F1
  fell as k grew (observations top-50: 37.8) "due to a lower signal-to-noise ratio".
- **PerLTQA** (Du et al., SIGHAN 2024, aclanthology 2024.sighan-1.18, Table 5, read as
  PDF): memory retrieval R@1 BM25 0.705, DPR 0.602, Contriever 0.486; DPR overtakes
  BM25 only from k = 3 and is about 100× slower (2.96 s vs 0.03 s).
- **MemoryBank** (Zhong et al., AAAI 2024): not re-read; it evaluates a memory-update
  mechanism, not search `[gap]` for this question.
- Letta's filesystem agent on LoCoMo and Sen et al. on LongMemEval are in research 16.

**For loci** `[design inference]`: the consistent memory-benchmark result is that
*what is indexed* matters more than the retriever. Short, assertive units retrieve
better than raw logs or summaries, and extra keys (facts, other names) raise recall.
For a grep agent the analogue is text: a concept note whose H1 or first lines carry
its other names ("Breadth-first search (BFS)") is found by more patterns. The
rules already put other names in the H1 or text (rules, "Writing concept notes").

### 1.4 Enterprise knowledge bases

- **EnterpriseRAG-Bench** (arXiv 2605.05253, May 2026, Tables 6–7, **summary**):
  ~500,000 synthetic documents across nine sources (Slack, Jira, Confluence, …), 500
  questions; GPT-5.4 answers for every arm. A **bash agent** ("grep, find, head, etc.",
  10-minute budget, low reasoning) against **single-shot BM25** and **vector** top-10:
  correctness 60.6 / 68.8 / 51.4; document recall 55.8 / 68.4 / 46.0; completeness
  61.1 / 56.0 / 42.9; invalid extra documents 2.0 / 9.0 / 9.3. The agent lost most on
  "Project Related" questions (37.5 vs 60.0) and won on recall for "Completeness"
  questions (59.0 vs 46.5), because "once it reaches a relevant document, it can explore
  nearby files and similar patterns". All three scored 100% on "Info Not Found". This
  is the one recent result where single-shot BM25 beats a grep agent, at a scale
  (500k documents) two orders above a personal vault, and with the agent at low
  reasoning effort.
- **AgenticRAG** (Suresh et al., Microsoft, arXiv 2605.05538, **summary**): tools
  `search` (up to five query rewrites per call, 10 results each with snippet and
  metadata), `find` (in-document substring match, up to 2 passages per pattern),
  `open` (1,800-line windows with a "Viewing lines [0–1799] of 3000" header). BRIGHT
  recall@1: single-shot 8.41%, agentic 43.49% (GPT-5-mini) and 49.59% (Sonnet 4.5), at
  2.6× the tokens. Ablation (Table 5): removing *semantic* matching from in-document
  `find` **raised** R@1 (46.34 vs 43.49); removing multi-query search left recall flat
  but raised tool calls 42% (6.79 vs 4.79).
- **Corpus2Skill, "Don't Retrieve, Navigate"** (Sun et al., arXiv 2604.14572, Table 1,
  **summary**): the corpus is compiled into a 3-level tree of `SKILL.md`/`INDEX.md`
  files with one-line summaries; the agent navigates down and fetches documents. WixQA
  (6,221 documents): token F1 0.456 against BM25 0.345, dense 0.364, RAPTOR 0.398 and
  an agentic BM25/dense/hybrid baseline 0.378; context recall 0.708 vs 0.452 for the
  agentic baseline. A wide tree (20 children per node) fell to F1 0.382 (Table 3).
- **HERB** (arXiv 2506.23139, abstract only): best agentic RAG scores 32.96 on 39,190
  heterogeneous enterprise artefacts; "retrieval as the main bottleneck".

**For loci** `[design inference]`: two results bear directly on a vault. First, an
agent exploring "nearby files" gets completeness that top-k retrieval lacks; loci's
areas and sub-folders give rg agents that neighbourhood for free (`rg -l … concepts/<area>/`,
`ls`). Second, Corpus2Skill is loci's area tree by another name (one-line summaries
per node, progressive disclosure), and it beat both flat retrieval and an agent with
search tools. Its narrow-tree result agrees with research 14 on bounding area size.

### 1.5 GraphRAG and agentic search on document corpora

- **"Do We Still Need GraphRAG?" (RAGSearch)**, Fan, Xue, Liu & Tan, arXiv 2604.09666
  (Apr 2026, Table 1, **summary**; Qwen-2.5-7B). Multi-hop exact-match (HotpotQA / 2Wiki
  / MuSiQue), dense vs graph backend: no agent 19.0 / 35.5 / 21.0 vs 46.7 / 62.6 / 48.0;
  Search-o1 agent 33.8 / 29.6 / 12.6 vs 42.8 / 65.6 / 32.4. Single-hop (NQ, PopQA,
  TriviaQA) is within a few points either way. Agentic search narrows the multi-hop gap
  only somewhat. Graph construction cost per 1M tokens: 0.7–1.7 h and $0–13 (Table 8).
  The graph backends are entity/passage graphs built by an LLM, not author-written
  links; the agent is 7B.
- Han et al. (arXiv 2502.11371) and GraphRAG-Bench (arXiv 2506.05690): plain RAG wins
  single-hop fact lookup, graphs win multi-hop and summarisation (research 01).
- **Agentic RAG surveys.** Singh et al., "Agentic Retrieval-Augmented Generation: A
  Survey" (arXiv 2501.09136) is a taxonomy with a qualitative comparison and no
  measurements (abstract read). I found no survey with a controlled measured comparison
  of agentic designs `[gap]`; the measured comparisons are the individual papers above.

### 1.6 Obsidian- and Notion-style vaults

No peer-reviewed study of agents searching a personal vault exists `[gap]`. What exists
is tooling with self-reported numbers:

- **obsidian-hybrid-search** (github.com/flowing-abyss/obsidian-hybrid-search, README,
  **summary**): BM25 + fuzzy title + vectors fused by RRF; on the Obsidian Help vault
  (171 notes, 58 queries) nDCG@5 0.733 vs qmd 0.659; on Andy Matuschak's notes (1,357
  notes, 5,000+ links, 78 hand-judged queries) nDCG@5 0.722. It attaches `links` and
  `backlinks` to every result but does not use them for ranking and reports no link
  ablation. No grep baseline.
- **Basic Memory** (github.com/basicmachines-co/basic-memory, README, **summary**):
  markdown notes with wikilink relations; tools for hybrid search and a `bm_context`
  tool that walks the link graph ("a traversable graph the LLM can follow link by
  link"). No benchmark results published.
- **Obsidian LLM Wiki plugin** (community.obsidian.md/plugins/karpathywiki): retrieval
  by Personalized PageRank over wikilinks, "PPR @5 = 27.1% vs pure-kNN baseline 24.1%"
  on an undescribed corpus `[unverified]`: no corpus size, query count or method given.
- **vault-agent-bench** (github.com/jppaolim/vault-agent-bench): 204 runs of 13
  harnesses updating one private Obsidian vault from a transcript; the method notes
  report nothing on how agents searched or on duplicates `[gap]`.
- **Obsidian Graph Analysis plugin** (github.com/SkepticMystic/graph-analysis): offers
  Jaccard, Adamic-Adar, common neighbours and a co-citation proximity score ("extra
  weight when the two notes are cited close together") to humans. Evidence that the
  PKM community wants link-based relatedness, not that it helps.

## 2. Search-tool design: which features measurably help

| Feature | Measured evidence | Source |
|---|---|---|
| **Summarised result list** (files + match counts) instead of paging through matches | 18.0 vs 12.0 resolved (iterative); no search at all 15.7. Iterative search made agents "look through every match exhaustively" | SWE-agent, Yang et al., NeurIPS 2024, Table 3 and §5.1 (summary) |
| **Result cap with a steering message** | SWE-agent's `search_dir` prints `./path (N matches)` per file and refuses above 100 files: "More than N files matched … Please narrow your search" (paper: 50) | `tools/search/bin/search_dir` (SWE-agent main); paper App. A.1 |
| **Viewer window size** | 100 lines 18.0; 30 lines 14.3; full file 12.7 | SWE-agent Table 3 |
| **Inline versus file delivery** | Codex CLI / GPT-5.4 grep: inline 93.1, results-in-a-file 55.2; vector ahead of grep in 5 of 10 pairs under file delivery. The extra "locate, open, integrate" step is brittle | Sen et al., arXiv 2605.15184, Table 1, §4.1.4 (summary) |
| **What a large output becomes** in Claude Code | Bash results "inline up to roughly 30,000 characters by default; past that, the path of a file saved to the session directory … plus a preview of up to the first 2,000 characters" | Claude Code tools reference, read 2026-11-01 |
| **Default to paths, not lines** | Claude Code's Grep tool (where present) defaults to `files_with_matches`; `count` mode gives per-file counts plus a total even when the list is truncated | same page, "Grep tool behavior" |
| **Concise versus detailed responses** | a `response_format` enum; Slack example 206 vs 72 tokens; Claude Code caps tool responses at 25,000 tokens. No accuracy numbers | Anthropic, "Writing effective tools for agents — with agents" (11 Sept 2025) |
| **Steering errors and truncation notes** | "prompt-engineer your error responses to clearly communicate specific and actionable improvements"; no numbers. ReAct: empty or unhelpful search results caused 23% of errors | Anthropic (same); ReAct §3.3 (summary) |
| **Better tool descriptions** | +5.85 points median success across models and domains; +67.46% execution steps; worse in 16.67% of cases; dropping the *examples* section did not hurt; compact variants often matched full ones | Hasan et al., "MCP Tool Descriptions Are Smelly!", arXiv 2602.14878, abstract and §5 (summary) |
| **Documentation versus demonstrations** | documentation alone matched few-shot demos on existing benchmarks and beat demos without docs on a new one; no figures in the abstract | Hsieh et al., arXiv 2308.00675 (abstract) |
| **Multiple query rewrites per call** | recall unchanged, 42% fewer tool calls | AgenticRAG Table 5 (summary) |
| **Semantic versus keyword in-document matching** | removing semantic matching raised R@1 43.49 → 46.34 | AgenticRAG Table 5 (summary) |
| **Keyword / semantic / chunk-read tools** | removing any one cost 0–5 points (largest: semantic on MuSiQue 74.1 → 69.4); keyword search returns sentence-level snippets, top 5 by default, and tells the agent snippets are not enough to answer | A-RAG, Du et al., arXiv 2602.03442, Table 2 (summary) |
| **Output truncation level** | caps of 50K/20K characters, compaction, summarisation: accuracy 69–77 across five levels, no monotone trend | Li et al., arXiv 2605.05242, Tables 1 and 6 (summary) |
| **How agents actually use rg** | of 3,168 shell commands: `rg \| head` 56.2%, `rg \| rg` 20.6%, `wc` 7.8%, single-keyword `rg` 6.3%, `ls` 4.3%, `cat` 0.1%; regex alternation (`a\|b\|c`) and `xargs grep -l` in case studies | Li et al. App. B.1 (summary) |
| **Line context** (`-C`), **regex versus keyword**, **ranking inside grep** | no ablation found `[gap]` | — |

What the table says `[design inference]`:

- The measured wins are about **volume and shape**: a count-per-file summary, a cap
  that tells the agent to narrow, a viewer that is neither tiny nor whole, results kept
  inline. With rg, all of these are flags and pipes the agent already uses (`-l`, `-c`,
  `| head`, `| rg`); what the framework can add is telling it when to use them.
- **Claude Code converts a large rg output into Sen et al.'s file condition.** A common
  word at 10k notes prints ~667 KB as lines (research 16 §3), far past 30,000
  characters, so the agent gets a file path and a 2,000-character preview. Keeping
  searches under that size is not only a token saving; it keeps the agent in the
  condition where grep did well.
- **Description quality helps modestly and costs steps.** The loci rules are the
  "description" of rg-in-a-vault; short task-shaped patterns are the compact variant the
  MCP study found as good as the full one.
- **Empty results need a sentence.** rg prints nothing and exits 1 on no match. ReAct's
  23% and research 16's silent-zero-hit defect point the same way: say in the rules what
  no output means and what to try next.

## 3. Links as a retrieval signal

### 3.1 Classic IR and bibliometrics

Definitions, not re-read: **bibliographic coupling** (two documents cite the same
work; Kessler, American Documentation 14(1):10–25, 1963); **co-citation** (two
documents are cited by the same work; Small, JASIS 24(4):265–269, 1973); **HITS**
hubs and authorities (Kleinberg, JACM 46(5):604–632, 1999); **PageRank** (Brin &
Page, Computer Networks 30:107–117, 1998).

- **TREC-8 Web Track** (Hawking, Voorhees, Craswell & Bailey, TREC 1999, read as PDF):
  "Little benefit was derived from the use of link-based methods, for standard TREC
  measures on the WT2g collection." Four of 20 link runs were "slightly higher
  (numerically)" than their baselines (Table 6). On the 100 GB collection, "one group
  investigated the use of PageRank scores and found no benefit". Links did help find
  documents "which linked to documents judged relevant": a neighbourhood signal, not a
  relevance signal.
- **Anchor text for site finding** (Craswell, Hawking & Robertson, SIGIR 2001,
  doi:10.1145/383952.383999): widely cited as showing that ranking by incoming anchor
  text beats page content for finding a site's home page; numbers `[unverified]`: the
  full text is closed and the abstract is elided in Semantic Scholar. Known-item and
  navigational queries are where links have helped most in web IR.
- **Boyack & Klavans, "Co-citation analysis, bibliographic coupling, and direct
  citation"**, JASIST 61(12):2389–2404, 2010 (abstract, **summary**): clustering 2.15M
  biomedical articles, "bibliographic coupling slightly outperforms co-citation
  analysis"; "direct citation is the least accurate mapping approach by far"; a
  coupling + text hybrid was best. Later work at 40M+ documents reversed the direct
  citation result with an extended variant `[unverified]`, secondary report only.
- **Schwarzer et al., "Evaluating Link-based Recommendations for Wikipedia"**, JCDL 2016
  (read as PDF). Related-article retrieval on 779,716 articles with "See also" links as
  gold, and on clickstreams: MAP@10 text similarity (Lucene MoreLikeThis) 0.13,
  co-citation proximity (CPA) 0.07, plain co-citation 0.03 (Fig. 6); clicks at k = 10
  CPA 83.87, MLT 80.64, CoCit 38.34. The abstract: MLT "performs well in identifying
  narrowly similar articles that share similar words and structure"; link measures
  "are better able to identify topically related information". Text similarity was
  stronger for niche articles; CPA's scores rose with the number of in-links and
  were weakest for articles with few (§4.3). The average article had 20.5 in-links.
- **Liben-Nowell & Kleinberg, "The link-prediction problem for social networks"**
  (CIKM 2003; JASIST 2007; read as the 2004 preprint, Table 2): improvement over a
  random predictor across five co-authorship networks: common neighbours 18.0–47.2×,
  Adamic/Adar 16.8–54.8×, Jaccard 16.4–42.3×, preferential attachment (degree product)
  4.7–15.2×. "A number of proximity measures lead to predictions that outperform chance
  by factors of 40 to 50."

### 3.2 Modern link-aware and graph-augmented retrieval

- **Asai et al., "Learning to Retrieve Reasoning Paths over Wikipedia Graph"**, ICLR
  2020, arXiv 1911.10470 (abstract): retrieving along Wikipedia's link graph beat the
  previous best on HotpotQA "by more than 14 points".
- **Xiong et al., Multi-hop Dense Retrieval (MDR)**, ICLR 2021, arXiv 2009.12756
  (abstract): matched "the best published accuracy on HotpotQA while being 10 times
  faster" and "does not require access to any corpus-specific information, such as
  inter-document hyperlinks". Links were useful for multi-hop retrieval until an
  iterative query-rewriting retriever made them unnecessary, which is what an agent
  doing repeated rg searches also does.
- **LinkBERT** (Yasunaga, Leskovec & Liang, ACL 2022, abstract): pretraining with
  linked documents in context, "+5% absolute improvement on HotpotQA and TriviaQA".
- **HippoRAG** (Gutiérrez et al., NeurIPS 2024, arXiv 2405.14831, Tables 2 and 5,
  **summary**): recall@2 / @5 averaged over MuSiQue, 2Wiki, HotpotQA: BM25 46.5 / 58.4,
  ColBERTv2 53.9 / 65.6, HippoRAG 57.4 / 72.9. Ablation of the graph step: using the
  query's nodes only gave 2Wiki 59.1 / 61.4; **adding their 1-hop neighbours gave 53.4
  / 74.7** (worse at @2, better at @5); full PPR 70.7 / 89.1. Dumping neighbours dilutes
  the top of the list; a ranked spread (PPR with node specificity) helps. HippoRAG 2's
  ablations are in research 02 §2.3.
- **A-Mem** (Xu et al., NeurIPS 2025, arXiv 2502.12110, Table 3, **summary**): on
  LoCoMo multi-hop F1, no link generation and no evolution 9.65; links without
  evolution 21.35; full 27.02. The paper is inconsistent on whether linked memories are
  added at retrieval time (Fig. 2 caption says yes, Eq. 10 says top-k by cosine only),
  so the gain cannot be attributed to link-following.
- **GraphReader** (Li et al., EMNLP 2024, arXiv 2406.14550, abstract): an agent with
  a 4k window reading nodes "and their neighbors" in a graph built from a long text
  "consistently outperforms GPT-4-128k across context lengths from 16k to 256k".
- RAGSearch (§1.5): graph backends keep their multi-hop lead under agentic search.

### 3.3 Do agents use link structure when it is exposed?

- **As the action space, yes and to great effect** for large models: the WikiGame
  link-aware condition (§1.2) lifted GPT-4.1 from 20% to 90% on hard games and removed
  most invalid links. Llama 3.1 8B did not benefit.
- **As an optional tool, rarely.** Bellam, "How Do Small Coding Agents Use Code Graphs
  and Embeddings?" (arXiv 2610.33958, abstract): the graph tools' gain "comes from name
  lookup at hop 0 and one expansion at hop 1"; agents chain expansions "in only 12-14%
  of episodes"; larger models use the tools less. An LSP study (arXiv 2608.13568,
  abstract): models "default to grep on localization (0-6% semantic use)" but reach for
  the LSP "about half the time" on reference-finding tasks.
- **Hubs orient, they do not answer.** Degree-guided navigation did badly (West &
  Leskovec); degree-based link prediction was 2–7× weaker than common neighbours
  (Liben-Nowell & Kleinberg); PageRank gave no benefit at TREC-8.

**What 3 means for a personal vault** `[design inference]`:

- A loci vault has author-written links, the kind that classic IR found useful for
  navigational and known-item tasks and for reaching pages *near* relevant ones, and
  weak for ranking by relevance.
- Personal vaults are sparse: hundreds of concepts, a handful of links each, against
  Wikipedia's 20.5 in-links per article. Schwarzer et al. found text beat co-citation
  exactly where links were few. So a co-citation or coupling "related notes" list would
  be noisy on most notes, and its value grows with link density.
- If loci ever ranks neighbours, HippoRAG's ablation says to rank them (shared links
  weighted by rarity, as in Adamic/Adar, or PPR), never to list a whole neighbourhood
  as equals.
- One hop is what agents use (Bellam: hop 0 and hop 1). Multi-hop traversal tools are
  unlikely to be used.

## 4. Implications for loci

### 4.1 rg patterns the framework rules should teach

Checked against ripgrep 15.2.0 on a six-note probe in the session scratchpad (not
committed), alongside research 16 §3–4.

| Task | Pattern (neutral examples) | Why |
|---|---|---|
| Is there a note on X? (known item by name) | `rg -il '^# .*dijkstra' concepts/` and `rg --files concepts/ \| rg -i dijkstra` | titles and slugs; paths only. Title-first matches how people and Wikispeedia agents home in: by name, then similarity |
| Mentions in text, a list of notes | `rg -il 'dijkstra\|shortest.path' concepts/ views/ journal/` | `-l` keeps output to paths (research 16: 43 KB of paths vs 667 KB of lines for a common word at 10k notes) |
| A common word: survey before reading | `rg -ic heap concepts/ \| sort -t: -k2 -nr \| head -20` | SWE-agent's summarised form (files + counts); Claude Code's `count` mode |
| Phrase with spelling variants | `rg -i 'breadth.first'` (hits "breadth-first", "breadth first", and a non-breaking hyphen) | research 16's ranking defect came from treating a phrase as separate words |
| Short Latin word or acronym | `rg -w -i bfs` | `-w` stops `bfs` matching inside other words |
| Both terms in one note (AND) | `rg -il dijkstra concepts/ \| xargs rg -il 'negative.weight'` | agents already chain (`rg \| rg` is 21% of calls in Li et al.); file-level AND, not line-level |
| Literal text with regex characters | `rg -F 'C++'` | `+`, `.`, `(` are regex syntax by default |
| Read around a hit | `rg -n -C2 'relaxation' concepts/graphs/dijkstra.md` | one file at a time; no study measures context lines `[gap]` |
| Within one area first | `rg -il … concepts/graphs/` then widen | agents gain completeness by exploring "nearby files" (EnterpriseRAG-Bench); areas are the neighbourhood |

**Volume rule.** Prefer `-l` or `-c` first; read lines only from a short list of
files. In Claude Code a Bash result over ~30,000 characters becomes a file path plus a
2,000-character preview (§2), the delivery under which grep did worst in Sen et al.
A plain rule: if a search may hit more than a few dozen notes, list or count first.

**Empty output.** `rg` prints nothing and exits 1 when nothing matches. Rules should
say: no output means no match *in the paths given, for that spelling*; try other names,
a shorter stem, the other script, then conclude the note does not exist.

**CJK caveats** (probe results):

- **Never use `-w` with Chinese or Japanese.** `rg -w 京都` matched only the H1 line
  `# 京都` and missed the body line `京都は古都である`; `rg -w 语言` matched nothing in
  `大型语言模型很大`. In Rust's regex, Han and kana are word characters, so a word
  boundary exists only next to spaces or punctuation.
- **Substring precision:** `rg 京都` also hits `東京都` (research 16 §4), and so does the
  title pattern `rg '^# .*京都'`. Check titles of the hits before concluding.
- **Korean:** search the stem without its ending (`공부`, not `공부하다`, to hit
  `공부했다`).
- **Unicode normalisation:** rg does not normalise. An NFC query `café` did not match an
  NFD `cafe` + U+0301 in the probe. Lint enforces NFC for paths, not text (research 16
  §5); text pasted from some sources may be NFD `[design inference]`. A note for the
  rules, or a lint, if it shows up in practice.
- Case: `-i` folds Unicode case; smart case (`-S`) folds only when the pattern is all
  lowercase.

### 4.2 Should loci expose link structure to help finding?

What loci has today: `loci links <note>` lists outgoing links (with prereqs marked),
incoming links, and **unlinked mentions** (notes naming the note by H1, its
parenthesised alias or its slug, ASCII case-folded, `-` as space, whole words for
Latin; `loci-core/src/index.rs` `unlinked_mentions`). `loci closure` walks prereqs;
`loci export` follows links to a depth.

| Option | Evidence for | Evidence against | Verdict `[design inference]` |
|---|---|---|---|
| **Keep `loci links` and teach it** as the step after a search hit (what links here, what this links to) and before writing links | link-aware agents nearly stop inventing link targets (WikiGame: invalid links 80% → 10%); one hop is what agents use (Bellam); backlinks are the "sense of location" people orienteer by (Teevan) | none | **Do.** Rules: after finding a note, `loci links` it to see its neighbourhood; write `[[slug]]` only for targets seen in `ls`, rg output or `loci links` |
| Unlinked mentions for exhaustive "every note that mentions X" | already a whole-name, case- and hyphen-folded search; covers rules step 4 better than top-k ever did | misses other names not in the H1 or slug, and CJK inflection | **Keep**; the rules may add one rg pass for other names |
| **Related by shared links** (co-citation + coupling, ranked by Adamic/Adar) as a section of `loci links` | link-proximity scores carry 16–55× chance signal (Liben-Nowell & Kleinberg); co-citation finds "topically related" articles text misses (Schwarzer); PPR beat seeds in HippoRAG | text beat co-citation on MAP 0.13 vs 0.07 and on sparsely linked articles; vaults are sparse; agents rarely use optional graph tools (0–14%); no vault evidence | **Not now.** Propose it (a short design note for approval) only if bench shows related-note misses that shared links would catch |
| A neighbourhood dump (all notes within N hops) | — | HippoRAG: adding 1-hop neighbours lowered recall@2 (59.1 → 53.4); agents chain hops in 12–14% of episodes | **Reject** |
| **Hubs** (most-linked notes) | humans pass through hubs early (West & Leskovec) | degree-only navigation "far off"; preferential attachment weakest predictor; PageRank no benefit at TREC-8; loci already has hand-written orientation: `AREA.md` scope notes and views | **Reject**; the area scope notes are the hubs |

### 4.3 What to measure with `loci bench --agent`

Research 16's plan compared grep-only against find-available. With `find` gone, the
open questions are whether the **rg rules** work on note-shaped tasks, whether
**`loci links`** earns its place in the rules, and whether a **related-notes**
feature would help.

**Vault.** Research 16's neutral sample vault (150–400 concepts, ~15% CJK), padded with
generated filler to 1k and 10k notes so that scale changes and targets do not. Record
link density (mean in-links per concept); the link evidence (§3) depends on it.

**Arms** (same model, harness and prompt; only the rules differ):
- **R0**: rules with "use rg" and nothing more.
- **R1**: the rg patterns of §4.1 (volume rule, empty-output rule, CJK caveats).
- **R2**: R1 plus "after a hit, run `loci links`; write links only to targets you saw".
- One frontier and one small model (the WikiGame and Bellam results differ by size).

**Case classes** (`bench/retrieval.jsonl`), adding to research 16's list:
- known item by title; known item by body words only; multi-word phrase with
  distractors; vocabulary gap; CJK (Chinese inside a longer run, kanji-only query,
  Korean dictionary form, `京都`/`東京都` precision trap); absent item;
- **exhaustive**: "every note that mentions X" (3–8 expected);
- **related notes**: "Which notes are most related to X?" with gold sets written by
  hand before running, split into notes that share links with X and notes that share
  only words, so the result says whether shared links would have helped;
- **filing**: "Which area and folder should a note on X go in?" (expect an area path);
- **linking**: "Add a note on X and link it from the notes that mention it", scored by
  `loci lint` (unresolved links) and by the expected incoming links.

**What to read** (bench reports pass rate and median time; the rest comes from
transcripts or the vault after the run):
- tool calls per case, by kind: `rg`/`grep`, `ls`, `loci links`, file reads;
- characters of tool output per case, and the **share of rg calls whose output passed
  ~30,000 characters** (spilled to a file in Claude Code);
- use of `-l`, `-c`, `| head`, `-w` (and `-w` with CJK);
- **invalid links written** (unresolved `[[…]]` from `loci lint`) for the linking cases;
- for exhaustive and related cases, recall against the gold set. bench passes a case
  when *any* expected item is mentioned (m1-m4 §3), so these need hand scoring or an
  all-of mode.

**Decision rules** (fix before running):
- Adopt R1's patterns if R1 beats R0 on pass rate or on output volume at equal pass
  rate, beyond the noise of three repeats.
- Keep the `loci links` rule (R2) if it lowers invalid links written or raises
  exhaustive or related recall; drop it from "Finding things" if agents ignore it and
  nothing changes.
- Propose a related-by-shared-links section only if the related-notes cases miss gold
  notes that share links with X in more than about a quarter of cases in R2, and those
  misses are not fixed by one more rg search.
- If vocabulary-gap cases dominate the misses, that is still the case for embeddings
  (research 16 option e), measured as its own arm.

## Recommendations

1. **Rules: teach rg by task, with volume and empty-output rules** (§4.1): title and
   slug patterns for "is there a note", `-l` for lists, `-c | sort | head` before
   reading a common word, `breadth.first` for phrases, `-w` only for short Latin words,
   `-F` for literal text, area-first scoping; never `-w` for Chinese or Japanese; check
   hit titles for Han substring traps; Korean stems. Main session's files; not edited
   here.
2. **Rules: put `loci links` in "Finding things"** as the follow-up to a hit and as the
   source of link targets. It is the one link feature with direct evidence (exposed
   links stop invented links).
3. **No new link commands now.** Reject hubs and neighbourhood dumps. Hold
   related-by-shared-links until bench shows a need; if it comes, rank by shared links
   with rarity weighting (Adamic/Adar) inside `loci links`, and propose it first.
4. **Bench:** run R0/R1/R2 on the case classes in §4.3, and record output volume, file
   spills, invalid links and `loci links` use, not only pass rate.
5. **Notes stay findable by text:** keep other names in the H1 or first lines (the
   memory-benchmark result that extra keys raise recall); this is what a grep agent and
   `unlinked mentions` both depend on.

## Gaps

- No study of LLM agents searching a personal markdown vault, peer-reviewed or
  controlled, with grep or anything else `[gap]`.
- No ablation of rg-specific features (context lines, `-l` versus lines, regex versus
  fixed strings) for agents `[gap]`; the nearest are SWE-agent's search-output and
  viewer ablations.
- No measurement of whether agents use a backlinks or related-notes tool on notes, or
  of co-citation on a sparse personal link graph `[gap]`.
- CJK search behaviour of grep agents is unstudied (research 16 also found nothing).
- Several 2026 numbers here are read through a summarising fetch (**summary**);
  EnterpriseRAG-Bench, RAGSearch and Corpus2Skill in particular should be spot-checked
  before they are quoted in DESIGN or the rules.

## Sources

Personal information management and navigation:
- Teevan, Alvarado, Ackerman & Karger, CHI 2004: https://doi.org/10.1145/985692.985745 (PDF: https://people.csail.mit.edu/teevan/work/publications/papers/chi04.pdf)
- Bergman et al., ACM TOIS 26(4), 2008: https://doi.org/10.1145/1402256.1402259 (abstract: https://cris.openu.ac.il/iw/publications/improved-search-engines-and-navigation-preference-in-personal-inf-2/)
- West & Leskovec, "Automatic versus Human Navigation in Information Networks", ICWSM 2012: https://cs.stanford.edu/~jure/pubs/navigating-icwsm12.pdf
- West & Leskovec, "Human wayfinding in information networks", WWW 2012 (cited via the ICWSM paper): https://doi.org/10.1145/2187836.2187920

Wiki navigation and web agents:
- Margiotta, Croce & Basili, CLiC-it 2025: https://aclanthology.org/2025.clicit-1.63.pdf
- Hickey, Knight & Bishop (2026): https://arxiv.org/abs/2610.13948
- Wu et al., WebWalker (2025): https://arxiv.org/abs/2501.07572
- Kim et al., The Amazing Agent Race (2026): https://arxiv.org/abs/2604.10261
- Yao et al., ReAct, ICLR 2023: https://arxiv.org/abs/2210.03629
- Zhou et al., WebArena, ICLR 2024: https://arxiv.org/abs/2307.13854

Memory and knowledge-base QA:
- Wu et al., LongMemEval, ICLR 2025: https://arxiv.org/abs/2410.10813
- Maharana et al., LoCoMo, ACL 2024: https://arxiv.org/abs/2402.17753
- Du et al., PerLTQA, SIGHAN 2024: https://aclanthology.org/2024.sighan-1.18/
- EnterpriseRAG-Bench (2026): https://arxiv.org/abs/2605.05253
- Suresh et al., AgenticRAG (2026): https://arxiv.org/abs/2605.05538
- Sun, Wei & Hsieh, Corpus2Skill (2026): https://arxiv.org/abs/2604.14572
- HERB (2025): https://arxiv.org/abs/2506.23139
- Fan, Xue, Liu & Tan, RAGSearch (2026): https://arxiv.org/abs/2604.09666
- Singh et al., Agentic RAG survey (2025): https://arxiv.org/abs/2501.09136

Tool design:
- Yang et al., SWE-agent, NeurIPS 2024: https://arxiv.org/abs/2405.15793 ; `search_dir`: https://github.com/SWE-agent/SWE-agent/blob/main/tools/search/bin/search_dir
- Sen et al., "Is Grep All You Need?" (2026): https://arxiv.org/abs/2605.15184
- Li et al., "Beyond Semantic Similarity" (2026): https://arxiv.org/abs/2605.05242
- Claude Code tools reference (read 2026-11-01): https://code.claude.com/docs/en/tools-reference
- Anthropic, "Writing effective tools for agents — with agents" (11 Sept 2025): https://www.anthropic.com/engineering/writing-tools-for-agents
- Hasan et al., "MCP Tool Descriptions Are Smelly!" (2026): https://arxiv.org/abs/2602.14878
- Hsieh et al., "Tool Documentation Enables Zero-Shot Tool-Usage" (2023): https://arxiv.org/abs/2308.00675
- Du et al., A-RAG (2026): https://arxiv.org/abs/2602.03442

Links and graphs:
- Kessler, "Bibliographic coupling between scientific papers", American Documentation 14(1), 1963 (not re-read)
- Small, "Co-citation in the scientific literature", JASIS 24(4), 1973 (not re-read)
- Kleinberg, "Authoritative sources in a hyperlinked environment", JACM 46(5), 1999 (not re-read)
- Brin & Page, "The anatomy of a large-scale hypertextual Web search engine", 1998 (not re-read)
- Hawking, Voorhees, Craswell & Bailey, "Overview of the TREC-8 Web Track": https://trec.nist.gov/pubs/trec8/papers/web_overview.pdf
- Craswell, Hawking & Robertson, SIGIR 2001: https://doi.org/10.1145/383952.383999
- Boyack & Klavans, JASIST 2010: https://doi.org/10.1002/asi.21419
- Schwarzer et al., JCDL 2016: https://d-nb.info/1163536415/34
- Liben-Nowell & Kleinberg, link prediction: https://www.cs.cornell.edu/home/kleinber/link-pred.pdf
- Asai et al., ICLR 2020: https://arxiv.org/abs/1911.10470
- Xiong et al., MDR, ICLR 2021: https://arxiv.org/abs/2009.12756
- Yasunaga, Leskovec & Liang, LinkBERT, ACL 2022: https://arxiv.org/abs/2203.15827
- Gutiérrez et al., HippoRAG, NeurIPS 2024: https://arxiv.org/abs/2405.14831
- Xu et al., A-Mem, NeurIPS 2025: https://arxiv.org/abs/2502.12110
- Li et al., GraphReader, EMNLP 2024: https://arxiv.org/abs/2406.14550
- Bellam (2026): https://arxiv.org/abs/2610.33958
- Xu, LSP for coding agents (2026): https://arxiv.org/abs/2608.13568

Vault tooling (self-reported):
- obsidian-hybrid-search: https://github.com/flowing-abyss/obsidian-hybrid-search
- Basic Memory: https://github.com/basicmachines-co/basic-memory
- Obsidian LLM Wiki plugin: https://community.obsidian.md/plugins/karpathywiki
- vault-agent-bench: https://github.com/jppaolim/vault-agent-bench
- Obsidian Graph Analysis: https://github.com/SkepticMystic/graph-analysis

Probe: ripgrep 15.2.0 on six one-line notes in the session scratchpad (not committed).
