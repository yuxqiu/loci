# 15 — Digests versus search: what a one-line summary per concept is for

Status: research, 2026-11-01. Builds on research 12 (hierarchy depth), 13 (area
descriptions, generated versus authored text, drift) and 14 (oversized areas,
progressive disclosure, tool search). Their findings are cited, not repeated.
Question: every concept has a required frontmatter `digest` (one sentence, at most
`budgets.digest` = 160 characters, no links). With a good search tool, what is it
for? Would an agent use it, would the user benefit, and what does it cost?

Where the digest is used today (checked in the code, 2026-11-01):

| Consumer | Use | Code |
|---|---|---|
| `loci digest --area X` | every concept as `- [[slug]]: digest`; a concept without a valid digest prints as `- [[slug]]` | `loci-core/src/digest.rs` |
| `loci digest` (root), `loci status` | **not used**: the root shows area titles, paragraphs, counts and the sub-folder outline only | m0 §10 |
| `loci find` | a BM25 field with boost ×2 (title ×3, path ×1, body ×1). Hits show title, path and a body-line snippet; **the digest is not shown** | `loci-core/src/search/mod.rs` |
| `loci closure` | `- [[slug]]: digest` per prerequisite | `commands/closure.rs` |
| `loci export` | glossary `**title**: digest` for linked notes not inlined; a note **without a valid digest gets no glossary entry** | `loci-core/src/export.rs` |
| lint | `fm.missing-field` (absent or empty), `digest.multiline`, `digest.has-link`, `digest.too-long` (warning) | `loci-core/src/lint.rs`, `digest.rs` |
| skills | `concepts`: "`digest` is one sentence with no links; it is what `loci digest` shows"; `intake` step 2: "find whether it already exists (`loci digest`, `loci find` …)" | `share/loci/skills/` |

`loci new` writes `digest: ""`, so a new concept fails lint until the agent writes one.

Nothing here changes DESIGN or the specs. Conventions as in research 12 to 14:
`[unverified]` means I did not confirm the claim in a primary source (the reason is
given); `[design inference]` means my extrapolation, with no study behind it; `[gap]`
means I looked and found no evidence.

## TL;DR

| Question | Finding | Strength |
|---|---|---|
| 1. Do agents browse an index or search? | **Search is the primary path.** Claude Code dropped RAG for grep/glob ("mostly vibes", by its author's account). In LocAgent, removing keyword search cost 19 points of file accuracy, and removing graph browsing cost 2 (Table 6). In CoSIL, removing iterative search cost 28% at function level (Table IV). **A compact listing helps as a complement.** Agentless's LLM reading a file tree (names only) found the right file in 78.7% of cases against 67.7% for embeddings, and 81.7% combined (Table 2) | Several agent ablations, all on code |
| 1b. Do one-line summaries in a listing beat bare names? | **Mixed.** The one same-pipeline test (Caumartin et al. 2026) put file paths or LLM "role" summaries in the prompt for the model to pick from: summaries gained 10–12% for a 32B model and lost up to 20% for a 14B model (Table 5). As a reranking signal they were about even (Table 7); as an extra retrieval route in Agentless they added 2–5% (Table 8). A flat list of every file summary did much worse than a directory-filtered hierarchy (MRR 0.50 → 0.23, Rafiei Oskooei et al. 2025, Table 4). Vendors preload one-line descriptions where the agent must choose without opening (skills, Claude Code's `MEMORY.md`), and search over those same descriptions when the list is too long (tool search) | Thin; one direct test, small open models, code |
| 2. Does a summary field improve retrieval over the body? | **Little, for whole short notes.** Anthropic saw "very limited gains" from generic document summaries and "low performance" from summary-based indexing; its 35–49% gains come from restoring a chunk's missing document context. Adding abstracts to paragraph-level full text gave +7% MAP, not significant (Lin 2009). Weighting an author-written title field gave about 1.5% on news (Robertson et al. 2004). Furnas et al.: recipe bodies held about 80% of users' keywords. What expansion does give: term re-weighting and vocabulary the body lacks (doc2query: copied words alone 18.4 → 19.7 MRR@10) | Good IR evidence; none on personal notes `[gap]` |
| 3. Do people browse or search their own material? | **They browse and recognise** (Teevan et al. 2004: 61% of searches used no keyword search; Barreau & Nardi 1995: users "preferred browsing lists of files rather than trying to remember exact file names"; Whittaker et al. 2011: scrolling 62% of email refinding). But every study lists **names or titles, not summaries**, and **search was the only method that predicted refinding success** (Whittaker et al. 2011). Wikipedia's short descriptions are meant to "complement and clarify the page title", not summarise, and may be "none" | Good human evidence; summaries untested `[gap]` |
| 4. What does a required digest cost? | Writing: low for an agent, and the generative-learning benefit of summarising (Fiorella & Mayer 2016, median d = 0.50) goes to whoever writes, here the agent. Drift: a second copy of the note's point that nothing checks against the body; drift comes from text that is never revisited (research 13 §E). Budget: a digest line is most of an area-digest line (research 14) | Indirect |
| 5. Alternatives | A lead sentence of the body is used as a definition in encyclopedic text (Kazama & Torisawa 2007), but static lead summaries are weaker relevance cues than query-biased snippets (Tombros & Sanderson 1998). No study compares an authored summary field with a derived lead sentence for an index `[gap]` | Thin |

**Recommendation (option e below):** keep a one-line summary for every concept and
keep the area listing, because the listing is the one place where an agent or the
user chooses without opening notes, and closure and export need a one-liner. Do not
expect it to help search much, and stop justifying it by search. Two changes are
backed by the evidence: define the digest as Wikipedia defines a short description,
a line that **complements the title** (what kind of thing it is, the distinguishing
point, other names) rather than restating it; and **show it in `find` hits** next to
the body snippet. Then run the benchmark below, which needs no code: it compares
the status quo with a vault whose digests are replaced by each note's lead sentence
(option c) and by its title (option d). If the lead sentence ties, move to option c
and remove the field, since that removes the drift. Reject option b: an optional
summary gives a patchy listing and an export glossary with holes.

## 1. How agents localise information

### Search as the primary path

- **Claude Code.** Boris Cherny, Latent Space podcast, "Claude Code: Anthropic's
  Agent in Your Terminal", 7 May 2025 (https://www.latent.space/p/claude-code,
  [00:45:30]): "very, very early versions of Claude actually used RAG … eventually, we
  landed on just agentic search … it outperformed everything. By a lot … In what
  benchmark? This was just vibes, so internal vibes. There's some internal benchmarks
  also, but mostly vibes"; "Just using regular code searching, you know, glob, grep"
  ([00:46:20]). Hacker News, 24 Feb 2025 (item 43164253): "In our testing we found
  that agentic search out-performed RAG for the kinds of things people use Code for."
  X, 1 Feb 2026 (status 2017824286489383315): RAG "doesn't have the same issues around
  security, privacy, staleness, and reliability". **What it compares:** an embedding
  index against grep. It says nothing about a summary listing, and there are no
  published numbers.
- **Search and grep are complementary, by measurement.** Cursor, "Improving agent
  with semantic search" (Heule, Jia & Jain, 6 Nov 2025, https://cursor.com/blog/semsearch):
  adding semantic search to grep gave "on average 12.5% higher accuracy in answering
  questions (6.5%–23.5% depending on the model)" on an internal benchmark, and "the
  combination of these two leads to the best outcomes". Vendor numbers; per-model
  figures appear only in a chart.
- **LocAgent** (Chen et al., ACL 2025, arXiv 2503.09089), Table 6 ablation
  (Qwen2.5-7B fine-tuned; file Acc@5 / module Acc@10 / function Acc@10): full system
  88.32 / 82.85 / 71.53; without entity search 68.98 / 61.31 / 53.28; without the BM25
  index 75.18 / 68.98 / 60.22; without graph traversal 86.13 / 78.47 / 66.06.
  Removing keyword search hurts far more than removing structure browsing. Its search
  output is tiered: entity ids only when more than three match, a skeleton preview for
  large files, full code only for three or fewer (Fig. 6).
- **CoSIL** (Jiang et al., arXiv 2503.22424v3), Table IV (Qwen2.5-Coder-32B, SWE-bench
  Lite): without iterative search, function Top-1 fell from 0.433 to 0.320 (−27.8%),
  because the structure tree alone "lacks concrete code segment information". Without
  the module call graph, file Top-1 fell from 0.613 to 0.540; the authors suggest "the
  LLM is misled by the repository-structure tree".
- **SWE-agent** (Yang et al., NeurIPS 2024, arXiv 2405.15793). Beyond the 50-result cap
  and the Table 3 ablation in research 14: `search_dir` returns **file paths with match
  counts only** ("Found 24 matches … /pylint/checkers/misc.py (9 matches)"), and the
  agent typically starts with "a broad, directory-level keyword search" before zooming
  in (§5.2). A search result is itself a compact listing.
- **AutoCodeRover** (Zhang et al., arXiv 2404.05427), Table 1: `search_class` returns
  only the class signature; method searches return the implementation. No ablation of
  signatures against bodies.

### A compact listing as a complement

- **Agentless** (Xia et al., arXiv 2407.01489). Localisation is "1) localize to
  suspicious files; 2) … classes, functions, and variables; 3) … code edit locations"
  (§3.1). Step 1 shows the LLM the repository tree: **names only, no summaries**. Table 2
  (§5.2.1, SWE-bench Lite, GPT-4o; share of issues whose ground truth stays in the
  candidates):

  | Stage | Variant | Contains GT | Cost |
  |---|---|---|---|
  | File | LLM reads the file tree | 78.67% | $0.02 |
  | File | embeddings, no folder filter | 67.67% | $0.05 |
  | File | embeddings, folder filter | 70.33% | $0.04 |
  | File | combined | 81.67% | $0.06 |
  | Element | complete file | 53.67% | $0.15 |
  | Element | skeleton (signatures, class/module comments) | 58.33% | $0.02 |

  "prompting-based and embedding-based retrieval methods can complement each other";
  with complete files "not only is the cost much higher but also the number of
  localized groundtruth issues is reduced". A compressed view beat the full text.
- **Aider's repo map** (https://aider.chat/docs/repomap.html; source `aider/repomap.py`).
  "a list of the files in the repo, along with the key symbols which are defined in each
  file", ranked by `nx.pagerank` over a file dependency graph, personalised towards files
  and identifiers in the chat; `--map-tokens` "defaults to 1k tokens", growing to about
  8k (`map_mul_no_files = 8`) when no files are in the chat. It holds **symbols, not
  prose summaries**, and I found no with/without benchmark on aider's pages `[gap]`.

### Summaries in the listing versus bare names

- **Caumartin, Chen & Costa, "Retrieval-Oriented Code Representations in Agentic Bug
  Localization"**, arXiv 2607.11046v1 (July 2026). The most direct test of the digest
  question I found. Each file is represented as its path (STRUCT), an LLM summary of its
  role ("concise … file's main classes and methods as well as its role", ROLESUM), two
  other summary types, or raw source.
  - **In the prompt, for the model to pick from** (the "browse an index" condition),
    Table 5, change in Hit@5 from paths to role summaries: Qwen 7B +21.7% (LCA) and
    −18.9% (SWE-bench); Qwen 14B −20.0% and −14.1%; Qwen 32B +11.6% and +10.2%. "role-aware
    summaries underperform file paths for Qwen 14B on both datasets." Bug-oriented
    summaries were worse everywhere, down to −75.5%.
  - **As a reranking signal**, Table 7: SWE-bench 0.780 (paths) against 0.788
    (summaries) with Qwen3 embeddings, 0.718 against 0.716 with CodeXEmbed; mixed on
    LCA (0.753 → 0.767; 0.727 → 0.667).
  - **As an extra route inside Agentless**, Table 8 (Qwen2.5-Coder-32B, Hit@6): paths plus
    embeddings 0.898; adding summary retrieval 0.918; adding summary reranking 0.940.
  - **As a BM25 document**, Table 2: summaries beat paths (0.284 → 0.440 Hit@5 on
    SWE-bench Verified), but raw source beat both (0.676). Index size: paths 2.9 M
    tokens, summaries 39.7 M, source 830 M.
  - Limits: code, open models up to 32B, LLM-written summaries of whole files.
- **A flat list of all summaries at scale.** Rafiei Oskooei et al., hierarchical
  natural-language summaries for microservice repositories, arXiv 2512.05908 (ICSE
  2026), Table 4: giving the LLM "the summaries of all files within that repository" as
  one list, instead of filtering by directory first, dropped Pass@10 from 0.82 to 0.67
  and MRR from 0.50 to 0.23, with 44% more tokens. This agrees with research 14:
  a listing must stay within a budget, and above it a hierarchy or search must narrow
  it first.
- **Meta-RAG** (Tawosi et al., arXiv 2508.02611): summaries per file, class and function
  "condense codebases by an average of 79.8%" (§3.1) and reach 84.67% file accuracy on
  SWE-bench Lite with GPT-4o, but there is no same-pipeline run without summaries, so it
  does not isolate their effect.

### Vendor practice: descriptions are what the agent chooses on

- **Skills.** "At startup, only the metadata (name and description) from all Skills is
  pre-loaded"; "The description is critical for skill selection: Claude uses it to
  choose the right Skill from potentially 100+ available Skills"
  (https://platform.claude.com/docs/en/agents-and-tools/agent-skills/best-practices).
  No accuracy figure is published.
- **Claude Code auto memory.** `MEMORY.md` is the "Index, one line per memory, loaded
  into every session"; topic files are read "on demand using its standard file tools";
  near the 200-line or 25KB cap Claude is told to "keep one line per entry"
  (https://code.claude.com/docs/en/memory, "How it works"). This is the closest
  production analogue of `- [[slug]]: digest`, with one difference: it is preloaded
  and small, while loci's area listing is fetched on demand.
- **Tool search searches the descriptions.** "Both tool search variants (regex and
  bm25) search tool names, descriptions, argument names, and argument descriptions";
  "Add common keywords to tool descriptions to improve discoverability"; "Add a system
  prompt section describing available tool categories"
  (https://platform.claude.com/docs/en/agents-and-tools/tool-use/tool-search-tool).
  The engineering post: "Tool search matches against names and descriptions, so clear,
  descriptive definitions improve discovery accuracy"
  (https://www.anthropic.com/engineering/advanced-tool-use). With search the
  description does not disappear: it becomes the document that is searched. That
  holds for tools, whose description is most of their text; a concept note has a body.
- **llms.txt.** A flat list of `[name](url): notes` (https://llmstxt.org). A server-log
  study reports that 97% of about 38,000 llms.txt files got no requests in May 2026
  (Ahrefs, ahrefs.com/blog/llmstxt-study) `[unverified]`: I saw it only through search
  snippets, and it measures fetches, not usefulness.

**What 1 means for loci** `[design inference]`. Agents find things mainly by search,
and every study that removes search loses more than one that removes structure. A
compact listing still adds something, mostly when the agent must choose without
opening anything and the listing fits in one read; it adds the most when combined with
search (Agentless, Cursor). Whether one-line summaries in that listing beat bare names
is unsettled: the one direct test is model-dependent, and frontier models were not
tested. loci's own read paths fit this split. For a known item, `loci find` is the
natural route, and its hits show no digest. Where the agent must survey (intake
deduplication, filing, "what do I know about X", building a view), it reads the area
listing, and the digest is the only text there besides the slug.

## 2. Summaries as retrieval aids

- **Contextual Retrieval** (Anthropic, Sept 2024, https://www.anthropic.com/news/contextual-retrieval).
  Claude is given the whole document and asked to "give a short succinct context to
  situate this chunk within the overall document"; the "contextual text, usually 50-100
  tokens, is prepended to the chunk before embedding it and before creating the BM25
  index". Retrieval failures (1 − recall@20, averaged over codebases, fiction, arXiv and
  science papers) fell 35% with contextual embeddings (5.7% → 3.7%), 49% with contextual
  BM25 added (→ 2.9%), and 67% with reranking (→ 1.9%). The negative result matters more
  here: "adding generic document summaries to chunks (we experimented and saw very
  limited gains) … and summary-based indexing (we evaluated and saw low performance)".
  The gain comes from giving a fragment the context it lost when it was cut out. A loci
  concept is indexed whole, so it has no such context to lose.
- **Document expansion.** doc2query (Nogueira, Yang, Lin & Cho, arXiv 1904.08375):
  BM25 18.4 → 21.5 MRR@10 on MS MARCO dev (Table 1). Of the added words "31% are new
  while the rest (69%) are copied"; copied words alone gave 19.7, new words alone 18.8
  (§5). Copying "can effectively perform term re-weighting". docTTTTTquery (Nogueira &
  Lin 2019): 0.184 → 0.277 with 40 sampled queries per passage. Doc2Query-- (Gospodinov,
  MacAvaney & Macdonald, ECIR 2023, arXiv 2301.03266): expansions hallucinate, and
  filtering them improved effectiveness "by up to 16%" while cutting the index by 33%. A
  digest is one short, curated expansion: expect its re-weighting effect, at a much
  smaller size.
- **Weighting an author-written field.** Robertson, Zaragoza & Taylor, "Simple BM25
  extension to multiple weighted fields", CIKM 2004. On Reuters, title weights from 1
  to 5.5 improved precision "so slightly that we do not believe this to be significant"
  (under 1.5% relative, §4.2). On a web collection the large gain (+41% P@10) came from
  **anchor text**, written by other people (§4.3). They also warn that adding up
  separately computed per-field scores, rather than weighting term frequencies before
  BM25 saturates, can fall below the unweighted baseline: on Reuters that combination
  "never reaches the baseline, for any weight settings" (§4.2). loci's `find` does add
  them up: one `BoostQuery(TermQuery)` per field and term, combined with
  `Occur::Should` (`search/mod.rs`). So the ×2 digest boost is untested, and it may help
  or hurt `[design inference]`.
- **Abstracts and full text.** Lin, "Is searching full text more effective than
  searching abstracts?", BMC Bioinformatics 10:46 (2009), TREC 2007 Genomics, BM25: whole
  articles 0.146 MAP against abstracts 0.163 (not significant); paragraph-sized spans
  0.240 (+47%, significant). Adding the abstract to spans gave 0.257 (+7%, not
  significant; Table 3). "key ideas in an article are likely reinforced multiple times
  … This potentially alleviates mismatches between query terms and terms used by
  authors." A loci concept is about the size of a span. Its body already repeats its
  key terms.
- **The vocabulary problem.** Furnas, Landauer, Gomez & Dumais, CACM 30(11), 1987: "In
  every case two people favored the same term with probability <0.20"; a single
  designer's access term gives "very poor access (10-20 percent hit rates)" (§4);
  "The recipe texts contain roughly 80 percent of the keywords selected by users", and
  full-text indexing was "roughly equivalent to extensive alias 'harvesting' from
  experts" (§4). A digest written by one author is that single designer's choice. The
  body already supplies most of the alias coverage. A digest helps search only where
  it holds a word the body lacks: another name, a broader term, a translation.
- **A 150-character summary on chunks.** Reuter, Lingenberg et al., NLLP 2025, arXiv
  2510.06999: "a single, concise summary as 'document fingerprint', approximately 150
  characters long", prepended to each 500-character chunk of long legal documents,
  roughly halved document-level mismatch with dense retrieval (§5.1). Again, chunks,
  not whole notes. dsRAG's vendor benchmark found contextual headers raised the
  average from 4.72 to 6.04 but moved its ~800-file markdown handbook only from 5.7 to
  5.8 (README, GPT-4o grader).
- **Summaries as relevance cues for the reader of a result list.** Tombros &
  Sanderson, SIGIR 1998: query-biased summaries beat "a static predefined summary
  composed of the title and first few sentences"; readers had to open the full text for
  1.32% of documents against 23.7% (§4.3); static lead summaries are "inadequate" on
  their own. Mohtadi et al., arXiv 2512.05334 (2025): GPT-4o relevance labels from 80-token
  summaries agreed with gold labels about as well as labels from full documents (Cohen's
  κ .524 against .517 on TREC DL-19, Table 3), with "systematic shifts in label
  distributions". So a static one-liner is a fair cue for an LLM, and a query-biased
  snippet is better for a person. Showing both costs one line.

**What 2 means for loci** `[design inference]`. For whole notes of up to 12,000
characters, a digest's contribution to BM25 should be small: it re-weights the terms
the body already has and adds the few words the body lacks. It is no reason to keep
the field. Measuring it is cheap (blank the digests in a copy of the vault and compare
recall@k and MRR at tool level, §What to measure). The stronger use is in result lists:
`find` hits carry only the first matching body line, which the user cannot judge
without context (Tombros & Sanderson), and the digest is not shown.

## 3. People: browsing versus searching their own material

- **Teevan, Alvarado, Ackerman & Karger**, "The Perfect Search Engine Is Not Enough",
  CHI 2004, pp. 415–422 (full text read). 15 MIT CS graduate students, 151 interviews;
  the authors call the findings qualitative. Keyword search was used "in only 39% of
  their searches, despite almost always knowing their information need up front" (p.
  415). Orienteering "decreased their cognitive load, allowed them to maintain a sense of
  location during their search, and gave them a better understanding of their search
  result" (p. 418); "it is easier to recall the intermediate steps than to explicitly
  name the target" (p. 419). A participant: "even when I saw its name, I wouldn't have
  known that that was the file I wanted until I saw all of the other names in the same
  directory" (p. 419).
- **Barreau & Nardi**, "Finding and reminding", SIGCHI Bulletin 27(3), 1995 (preprint
  read): "users overwhelmingly preferred location-based search"; "users … preferred
  browsing lists of files rather than trying to remember exact file names"; one user
  could not recall the name of a file made that morning: "Scanning the list to recognize
  the file name was easier than trying to remember the exact name". And: "No matter how
  good text-based search is, there is no reminding function that comes along with
  logical search."
- **Bergman et al.**, TOIS 2008 (abstract only; full text paywalled `[unverified]`):
  users "estimated" navigation for 56–68% of retrievals and search for 4–15%; search
  was "a last resort". Dinneen & Julien, "The ubiquitous digital file", JASIST 2019
  (arXiv 2109.09668, p. 14), reviewing more than 230 publications: "A preference for
  navigating to files is much more common than a preference for searching".
- **The counterweight.** Whittaker, Matthews, Cerruti, Badenes & Tang, "Am I wasting my
  time organizing email?", CHI 2011 (345 users, more than 85,000 refinding actions; full
  text read): scrolling 62% of accesses, search 18%, folders 12% (Table 2). "high filers
  were no more successful at finding messages than low filers"; "people who relied on
  search were more likely to have successful finding sequences (r(356)=0.15, p<0.005).
  None of the other behaviors was correlated with success." Effortful preparation for
  browsing (filing) did not pay; cheap browsing (scrolling a list) was popular; search
  predicted success. Whittaker & Sidner (CHI 1996): filing is "cognitively difficult"
  because it requires imagining future retrieval, and 35% of folders held one or two
  items.
- **What the line beside a title should say.** Wikipedia short descriptions
  (WP:SHORTDESC, read 2026-11-01) exist "to complement and clarify the page title … to
  help a user identify the desired article in a list of search results"; "the aim is not
  to provide a definition of the article's topic, nor to summarise the lead"; "none" is
  allowed when the title suffices; longer than 40 characters "may be truncated". They
  began in 2017 from Wikidata descriptions, and a 2017–2018 RfC moved them into local
  text "after concerns were raised about accuracy, suitability, and the potential for
  hard-to-detect vandalism on Wikidata".
- **Reading from summaries.** King, Tenopir & Clarke, D-Lib 12(10), 2006: in one
  pediatrics journal, abstract-only readings were about 7,200 per article against 14,700
  readings of the text, and "the entire article may be read later". Hartley (JMLA 2004)
  on structured abstracts: easier to search, but subject to "the same sorts of
  omissions and distortions" as traditional ones. People use short summaries to decide
  what to read; I found no number for how often the decision is right `[gap]`.
- **Zettelkasten practice** (practitioner sources, not studies). Luhmann's index had
  "roughly 1,250 entries" for about 23,000 cards and listed "only one to four places"
  per term; there were no per-card summaries, and the absence of "a table of contents
  turned the index into the key tool" (Schmidt, Sociologica 12(1), 2018, §6).
  Matuschak: titles "become an abstraction for the note itself"
  ("Evergreen note titles are like APIs"); "If I write a note but struggle to summarize
  it in a sharp title, that's often a sign that my thinking is muddy"
  ("Prefer note titles with complete phrases to sharpen claims").

**What 3 means for loci** `[design inference]`. The user benefits from a list to scan:
recognising an item among its neighbours is easier than naming it, and a list reminds
in a way search cannot. Every study, though, lists names or titles, and none tests
whether a summary beside them helps. Wikipedia's answer is a short line that adds what
the title lacks, and nothing when it lacks nothing. loci's digest is closer to a
summary (the `concepts` skill: "One sentence that summarises the concept"). A slug
such as `dijkstra` needs a line saying what it is; a title such as "Dijkstra's
algorithm" needs a line saying what is distinctive about it ("shortest paths from one
source with non-negative weights").

## 4. Costs

- **Writing.** For an agent, writing one sentence costs little. Summarising helps the
  writer understand (Fiorella & Mayer, Educ. Psychol. Rev. 28, 2016: 26 of 30
  comparisons positive, median d = 0.50; Dunlosky et al., PSPI 2013, §3.5, still rate it
  "low utility" because it depends on skill). In loci the agent writes the digest, so
  the user gets none of that benefit unless they write or revise digests themselves.
  The forcing function survives: if the agent cannot write a sharp line, the note may
  hold more than one concept (Matuschak's test) `[design inference]`.
- **Drift.** The digest restates the body in a second place that no check ties to it.
  Research 13 §E covers the evidence: comments drift when they are never revisited, not
  because they are updated late (Fluri et al. 2007; Wen et al. 2019); stale references
  in agent context files (Treude & Baltes 2026). Tan et al. (EMSE 2023, abstract only
  `[unverified]` beyond it) found outdated code references in "most projects" at some
  point. loci's digest sits in the same file the agent edits, which favours updates in
  the same change (Fluri: when a comment changed with its code it did so in the same
  revision about 97% of the time). Hand edits by the user, which the design allows (DESIGN
  §5.1), are the case where the digest is easiest to forget. I found no measurement of
  digest drift in an agent-maintained vault `[gap]`.
- **Quality at creation.** Greenberg et al. (DC-2001, 11 records): authors "can create
  good quality metadata"; the optional description field was skipped once. No drift was
  measured.
- **Budget.** The digest is most of each area-listing line, and the line count drives
  when an area overflows `area_index` (research 14: about 130 characters per full line
  against 26 for a slug entry). Research 14's design E1 already drops digests first when
  a listing is over budget, as Claude Code's skills listing does.

## 5. Options for loci

### (a) Keep the required digest (status quo)

- **digest:** unchanged. **find:** unchanged (×2 field). **closure, export:** unchanged.
  **lint:** unchanged. **Skills:** unchanged.
- **Agent read path:** search for known items; area listing to survey, deduplicate and
  file; closure lines when planning.
- **Human use:** a scannable area listing; closure and export glossary lines.
- **For:** every listing line carries scent (skills and `MEMORY.md` choose on one line;
  Claude Code warns that names without descriptions lose "the keywords Claude needs",
  research 14); the user browses rather than recalls (§3); closure and export have their
  one-liner; the cost of writing is low for an agent.
- **Against:** little search benefit (§2); untested boost (Robertson et al.); a second
  copy of the note's point that can drift; the field is defined as a summary where the
  evidence favours a line that complements the title; not shown in `find` hits, the most
  common read path.

### (b) Keep the digest but make it optional

- **digest:** `- [[slug]]` for notes without one (already the fallback). **find:** the
  field is empty for those notes. **closure:** slug only. **export:** today a note without a
  valid digest gets no glossary entry, so the glossary gets holes unless it falls back
  to something. **lint:** drop `fm.missing-field` for `digest`; keep the format checks.
  `loci new` stops writing `digest: ""`. **Skills:** say when to write one.
- **Agent read path and human use:** as (a) where present, slugs elsewhere.
- **For:** Wikipedia allows "none" when the title suffices; less to write and maintain.
- **Against:** an agent following "write one when useful" decides inconsistently, so the
  listing mixes two line formats with no rule the reader can rely on. Optional fields
  get skipped (Greenberg et al.: the one optional field was the one left empty). The
  export glossary needs a fallback, which is option c's derivation anyway. It keeps
  both the drift and the field `[design inference]`.

### (c) Derive the line from the note (lead sentence) and drop the field

- **digest:** each line shows the first sentence of the body's first paragraph, with
  links reduced to their text, cut at `budgets.digest` with an ellipsis. **find:** drop
  the `digest` field (the lead sentence is already in `body`), or index the derived lead
  as its own field to keep the re-weighting. **closure, export:** the derived line.
  **lint:** remove the `digest.*` rules and `fm.missing-field` for `digest`; add a rule
  that a concept starts with a prose paragraph after the H1 (warning), and perhaps a
  length warning on its first sentence. **Schema:** removing a frontmatter field changes
  `schema` and needs `loci upgrade` (m0 §11). The upgrade "never changes note prose", so
  it may delete the field but must not move its text into the body; the two-release
  retirement (`deprecated.digest`, then removal) gives the agent time to make each note's
  opening sentence carry the point. **Skills:** `concepts` says to open with one sentence
  that says what the concept is; `intake` unchanged. `share/loci/rules.md` line 42
  changes.
- **Agent read path:** as (a). **Human use:** as (a), with the line now the opening of the
  note itself.
- **For:** nothing to drift: the line is the text. One fewer field and four fewer lint
  codes. Encyclopedic writing already opens with a definition (Kazama & Torisawa 2007
  used Wikipedia first sentences as definitions). Hand edits keep the index current.
- **Against:** it imposes an opening on notes written "in the user's style" (`STYLE.md`).
  A first sentence may be long, start with a formula or code, or depend on the H1 ("It
  is …"). Static lead sentences are weak relevance cues (Tombros & Sanderson). Sentence
  splitting across languages, including CJK, is new code with edge cases. No study
  compares a derived lead with an authored line for an index `[gap]`. Whether real notes'
  lead sentences are good enough is the decisive unknown, and it is measurable (below).

### (d) Drop digests and area listings; rely on `find` and titles

- **digest:** the root stays; `--area` goes, or lists titles only. **find:** no digest field.
  **closure:** slugs and titles. **export:** a glossary of titles is pointless, so links
  become plain text. **lint:** `digest.*` rules removed; schema change as in (c).
  **Skills:** `intake` and `concepts` deduplicate and file with `find` and the root.
- **Agent read path:** search only, plus the root and the sub-folder outline for filing.
- **Human use:** search and the file tree.
- **For:** search is the agents' primary path and removing structure costs least
  (LocAgent, CoSIL); names-only listings work for code (Agentless file tree); Luhmann
  ran a large card index without card summaries; least to maintain; search predicted
  refinding success (Whittaker et al. 2011).
- **Against:** the survey tasks (deduplication, filing, "what do I know about X", building
  a view) lose their one-read overview, and search needs the right words, which one
  author's vocabulary rarely matches (Furnas). People browse their own material, and a
  list reminds in a way search does not (Barreau & Nardi; Teevan et al.). closure and
  export lose their gloss. Names without descriptions "remove the keywords" (Claude Code
  skills docs, research 14). It goes further than any evidence asks.

### (e) Keep the line, redefine it, show it where it is read, and test (c)

1. **Keep a required one-line summary and the area listing** (a), and change its
   definition in the `concepts` skill and `share/loci/rules.md`: the line
   complements the title. It says what kind of thing the concept is and what
   distinguishes it, and it carries other names and a broader term where the title
   lacks them, which is the one place a digest helps BM25 (§2). It does not restate
   the title. This is Wikipedia's short-description rule (§3) and the skills
   guidance "include key terms" (research 13 §F). It needs no code change.
2. **Show the digest in `find` hits**, one line under the title, beside the
   query-biased snippet (Tombros & Sanderson; Mohtadi et al.). `find --json` gains a
   `digest` field per hit (an additive change; no schema bump).
3. **Run the benchmark below** with the lead sentence (option c) and titles only (option d) as
   variants. Adopt (c) if it ties.

- **Effect on lint, closure, export:** none until the benchmark decides. **Agent read
  path:** search hits now carry a one-line gloss, so fewer notes are opened to check a
  hit `[design inference]`.
- **For:** keeps what the human and vendor evidence supports (a scannable list, a line
  per choice); moves the line to where it is read most; aims it at what the title and
  body lack; drift can then be weighed against measured value.
- **Against:** keeps the drift risk until the benchmark runs; adds one line per `find` hit
  (k = 10 by default, about 1,600 characters at most).

I also considered generating digests with an LLM at read time. Every read would call
a model, `loci` has no model dependency, and `loci digest` would no longer give the
same bytes for the same tree (m0 §9.4). Rejected.

## Recommendation

Adopt **(e)**. The answer to the user's question:

- **With good search, what is the digest for?** Not for search: the evidence says it
  adds little to BM25 over a whole note (§2). It is for **choosing without opening**:
  the area listing, closure and export, and, if added, `find` hits. People choose by
  recognising an item in a list (§3); vendors give agents one line per option wherever
  they must choose (§1).
- **Would agents use it?** For known items they search, and the digest is not in the
  hits today. For surveys (intake deduplication, filing, building a view) the area
  listing is the only one-read overview, and the digest is its only text besides the
  slug. Whether that line beats a bare name or title for a frontier model is unmeasured
  `[gap]`; the one direct test is mixed and uses small models.
- **Would the user benefit?** They browse their own material and recognise rather than
  recall, but the evidence is about names, not summaries. A line that complements the
  title is what Wikipedia found worth maintaining.

Do not adopt (b). Do not adopt (d) without the benchmark: it removes the one surface
the human evidence supports. Adopt (c) in place of the field if the benchmark shows
the lead sentence does as well, because it ends drift without losing the listing.

## What to measure

The decisive evidence is missing `[gap]`: no study compares, on a note collection, an
authored one-liner, a derived lead sentence, a title only and no listing. Every
variant below is a **copy of the same sample vault with its digests rewritten by a
script**, so no loci code changes. Use a neutral sample vault (8–12 areas, 150–400
concepts, written in a consistent style; research 13's benchmark vault can be reused).

**Variants** (same notes, only `digest:` changes):

- **V0** status quo.
- **V1** blank: every `digest` set to `""`. The listing prints an empty digest as
  `- [[slug]]` and `find` indexes nothing for it, so this measures search without the
  field and a slugs-only listing (lint reports `fm.missing-field`, which the run
  ignores).
- **V2** lead: `digest` := the first sentence of the body, links reduced to their text,
  cut at 160 characters. Simulates option c.
- **V3** title: `digest` := the H1. Simulates a titles-only listing (option d's listing)
  while keeping slugs; in `find` it re-weights title terms, so read V3's tool-level
  numbers as "title counted twice", not as "no digest".
- **V4** redefined: digests rewritten under the rule of option e (complement the title,
  other names, broader term). Measures the redefinition itself.

**Lead-sentence audit** (on V2's script output, before any agent run): the share of
concepts whose first body block is a prose paragraph; the share of lead sentences of
at most 160 characters; the share that start with a pronoun referring to the H1. If
under about 80% of notes have a usable lead, option c needs a style change first
`[design inference]`.

**Cases** (`bench/retrieval.jsonl`, `expect` = slug):

- **Known-item, paraphrased:** questions whose key words are absent from the slug and
  H1 and present in the body (tests search with and without the field).
- **Vocabulary-gap:** questions using another name or broader term that appears only in
  the V0/V4 digest (the case where a digest should help `find`; V1 and V2 should lose).
- **Survey/deduplication:** "Does the vault already have a note on X? Reply with its
  wikilink, or `none`." Include cases where the note exists under a different name; score
  false `none` answers by hand, as research 14 suggests.
- **Filing:** "Which area folder would a new note about X go in?" (`expect` = folder
  path), as in research 13.

**Runs.**

1. Tool level, all variants: `loci bench -k 10`, recall@10 and MRR. V0 against V1
   isolates the digest's search contribution; Robertson et al. predict a small
   difference.
2. Agent level, V0–V4: `loci bench --agent <command>`, pass rate and median time,
   at least three repeats per variant to estimate run-to-run noise. bench does not read
   token or tool counts (m1-m4 §3), so record from the agent harness the number of
   `loci digest --area` and `loci find` calls and the characters of listing output read.
3. If (e)'s `find` change is implemented, rerun V0 at agent level with and without the
   digest in hits.

**Decision rules** (fix before running):

- Adopt (c) if V2 is within run-to-run noise of V0 on every case type and the audit
  passes.
- Keep a field (V0 or V4) only if it beats V2 by more than the noise on survey or
  vocabulary-gap cases. Adopt V4's definition if it beats V0 there.
- Consider (d) only if V1 and V3 tie V0 on survey and deduplication cases as well as on
  known-item cases.
- Report the V0–V1 tool-level difference whatever the outcome. It tells whether the ×2
  boost earns its place under the summed-field scoring.

## Sources

Agents and localisation:
- Cherny, Latent Space podcast (7 May 2025): https://www.latent.space/p/claude-code ; Hacker News comment (24 Feb 2025): https://news.ycombinator.com/item?id=43164253 ; X post (1 Feb 2026): https://x.com/bcherny/status/2017824286489383315
- Cursor, "Improving agent with semantic search" (6 Nov 2025): https://cursor.com/blog/semsearch
- Agentless, Xia et al. (2024): https://arxiv.org/abs/2407.01489
- SWE-agent, Yang et al., NeurIPS 2024: https://arxiv.org/abs/2405.15793
- AutoCodeRover, Zhang et al. (2024): https://arxiv.org/abs/2404.05427
- LocAgent, Chen et al., ACL 2025: https://arxiv.org/abs/2503.09089
- CoSIL, Jiang et al. (2025): https://arxiv.org/abs/2503.22424
- Caumartin, Chen, Costa (2026): https://arxiv.org/abs/2607.11046
- Rafiei Oskooei et al. (2025), ICSE 2026: https://arxiv.org/abs/2512.05908
- Meta-RAG, Tawosi et al. (2025): https://arxiv.org/abs/2508.02611
- Aider repo map: https://aider.chat/docs/repomap.html ; https://aider.chat/2023/10/22/repomap.html ; source: https://github.com/Aider-AI/aider/blob/main/aider/repomap.py
- Agent Skills best practices: https://platform.claude.com/docs/en/agents-and-tools/agent-skills/best-practices
- Claude Code memory: https://code.claude.com/docs/en/memory
- Tool search tool: https://platform.claude.com/docs/en/agents-and-tools/tool-use/tool-search-tool ; "Introducing advanced tool use": https://www.anthropic.com/engineering/advanced-tool-use
- llms.txt: https://llmstxt.org/ ; Ahrefs llms.txt study: https://ahrefs.com/blog/llmstxt-study/ (search snippets only)

Retrieval:
- Anthropic, "Introducing Contextual Retrieval" (2024): https://www.anthropic.com/news/contextual-retrieval
- doc2query, Nogueira et al. (2019): https://arxiv.org/abs/1904.08375 ; docTTTTTquery: https://cs.uwaterloo.ca/~jimmylin/publications/Nogueira_Lin_2019_docTTTTTquery-v2.pdf
- Doc2Query--, Gospodinov et al., ECIR 2023: https://arxiv.org/abs/2301.03266
- Robertson, Zaragoza, Taylor (2004), CIKM: https://doi.org/10.1145/1031171.1031181
- Lin (2009), BMC Bioinformatics 10:46: https://doi.org/10.1186/1471-2105-10-46
- Furnas, Landauer, Gomez, Dumais (1987), CACM 30(11): https://doi.org/10.1145/32206.32212
- Reuter, Lingenberg et al. (2025), NLLP: https://arxiv.org/abs/2510.06999
- dsRAG README: https://github.com/D-Star-AI/dsRAG
- Tombros & Sanderson (1998), SIGIR: https://doi.org/10.1145/290941.290947
- Mohtadi, Roitero, Mizzaro, Demartini (2025): https://arxiv.org/abs/2512.05334
- Kazama & Torisawa (2007), EMNLP-CoNLL: https://aclanthology.org/D07-1073/

People and personal information:
- Teevan, Alvarado, Ackerman, Karger (2004), CHI: https://people.csail.mit.edu/teevan/work/publications/papers/chi04.pdf
- Barreau & Nardi (1995), SIGCHI Bulletin 27(3): https://doi.org/10.1145/221296.221307 ; preprint: http://www.ibiblio.org/msmckoy/spring2006/articles/fileOrganizationFromTheDesktop.pdf
- Bergman et al. (2008), TOIS 26(4): https://doi.org/10.1145/1402256.1402259 (abstract only)
- Dinneen & Julien (2019), JASIST: https://doi.org/10.1002/asi.24222 ; https://arxiv.org/abs/2109.09668
- Whittaker & Sidner (1996), CHI: https://doi.org/10.1145/238386.238530
- Whittaker, Matthews, Cerruti, Badenes, Tang (2011), CHI: https://doi.org/10.1145/1978942.1979457
- Wikipedia:Short description: https://en.wikipedia.org/wiki/Wikipedia:Short_description
- King, Tenopir, Clarke (2006), D-Lib 12(10): https://www.dlib.org/dlib/october06/king/10king.html
- Hartley (2004), JMLA 92(3): https://www.ncbi.nlm.nih.gov/pmc/articles/PMC442180/
- Schmidt (2018), Sociologica 12(1): https://doi.org/10.6092/issn.1971-8853/8350
- Matuschak, "Evergreen note titles are like APIs": https://notes.andymatuschak.org/Evergreen_note_titles_are_like_APIs

Costs:
- Fiorella & Mayer (2016), Educ. Psychol. Rev. 28: https://doi.org/10.1007/s10648-015-9348-9
- Dunlosky et al. (2013), PSPI 14(1): https://doi.org/10.1177/1529100612453266
- Greenberg, Pattuelli, Parsia, Robertson (2001), DC-2001: https://dcpapers.dublincore.org/files/articles/952106472/dcmi-952106472.pdf
- Tan et al. (2023), EMSE: https://doi.org/10.1007/s10664-023-10397-6 (abstract only)
- Fluri et al. (2007), Wen et al. (2019), Treude & Baltes (2026): see research 13
