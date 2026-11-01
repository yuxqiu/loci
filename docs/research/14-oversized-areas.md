# 14 — Oversized areas: what an index node does when its listing outgrows its budget

Status: research, 2026-11-01. Builds on research 12 (hierarchy depth) and research 13
(area descriptions and the generated sub-folder outline); their findings are cited, not
repeated. Question: a broad area such as `mathematics` with 400 concepts outgrows the
area-digest budget. Splitting it into top-level areas loses the middle level
(mathematics > linear algebra) that the user wants to keep. How should a hierarchical
index read by an agent handle a node whose flat listing is too big for its budget?

Current loci design (m0 §9.4, §10, §11): `loci digest` prints the root (each area's
`AREA.md` H1, path, count and paragraph; research 13 adds a generated outline of the
area's sub-folders with counts). `loci digest --area X` prints every concept in X as
`- [[slug]]: digest`, loose concepts first, then one `## <relative-dir>` group per
sub-folder at any depth. Budgets in characters: `digest` = 160, `area_paragraph` = 600,
`root_index` = 8000, `area_index` = 24000. Output over budget is `index.over-budget`
(warning), and `digest` still prints everything. At 24000 characters an area holds about
120 concepts if every line is about 200 characters (research 12 §3), or about 200 at
about 120 characters per line (arithmetic, not measured).

Nothing here changes DESIGN or the specs. Conventions as in research 12 and 13:
`[unverified]` means I did not confirm the claim in a primary source (the reason is
given); `[design inference]` means my extrapolation, with no study behind it; `[gap]`
means I looked and found no evidence. Quotes marked "(via fetch tool)" came through a
summarising web fetcher because the page refused direct download; all other quotes were
checked against the downloaded text.

## TL;DR

| Question | Finding | Strength |
|---|---|---|
| 1. LLM hierarchical indexes | Every system bounds node size and **splits only the nodes that exceed the bound**: RAPTOR re-clusters a cluster whose text exceeds the summariser's token limit; LATTICE re-partitions any node with more than M = 10–20 children; GraphRAG's hierarchical Leiden splits only communities of at least `max_cluster_size` (default 10 in GraphRAG); PageIndex splits a node only when it is both over 10 pages and over 20,000 tokens. Two numbers recur: a node must **fit one LLM call**, and a choice should be **about 10 options** ("An LLM can compare O(10) candidates reliably in a single prompt", LATTICE). Two systems **swap in shorter representations instead of splitting**: GraphRAG substitutes sub-community summaries for their members "until they fit"; LATTICE shows a 1–2-word topic per document near the root and a sentence deeper down | Good for document retrieval; nothing on authored note folders `[gap]` |
| 2. Progressive disclosure for agents | Vendors cap what is always loaded and put the rest one hop away: `SKILL.md` under 500 lines, references one level deep; `MEMORY.md` loads its first 200 lines or 25KB; llms.txt files at sub-paths, "agents should use the most specific one". When a listing is over budget, Claude Code **keeps every name and drops descriptions** (skills listing), and MCP tool search loads **only tool names** up front. Tool search raised accuracy (Opus 4: 49% → 74%) and cut token use 85% (vendor numbers). SWE-agent refuses search results over 50 and asks for a narrower query; that beat both exhaustive paging and no search tool | Vendor docs plus one peer-reviewed agent study |
| 3. HCI and IR | Each extra level costs time, and a middle level helps only when its labels are clear (research 13 §C). For a very large node: Scatter/Gather users "often reclustered" large clusters and did not want more than five clusters; with the cluster count fixed, large clusters "are less able to sharply define a topic". Flamenco shows **only the level below the current one, with counts and a trail of the levels above**, and shows a few salient labels with a "more" link when a facet is too big. Faceting needs several labels per item, which loci does not have | Good (user studies, design reports) |
| 4. Library classification | Dewey prints an outline (summary) **only for large classes**: single-level for classes whose subdivisions cover 4–40 pages, two-level beyond 40 pages, none below 4. A **centered entry** creates a middle level (a span of numbers forming one concept) where the notation has no level for it. Standard subdivisions (Table 1) and UDC auxiliaries keep recurring aspects (form, place, process) out of the enumerated tree by combining notation instead of listing it | Primary manuals |

**Recommendation (design E1 below):** keep two levels in the common case and add a
third only where the budget forces it. Each digest page the agent may read must fit its
budget. When an area's full listing does not fit, `loci digest --area X` keeps the loose
concepts with their digests, lists every sub-folder concept **by slug only**, and says so
on its first line. `loci digest --area X/sub` prints the full listing of that sub-folder
and its descendants, under the same rule. `index.over-budget` then fires only for a page
that is still over budget after shortening. That is a vault that needs restructuring,
not a broad area. Promote a sub-folder to an area (design C) only when it needs its own
scope note. Before adopting, measure it against design D (raise the budget), which is
the simplest option and has no direct evidence against it at this size for current
models `[gap]`.

## 1. LLM retrieval and agent memory

### Node size and branching per system

| System | Bound on a node | Value | What happens to an oversized node | Stated reason |
|---|---|---|---|---|
| RAPTOR (Sarthi et al., ICLR 2024) | summariser token threshold | not given; mean 6.7 children (App. C, Table 10) | recursively re-clustered | "ensuring that the context remains within the token threshold" (§3) |
| LATTICE (Gupta et al., arXiv 2510.13217v2) | children per node | M = 10–20 (§3.1, App. B.1.1) | re-partitioned by an LLM until every node has ≤ M children (§2.1) | the LLM "can compare O(10) candidates reliably in a single prompt" (§1) |
| MemWalker (Chen et al., arXiv 2310.05029) | children per node | 8, 5, 8 by dataset (§4.2) | fixed at build time | performance "plateaus as the maximum number of nodes increases" (§5, Fig. 5) |
| GraphRAG (Edge et al., arXiv 2404.16130) + graspologic | community size | `max_cluster_size` = 10 (GraphRAG `defaults.py`); library default 1000 | only clusters ≥ the bound are split again, adding a level | summaries must fit the context window (§3.1.5) |
| PageIndex (VectifyAI) | pages and tokens per node | 10 pages and 20,000 tokens (`config.yaml`) | a leaf over both bounds is re-indexed into child sections | not stated |
| HiRAG (Huang et al., arXiv 2503.10150) | number of layers | stop when cluster sparsity changes < 5% (§5.4) | no further layer | "cluster quality has little or no improvement after that" |
| MemTree (Rezazadeh et al., arXiv 2410.14052) | none on branching | mean branching 2.1, max depth 13 (§5.3, Table 5) | depth-adaptive similarity threshold decides where to insert | — |
| H-MEM (Sun & Zeng, arXiv 2507.22925) | fixed four layers | Domain, Category, Memory Trace, Episode (§3.1) | — | "analogous to section, subsection, subsubsection, and content" |
| A-MEM (Xu et al., NeurIPS 2025, arXiv 2502.12110) | no hierarchy | top-k retrieval, k = 10–50 tested (§4.5) | — | — |

Details and sources:

- **RAPTOR.** "Should a local cluster's combined context ever exceed the summarization
  model's token threshold, our algorithm recursively applies clustering within the
  cluster, ensuring that the context remains within the token threshold" (§3,
  Clustering Algorithm). The number of clusters comes from BIC, so there is no fixed
  branching factor; Appendix C reports 6.7 child nodes per parent on average and a 0.28
  summary-to-children length ratio. On querying, "The collapsed tree approach
  consistently performs better" than tree traversal, because "by searching through all
  the nodes simultaneously, it retrieves information that is at the correct level of
  granularity" (§3, Querying). **Correction to how research 13 §A used this:** the
  collapsed tree is not a listing. It is a cosine-similarity selection over all nodes
  under a token budget ("we use the collapsed tree with 2000 maximum tokens, which
  approximately equates to retrieving the top-20 nodes", §3). In loci its analogue is
  `loci find -k`, a ranked selection, more than the area digest. What transfers is
  that a fixed per-level quota (traversal) loses to choosing across levels.
- **LATTICE** (arXiv 2510.13217v2, 25 May 2026). The tree constrains "the branching
  factor to |C(v)| ≤ M" (§2.1), with "branching factor M = 10–20" (§3.1). Top-down
  construction mirrors "hierarchical k-means: begin with a single root containing every
  leaf, and recursively partition any cluster whose size exceeds a threshold"; "Any
  newly created internal node whose child count exceeds M is added to a partitioning
  queue … The recursion terminates when every node satisfies |C(v)| ≤ M" (§2.1). Listing
  everything in one prompt is ruled out because "the prompt is already in the millions
  of tokens" (§2.1). The rationale for about ten children is in §1: "An LLM can compare
  O(10) candidates reliably in a single prompt", citing listwise-reranking work. The
  construction also **shortens each item rather than dropping items** when a node is
  big: each document has summaries of at most 2^(i+1) words at five levels, and
  "SelectSummaryLevel … picks the most specific level … that fits within ClusterLLM's
  context budget. … near the root the level is coarse (a few-word topic per document),
  deeper in the tree it is fine (a sentence per document)" (§2.1). The ablations sweep
  beam size against iterations, not branching factor; I found no ablation of M `[gap]`.
  Static summaries go stale: "stale tree summaries hurt the static LATTICE index"
  (App. B.5).
- **MemWalker.** "We set the maximum number of nodes max_t M_t = 8, 5, 8 and segment size
  |c| = 1000, 1000, 1200 for QuALITY, SummScreenFD, and GovReport" (§4.2). "Summarizing
  larger segments is generally more beneficial than smaller segments as well as
  connecting more children nodes to the parent. However, the performance plateaus as the
  maximum number of nodes increases" (§5). Carrying the visited path as working memory
  matters: without it, "a 5–13% drop in accuracy" (§5, Fig. 3).
- **GraphRAG.** Leiden is applied "in a hierarchical manner, recursively detecting
  sub-communities within each detected community until reaching leaf communities that
  can no longer be partitioned" (§3.1.4). GraphRAG's default is `max_cluster_size: int =
  10` (`graphrag/config/defaults.py`, main branch, read 2026-11-01). The graspologic
  documentation for `hierarchical_leiden`: "Any partition or cluster with membership >=
  max_cluster_size will be isolated into a subnetwork … Once all clusters with membership
  >= max_cluster_size have been completed, the level increases … until every cluster's
  membership is < max_cluster_size or if they cannot be broken into more than one new
  community" (default 1000 in the library). So **only oversized communities get another
  level**; small ones stay where they are. When a higher-level summary would not fit,
  GraphRAG shortens instead of dropping: "rank sub-communities in decreasing order of
  element summary tokens and iteratively substitute sub-community summaries (shorter) for
  their associated element summaries (longer) until they fit within the context window"
  (§3.1.5). The paper names a drill-down mode as future work: "a more exploratory 'drill
  down' mechanism that follows the information scent contained in higher-level community
  summaries" (§6.2).
- **Choosing a level (GraphRAG).** The paper evaluates fixed levels C0–C3; C0 used
  "over 97% fewer tokens" than summarising source text (§5.1), and level counts grow
  fast (Podcast: 34, 367, 969, 1,310 units at C0–C3; Table 2). Microsoft's follow-up,
  "GraphRAG: Improving global search via dynamic community selection" (Microsoft Research
  blog, 2024; via fetch tool), replaces the fixed level with a descent: "Starting from the
  root … we use an LLM to rate how relevant a community report is"; an irrelevant report
  is removed with its sub-communities, and a relevant one is expanded. At level 1 this
  gave "an average cost reduction of 77%" with no statistically significant quality
  difference (about 470 reports selected instead of about 1,500). Not peer-reviewed. The
  shipped defaults (`dynamic_search_max_level = 2`, `dynamic_search_threshold = 1`) are in
  `defaults.py`.
- **PageIndex.** `config.yaml`: `max_page_num_each_node: 10`, `max_token_num_each_node:
  20000`. In `page_index_classic.py`, `process_large_node_recursively` re-indexes a node
  only `if (not node.get('nodes') and node['end_index'] - node['start_index'] >
  opt.max_page_num_each_node and token_num >= opt.max_token_num_each_node …)`, that is, a
  leaf over both bounds; the recursion then continues into the new children (repository
  main branch, read 2026-11-01).
- **HiRAG.** Layers are added until "the change rate of cluster sparsity is lower than
  ε" with "ε = 5%", stopping "when the majority of the clusters consist of only a small
  number of entities, meaning that the entities can no longer be effectively grouped"
  (§5.4). A stopping rule based on cluster quality, not size.
- **MemTree.** No branching cap: on Multihop-RAG, 3,154 nodes, "maximum depth of 13 and
  an average branching factor of 2.1" (§5.3). Retrieval ignores the tree shape: it
  computes "cosine similarities between e_q and all tree nodes" and takes the top-k
  (§3.2). The tree organises writes; reads are flat.
- **H-MEM** uses four fixed layers, with index pointers from each entry to its children
  "allowing efficient index-based routing without the need for exhaustive similarity
  computation" (§3.1). **A-MEM** has no hierarchy: notes are linked, and "we retrieve the k
  most relevant memories" (§3.4), with k from 10 to 50 tested (§4.5).

**What 1 means for loci** `[design inference]`. Three patterns hold across systems:
(1) the bound is on what one reader call must hold, and **only nodes over the bound get
another level** (RAPTOR, LATTICE, GraphRAG, PageIndex). No system adds a level to every
node because one node is big. (2) Before splitting, two systems **shorten the
representation of each child** (GraphRAG's substitution; LATTICE's coarse summaries near
the root). (3) Navigating systems keep the path context (MemWalker's working memory),
and systems that only need recall skip the tree at read time (RAPTOR collapsed, MemTree,
A-MEM). For loci, (1) says to add the drill-down level only to oversized areas, and (2)
suggests a shortened listing (slugs without digests) before a bare outline. The ~10-way
choice bound is for an LLM *scoring* candidates against each other. An agent scanning a
listing for a known name is closer to the navigational case in research 13 §B (Cutrell &
Guan), where short entries did no harm.

## 2. Progressive disclosure for agents

- **Agent Skills** (Anthropic, "Skill authoring best practices",
  platform.claude.com, read 2026-11-01). "Keep the `SKILL.md` body under 500 lines for
  optimal performance. Split content into separate files when approaching this limit."
  The domain pattern: "For Skills with multiple domains, organize content by domain to
  avoid loading irrelevant context"; the overview lists each domain with key terms and a
  link ("**Finance**: Revenue, ARR, billing → See reference/finance.md") and adds a "Quick
  search" with grep. "Keep references one level deep from `SKILL.md`", because with
  nested references "Claude might use commands such as `head -100` to preview content
  rather than reading entire files". "For reference files longer than 100 lines, include
  a table of contents at the top." So: a capped overview, one hop to detail, and search
  as a parallel route. The 500-line figure is a recommendation, not a measured threshold.
- **Claude Code memory** (code.claude.com/docs/en/memory). "The first 200 lines of
  `MEMORY.md`, or the first 25KB, whichever comes first, are loaded at the start of every
  conversation. Content beyond that threshold is not loaded at session start. Claude
  keeps `MEMORY.md` concise by moving detailed notes into separate topic files." Near the
  limit, Claude Code "reminds Claude to shorten it: keep one line per entry, move detail
  into topic files"; over the limit it returns an error telling Claude to rewrite the
  index. CLAUDE.md: "target under 200 lines per CLAUDE.md file"; subdirectory files
  "load on demand"; imports go at most "four hops" deep. **The index is capped, the
  overflow is moved one hop away, and going over the cap is reported to the writer, not
  hidden.**
- **Skill listing over budget** (code.claude.com/docs/en/skills, "Skill descriptions are
  cut short"). "The listing always contains every skill name, but if you have many
  skills, Claude Code drops some descriptions to fit the listing's character budget,
  which removes the keywords Claude needs to match your request. The budget scales at 1%
  of the model's context window." Low-priority skills can be set to `"name-only"`. This
  is the closest production precedent for a slug-only listing, and the vendor names its
  cost: names without descriptions lose matching keywords.
- **MCP tool search** (code.claude.com/docs/en/mcp, "Scale with MCP tool search"): "Only
  tool names and server instructions load at session start". In `auto` mode, deferred
  tools load up front "while their definitions total under 10% of the context window. At
  10% or more, tool search turns on." The engineering post "Introducing advanced tool
  use" (anthropic.com/engineering/advanced-tool-use): "This represents an 85% reduction in
  token usage"; "Opus 4 improved from 49% to 74%, and Opus 4.5 improved from 79.5% to
  88.1% with Tool Search Tool enabled" (internal MCP evaluations, not published);
  use it when "Tool definitions consuming >10K tokens" or "10+ tools available"; "Keep
  your three to five most-used tools always loaded, defer the rest." Server-level
  instructions plus names up front, detail on demand: the same split as an area
  paragraph plus slugs.
- **llms.txt** (llmstxt.org): files may sit "at the root path `/llms.txt` of a website or
  at any subpath (e.g. `/docs/llms.txt`). A file covers the URLs under its path, and
  where more than one file applies, agents should use the most specific one." "The file
  itself stays small enough to fit in context. The detail lives behind the links, and is
  fetched only when needed." An "Optional" section holds "links an agent can skip when a
  shorter context is needed." Sub-path files are the format's own drill-down.
- **Sitemaps** (sitemaps.org protocol), a non-LLM precedent for a hard size cap: "each
  Sitemap file that you provide must have no more than 50,000 URLs and must be no larger
  than 50MB"; over that, "you must create multiple Sitemap files" listed "in a Sitemap
  index file". Splitting is triggered by size alone.
- **Context engineering** (anthropic.com/engineering/effective-context-engineering-for-ai-agents):
  just-in-time agents "maintain lightweight identifiers (file paths, stored queries, web
  links, etc.)" and load data at runtime; Claude Code is a hybrid where "CLAUDE.md files
  are naively dropped into context up front, while primitives like glob and grep allow it
  to navigate"; "runtime exploration is slower than retrieving pre-computed data"; LLMs
  have an "attention budget" that every token depletes.

**Evaluations of list versus filter versus drill down.**

- **SWE-agent** (Yang, Jimenez, Wettig, Lieret, Yao, Narasimhan & Press, NeurIPS 2024,
  arXiv 2405.15793). "The search commands return at most 50 results for each search
  query; if a search exceeds this number, we do not report the results and instead
  suggest that the agent write a more specific query" (§3). Ablation on SWE-bench Lite
  (§5.1, Table 3): the full interface scored 18.0%; with no search commands, 15.7%; with
  iterative (paged) search, 12.0%, because "when given a large number of search results,
  agents tend to look through every match exhaustively … This inefficient behavior can
  exhaust an agent's cost budget or context window". The file viewer showed the same
  shape: "Either too little content (30 lines, 14.3%) or too much (entire file, 12.7%)
  lowers performance." **The one peer-reviewed agent result here: a capped page plus an
  explicit instruction to narrow beat both paging through everything and having no
  tool.** The task is code search, not a note index.
- **Many options in one prompt.** Liu et al., "Lost in the Middle", TACL 2024 (arXiv
  2307.03172): performance "significantly degrades when models must access relevant
  information in the middle of long contexts" (§1). Gan & Sun, "RAG-MCP", arXiv
  2505.03275 (2025, preprint): with N tool schemas in the prompt, selection accuracy falls
  as N grows; retrieving a subset first gave 43.13% selection accuracy against 13.62% and
  18.20% for two baselines (§4.2, Table 1). Both used models older than current frontier models, and neither tests
  a sorted, grouped listing scanned for a known name `[gap]`.
- **No study compares, for an LLM agent, a full listing, a shortened listing and a
  drill-down over the same index of 100–1000 entries** `[gap]`. The vendor thresholds
  (500 lines, 200 lines/25KB, 10% of context, 1% of context) are engineering choices;
  none is published with the experiment behind it.

**What 2 means for loci** `[design inference]`. The vendor pattern is the same in four
products: always load names plus a short scope statement; load detail one hop away;
when the listing is over budget, drop descriptions before names (skills listing) or
defer everything except names (tool search); tell the reader what was left out (the
`MEMORY.md` error, SWE-agent's "write a more specific query"). The one controlled agent
study says an explicit cap with a hint beats both exhaustive output and no tool.

## 3. HCI and IR

- **Depth and breadth** (research 13 §C, research 12 §3): time rises with depth (Miller
  1981; Kiger 1984; Bergman et al. 2010: +2.2 s per level); Larson & Czerwinski (CHI
  1998, abstract via OpenAlex): "while increased depth did harm search performance on
  the web, a medium condition of depth and breadth outperformed the broadest shallow web
  structure overall"; their 16×32 hierarchy was fastest and 8×8×8 slowest (Zaphiris 2001,
  §2.1, full text read). Snowberry, Parkinson & Sisson (1983): "error rates increased from
  4.0% to 34.0% as depth increased from a single level to six levels", and "visual
  scanning was predictive of performance, especially in the deepest hierarchies" (as
  summarised by Zaphiris 2001, §2, §2.1). Miller & Remington (2004): a middle level
  helps when its labels are clear and hurts when they are not; listing the second level
  under each first-level label "lifts" two levels onto one page (research 13 §C).
- **Broad but grouped.** Snowberry et al. (1983) also tested a single-level menu of 64
  options grouped by category, as well as Miller's unrelated grouping, and Parkinson,
  Sisson & Snowberry, "Organization of broad computer menu displays" (*IJMMS* 23(6),
  1985), studied how to lay out broad menus. I found neither text nor abstract; the
  description is from search snippets of Paap & Roske-Hofstrand (1986) and a later
  CHI paper `[unverified]`, so I draw no conclusion from it.
- **Scatter/Gather with large clusters.** Hearst & Pedersen, "Reexamining the cluster
  hypothesis: Scatter/Gather on retrieval results", SIGIR 1996 (full text read). Results
  were clustered into 5 clusters; "The number of clusters was chosen arbitrarily, but
  reflects users' preference for smaller numbers of clusters". With more documents,
  cluster-based ranking lost its advantage: "as n increases the cluster sizes increase as
  well (since the number of clusters is fixed at 5). Large clusters are less able to
  sharply define a topic of interest". From the user study: "Large clusters were often
  reclustered. None of the participants thought having more than five clusters would be
  a good idea." Reclustering is dynamic: "different topics are seen depending on the
  subcollection clustered". **Users dealt with a big node by drilling into it and
  regrouping it, not by widening the top level.**
- **Faceted navigation (Flamenco).** Hearst, "Design Recommendations for Hierarchical
  Faceted Search Interfaces", ACM SIGIR Workshop on Faceted Search, 2006 (full text read
  from the Flamenco project PDF). On exposing a hierarchy: fly-away menus showing a whole
  sub-hierarchy break down "if the number of choices is very large, (as in the case with
  names of artists, for example)"; an Explorer-style expandable tree "can get very large
  and require extensive scrolling" when many branches are open. "In Flamenco we adopted a
  step-by-step drill-down approach in which the level just below the currently selected
  level is visible, along with a trail indicating the higher level concepts positioned
  just above the labels" (§3), with counts beside each label (Fig. 1). For a facet too
  big to show: "in those cases in which there is room for only a few of many labels for a
  facet to be shown, it can be useful to show either the most salient or the most
  frequently occurring in the initial view, with a link to see more choices which are
  then shown in a predictable order" (§4), and "in most cases users prefer the
  predictability of a well-known ordering" (§4). Hearst, "UIs for Faceted Navigation:
  Recent Advances and Remaining Open Problems", HCIR 2008 (full text read): one of the
  "two biggest challenges" is that "large category systems, especially subject-oriented
  category systems, are still not well-supported in the interface" (§1).
- **Faceting as an alternative to depth.** Faceted search assigns "multiple labels … to
  each item, as opposed to a strictly hierarchical system in which items are placed into
  single categories or folders" (Hearst 2008, §2). loci concepts have one folder, and the
  frontmatter allows no tags (m0 §6: `fm.unknown-field` for `tags:`); the cross-cutting
  paths are links, `prereqs` and views (research 13 §D). Faceting would need a new
  metadata field and would not by itself shrink a subject listing, which Hearst names as
  the unsolved case. Out of scope here.
- **Search versus navigation in personal collections.** Teevan, Alvarado, Ackerman &
  Karger, CHI 2004 (abstract via OpenAlex; ACM DL closed): participants "navigated to
  their target with small, local steps using their contextual knowledge as a guide, even
  when they knew exactly what they were looking for in advance". Bergman, Beyth-Marom
  (listed as Marom), Nachmias, Gradovitch & Whittaker, *ACM TOIS* 2008 (abstract via
  OpenAlex): users estimated navigation for "56-68% of file retrieval events but searched
  for only 4-15%", and better desktop search changed this little; search was "a last
  resort". These are people, not agents; the full texts are paywalled `[unverified]`
  beyond the abstracts.

**What 3 means for loci** `[design inference]`. The human evidence argues against both
extremes: a very broad top level (design C makes the root broader) and an extra level
everywhere (a mandatory `--area X/sub`). Scatter/Gather and Flamenco agree on the
remedy for one big node: show its children with sizes and a trail, and drill into it
only when needed. Where a node shows only some of its members, Flamenco shows the most
salient ones in a predictable order with a "more" link. A complete, sorted slug list
avoids having to pick which members are salient.

## 4. Library classification

- **Summaries only for big classes.** OCLC, *Introduction to the Dewey Decimal
  Classification* (full text read), §7.1: "two-level summaries are provided for each main
  class and division of the schedules and main numbers of Table 2 with subdivisions that
  extend beyond forty pages"; "Single-level summaries … provide an overview of classes
  whose subdivisions cover between four and forty pages". Classes under four pages get
  none. Summary headings "have been edited for browsing purposes" (§4.2). **Yes, there is
  a size threshold, and it is in pages: the outline appears when the listing no longer
  fits a few pages, and a second outline level appears at ten times that size.** The
  summary is derived from the schedule, so it cannot drift from it.
- **Centered entries keep a middle level the notation lacks.** "A centered entry is used
  to indicate and relate structurally a span of numbers that together form a single
  concept for which there is no specific hierarchical notation available" (§4.20). Dewey
  keeps a natural middle concept as a grouping heading over a span rather than forcing
  it into a new notational level or giving it up. The loci analogue is a sub-folder that
  groups concepts and has no description of its own (Z39.19 node labels, research 13
  §D) `[design inference]`.
- **Hierarchy is ten-way by construction.** "Each main class is further divided into ten
  divisions, and each division into ten sections (not all the numbers for the divisions
  and sections have been used)" (§4.1). A decimal notation caps the branching of every
  enumerated level at ten. The bound is a property of the notation, not a usability
  finding, but it lands where LATTICE's M does.
- **Standard subdivisions and auxiliaries.** "A standard subdivision represents a
  recurring physical form (such as a dictionary, periodical, or index) or approach (such
  as history or research) and thus is applicable to any subject" (§8.3); it is added to a
  number, not enumerated under each class. UDC (udcc.org, "About UDC structure", via fetch
  tool): main tables "contain the outline of the various disciplines of knowledge";
  "Common auxiliaries, are applicable throughout the main tables" (place, language, form);
  special auxiliaries "are usually facets of concepts related to operations, techniques,
  processes, materials, agents etc." within one class. UDC calls itself "an
  analytico-synthetic and/or faceted classification". **Both schemes keep classes from
  over-filling by moving cross-cutting aspects out of the enumerated tree** into
  combinable notation. In loci, cross-cutting aspects already live outside the folders
  (links, `prereqs`, views); the folder tree should carry topic only.
- **Node labels group siblings.** Z39.19 §8.3.5: "node labels may be used to show the
  principles of division among a set of sibling terms", and they "are not terms, and must
  not be used as indexing terms" (ANSI/NISO Z39.19-2005 (R2010), full text read).
  Grouping a big array under labels is the standard's device for a long list of siblings,
  without promoting the labels to terms.

**What 4 means for loci** `[design inference]`. Library practice adds an outline to a
class *because* the class got big and generates it from the schedule. It keeps natural
middle concepts as grouping headings (centered entries, node labels) rather than giving
them up. It controls size by taking cross-cutting aspects out of the tree. That fits
"show an outline only when the area is over budget". It argues against design C when C
is used only to save space.

## Design evaluation

Neutral example vault. Area `mathematics` (400 concepts): 30 loose concepts;
sub-folders `algebra` (60), `analysis` (80), `geometry` (40), `linear-algebra` (60),
`linear-algebra/numerical` (30), `probability` (70), `topology` (30). Other areas:
`statistics` and `machine-learning`. Size assumptions (arithmetic, not measured): a
full line `- [[slug]]: digest` averages 130 characters; a slug entry `[[slug]], ` 26
characters; 4 characters per token. The full area listing is then about 52,000
characters (about 13,000 tokens), over `area_index` = 24000 by a factor of about 2.2.

Two tasks are used in the comparison. *Retrieve*: answer a question whose answer is
`[[power-iteration]]` in `linear-algebra/numerical`. *File*: decide where a new concept
on Gershgorin discs goes, and whether a note already exists.

### Design A. Drill-down with an outline-only area page

- **Rendering.** If the full listing fits, unchanged. If not, the area page prints the
  loose concepts with digests and then one line per sub-folder with its count:

  ```markdown
  # Mathematics (concepts/mathematics)

  This area's full listing is 52,140 characters (budget 24,000). Sub-folders are listed
  by name; run `loci digest --area mathematics/<sub-folder>` for their concepts.

  - [[proof-by-induction]]: Prove a statement for all naturals from a base case and a step.
  …

  ## Sub-folders

  - algebra (60)
  - analysis (80)
  - geometry (40)
  - linear-algebra (60)
  - linear-algebra/numerical (30)
  - probability (70)
  - topology (30)
  ```

  `loci digest --area mathematics/linear-algebra` prints the concepts directly in that
  folder, then a `## numerical` group, as an area digest does, under the H1
  `Mathematics › linear-algebra (concepts/mathematics/linear-algebra)` (Flamenco's trail).
- **JSON.** Area: `{"area", "dir": null, "title", "chars", "budget", "shortened": true,
  "groups": [{"dir": "", "concepts": [...]}], "subfolders": [{"dir", "concepts": n}]}`.
  Drill: the same shape with `"dir": "linear-algebra"`. Field names are illustrative.
- **Always allowed, or only when over budget?** Allow `--area X/sub` always. It is a
  read-only filter and its output is a deterministic function of the tree. Refusing it
  would make a valid command depend on vault size, and an agent that saw a sub-folder in
  the root outline would hit a refusal. The *area page* switches format only when over
  budget, which is the GraphRAG/PageIndex/Dewey pattern (split only oversized nodes). The
  `concepts` skill says to drill only when the area page says it was shortened.
- **Budget and lint.** `index.over-budget` checks each page as printed: the root, each
  area page, and each drill page of a shortened area. A shortened area is not a
  warning. A page still over budget is (for example, 200 loose concepts with no
  sub-folders). The hint: move loose concepts into sub-folders, or promote one.
- **`mv`.** Nothing is stored, so nothing to rewrite. A renamed sub-folder changes its
  drill path, and the next digest shows the new one. Unknown paths: `digest.no-area`,
  extended to name the sub-folder.
- **Read path.** Retrieve: root (≤ 8,000) → area (about 4,600) → drill
  `linear-algebra` (90 concepts, about 11,800) = 3 reads, about 24,000 characters.
  Every retrieval in a shortened area costs 3 reads, and the agent must choose the
  sub-folder from its name alone.
- **For.** Matches Flamenco (level below, counts, trail), Dewey (outline for big classes,
  generated), the sub-path files of llms.txt, and GraphRAG's dynamic selection (descend
  only into relevant branches, −77% cost). Outline lines are few, within LATTICE's
  "O(10) candidates".
- **Against.** The choice among sub-folders is made without seeing any leaves. That is
  where research 13 §A found inner-node descriptions earn their place, and loci
  sub-folders have none, so scent rests on folder names (research 13, option 4
  "Against"). Every retrieval pays a third hop (Bergman et al.; MemWalker: each step can
  go wrong, 15–20% reverts). Duplicate checks when filing need every relevant sub-folder
  opened, or `loci find`.

### Design B. Adaptive area page with representative slugs

- **Rendering.** As A, but each sub-folder line carries its count and k representative
  slugs, Scatter/Gather's "typical titles" or Flamenco's "most salient" labels:
  `- linear-algebra (60): [[eigenvalue]], [[matrix-rank]], [[singular-value-decomposition]], … more`.
  Requires a salience rule. The only deterministic signal loci has is incoming-link
  count. Alphabetical first-k would be predictable but carries no salience.
- **JSON.** As A, with `"sample": [slug, …]` per sub-folder.
- **Budget, lint, `mv`.** As A. One new parameter, k, which no source fixes (Scatter/Gather
  showed "a number of typical titles", research 13 §A).
- **Read path.** Retrieve: 2 reads (about 13,000 characters) when the target is a sample;
  otherwise 3 (about 25,000). File: the agent cannot tell absence from "not sampled".
- **For.** Scatter/Gather digests (82% correct cluster choice, research 13 §A); Flamenco
  §4. Cheaper than listing every slug.
- **Against.** A new ranking abstraction and a magic k. Link counts favour old, hub
  concepts, not the ones a query needs. Blackmon et al. (2003) and Dumais & Landauer
  (1983) found that examples added to a clear label bought nothing measurable (research
  13 §B). A partial list invites a false "no such note" conclusion, which hurts filing.

### Design C. Promote sub-folders to areas

- **Rendering.** None new. `loci mv concepts/mathematics/linear-algebra
  concepts/linear-algebra`, then write `concepts/linear-algebra/AREA.md` (H1 and scope
  paragraph; research 13 option 5).
- **JSON, lint.** Unchanged. `area.missing` fires until the new `AREA.md` exists.
  Boundary links in other areas' paragraphs need updating by hand where scope moved.
- **`mv`.** Existing folder move; path links rewritten (m0 §9.5).
- **Root budget.** Seven new areas at about 500 characters of paragraph plus heading and
  outline each add about 4,000 characters, which puts a ten-area root near or over
  `root_index` = 8000 (research 12 §3 estimated about 12 areas fit).
- **Read path.** Retrieve: root (larger) → `linear-algebra` area (about 11,800) = 2 reads,
  about 20,000 characters.
- **For.** No new code or concept. Each new area gets a scope note, which research 13
  found most useful where the reader picks an area without seeing leaves. Research 12
  §4(a)3 recommended it.
- **Against.** It drops the middle level the user wants to keep. It widens the root,
  where Larson & Czerwinski found the broadest structure lost to a medium one and Miller &
  Remington found that a clear middle level speeds navigation. Dewey keeps such middle
  concepts as centered entries rather than discarding them. Use it when a sub-folder is
  distinct enough to need its own scope note, not as a size fix.

### Design D. Raise or remove the area budget; rely on `loci find`

- **Rendering, JSON, `mv`.** Unchanged. `area_index` set to, say, 60000, or the area check
  dropped from `index.over-budget`.
- **Read path.** Retrieve: root → area (about 52,000) = 2 reads, about 60,000 characters
  (about 15,000 tokens). Or `loci find "power iteration eigenvector"` = 1 call, if the
  agent's terms match.
- **For.** Simplest: one config number. Keeps the "one read shows every leaf" property
  research 12 and 13 relied on. 15,000 tokens is a small share of current context
  windows. `find` is the analogue of RAPTOR's collapsed tree and MemTree's flat read,
  which beat or replace traversal in those systems.
- **Against.** Every vendor in §2 caps the always-read index and moves detail one hop
  away, and Claude Code treats an over-cap `MEMORY.md` as an error to fix. Lost in the
  Middle and RAG-MCP report degradation with long contexts and many options (older
  models, other tasks). Anthropic's tool search improved accuracy when it stopped loading
  everything (internal numbers). SWE-agent found exhaustive output hurt. People navigate
  rather than search even with good search tools (Teevan et al.; Bergman et al. 2008).
  Whether 15,000 tokens of sorted, grouped lines hurt a current model's lookup is
  unmeasured `[gap]`, so D stays the control in the benchmark.

### Design E1 (recommended). Budgeted pages: slug-only shortening plus drill-down

Design A's drill-down, but the shortened area page lists every sub-folder concept by
slug instead of only the sub-folder names. GraphRAG's "substitute shorter
representations until they fit", LATTICE's coarse summaries near the root, and Claude
Code's "every skill name, fewer descriptions" give the shortening. GraphRAG,
PageIndex, Dewey and Flamenco give "drill into the oversized node only".

- **Rule.** Render the full area listing. If it fits `area_index`, print it (unchanged
  from today). Otherwise keep the loose concepts with digests, since no drill page holds
  them, and print each sub-folder group as its count plus a comma-separated list of
  `[[slug]]` links. Apply the same rule to a drill page (`--area X/sub`) against the
  same budget. Shorten the whole page, not the largest groups first: GraphRAG's greedy
  substitution would keep more digests, but the page's shape would then depend on group
  sizes in a way the agent cannot predict, and would change when one note is added
  `[design inference]`.
- **Rendering.**

  ```markdown
  # Mathematics (concepts/mathematics)

  Shortened: the full listing is 52,140 characters (budget 24,000). Sub-folder concepts
  are listed by slug; `loci digest --area mathematics/<sub-folder>` shows their digests.

  - [[proof-by-induction]]: Prove a statement for all naturals from a base case and a step.
  …

  ## linear-algebra (60)

  [[eigenvalue]], [[gram-schmidt-process]], [[matrix-rank]], …

  ## linear-algebra/numerical (30)

  [[conjugate-gradient-method]], [[lu-decomposition]], [[power-iteration]], …
  ```

  The first line makes the shortening loud, as Claude Code's `MEMORY.md` error and
  SWE-agent's "write a more specific query" do. It tells the agent exactly what was
  dropped and how to get it. Groups stay in m0 §10 order; slugs keep the m0 §10 sort.
  The drill page is as in design A.
- **JSON.** Area and drill pages share one shape: `{"area", "dir": null | "<sub>",
  "title", "chars": <full length>, "budget", "shortened": bool, "groups": [{"dir",
  "count", "concepts": [{"slug", "path", "digest": <string or null>}]}]}`. `digest` is
  null in a shortened group, so `--json` readers get what the agent reads. Names
  illustrative.
- **Budget arithmetic** (assumptions above). Shortened page: 30 × 130 + 370 × 26 + headings
  ≈ 14,500 characters, under 24,000. Shortening stops fitting at roughly 800 sub-folder
  concepts plus a normal number of loose ones. Past that, the page is over budget and
  lint says so. No third tier (outline only) unless measurement shows it is needed:
  a vault at that size needs restructuring, and saying so is better than hiding more.
- **Lint.** `index.over-budget` checks each page as printed: the root; each area page
  after shortening; and each drill page reachable from a shortened page (each sub-folder
  that holds concepts at any depth), also after shortening. So the warning means "some
  page the agent may read is too big", which only restructuring fixes. It no longer
  fires on a broad area the user keeps on purpose, so warnings keep their signal. Hint
  text: put loose concepts into sub-folders, split the largest sub-folder, or promote a
  sub-folder to an area.
- **`mv`.** Nothing stored, nothing to rewrite. Drill paths are folder paths and follow
  any move. `digest.no-area` covers an unknown `X/sub`.
- **Root digest.** Unchanged (title, path, count, scope paragraph, generated outline per
  research 13).
- **Read path.**
  - Retrieve, known term: root → shortened area (about 14,500): the agent sees
    `[[power-iteration]]` and reads the note = 2 index reads, about 22,500 characters,
    against about 60,000 under D.
  - Retrieve, digest needed (an informational question where the slug is not enough):
    root → area → drill `linear-algebra` (about 11,800) = 3 reads, about 34,000.
  - File: the shortened page shows every slug, so a duplicate check needs no drill when
    the slug is predictable; choosing between `linear-algebra` and `analysis` uses folder
    names plus the slugs under them (Scatter/Gather "typical titles", here complete).
  - `find` stays the parallel route, as the skills guide pairs its domain index with a
    grep "Quick search".
- **Always allowed?** `--area X/sub` is always allowed, as in A. The skill says: drill
  only when the area page says "Shortened", or to focus on one sub-folder by choice.
- **For.** Every source in §1 that splits does so only for oversized nodes. Shortening
  each child before splitting is what GraphRAG and LATTICE do. Claude Code's skills
  listing keeps every name and drops descriptions over budget. Tool search loads names up
  front. Flamenco shows the level below with counts and a trail. Dewey adds an outline
  only for big classes. A complete slug list supports known-item lookup and duplicate
  checks, the navigational tasks where shorter entries did no harm (Cutrell & Guan,
  research 13 §B). The common case (areas within budget) is untouched, so research 12
  and 13's "two hops, every leaf visible" holds wherever it held before. The middle level
  stays, as Dewey's centered entries keep theirs.
- **Against.** Slugs without digests lose keywords: Claude Code's documentation says
  exactly this about names-only skill entries. Informational questions may need the
  third hop. The page switches format at a size threshold, so one added note can change
  the whole page. This is predictable and announced, but it is a discontinuity
  `[design inference]`. Comma-separated slugs are a second line format in the digest.
  No study tests slug-only listings for an LLM agent `[gap]`.

### Design E2. A sub-folder scope note once it becomes a drill target

- **Idea.** Let a sub-folder carry a description (an `AREA.md`-like file) only when it is
  a drill target, so the shortened area page can show a line of scent per sub-folder.
- **For.** Under design A the agent picks a sub-folder without seeing leaves, the case
  where research 13 §A found inner-node descriptions help. Dewey gives notes to classes,
  and a drill target behaves like a class.
- **Against.** A file that is required or allowed depending on how many notes the area
  holds makes a lint rule flip as notes are added. It brings back research 13 option 2's
  costs: a new file kind at depth, `mv` edge cases for merges and promotions, drift
  (LATTICE App. B.5; research 13 §E). Under E1 the agent already sees every slug under
  each sub-folder, which removes most of the need. Z39.19 gives grouping levels node
  labels, not scope notes, and research 13 already says a sub-folder that needs a scope
  note should be promoted (design C). Keep it as the fallback if E1 measures badly
  on filing.

### Summary of the designs

| Design | Area page when over budget | Extra level | New code | Retrieve reads / chars (example) | Main risk |
|---|---|---|---|---|---|
| A | loose + outline | always, in shortened areas | drill flag, lint per page | 3 / ~24k | choosing a sub-folder by name alone |
| B | loose + outline + k samples | usually | A + salience rule, k | 2–3 / ~13–25k | partial list misread as complete |
| C | n/a (areas split) | none | none | 2 / ~20k (root grows) | loses the middle level; root over budget |
| D | full listing | none | config value | 2 / ~60k | long-context and many-option degradation (unmeasured at this size) |
| E1 | loose + all slugs | only when the slug is not enough | drill flag, shortened render, lint per page | 2–3 / ~22–34k | slugs lose keywords |
| E2 | A or E1 + sub-folder notes | as A/E1 | E1 + new file kind | as A/E1 | drift; size-dependent lint |

## Recommendation

1. **Adopt E1 if the benchmark below supports it:** `--area X/sub` always available; an
   area page shortens to slug-only sub-folder groups when its full listing exceeds
   `area_index`, with a first line saying so; drill pages follow the same rule;
   `index.over-budget` checks pages as printed. Spec touch points: m0 §9.4 (argument and
   JSON), §10 (shortened format, drill H1), §7 (`index.over-budget` meaning), and the
   `concepts` skill and `rules.md` (when to drill). `area_index` stays at 24000.
2. **Keep D as the control and the default if it ties.** If raising `area_index` to fit
   the 400-concept area does not lower the pass rate or raise median time beyond
   run-to-run noise, take D: one number, no new format.
3. **Promote (C) only for scope, not size.** A sub-folder that needs its own scope note
   becomes an area (research 13). Do not adopt B or E2 now; E2 is the fallback if E1
   loses on filing.

## What to measure with `loci bench --agent`

Build a neutral sample vault with one 400-concept `mathematics` area (layout as in the
example; titles from a public syllabus or textbook index), plus `statistics` and
`machine-learning` with deliberately confusable boundaries, and 8–10 smaller areas so
the root is realistic. Each variant changes only the digest code or `loci.toml`; the
notes stay the same.

- **Variants.** V0 = D (`area_index` raised so the full listing prints); V1 = E1;
  V2 = A (outline only); V3 = B (k = 3, by incoming links); V4 = C (sub-folders promoted,
  each with a scope-note `AREA.md`).
- **Cases** (`bench/retrieval.jsonl`):
  - known-item retrieval of concepts in large sub-folders (`expect`: slug), with
    questions phrased with and without the slug's words, to separate slug-scent from
    digest-scent;
  - informational questions answerable only from a digest (slug words absent);
  - filing: "Where would you file a new concept about X? Reply with the folder path."
    (`expect`: folder path; bench matches a path not followed by `/`, research 13);
  - duplicate checks: "Is there already a note on X? Reply with its [[slug]]." Bench
    cannot score a correct "none" (a case passes only by mentioning an expected item), so
    include only cases where the note exists, and record false "none" answers by hand.
- **Report.** Pass rate and median wall time per variant (bench reports both). bench
  does not read tokens or tool calls (m1-m4 §3); record from the agent harness the
  number of `loci digest` calls, whether `--area X/sub` was used when the area was not
  shortened, `loci find` calls, and characters of digest output read. Record page sizes
  from `loci digest --json` (`chars`).
- **Repeat** each variant at least five times; agent runs are noisy.
- **Decision rule** (fix before running): adopt E1 over D only if V1 beats V0 on pass
  rate, or ties on pass rate and wins on median time or digest characters, without
  losing on filing. Prefer V1 over V2 only if it wins on known-item retrieval or
  filing. If V4 beats V1 clearly on filing, revisit E2 before C, since C gives up the
  middle level.

## Sources

LLM retrieval and agent memory:
- RAPTOR, Sarthi, Abdullah, Tuli, Khanna, Goldie & Manning, ICLR 2024: https://arxiv.org/abs/2401.18059 (HTML read: https://arxiv.org/html/2401.18059)
- LATTICE, Gupta et al., arXiv 2510.13217v2 (2026): https://arxiv.org/html/2510.13217
- MemWalker, Chen, Pasunuru, Weston & Celikyilmaz (2023): https://arxiv.org/html/2310.05029
- GraphRAG, Edge et al. (2024): https://arxiv.org/html/2404.16130 ; defaults: https://github.com/microsoft/graphrag/blob/main/packages/graphrag/graphrag/config/defaults.py ; configuration docs: https://microsoft.github.io/graphrag/config/yaml/
- graspologic `hierarchical_leiden`: https://graspologic-org.github.io/graspologic/reference/reference/partition.html
- GraphRAG dynamic community selection, Microsoft Research blog (2024): https://www.microsoft.com/en-us/research/blog/graphrag-improving-global-search-via-dynamic-community-selection/ (via fetch tool; direct download refused)
- PageIndex: https://github.com/VectifyAI/PageIndex (`pageindex/config.yaml`, `pageindex/page_index_classic.py`, main branch, read 2026-11-01)
- HiRAG, Huang, Huang, Yang, Pan, Chen, Ma, Chen & Cheng (2025): https://arxiv.org/html/2503.10150
- MemTree, Rezazadeh, Li, Wei & Bao (2024): https://arxiv.org/html/2410.14052
- H-MEM, Sun & Zeng (2025): https://arxiv.org/html/2507.22925
- A-MEM, Xu, Liang, Mei, Gao, Tan & Zhang, NeurIPS 2025: https://arxiv.org/html/2502.12110

Agents and progressive disclosure:
- Agent Skills best practices: https://platform.claude.com/docs/en/agents-and-tools/agent-skills/best-practices
- Claude Code memory: https://code.claude.com/docs/en/memory
- Claude Code skills (listing budget): https://code.claude.com/docs/en/skills.md
- Claude Code MCP tool search: https://code.claude.com/docs/en/mcp.md
- Introducing advanced tool use: https://www.anthropic.com/engineering/advanced-tool-use
- Effective context engineering for AI agents: https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents
- llms.txt: https://llmstxt.org/ (source: https://llmstxt.org/index.md)
- Sitemaps protocol: https://www.sitemaps.org/protocol.html
- SWE-agent, Yang et al., NeurIPS 2024: https://arxiv.org/html/2405.15793
- Lost in the Middle, Liu et al., TACL 2024: https://arxiv.org/html/2307.03172
- RAG-MCP, Gan & Sun (2025): https://arxiv.org/html/2505.03275

HCI and IR:
- Zaphiris (2001), Age differences and the depth-breadth tradeoff: https://ktisis.cut.ac.cy/server/api/core/bitstreams/e7f2092f-b90e-41b4-b946-d853b5f1c53d/content
- Larson & Czerwinski (1998), CHI: https://doi.org/10.1145/274644.274649 (abstract via OpenAlex)
- Snowberry, Parkinson & Sisson (1983), *Ergonomics* 26(7), 699–712; Parkinson, Sisson & Snowberry (1985), *IJMMS* 23(6), 689–697: https://doi.org/10.1016/S0020-7373(85)80063-8 (neither read)
- Hearst & Pedersen (1996), SIGIR: https://people.ischool.berkeley.edu/~hearst/papers/sg-sigir96/sigir96.html
- Hearst (2006), Design Recommendations for Hierarchical Faceted Search Interfaces: https://flamenco.berkeley.edu/papers/faceted-workshop06.pdf
- Hearst (2008), UIs for Faceted Navigation, HCIR: https://people.ischool.berkeley.edu/~hearst/papers/hcir08.pdf
- Teevan, Alvarado, Ackerman & Karger (2004), CHI: https://doi.org/10.1145/985692.985745 (abstract via OpenAlex)
- Bergman, Beyth-Marom, Nachmias, Gradovitch & Whittaker (2008), *ACM TOIS*: https://doi.org/10.1145/1402256.1402259 (abstract via OpenAlex)
- Miller & Remington (2004), Cutrell & Guan (2007), Blackmon et al. (2003), Dumais & Landauer (1983), Bergman et al. (2010): see research 12 and 13

Library classification:
- OCLC, Introduction to the Dewey Decimal Classification: https://www.oclc.org/content/dam/oclc/dewey/versions/print/intro.pdf
- UDC Consortium, About UDC structure: https://udcc.org/index.php/site/page?view=about_structure (via fetch tool)
- ANSI/NISO Z39.19-2005 (R2010): https://groups.niso.org/higherlogic/ws/public/download/12591/z39-19-2005r2010.pdf
