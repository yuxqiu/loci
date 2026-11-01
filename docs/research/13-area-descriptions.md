# 13 — What an area description should contain, and at which levels

Status: research, 2026-11-01. Builds on research 12 (hierarchy depth and per-folder
description files), which surveyed PKM tools and docs sites and is not repeated here.
Verification pass, 2026-11-01: the sources that earlier came only through
secondary summaries or abstracts were re-fetched and checked against the primary
text where it was available; the remaining `[unverified]` markers say why.
Question: loci describes only top-level areas (`AREA.md`: an H1 and one paragraph).
A broad area such as `mathematics` gets a paragraph too general to help, and narrow
areas lose the natural middle levels (mathematics > linear algebra > eigenvalues).
Should descriptions exist at other levels, and what should a description contain?

Current loci design (m0 §3, §6.3, §10; D78): concepts live at
`concepts/<area>/**/<slug>.md`, slugs are unique vault-wide, links are bare
`[[slug]]` or vault paths. Each top-level area folder has a required `AREA.md` (H1,
one paragraph of at most `budgets.area_paragraph` = 600 characters; links allowed
and checked). `loci digest` prints each area's H1, path, concept count and
paragraph (budget `root_index` = 8000). `loci digest --area X` prints every concept
in X (slug and digest), with concepts directly in the area first, then one
`## <relative-dir>` group per sub-folder (budget `area_index` = 24000). Sub-folders
have no description; a file named `AREA.md` in a sub-folder is a concept that fails
`slug.format`. The agent uses the root digest to orient, to choose where to file a
new concept, and to find things.

Nothing here changes DESIGN or the specs. Conventions as in research 12:
`[unverified]` means I did not confirm the claim in a primary source (often because
the publisher blocked access; the secondary source is then named);
`[design inference]` means my extrapolation, with no study behind it; `[gap]` means
I looked and found no evidence.

## TL;DR

| Question | Finding | Strength |
|---|---|---|
| A. Do summaries at inner nodes help find a leaf? | Yes, **when the reader must choose a branch without seeing the leaves** (MemWalker, LATTICE, PageIndex, Scatter/Gather). RAPTOR's "collapsed tree", which ranks the nodes of every level by similarity and keeps the best up to 2000 tokens, beat stepping down the tree; it is a ranked pick, closer to `loci find -k` than to a full listing (corrected by research 14). Systems store a generated summary per node, or key terms plus sample children, and regenerate it; LATTICE reports that stale summaries "mislead the search" | Good for LLM retrieval over documents; nothing on personal note vaults `[gap]` |
| B. What makes a branch label useful? | Label quality matters more than structure: label ratings explained 58.2% of the variance in navigation time (Miller & Remington 2004); user-centred labels beat company-centred ones, and better organisation helped only when labels were poor (Resnick & Sanchez 2004, abstract). The named failure modes are weak-scent, unfamiliar and confusable labels and competing headings; competing headings cost the most and were repaired by offering a path under every competing heading, not by rewording (CWW, Blackmon et al. 2003, 2005). Adding example members under a label added nothing measurable (Blackmon et al. 2003; Dumais & Landauer 1983, abstract). Longer search-result snippets helped informational tasks but hurt navigational ones (Cutrell & Guan 2007) | Good (human studies and a validated model) |
| C. Depth vs breadth | Each level costs time (Bergman et al. 2010: +2.2 s per level). A middle level pays off only when its labels are clear; with vague upper labels, a flatter structure was faster (Miller & Remington 2004). Help fields previewing the next level helped at upper levels only (Snowberry et al. 1985, as summarised by the NRC), but a later study found no systematic benefit (Kreigh et al. 1990, abstract) | Good for click navigation; indirect for an index an agent reads in one piece |
| D. What LIS says a category description contains | Thesaurus scope note: the chosen meaning, what is deliberately excluded, and how to tell it from overlapping terms, with reciprocal notes on the other term (Z39.19 §6.2.2). Dewey: scope and definition notes, "class here" (major topics), "including" (minor topics), "class elsewhere" (where neighbouring topics go). UDC scope notes use the same pattern ("Class here …; for … see …") and include reciprocal pairs. Notes are given **once, at the highest level**, and apply downward (hierarchical force). Grouping levels below a term get **node labels**, not descriptions (Z39.19 §8.3.5; ISO 25964-1 2.38). Dewey lists subdivisions in summaries only where a class is large | Standards and the systems' own manuals and data |
| E. Generated vs authored | Generated structure never drifts; authored text gives better scent but drifts. Developer-written agent context files beat LLM-generated ones, and neither helped agents find the right files (Gloaguen et al. 2026). 23% of repositories had stale code references in agent configuration files (Treude & Baltes 2026) | Thin; mostly coding agents, not note vaults |
| F. LLM agents | A short description per option is what the agent selects on (skills `description`, MCP tool descriptions: description defects changed tool selection by up to 11.6 points). Indexes are flat lists with one level of reference (llms.txt, skills). No study compares "description at any depth" with "description at a fixed level" `[gap]` | Vendor docs plus two 2026 preprints |

**Recommendation (option 5 below):** keep `AREA.md` at the top level only, but
(1) say exactly what its paragraph contains, as a **scope note**: what belongs, a few
key topics that have no sub-folder (optional; the weakest-supported part), and what
looks as if it belongs but goes elsewhere, with a path link to that sibling area; and (2) have `loci digest` print,
under each area's paragraph, a **generated outline** of the area's sub-folders with
concept counts. Sub-folders stay without descriptions (node labels). A sub-folder that
needs its own paragraph should become an area. This removes the "vague broad area"
problem without a new file kind or a drift-prone table of contents. Measure it with
`loci bench --agent` before adopting; keep option 2 (optional description at any
depth) as the fallback if the outline proves too thin.

## A. Hierarchical summaries for navigation and retrieval

### What each system stores per node

| System | Node content | Built how | Kept fresh how | Reader sees leaves? |
|---|---|---|---|---|
| Scatter/Gather (Cutting et al. 1992) | "cluster digest": topical words plus a few typical titles | generated (frequent terms; titles nearest the centroid) | recomputed per interaction | no |
| RAPTOR (Sarthi et al. 2024) | free-text LLM summary of a cluster | bottom-up: embed, GMM-cluster, summarise, repeat | not addressed (one-off build) | yes (collapsed tree) |
| GraphRAG (Edge et al. 2024) | "report-like" community summary: title, executive summary, rating, 5–10 findings | Leiden hierarchy; higher levels use sub-community summaries | not addressed (full re-index) | no |
| MemWalker (Chen et al. 2023) | summary of segments, then summary of summaries | bottom-up, 5–8 children per node | static per document | no (navigates) |
| MemTree (Rezazadeh et al. 2024) | aggregated text plus embedding | top-down insertion | every ancestor on the insertion path is rewritten by an LLM | no |
| LATTICE (Gupta et al. 2025) | LLM-written cluster title and description, or subtree summary | top-down LLM clustering (better) or bottom-up | static; stale summaries "mislead the search" | no (navigates) |
| PageIndex (VectifyAI) | section title, page range, summary (≤150 words by default) | structure from document layout; LLM summarises | rebuilt per document | no (navigates) |

Details and sources:

- **Scatter/Gather.** Cutting, Karger, Pedersen & Tukey, "Scatter/Gather: a
  cluster-based approach to browsing large document collections", SIGIR 1992,
  pp. 318–329 (doi 10.1145/133160.133214). I did not obtain the 1992 text; the
  digest is described by the same group: summaries "consist of topical terms that
  characterize each cluster generally, and a number of typical titles that sample the
  contents of the cluster" (Hearst & Pedersen, "Reexamining the cluster hypothesis",
  SIGIR 1996, Introduction). In their user study, participants chose the cluster with
  the most relevant documents in 31 of 38 cases (≈82%), with caveats the authors state
  (ties counted as correct; relevance judged from the first 11 documents)
  (same paper, "Analysis of participants' use of Scatter/Gather"). Pirolli & Card
  (1999), who model Scatter/Gather users, describe the same digest: "Topical words are
  those that occur most frequently in a cluster, and typical titles" are those nearest
  the cluster's centroid; "This summary is called a cluster digest" (PARC UIR technical
  report version of the *Psychological Review* paper, section on the Scatter/Gather
  browser, pp. 35–36). They also report that their scent model computed from these
  digests "tracks quite well the underlying (distal) structure of relevant
  information" (same report, p. 48, Figures 11–12). Pirolli, Schank,
  Hearst & Diehl (CHI 1996, pp. 213–220) report that Scatter/Gather communicated the
  collection's topic structure better than word search; the only free copy is an image
  scan with no text layer and the ACM DL refused access, so the finding is from the
  abstract as indexed by Semantic Scholar `[unverified]`. **The point for loci:**
  the best-studied browsing digest is made of
  *key terms and sample children*, generated from the children, not of an authored
  summary.
- **RAPTOR.** Sarthi, Abdullah, Tuli, Khanna, Goldie & Manning, "RAPTOR: Recursive
  Abstractive Processing for Tree-Organized Retrieval", ICLR 2024 (arXiv
  2401.18059). Leaves are ~100-token chunks; inner nodes are gpt-3.5-turbo summaries
  of GMM clusters (§3). Two query modes: tree traversal (layer by layer) and collapsed
  tree (all nodes of all layers in one pool, ranked by cosine similarity to the query
and kept until 2000 tokens, about the top 20 nodes). "The collapsed tree approach
  consistently performs better" (§3, Querying; Figure 3, 20 QASPER stories). Inner
  nodes matter: 18.5–57% of retrieved nodes are non-leaf (Appendix I.2), and using all
  layers beat any single layer (§4.1, Table 8). About 4% of summaries had minor
  hallucinations, which "did not propagate to higher layers" (§3; Appendix E.2). The
  paper does not discuss updating the tree.
- **GraphRAG.** Edge, Trinh, Cheng, Bradley, Chao, Mody, Truitt, Metropolitansky, Ness
  & Larson, "From Local to Global: A Graph RAG Approach to Query-Focused
  Summarization", arXiv 2404.16130 (2024). Community summaries are built per level of a
  Leiden hierarchy; higher levels substitute shorter sub-community summaries for
  element summaries (§3.1.5). The template is a title, an executive summary, an impact
  rating and "a list of 5-10 key insights" (Appendix E.2). Root-level summaries (C0)
  needed "over 97% fewer tokens" than summarising source text (§5.1) and showed no
  statistically significant difference from lower levels on claim-based measures
  (§5.2). The task is global sense-making ("what are the themes?"), not finding a
  leaf, and the paper does not cover index updates.
- **MemWalker.** Chen, Pasunuru, Weston & Celikyilmaz, "Walking Down the Memory Maze:
  Beyond Context Limit through Interactive Reading", arXiv 2310.05029 (2023). The LLM
  reads node summaries and chooses a child or goes back (§3). It "only works when the
  LLM exhibits a strong enough reasoning capability" (§7); reverts happen in about
  15–20% of examples, with 60–80% recovery (§5, Table 4). Every navigation step is a
  decision that can go wrong.
- **MemTree.** Rezazadeh, Li, Wei & Bao, "From Isolated Conversations to Hierarchical
  Schemas: Dynamic Tree Memory Representation for LLMs", arXiv 2410.14052 (2024).
  Each node stores aggregated content, an embedding, parent, children and depth (§3).
  On insertion every parent on the path is rewritten by an "LLM-based operation" so it
  "reflect[s] the new information" (§3.1): average 3.27 LLM calls per insertion
  (Appendix A.3.4). Parent content becomes more abstract as descendants accumulate
  (§3.1). Freshness is bought by rewriting the summary on every write.
- **LATTICE.** Gupta, Chang, Bui, Hsieh & Dhillon, "LLM-guided Hierarchical Search for
  End-to-end Reasoning Intensive Retrieval", arXiv 2510.13217 (2025). Inner nodes
  carry LLM-written descriptions; the bottom-up prompt asks, per child, for a
  "one-line summary", "key entities" and "genre or category", then a common theme and
  a summary that must "include all key entities" (Appendix D, Figure 13). LLM top-down
  clustering beat bottom-up construction (Biology 69.4 vs 67.6, TheoremQA-T 46.0 vs
  33.5; Table 4). Scores from different branches had to be calibrated (dropping
  calibration: 69.5 → 46.9 nDCG@10 on Biology; Fig. 5). On staleness: "Our tree
  summaries are computed once over the full corpus, so subtrees near excluded leaves
  become stale and mislead the search" (§3.2); the static tree is named as a
  limitation (Appendix A). A search-guided insertion that leaves summaries unchanged
  scored slightly higher than a static build on one dataset, which the authors say
  they "do not claim as a general phenomenon" (Appendix B.9, Table 8).
- **PageIndex.** github.com/VectifyAI/PageIndex: "replaces the vector index with a
  hierarchical tree index and lets an LLM reason its way through it"; the tree
  structure "is extracted from the document layout without an LLM", and the index
  model "only summarizes and refines it" (README). Nodes carry `start_index`,
  `end_index` and `summary` (release v0.2.21); each summary is capped at
  `summary_max_words` (default 150; v0.2.20). An option to omit node summaries
  (`--if-add-node-summary`) is reported in a search summary `[unverified]`. The
  claimed 98.7% on FinanceBench is the vendor's own, not peer-reviewed.

**What A means for loci** `[design inference]`. In loci the agent sees the leaves
(concept slugs and digests) as soon as it opens an area digest, and every sub-folder's
concepts are in that one output. (An earlier version called this RAPTOR's "collapsed
tree"; it is not: the collapsed tree is a similarity-ranked pick under a token budget,
not a full view. Research 14 has the correction.) Inner-node descriptions earn their place only where the
agent chooses *without* seeing leaves, which in loci is the root digest. Every system
above keeps those descriptions fresh by regenerating them; none relies on a person
or agent remembering to edit them.

## B. Information scent: what makes a branch label useful

- **Theory.** Pirolli & Card, "Information foraging", *Psychological Review* 106(4),
  643–675 (1999), doi 10.1037/0033-295X.106.4.643. "Information scent is the
  (imperfect) perception of the value, cost, or access path of information sources
  obtained from proximal cues, such as bibliographic citations, WWW links, or icons
  representing the sources"; it "is the proximal means by which a forager judges the
  value or relevance of distal information sources" (read in the authors' PARC UIR
  technical report of January 1999, "accepted for publication in Psychological
  Review", section "Information Diet and Scent-following", p. 10, and p. 48; the APA
  version is paywalled, so page numbers are the report's). Chi, Pirolli, Chen & Pitkow
  (CHI 2001) model scent from link text and user goals; I found no free copy and the
  ACM DL refused access `[unverified]`.
- **Label quality beats structure.** Miller & Remington, "Modeling information
  navigation: implications for information architecture", *Human-Computer
  Interaction* 19(3), 225–271 (2004) (full text read). Their model (MESA) and a study
  with 45 participants found: "the quality of link labels is a greater factor for
  navigation times than the structure of the pages"; "the averages of judged label
  ratings were able to account for 58.2% of the variance in navigation times"; they
  "advise designers to structure Web sites using the most reliable link labels,
  rather than trying to achieve pages with the 'optimal' number of links if it forces
  the use of ambiguous labels" (§6). "A top level with just a half dozen links could
  be part of an effective structure if the top level links reliably led the user to
  the next level" (§6).
- **Same result from a labelling experiment.** Resnick & Sanchez, "Effects of
  organizational scheme and labeling on task performance in product-centered and
  user-centered retail Web sites", *Human Factors* 46(1), 104–117 (2004): 60
  participants; user-centred labels gave "a significant benefit in performance and
  satisfaction" over company-centred labels, and "User-centered organization did not
  result in improved performance except when the label quality was poor" (abstract,
  via PubMed). The full text is behind SAGE's paywall and Unpaywall and Semantic
  Scholar list no open copy, so the effect sizes are `[unverified]`.
- **Named failure modes and their repairs.** The Cognitive Walkthrough for the Web
  (CWW; full texts read from Kitajima's homepage) uses LSA to estimate scent between a
  goal and each heading and link, following the CoLiDeS model: the user first attends
  to the heading region most similar to the goal, then picks the most similar link in
  it (Blackmon, Kitajima & Polson, CHI 2003, "Review of CWW"). The 2002 and 2003 papers
  name three problem types: "unfamiliar heading/link texts, confusable heading/link
  texts, and goal-specific competing heading/link texts" (CHI 2003, Introduction). The
  2005 paper names four: weak-scent link, unfamiliar, competing headings and competing
  links (Blackmon, Kitajima & Polson, CHI 2005, "Navigation Usability Problems CWW
  Detects"). Its regression over 164 tasks explains 57% of the variance in clicks:
  "Mean total clicks = 2.199 + 1.656 if the correct link is unfamiliar + 1.464 if the
  correct link has weak-scent + 0.754 times the number of competing links nested under
  competing headings" (same paper, p. 36). These three predictors are the classes
  Habuchi & Takeuchi (CogSci 2012, full text read) test; their eye-tracking study found
  that pages with these problems lowered success and raised clicks and time, with weak
  scent the worst at the heading stage (Discussion). The repairs: confusable headings
  were fixed by rewording the links under them so that each heading's distinguishing
  features stood out (CHI 2002, "Repairs"); unfamiliar labels by higher-frequency words
  ("Paleontology & Fossils"); competing headings by "making it possible for users to
  find things in more than one way, putting links to the item all the places where
  CWW predicts users are most likely to look" (CHI 2003, "CWW Repairs"). In the 2003
  experiments, full repairs halved the mean clicks, the gain was larger for competing
  headings and links than for unfamiliar labels, and repairing only the competing
  problems "captured most of the performance gains" (CHI 2003, Experiments 1–2).
- **Breadth and scent together.** Katz & Byrne, "Effects of scent and breadth on use
  of site-specific search on e-commerce Web sites", *ACM TOCHI* 10(3), 198–220
  (2003) (full text read, authors' copy): "higher scent yielded less searching, as did
  broader menus" (§3.2). "Given broad, high-scent menus, participants searched less
  than 10% of the time, but they searched almost 40% of the time when faced with
  narrow, low-scent menus", and "the breadth effect in this study turned out to be
  larger than the scent effect" (§3.3, p. 217).
- **Do descriptions help beyond labels?** Mixed:
  - "There have also been studies looking at the advantages of adding menu descriptors
    (e.g., Latremouille and Lee, 1981; Snowberry et al., 1985); these have been found
    to improve performance when applied to upper levels of the menu hierarchy only"
    (National Research Council, *Emerging Needs and Opportunities for Human Factors
    Research*, 1995, ch. 7, p. 204). The Snowberry paper is "Effects of help fields on
    navigating through hierarchical menu structures", *IJMMS* 22, 479–491; its help
    fields listed the upcoming selections ("adding items containing upcoming
    selections resulted in greater accuracy and faster search times", as summarised by
    Schlimmer & Wells, *JAIR* 5, 1996, p. 332). Elsevier blocks the 1985 text and no
    open copy exists, so the "upper levels only" detail rests on the NRC summary
    `[unverified]`.
  - Kreigh, Pesot & Halcomb, "An evaluation of look-ahead help fields on various
    types of menu hierarchies" (*IJMMS* 32(6), 649–661, 1990) gave panels "plus help
    fields containing upcoming alternatives", the same manipulation as Snowberry et
    al.; "the addition of help fields did not enhance subject performance in any
    systematic fashion" (abstract, as returned by a web search of the ScienceDirect
    page; ScienceDirect itself returned 403 and no open copy exists, so the full text
    is `[unverified]`). Schlimmer & Wells (1996, p. 332) likewise say the Snowberry
    result "has not been confirmed".
  - Cutrell & Guan, "What are you looking for? An eye-tracking study of information
    usage in web search", CHI 2007 (full text read; 18 participants; snippets of
    about one, two to three, or six to seven lines). Across both task types snippet
    length had no main effect, but the interaction did: "For navigational tasks, the
    time on task remained the same for short- and medium-length snippets but increased
    by 10 seconds for long snippets. In contrast, informational tasks showed an
    improvement in task time of 24 seconds with long snippets", and medium snippets
    already cut navigational accuracy "from 58% to 47%" (Results, "Task Type &
    Snippet Length"). Longer snippets drew gaze away from the URL, while the title's
    share of gaze "stayed roughly constant"; participants "read them, whether or not
    the snippets were relevant to their task" (same section, Figure 9). These are
    query-dependent search-result snippets, not category descriptions; the transfer
    to loci is that finding a known item resembles the navigational case.
  - Dumais & Landauer, "Using examples to describe categories", CHI 1983, pp.
    112–115, compared Yellow Pages category names, names plus examples, and examples
    alone: "three examples alone were essentially as good a way to name these
    categories as either an expertly chosen name or a name plus examples" (abstract,
    from the ACM metadata as served by OpenAlex; the ACM DL returned 403 and no open
    copy exists, so the full results are `[unverified]`). Blackmon et al. (CHI 2003)
    found the same for web headings: adding 5–10 examples under each repaired link
    made no significant difference, and "the effort required to generate examples …
    was not worth it" (Discussion of Experiment 2). Examples can stand in for a weak
    name, but added to a good name they bought nothing measurable.
- **Vague high-level labels.** No study isolates "a broad category with a vague
  description" as such `[gap]`. The nearest evidence is Miller & Remington's
  simulations, where the cost of an ambiguous label is a wrong turn followed by
  back-tracking (§3.4), and CWW's *competing headings*, the problem whose repair gave
  the largest gains (CHI 2003): the risk is less that `mathematics` is vague than that
  a concept plausibly belongs under two areas (`mathematics` and `machine-learning`
  for an eigenvalue method). CWW's own repair is a second path (a link under each
  competing heading), which a single-parent folder tree cannot give a concept; the
  nearest equivalents are a pointer from the competing area to the right one and
  links between concepts `[design inference]`.

## C. Depth versus breadth

- **Menus.** Miller (1981) compared 64 items as 64¹, 2⁶, 4³ and 8²; response time
  rose with depth (as summarised by Zaphiris 2001, §2.1, full text read; Miller's
  paper not read). Kiger (1984, *IJMMS* 20, 201–213) added 16×4 and 4×16: time and
  errors increased with depth, 4×16 was fastest with fewest errors, and users
  preferred 8² (Zaphiris 2001, §2.1). Kiger's abstract: "The results suggest
  minimizing the depth of tree structures by providing menus of eight or nine
  selections each" (abstract as shown on the ResearchGate listing, retrieved through
  a web search; Elsevier returned 403, and Unpaywall, OpenAlex and Semantic Scholar
  list no open copy or abstract, so the paper itself is `[unverified]`). Zaphiris
  (HFES 2000)
  replicated Kiger's structures with web links and found 8² fastest (Zaphiris 2001,
  §2.1).
- **Web.** Larson & Czerwinski (CHI 1998): 512 targets as 8×8×8, 16×32 and 32×16;
  16×32 was fastest and 8×8×8 slowest (Zaphiris 2001, §2.1; Miller & Remington 2004,
  §1; abstract read in research 12).
- **The interaction with labels.** Miller & Remington (2004) reconcile the two: Miller
  (1981) had under 1% selection errors (clear labels), Larson & Czerwinski reported
  frequent back-tracking (ambiguous labels) (§1). Two levels beat three when "the
  categorical link labels are sufficiently ambiguous so that the user must perform
  some backtracking", and "a three-tiered structure produces faster navigation times
  when the compared structures have clear labels" (§6). **A middle level helps when
  its labels are clear and hurts when they are not.**
- **Personal files.** Bergman, Whittaker et al. (JASIST 2010): retrieval time =
  4.956 + 2.236 × depth + 0.106 × folder size, mean depth 2.86 (details in research
  12 §3).
- **Gathering levels on one page.** Miller & Remington note that listing the second
  level under each first-level label "lifts" two levels onto one page, and people scan
  it like a two-level structure without the cost of a click (§6, citing Hornof &
  Halverson 2003). That is what a sub-folder outline on the root digest does
  `[design inference]`.

**What C means for loci** `[design inference]`. The digest already gathers all
sub-folders of an area onto one page, so loci pays the hop cost once (root, then area)
whatever the folder depth. The open question is not how deep folders go but whether
the root gives enough scent to pick the right area. That depends on the quality of
the area labels and on whether confusable areas are told apart.

## D. Library and information science

- **Scope notes (ANSI/NISO Z39.19-2005 (R2010), full text read).** Definition: "A
  note following a term explaining its coverage, specialized usage, or rules for
  assigning it" (§4.1). §6.2.2: a scope note is used "to restrict or expand
  the application of a term", "to distinguish between terms that have overlapping
  meanings in natural language", or "to provide other advice on term usage to either
  the indexer or the searcher". It "should state the chosen meaning of a term. It may
  also indicate other meanings that are recognized in natural language, but which
  have been deliberately excluded". Example: "illuminations — SN Includes both the
  ornamental decoration and the illustrations in manuscripts …". §6.2.2.1
  *Reciprocal scope notes*: when a scope note mentions another term, that term should
  generally get a reciprocal note, so "when a change is made to one of the terms, or
  it is deleted, the effect on the other term will be considered" (example: dogtrots
  and breezeways, each saying how it differs from the other).
- **SKOS (W3C Recommendation, 2009; Primer §2.4).** `skos:scopeNote` "supplies some,
  possibly partial, information about the intended meaning of a concept";
  `skos:definition` "supplies a complete explanation"; `skos:example`,
  `skos:historyNote`, `skos:editorialNote` and `skos:changeNote` are separate. A scope
  note is the partial, usage-oriented kind: what to file here, not a full account.
- **ISO 25964-1:2011** is paywalled. The only free copy of the official text is the
  preview sample served by the reseller iTeh Standards, which stops at term 2.47. It
  confirms that the standard
  has §5.2 "Scope notes" (p. 20) and §5.3 "Reciprocal scope notes" (p. 21) (table of
  contents), and defines a node label as a "label inserted into a hierarchical or
  classified display to show how the terms have been arranged", which "is neither a
  preferred term nor a non-preferred term" (2.38). The scope-note definition (2.50) and
  §5.2 are outside the preview, so I use SKOS (above) as the free substitute
  `[unverified]` for ISO's own wording.
- **Dewey (OCLC, *Introduction to the Dewey Decimal Classification*, full text
  read).** Notes that describe a class (§7.10–7.15): *definition notes* (meaning of
  the heading), *scope notes* ("whether the meaning of the number is narrower or
  broader than is apparent from the heading"), *class-here notes* ("major topics in
  a class … considered to approximate the whole of the class"), *variant-name
  notes*. *Including notes* (§7.18) list minor topics with "standing room", which may
  later get their own number. *Class-elsewhere notes* (§7.20) "lead the classifier to
  interrelated topics, or distinguish among numbers in the same notational
  hierarchy". **Hierarchical force** (§4.18): "whatever is true of the whole is true
  of the parts", so "hierarchical notes are usually given only once—at the highest
  level of application". Notes of types A and C have hierarchical force; including
  notes do not (§7.9).
- **Dewey summaries (§4.2, §7.1).** A summary lists a class's subdivisions by number
  and heading. Two-level summaries are given for classes whose subdivisions extend
  beyond 40 pages, single-level summaries for 4 to 40 pages; none for smaller
  classes. Summary headings "have been edited for browsing purposes". "The summaries
  are a good means of mental navigation" (§5.5). This is a table of contents derived
  from the children, shown only where a class is big.
- **Node labels (Z39.19 §7.7, §8.3.5).** A node label is "a 'dummy' term … not assigned
  to documents when indexing … to indicate the logical basis on which a class has been
  divided" (§4.1). "A compound term should not be created solely for the reason that it
  forms a logical level in a hierarchy … node labels should be used instead" (§7.7).
  Node labels "are not terms, and must not be used as indexing terms" (§8.3.5). The
  standard gives scope notes to terms, not to node labels. A loci sub-folder that only
  groups concepts plays this role.
- **Polyhierarchy (Z39.19 §8.3.4).** "Some concepts belong, on logical grounds, to more
  than one category" (example: pianos under both stringed and percussion
  instruments). A folder tree cannot express this; Dewey handles it with
  class-elsewhere notes that say where a shared topic goes.
- **Facets (Z39.19 §5.3.4).** Facet analysis, "based on the work of Ranganathan",
  takes a bottom-up approach and suits "interdisciplinary areas where there is more
  than one perspective" and "vocabularies where multiple hierarchies are required".
  I did not read Ranganathan. In loci, links, `prereqs` and views already provide the
  cross-cutting paths; the folder tree only has to say where a note lives.
- **UDC** (UDC Summary, about 2,600 classes from the UDC Master Reference File, read
  through the UDC Consortium's TemaTres vocabulary server and its API,
  https://vocabularyserver.com/udc/en/). It has four note types: scope note (177 in the
  Summary), application note (142), including note (639) and example (273) (server's
  "About" page). So most classes carry no note at all. The notes support the Dewey
  points and contradict none of them:
  - Scope notes say what is classed here and send neighbouring aspects elsewhere.
    519.8 *Operational research (OR): mathematical theories and methods*: "Class here
    mathematical theories and methods for operational research. For operational
    research as a management subject in general, see 005.31. For operational research
    as methods in social sciences see 303". This is the `power-iteration` case: the
    mathematical treatment stays in mathematics, the applied one goes to the applied
    class.
  - Boundaries are written on both sides. 532 *Fluid mechanics in general*: "Mechanics
    of gases alone is classed at 533"; 533 *Mechanics of gases*: "Class information
    about both liquids and gases in 532".
  - Including notes list minor topics without their own number, for example 510.3
    *Set theory*: "Naive set theory. Axiomatic set theory. Set-theoretic
    constructions. …".
  - Notes apply downward from the class that carries them: 64 *Home economics*: "The
    62-1/-9 auxiliaries are applicable throughout 64"; 37 *Education*: "Throughout the class the word
    'school' is used in the sense of any educational institution".

**What LIS says a category description should contain**: (1) the chosen meaning or
coverage; (2) the major topics that make up the class (Dewey class-here) and, where
useful, minor topics that would not be guessed (including); (3) what is excluded and
where it goes instead, with a reciprocal note on the other class (Z39.19 §6.2.2,
§6.2.2.1; Dewey class-elsewhere; UDC "for … see …" scope notes). It is written once,
at the highest level it applies to, and most classes need none. Grouping levels below
get only a label. A list of subdivisions is derived and shown only for large classes.

## E. Generated versus authored indexes

- **Generated structure does not drift.** Dewey summaries are derived from the
  schedule (with edited headings); Docusaurus's `generated-index` "displays all the
  direct children of this category" (research 12); every LLM system in §A
  regenerates its inner-node text. LATTICE shows what happens when they are not
  regenerated: stale summaries "mislead the search" (§3.2).
- **Authored text: better scent, but it drifts.** No study compares an authored and a
  generated table of contents for the same hierarchy `[gap]`. Related evidence:
  - Gloaguen, Mündler-Sasahara, Müller, Raychev & Vechev, "Evaluating AGENTS.md: Are
    Repository-Level Context Files Helpful for Coding Agents?", arXiv 2602.11988
    (2026). Context files "do not generally improve task success rates" and raise
    inference cost by "over 20%" (abstract). Developer-written files did significantly
    better than LLM-generated ones (p = 3.8%; §4.2). Repository overviews were "not
    effective": agents took no fewer steps before touching the files the fix needed
    (§4.3). They recommend omitting LLM-generated context files and keeping
    human-written ones to "specific additional instructions beyond what is already
    available" (§1, §6).
  - Lulla, Mohsenimofidi, Galster, Zhang, Baltes & Treude, "On the Impact of AGENTS.md
    Files on the Efficiency of AI Coding Agents", arXiv 2601.20404 (2026): with
    AGENTS.md, median runtime fell 28.64% and output tokens 16.58%, with "comparable
    task completion behavior" (abstract). The two studies measure different costs.
  - Treude & Baltes, "Context Rot in AI-Assisted Software Development", arXiv
    2606.09090 (2026): an existing documentation consistency checker found "stale code
    element references in 23.0% of repositories" in AI configuration files (abstract).
  - Chakrabarti, "Why Does CLAUDE.md Keep Growing?", arXiv 2608.11095 (2026): across
    247,694 instruction lifetimes, agent instruction files "grow without bound" (+226%),
    and older instructions are less likely to be deleted (abstract).
  - Code comments, the classic case of authored descriptions next to the thing they
    describe, mostly do not change when the code changes. Wen, Nagy, Bavota & Lanza,
    "A Large-Scale Empirical Study on Code-Comment Inconsistencies", ICPC 2019 (full
    text read), mined 1.3 billion AST-level changes in 1,500 systems: "most of the code
    change categories rarely trigger changes in the related method and class
    comments"; "13% to 20% of code changes trigger a comment change" (§IV.A). Their
    examples include a rename that left a comment naming an identifier that no longer
    existed, fixed 20 days later (§IV.A). Not every unchanged comment is stale, since
    many code changes need no comment change. Fluri, Würsch & Gall, "Do code and
    comments co-evolve?", WCRE 2007 (full text read, ZORA copy), found that "newly
    added code … barely gets commented" (abstract) and that only 23%, 52% and 43% of
    comment changes in three projects were triggered by code changes (§3.3); but when
    a comment did change with its code, it did so in the same revision about 97% of
    the time, which surprised them: "We did not expect that developers are that
    disciplined" (§3.3, §3.5). So drift comes from comments that are never touched, not from late
    updates.
- **LLM-maintained memories.** Systems that let the LLM rewrite summaries on each
  write (MemTree §3.1; A-MEM, Xu et al., NeurIPS 2025, arXiv 2502.12110: new memories
  "trigger updates to the contextual representations and attributes of existing
  historical memories") stay fresh at the cost of a rewrite per write, and recursive
  summaries carry errors: about 4% of RAPTOR summaries had minor hallucinations (§3).
  BooookScore (Chang, Lo, Goyal & Iyyer, ICLR 2024, arXiv 2310.00785) found that
  updating one summary incrementally keeps more detail but is less coherent than
  hierarchical merging (abstract). LongMemEval (Wu et al., ICLR 2025, arXiv
  2410.10813) tests knowledge updates as one of five memory abilities and reports a
  30% accuracy drop for commercial assistants over sustained interactions (abstract);
  it does not isolate summary staleness. I found no measurement of how fast an
  agent-maintained *folder description* goes stale `[gap]`.

**What E means for loci** `[design inference]`. loci already prefers generated views:
the index "is never stored in the vault" (m0 §3) and `loci digest` builds it on
demand. Anything derivable from the tree (sub-folder names, counts, the concepts in
them) should be generated. Authored text should hold only what the tree cannot say:
the boundary of the area and where neighbouring topics go. That is also what
Gloaguen et al. recommend for context files: only what is not already available.

## F. LLM agents: orientation files and descriptions

- **The description is what the agent selects on.** Anthropic's skills guide: only
  `name` and `description` are preloaded; the description "should include both what
  the Skill does and when to use it"; "Be specific and include key terms"; avoid
  "Helps with documents"; it is "critical for skill selection: Claude uses it to choose
  the right Skill from potentially 100+ available Skills"
  (https://platform.claude.com/docs/en/agents-and-tools/agent-skills/best-practices).
  Its table-of-contents example lists each child with key terms: "**Finance**:
  Revenue, ARR, billing → See reference/finance.md". That is a label plus "class here"
  terms, as in a Scatter/Gather digest.
- **Description quality changes selection.** Hasan, Li, Rajbahadur, Adams & Hassan,
  "MCP Tool Descriptions Are Smelly!", arXiv 2602.14878 (2026): 97.1% of 856 tool
  descriptions had at least one defect; fully augmented descriptions raised task
  success by a median 5.85 points, raised execution steps by 67.46%, and regressed in
  16.67% of cases; "compact variants … often preserve behavioral reliability"
  (abstract). Wang et al., "From Docs to Descriptions", arXiv 2602.18914 (2026):
  description defects "significantly affect LLM tool selection"; functionality and
  accuracy had the largest effects (+11.6 and +8.8 points) (abstract). So a short,
  accurate statement of what an option covers matters, and more text is not free.
- **Flat indexes, one level of reference.** llms.txt: an H1 (the only required
  section), a blockquote with "key information necessary for understanding the rest
  of the file", then flat H2 lists of `[name](url): notes` (https://llmstxt.org/).
  Skills: "Keep references one level deep from `SKILL.md`" (guide above). Research
  12 covers Claude Code's per-directory `CLAUDE.md` and AGENTS.md; both scope
  *instructions*, not content descriptions.
- **Hierarchy for agents.** AnyTool (Du, Wei & Zhang, arXiv 2402.04253, 2024) splits
  a large API collection into category, tool and API agents; removing the hierarchy dropped the
  pass rate from 58.9% to 22.4% on the G2-I subset (Table 3). The meta-agent starts from category names
  and asks for tool descriptions on demand (bootstrap prompt). This splits the work
  between agents; it says little about descriptions.
- **Uniform rule vs fixed level.** No study compares letting any folder carry a
  description with describing one fixed level `[gap]`. The practice in research 12
  (folder notes, MkDocs, Docusaurus, Dendron, Notion) is "optional at any level"; the
  only *required* descriptions sit where a reader chooses among siblings without
  seeing their contents (skill `description`, JDex entry, llms.txt H1).

## Evaluation of the options

A neutral example vault for all options: areas `mathematics` (140 concepts, with
sub-folders `analysis`, `geometry`, `linear-algebra`, `linear-algebra/numerical`,
`probability`), `statistics` and `machine-learning`. The test case is filing a new
concept, `power-iteration` (an eigenvalue algorithm), which could go under
`mathematics/linear-algebra/numerical` or `machine-learning`.

### Option 1. Status quo: `AREA.md` at the top level only

- **File:** `concepts/<area>/AREA.md`: H1 title; one paragraph, ≤600 characters,
  content unspecified beyond "saying what belongs there" (concepts skill).
- **Digest:** root shows title, path, count, paragraph. Area digest unchanged.
- **Lint, `mv`:** as today (`area.missing`, `area.format`, `area.too-long`; `mv`
  carries `AREA.md` with the folder).
- **Drift:** the paragraph is free prose; nothing checks it against the folder.
- **For:** required descriptions where the agent chooses without seeing leaves
  (skills, JDex; research 12). Dewey gives notes "once—at the highest level".
- **Against:** the user's problem. "What belongs there" invites a summary, and a
  summary of a broad area is vague: the "Helps with documents" failure in the skills
  guide. The root shows nothing below the area, so the middle levels are invisible
  when choosing where to file. Nothing tells `mathematics` from `machine-learning`
  for `power-iteration` (CWW's competing headings).

### Option 2. Uniform rule: any concept folder may have a description file

- **File:** `concepts/<area>/**/AREA.md` becomes kind A at any depth. Required at the
  top level, optional below. Same format: H1 and one paragraph.
- **Digest:** area digest prints the paragraph under each `## <dir>` heading. The root
  cannot afford sub-folder paragraphs (10 areas × 5 sub-folders × 600 characters is
  30,000 against `root_index` = 8000), so it would show at most sub-folder titles.
- **Lint:** `area.missing` stays top-level only; `area.format` and `area.too-long`
  apply at every depth (one budget, or a new `budgets.subarea_paragraph`). The rule
  that a sub-folder `AREA.md` is a concept failing `slug.format` goes away.
- **`mv`:** a folder move carries its description. Merging two folders that both have
  one is refused by the existing `mv.exists`. The precise message for "destination
  would be an area's `AREA.md`" (D145, D155) must cover every depth. Promoting a
  sub-folder to an area keeps its file, which becomes required. Previously a
  sub-folder `AREA.md` was a lint error, so no valid vault changes meaning
  `[design inference]`.
- **Drift:** high for optional files nobody is prompted to revisit. No check ties a
  sub-folder paragraph to its concepts.
- **For:** one rule instead of a top-level special case; it matches folder-note
  plugins, Notion and Docusaurus (research 12). An authored sub-folder paragraph has
  better scent than a folder name (Miller & Remington; Resnick & Sanchez).
- **Against:** the text lands where it is least needed. In the area digest the agent
  already sees every concept's digest, so a sub-folder paragraph sits next to the
  leaves it summarises. At the root, where it would
  help, the budget leaves room only for titles, which folder names already give.
  Help fields helped at upper levels only (Snowberry et al. via the NRC), the same
  look-ahead fields gave no systematic benefit in a later study (Kreigh et al.), and
  longer search-result snippets hurt navigational search (Cutrell & Guan). Z39.19 and
  ISO 25964-1 (2.38) give grouping levels node labels, not scope notes. A new kind of optional file is also more surface for the agent to maintain.

### Option 3. `AREA.md` as an authored table of contents

- **File:** H1; paragraph; then a list with one line per sub-folder:
  `- linear-algebra: vector spaces, matrices, eigenvalues and decompositions`.
- **Digest:** root prints the list under the paragraph; the area digest could repeat
  each line under its `## <dir>` heading.
- **Lint:** `area.format` must accept the list. A new rule (say `area.toc`) must check
  that the list names exactly the existing sub-folders. Folders are not link targets
  (m0 §5.3 resolves files), so the existing link check cannot do it.
- **`mv`:** renaming or moving a sub-folder leaves a stale line in the parent's
  `AREA.md`, and in two parents when moving across areas. `mv` edits only links (m0
  §9.5 step 2), so it would need new editing logic, or else accept that a move
  produces lint errors.
- **Drift:** the highest. Names are checkable only by the new rule; the one-line
  digests are prose about other files.
- **For:** good scent, of the kind with evidence behind it: label plus key terms
  (skills TOC example; Scatter/Gather topical words; Dewey summaries and class-here
  notes). One file per area keeps everything the root shows in one place.
  Human-written beat LLM-generated context (Gloaguen et al.).
- **Against:** it duplicates the tree in prose, which is the drift pattern of §E
  (LATTICE §3.2; Treude & Baltes; code comments that are never touched, Wen et al.).
  The parent holds facts about its children, so moves touch a file outside the moved
  folder. Dewey shows summaries only for large classes, derived from the schedule.
  Gloaguen et al. found that overviews did not help agents find files. Key terms added
  to an already clear label bought nothing measurable in the two studies that tested
  it (Blackmon et al. 2003, examples under links; Dumais & Landauer 1983, examples
  with a name).

### Option 4. Generated table of contents; authored text limited to scope notes

- **File:** `AREA.md` unchanged in format (H1, one paragraph). The paragraph is
  rewritten as a scope note (see option 5 for the fields). Nothing new on disk.
- **Digest:** root adds one generated line per area listing the sub-folder paths
  that hold concepts, with counts, in the same order as the area digest's groups:

  ```markdown
  ## Mathematics (concepts/mathematics, 140 concepts)

  <paragraph>

  Sub-folders: analysis (31), geometry (12), linear-algebra (24), linear-algebra/numerical (9), probability (18)
  ```

  `digest --json` root entries gain `"groups": [{"dir", "concepts"}]`. The area digest
  is unchanged.
- **Lint, `mv`:** no new rule; nothing to rewrite on `mv`. `index.over-budget` on the
  root covers the extra length.
- **Drift:** none for the outline.
- **For:** a derived table of contents, as in Dewey summaries and Docusaurus's
  generated index; a Scatter/Gather-style digest of children (82% correct cluster
  choice). It lifts the middle level onto the root page, which Miller & Remington
  describe as scanning two levels without a click. Sub-folders stay node labels
  (Z39.19 §8.3.5). Research 12 already proposed it as optional.
- **Against:** a folder name is only a label. `misc` or `basics` gives no scent (CWW
  "unfamiliar" or weak scent), and nothing in the outline distinguishes confusable
  areas. Root budget: with 600-character paragraphs and an outline line of about 100
  characters, about 10 areas fit in 8000 characters (arithmetic, not measured).

### Option 5 (recommended). Scope-note `AREA.md` plus generated outline

Option 4's generated outline, plus a defined content for the one authored paragraph,
based on Z39.19 §6.2.2 and Dewey §7.10–7.20, with boundaries written as checked links.

- **File:** `concepts/<area>/AREA.md`, format unchanged (H1, exactly one paragraph,
  ≤ `area_paragraph`). The paragraph has three parts, in this order:
  1. **Scope** (Dewey scope/definition note, SKOS `scopeNote`): what kinds of
     concept the area holds, in one sentence. Not a summary of its contents.
  2. **Key topics without a sub-folder** (Dewey class-here and including; UDC
     including notes; skills "key terms"; Scatter/Gather topical words): the search
     terms an agent would use that the outline does not show. Leave out topics that
     already have a sub-folder; the outline lists them. This part has the weakest
     support: examples added to a clear label bought nothing measurable (Blackmon et
     al. 2003; Dumais & Landauer 1983), so keep it to a few terms that the area name
     would not suggest, or leave it out.
  3. **Boundaries** (Z39.19 "deliberately excluded" meanings; Dewey class-elsewhere;
     UDC "for … see …" scope notes): topics that look as if they belong but go
     elsewhere, each with a path link to the sibling area, for example
     `[[concepts/machine-learning/AREA]]`. Path links resolve (m0 §5.3),
     `link.dangling` catches a renamed sibling, and `mv` rewrites them (m0 §9.5). The
     sibling should carry the reciprocal boundary (Z39.19 §6.2.2.1; UDC 532/533).

  Example:

  ```markdown
  # Mathematics

  Definitions, theorems and proof techniques, and the numerical methods that
  compute them. Also number theory, combinatorics and logic. Statistical inference
  and experiment design go in [[concepts/statistics/AREA]]; learning algorithms go
  in [[concepts/machine-learning/AREA]] even when their core is linear algebra.
  ```

  This settles `power-iteration`: it is a numerical method, so it goes under
  `mathematics/linear-algebra/numerical` unless it is filed as a learning algorithm.
  The reciprocal sentence in `machine-learning/AREA.md` says the same.
- **What it must not contain:** a list of sub-folders (generated); a summary of the
  concepts (their digests do that); history or editorial notes (git has them).
- **Digest:** as in option 4. The paragraph is printed verbatim, as today.
- **Lint:** no new rule is needed. `link.dangling` already checks the boundary links.
  A rule requiring at least one boundary link once a vault has two or more areas is
  possible, but I found no evidence that it is worth the noise; leave it to the
  `concepts` skill and to measurement.
- **`mv`:** nothing new. Moving or renaming an area rewrites the boundary links in
  the other areas' paragraphs. The outline is regenerated.
- **Drift:** outline none. Boundary links are checked for existence, not meaning.
  Scope and key-topic sentences can still go stale; they are short and on the page
  the agent reads at every session start.
- **Sub-folders:** no description file (D78 and research 12 stand). If a sub-folder
  needs its own scope note, the evidence says it is acting as a class rather than a
  node label: promote it to an area with `loci mv` (research 12 §4(b)3). Z39.19
  §7.7 says the same about compound terms created only to form a level.
- **For:** each part has a basis. Authored text covers what only a person or agent can
  say: boundaries between confusable areas. Competing headings are the costliest
  label problem in CWW (CHI 2003, 2005); CWW repairs them with a second path, which a
  folder tree cannot give, so the boundary link (a pointer from the competing area to
  the right one) is the nearest equivalent `[design inference]`. Library practice
  writes exactly such notes (Z39.19 "distinguish between terms that have overlapping
  meanings"; Dewey class-elsewhere; UDC 519.8 "For operational research as a
  management subject in general, see 005.31"). The middle level is shown, generated
  and never stale (Dewey summaries; Scatter/Gather). Notes are given once at the
  highest level (Dewey hierarchical force; UDC "throughout"). Text stays short
  (Cutrell & Guan: long snippets slowed navigational search; MCP "compact
  variants"). It needs no new file kind or lint rule and changes no `mv`
  behaviour. It answers the user's concern 1: a broad area's paragraph no longer has to
  summarise everything; it states edges and the outline shows the inside. It answers
  concern 2: the file stays one per area, and its content is specified.
- **Against:** the outline is still labels only (see option 4). The three-part shape
  is guidance, not checked. The root budget tightens. No study tests this exact
  combination for an LLM agent `[gap]`.

### Open issue for broad areas (not part of the recommendation)

A broad area runs into `area_index` (24000 characters, about 120 concepts; research
12 §3). Research 12 said to split the area, but splitting is what loses the middle
level the user wants. If broad areas are the goal, one option is to let `--area` take
a sub-path (`loci digest --area mathematics/linear-algebra`) as a *filter* used only
when the area digest is over budget. Unlike a mandatory third level, this adds a hop
only when needed. MemWalker (every step can go wrong) argues against making it the
default; RAPTOR's collapsed-tree result is about ranked retrieval and does not bear on
it (research 14). Decide this separately and measure it.

## Recommendation

1. **Adopt option 5.** Specify the `AREA.md` paragraph as a scope note (scope; key
   topics without a sub-folder; boundaries with path links to sibling areas, written
   on both sides). Put this in the `concepts` skill and `share/loci/rules.md`, and in
   m0 §6.3 as guidance (the format rule is unchanged). The verification pass did not
   change this, but it shifted the weight: the boundary part is better supported than
   before (CWW: competing headings cost the most; UDC writes the same "for … see …"
   notes), and the key-topics part is weaker (adding examples to a clear label did not
   help), so the guidance should present key topics as optional and short.
2. **Add the generated sub-folder outline to the root digest** (m0 §10; `digest --json`
   root `groups`). Reconsider `root_index` or `area_paragraph`: a scope note may fit in
   about 400 characters, which pays for the outline line `[design inference]`.
3. **Keep sub-folders without descriptions.** Promote a sub-folder to an area when it
   needs its own scope note.
4. **Fallback:** if measurement shows that folder names give too little scent when
   filing, adopt option 2 with the description shown only in the area digest, and
   note that the evidence suggests it adds little there. Do not adopt option 3.

## What to measure

The evidence on LLM agents navigating a personal note index is thin `[gap]`.
Before changing the spec, build a benchmark on a sample vault with 8–12 areas, at
least one broad area with sub-folders, and at least two deliberately confusable
areas:

- **Retrieval cases** (existing `bench/retrieval.jsonl` format): questions whose
  answer is a concept in a sub-folder of a broad area, and in confusable areas.
  `expect` lists the slug.
- **Filing cases**: "Where would you file a new concept about X? Reply with the
  folder path." `expect` holds the folder path. `mentions` in `bench.rs` matches a
  path entry when the agent prints it not followed by `/`, so a bare folder path
  works at the agent level. Tool-level `find` cannot score folders.
- **Variants**, same notes, only the area metadata changes: (1) status quo; (2)
  scope-note paragraphs only; (3) scope notes plus outline (option 5); (4) option 5
  plus optional sub-folder descriptions in the area digest (option 2); (5) outline
  with summary-style paragraphs (option 4 without scope notes).
- **Report:** pass rate and median wall time from `loci bench --agent`. Also record
  root and area digest sizes. bench does not read token or tool counts (m1-m4 §3), so
  record them from the agent harness if possible.
- **Decision rule** (to fix before running): adopt option 5 if it beats the status quo
  on filing in confusable areas and does not lose on retrieval; adopt option 2's
  sub-folder descriptions only if variant 4 beats variant 3 by more than run-to-run
  noise (repeat each variant several times).

## Sources

Hierarchical summaries and retrieval:
- Cutting, Karger, Pedersen, Tukey (1992), Scatter/Gather, SIGIR: https://doi.org/10.1145/133160.133214 (not read; described by Hearst & Pedersen and by Pirolli & Card)
- Hearst & Pedersen (1996), Reexamining the cluster hypothesis, SIGIR: https://people.ischool.berkeley.edu/~hearst/papers/sg-sigir96/sigir96.html
- Pirolli, Schank, Hearst, Diehl (1996), CHI: https://people.ischool.berkeley.edu/~hearst/papers/chi1996.pdf (image scan with no text layer; abstract only)
- RAPTOR, Sarthi et al., ICLR 2024: https://arxiv.org/abs/2401.18059 ; https://openreview.net/forum?id=GN921JHCRw
- GraphRAG, Edge et al. (2024): https://arxiv.org/abs/2404.16130
- MemWalker, Chen et al. (2023): https://arxiv.org/abs/2310.05029
- MemTree, Rezazadeh et al. (2024): https://arxiv.org/abs/2410.14052
- LATTICE, Gupta et al. (2025): https://arxiv.org/abs/2510.13217
- PageIndex README and releases: https://github.com/VectifyAI/PageIndex ; https://github.com/VectifyAI/PageIndex/releases
- BooookScore, Chang et al., ICLR 2024: https://arxiv.org/abs/2310.00785
- A-MEM, Xu et al., NeurIPS 2025: https://arxiv.org/abs/2502.12110
- LongMemEval, Wu et al., ICLR 2025: https://arxiv.org/abs/2410.10813

Information scent, labels, depth and breadth:
- Pirolli & Card (1999), Psychological Review 106(4): https://doi.org/10.1037/0033-295X.106.4.643 (APA paywalled); read as the PARC UIR technical report, Jan. 1999: https://web.archive.org/web/20060319000003/http://www2.parc.com/istl/projects/uir/pubs/items/UIR-1999-05-Pirolli-Report-InfoForaging.pdf
- Chi, Pirolli, Chen, Pitkow (2001), CHI: not read (no free copy found)
- Miller & Remington (2004), HCI 19(3): https://facsrv.cs.depaul.edu/~cmiller/hci460/materials/hci2004.pdf
- Resnick & Sanchez (2004), Human Factors 46(1): https://doi.org/10.1518/hfes.46.1.104.30390 (abstract via PubMed: https://pubmed.ncbi.nlm.nih.gov/15151158/ ; full text paywalled)
- Habuchi & Takeuchi (2012), CogSci: https://escholarship.org/uc/item/2q77v9hc
- Blackmon, Polson, Kitajima, Lewis (2002), CWW, CHI: https://doi.org/10.1145/503376.503459 ; full text: http://oberon.nagaokaut.ac.jp/ktjm/English/PAPERS(E)/CHI2002.pdf
- Blackmon, Kitajima, Polson (2003), repairs, CHI: https://doi.org/10.1145/642611.642698 ; full text: http://oberon.nagaokaut.ac.jp/ktjm/English/PAPERS(E)/CHI2003BlackmonEtAl.pdf
- Blackmon, Kitajima, Polson (2005), severity and repairs, CHI: https://doi.org/10.1145/1054972.1054978 ; full text: http://oberon.nagaokaut.ac.jp/ktjm/English/PAPERS(E)/CHI2005.pdf
- Katz & Byrne (2003), TOCHI 10(3): https://doi.org/10.1145/937549.937551 ; full text: http://chil.rice.edu/research/pdf/KatzByrne03.pdf
- Cutrell & Guan (2007), CHI: https://www.microsoft.com/en-us/research/wp-content/uploads/2016/02/Whatareyoulookingfor-cutrell-chi07.pdf (full text read)
- Dumais & Landauer (1983), CHI: https://doi.org/10.1145/800045.801592 (abstract only, via OpenAlex: https://api.openalex.org/works/https://doi.org/10.1145/800045.801592)
- National Research Council (1995), ch. 7, p. 204: https://www.nationalacademies.org/read/4940/chapter/11
- Snowberry, Parkinson, Sisson (1985), IJMMS 22: https://doi.org/10.1016/S0020-7373(85)80052-3 (not read; paywalled)
- Schlimmer & Wells (1996), JAIR 5, p. 332: https://www.jair.org/index.php/jair/article/download/10179/24156
- Kreigh, Pesot, Halcomb (1990), IJMMS 32(6): https://doi.org/10.1016/S0020-7373(05)80105-1 (abstract via web search; publisher returned 403)
- Kiger (1984), IJMMS 20: https://doi.org/10.1016/S0020-7373(84)80018-8 (abstract via ResearchGate listing; paywalled)
- Zaphiris (2001), Age differences and the depth-breadth tradeoff: https://ktisis.cut.ac.cy/server/api/core/bitstreams/e7f2092f-b90e-41b4-b946-d853b5f1c53d/content
- Bergman et al. (2010) and Larson & Czerwinski (1998): see research 12

Library and information science:
- ANSI/NISO Z39.19-2005 (R2010): https://groups.niso.org/higherlogic/ws/public/download/12591/z39-19-2005r2010.pdf
- SKOS Primer §2.4: https://www.w3.org/TR/skos-primer/ ; SKOS Reference: https://www.w3.org/TR/skos-reference/
- OCLC, Introduction to the Dewey Decimal Classification: https://www.oclc.org/content/dam/oclc/dewey/versions/print/intro.pdf
- ISO 25964-1:2011: paywalled; preview sample from the reseller iTeh Standards (to 2.47): https://cdn.standards.iteh.ai/samples/53657/808a87d0fbea484bb0013de6a2eea4a9/ISO-25964-1-2011.pdf
- UDC Summary (UDC Consortium) on TemaTres: https://vocabularyserver.com/udc/en/ ; API: https://vocabularyserver.com/udc/en/services.php (e.g. task=fetchNotes&arg=1188 for 519.8)

Agents, context files, descriptions:
- Agent Skills best practices: https://platform.claude.com/docs/en/agents-and-tools/agent-skills/best-practices
- llms.txt: https://llmstxt.org/
- Gloaguen et al. (2026): https://arxiv.org/abs/2602.11988
- Lulla et al. (2026): https://arxiv.org/abs/2601.20404
- Treude & Baltes (2026): https://arxiv.org/abs/2606.09090
- Fluri, Würsch, Gall (2007), WCRE: https://doi.org/10.1109/WCRE.2007.21 ; full text (ZORA): https://www.zora.uzh.ch/id/eprint/72263/1/20121211115644_merlin-id_2530.pdf
- Wen, Nagy, Bavota, Lanza (2019), ICPC: https://doi.org/10.1109/ICPC.2019.00019 ; full text: https://www.inf.usi.ch/lanza/Downloads/Wen2019a.pdf (host returns 403; read via the Internet Archive copy of 2024-04-19)
- Chakrabarti (2026): https://arxiv.org/abs/2608.11095
- Hasan et al. (2026): https://arxiv.org/abs/2602.14878
- Wang et al. (2026): https://arxiv.org/abs/2602.18914
- AnyTool, Du et al. (2024): https://arxiv.org/abs/2402.04253
