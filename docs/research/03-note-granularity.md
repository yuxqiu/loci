# Note granularity: how big should a concept note be?

Compiled 2026-10-30. Builds on `01-literature-review.md` and `02-deep-dive.md` and does not repeat them. Question: in a vault where the agent writes and maintains concept notes and the human reviews diffs, what is the unit of a note, and which size and structure signals can `loci lint` measure to trigger a split or a merge?

Method: primary sources (archive papers, guideline pages, papers, source code) were read via the URLs listed. Anything I could not confirm against a primary source is marked **[unverified]**. Word-to-token conversions assume about 1.3 tokens per English word. That is a rule of thumb, not a measurement, and LaTeX-heavy notes will run higher **[unverified for this vault]**.

---

## 1. Human practice

**Luhmann (Schmidt's archive work).** The card index has about 90,000 handwritten cards "in A-6 format", in two collections: ZK I has about 23,000 cards in 108 sections, and ZK II has about 67,000 cards (Schmidt 2018, p. 2). Luhmann used thin paper, often cut from used A4 sheets, and wrote mostly on the front (Niklas Luhmann-Archiv). Three facts matter for sizing:
- **A card was a physical limit, not a semantic one.** An idea that grew was continued on inserted cards ("a sequence 1 – 1a – 1b of mono-thematic cards was also possible", Schmidt 2018, n. 12). The unit of thought was often a short *chain* of cards, not one card.
- **Growth happened by inward branching.** When a side aspect became interesting, Luhmann inserted new cards at that point, and the branch could become "their own subsection" (Schmidt 2018). This is the same as promoting a sub-idea to its own note once it has grown.
- **Hub cards existed.** At the start of a section a card could list "up to 25 references" to related cards (Schmidt 2018). That is a structure note, or a MOC.

**Ahrens.** "Write exactly one note for each idea … as precise, clear and brief as possible". He also advises treating a digital note "as if the space were limited" (*How to Take Smart Notes*, 2017; quoted via Pollock's excerpts and Goodreads, so **secondary**). The constraint is one idea per note. There is no word count.

**Matuschak.** Evergreen notes should be "only about one thing" and "capture the entirety of that thing". If notes are too broad, "links to that note will be muddied". If they are too fragmented, "you'll also fragment your link network". He concludes there is "no clear litmus test … just a bunch of tradeoffs" ("Evergreen notes should be atomic"). Concept orientation is the part that matters for migration: note-per-book leaves "a scattered set of notes on the concept, perhaps referring to it by different names" ("…should be concept-oriented"). That is exactly what happens to lecture notes.

**Zettelkasten.de on over-atomization.** Atomicity is "a processing direction … not a rigid law". Short is not the same as atomic, since atomic notes "come in different sizes". The guide also advises against splitting a problem from its solution just to be orthodox: "usability … is obviously more important than orthodoxy" (Fast, *The Complete Guide to Atomic Note-Taking*, n.d.). Beyond this, the Obsidian/MOC community's complaints about over-atomization (thousands of stub notes, link soup) are practitioner anecdote. I found no study **[gap]**.

**Wikipedia.** There are three relevant guidelines.
- *Size* (WP:SIZERULE), measured in readable prose, which excludes lists, tables and markup: >15,000 words "almost certainly should be divided"; >9,000 "probably"; >8,000 "may need"; <6,000 "length alone does not justify division"; <150 words for over two months, "consider merging".
- *Summary style* (WP:SUMMARY): spun-off sections leave a summary plus a `{{Main}}` link in the parent. The child's lead is the basis for that summary. Don't split if "only a few sentences could be written". Use `{{Sync}}` when parent and child drift apart.
- *Notability, standalone pages* (WP:PAGEDECIDE): even a notable topic "at times … is better to cover … as part of a larger page". The deciding questions are whether the topic needs the surrounding context to be understood and whether related topics are better grouped.

**Reference works.**
- The **Stanford Encyclopedia of Philosophy** now says entries "average 12,000 words". It encourages "nested" supplementary documents for detail that is unlikely to get its own entry. In 1999 the range was 2,000–6,000 words (SEP guidelines, current and archived). SEP entries are *topic* essays, much broader than one concept.
- The **nLab** rule is "only one page on a given concept, regardless of variations in grammar". Variant names become `[[!redirects …]]`. A new page must be "linked to from at least one relevant other page". Duplicates are merged and the loser's name redirects (nLab HowTo). nLab does not give a split rule.
- **Mathlib's** `longFile` linter warns above 1,500 lines, calling it "a soft upper bound" (`Mathlib/Tactic/Linter/Style.lean`). It is a precedent for a *soft, lint-enforced* size budget that a reviewer can override.

**Takeaway.** Every tradition agrees on *one concept per page, one canonical home, aliases for variant names*. None of them derives a word count from first principles. Where numbers exist (Wikipedia, SEP, mathlib), they are soft thresholds tuned for a reader or reviewer, and they come paired with a summary-plus-link convention for splits.

## 2. Learning science

- **Element interactivity** (Sweller 2010). Intrinsic load comes from elements that must be processed *together*. Splitting interacting elements apart does not remove the interaction. It moves the load to the reader, who has to reassemble them. A data structure, its invariant argument and the assumption it relies on interact, so they belong in one note.
- **Isolated-then-interacting** (Pollock, Chandler & Sweller 2002). For novices, presenting isolated elements first and the full interacting version second beat presenting the full version twice. The benefit was expected to vanish for experts. This supports *ordering within* a note (intuition → pieces → full statement), not splitting it.
- **Segmenting** (Mayer). Learner-paced segments improve transfer: median d = 0.67 over 7/7 tests (Mayer 2021, as tabulated by UNH). The independent meta-analysis of Rey et al. (2019; 56 studies, 88 comparisons) found small-to-medium effects. In a static text note the segments are *headings*: the reader already sets the pace.
- **Subgoal labels** (Catrambone 1998). Labeling groups of steps in worked examples improved transfer to novel problems, and abstract labels worked better than surface ones. In a note, worked examples and proofs should carry headings that name their purpose. The subgoals do not need to be separate notes.
- **Minimum information principle** (Wozniak 1999). Atomicity pays off at the *flashcard* level, because each item is scheduled at its own pace. Rule 17 adds that redundancy is acceptable. This argues for atomic *cards derived from* notes, not for atomic notes. Kirkby & Matuschak (2026, in 01) already showed that card construction needs a human in the loop.

**Takeaway.** The learning science supports *coherent notes with labeled internal segments*. Atomicity belongs to cards, not to notes.

## 3. Agent and retrieval evidence

- **Retrieval chunk size depends on the task.** Bhat et al. (2025) found 64–128-token chunks best for short factual answers and 512–1024 tokens best when broader context is needed, and the best size also varied with the embedding model. Chroma (Smith & Troynikov 2024) found 200–400-token chunks with no overlap a strong default, while 800-token chunks with 400 overlap did worst on precision and IoU. Caspari et al. (2026) found that expensive chunkers "rarely provide consistent gains over simpler chunking". qmd (02) chunks at 900 tokens and prefers heading boundaries. **Implication:** the retriever chunks by section, so note length need not equal chunk length. A *section* is what should be retrieval-sized, roughly ≤ 600 words (about 800 tokens).
- **Context rot / lost in the middle** (Liu et al. 2024; Chroma 2025, both in 01). Degradation appears as total context grows into the tens of thousands of tokens and as irrelevant material is added. A 2,000-word note is about 2.6k tokens, so reading one whole note before editing is cheap. The real costs are (a) bundling *irrelevant* material into one note, which adds noise to every read, and (b) the agent loading many notes at once. Both favor focused notes. Neither argues for tiny ones.
- **Agent memory practice.** A-Mem (02) stores one note per interaction and rewrites linked notes as new ones arrive. Later work found that rewriting step fragile (01). LongMemEval (01) found that round-level keys beat session-level keys, meaning finer index keys over the same content. Basic Memory's skill guidance says "one fact per observation" (a line), but also "longer notes with more context are *more* discoverable", and "if the entity already exists, update it … rather than creating a new note" (basicmachines-co `memory-notes` skill). Letta memory blocks have a per-block character `limit`. The docs show 4,000–5,000 in examples, and I did not confirm the default in source **[unverified]**. Karpathy's gist gives no page-size guidance and says a single source "might touch 10-15 wiki pages" (02).
- **Atomic units multiply duplicates.** LightRAG merges entities "solely based on exact name matches", which yields "multiple disconnected nodes for the same entity under different names" (HKUDS/LightRAG #1323; also #485, #1631). The more notes an agent creates, the more name-resolution decisions it makes, and each one can fail silently. nLab's rule (one page per concept, plus redirects) and Graphiti's split (LLM proposes, code applies; 02) are the countermeasures.
- **Human diff review.** A vendor study at Cisco (SmartBear, Cohen 2006: 2,500 reviews) found that defect detection falls above 200 lines and is near zero above 400. It recommends reviewing fewer than 200–400 lines at a time. This is code, not prose, and a vendor case study, so treat it as weak and **analogical** evidence. It bounds the *diff per agent commit* more than the note size.

**Takeaway.** Retrieval wants retrieval-sized sections, not tiny files. The agent's context budget does not constrain notes of a few thousand words. The main costs of atomization are duplicate concepts and many-file diffs.

## 4. Recommended policy

Three units, three purposes:

| Unit | Purpose | Size driver |
|---|---|---|
| **Note** | one canonical home for a concept: definition plus everything that interacts with it | coherence (element interactivity), one-concept rule |
| **Section** (`##` heading) | sub-idea, worked example, proof, variant; link and retrieval target | retrieval chunk (~200–800 tokens), subgoal labels |
| **Card** | recall atom for FSRS | minimum information principle |

### 4.1 What earns a note

A concept gets its own note when **both** hold:
1. **It has its own definition.** You could state it as a glossary entry or a `Definition` environment without first defining its parent. Examples are "Dijkstra's algorithm" and "binary heap". By contrast, "correctness of Dijkstra's algorithm" is a section of the Dijkstra note.
2. **It is referenced from at least 2 distinct other notes or views**, or it is a prerequisite target of one. Links to `[[parent#heading]]` count. This is nLab's "linked from at least one page" rule tightened by one, so that a note is a shared hub and not a private branch. The threshold of 2 is a starting value and needs calibrating (4.5).

Otherwise the idea is a **heading inside the concept that owns it**, and its name is an `aliases:` entry on that concept so that `loci find` still resolves it. This applies WP:PAGEDECIDE: when the topic needs the context around it, it stays inside the larger page.

**Draft dissolution.** A draft that teaches several ideas moves its content into the notes of the concepts it explains. Each worked example goes under an `## Example: <purpose>` heading in the concept it illustrates, not into a note of its own. If the draft's order matters, it becomes a curated view that lists `[[concept]]` and `[[concept#heading]]` links in that order (D2, DESIGN §views).

### 4.2 Length budgets (soft; lint warns, human decides)

Counts are of body prose: they exclude frontmatter, code blocks, display math and link lists, mirroring Wikipedia's readable-prose rule.
- **Note:** target 150–1,500 words. Warn above **2,000 words** (~2.6k tokens). Strong warn above **3,500 words**. These numbers are far below Wikipedia's 6,000/8,000-word thresholds, deliberately: a Wikipedia article covers a *topic* with a summary hierarchy, while a concept note covers *one concept*. The upper bound mostly reflects how much prose one person can review in one diff (§3, analogical).
- **Section:** warn above **600 words** (~800 tokens), so that a section fits one retrieval chunk at qmd-like sizes (Chroma; Bhat et al.).
- **Stub:** warn below **150 words** if the note is older than 60 days. This is Wikipedia's merge threshold, unchanged.

### 4.3 Promotion (heading → note)

Flag a heading for promotion when **any** of the following holds:
- **P1.** ≥ 2 distinct notes or views link to `[[X#H]]`, counted from markdown-oxide-compatible heading links, block refs included.
- **P2.** The section exceeds 600 words **and** has its own sources or prerequisites that the parent lacks.
- **P3.** The heading's term appears as a link target or alias query from other areas. This one is a weaker signal: it means the concept is used across domains.

On promotion, follow WP:SUMMARY:
- The parent keeps a 1–3 sentence summary and a `Main: [[child]]` link.
- The child's first paragraph is the definition, and the parent summary is drawn from it.
- Existing `[[X#H]]` links are rewritten to `[[child]]` in the same commit.
- The old heading name becomes an alias of the child.
- `loci lint` fails if any `[[X#H]]` link dangles.

### 4.4 Merge (note → heading) and dedup

- **M1.** The note is a stub (< 150 words, older than 60 days) **and** has ≤ 1 distinct inbound linker. Merge it into that linker or into its parent concept, and keep its title as an alias.
- **M2.** An alias or normalized title collides with another note's title or alias. Normalize by case, plural→singular, hyphens and spaces, the way nLab does. This is a hard lint **error**, because two homes for one name is exactly the LightRAG failure.
- **M3.** Two notes' digest lines or definitions have high lexical/embedding similarity. In that case lint *proposes* a pair, the agent drafts "duplicate / refinement / distinct" with a rationale, and the human decides in the diff (Graphiti split, 02). Never auto-merge.

### 4.5 What `loci lint` measures, and how to test the policy

Per note:
- `prose_words`
- `max_section_words`
- `inbound_notes` (distinct linkers)
- `heading_inbound[h]` (distinct notes linking `#h` or `^block` inside `h`)
- `external_headings` (number of headings with any inbound link)
- `age_days`
- alias collisions

There is also a **hub signal**: `external_headings ≥ 3` with each heading linked from a different area. That pattern suggests the note is really a topic and should be split into a hub concept (a definition plus a summary per child) and its children, the way Luhmann's hub cards worked.

**Testable success criteria for the migration** (all to be checked on the real vault):
1. After migration, ≤ 10% of notes trip a size warning. If more do, the thresholds are wrong for this vault: recalibrate from the observed distribution rather than forcing splits.
2. M2 reports zero alias or title collisions.
3. On a fixed set of about 30 real questions (drawn from the user's own topics), `loci find` top-5 section recall is at least as high as on the pre-migration vault.
4. The median agent commit touches ≤ 5 notes and ≤ 400 changed prose lines, so the diffs stay reviewable.
5. Over 3 months, splits plus merges per month trend down. If promotions and merges oscillate on the same concepts, the thresholds are too tight.

**Known gaps.** No controlled study compares note granularities for personal knowledge bases. All thresholds above are starting values taken from reader- or reviewer-facing conventions (Wikipedia, mathlib, SmartBear) and retrieval benchmarks on other corpora. Validate them on this vault before hardening any of them from warnings into errors.

---

## Sources

- Schmidt, J.F.K., "Niklas Luhmann's Card Index: The Fabrication of Serendipity", *Sociologica* 12(1):53–60, 2018. https://doi.org/10.6092/issn.1971-8853/8350
- Niklas Luhmann-Archiv, "Zettelkasten" (card format, front-side use). https://niklas-luhmann-archiv.de/nachlass/zettelkasten
- Ahrens, S., *How to Take Smart Notes*, 2017. Quoted via https://rufuspollock.com/post/ahrens-how-to-take-smart-notes and https://www.goodreads.com/work/quotes/55634026 (secondary).
- Matuschak, A., "Evergreen notes should be atomic", working notes. https://notes.andymatuschak.org/Evergreen_notes_should_be_atomic
- Matuschak, A., "Evergreen notes should be concept-oriented", working notes. https://notes.andymatuschak.org/Evergreen_notes_should_be_concept-oriented
- Fast, S., "The Complete Guide to Atomic Note-Taking", zettelkasten.de, n.d. https://zettelkasten.de/atomicity/guide/
- Wikipedia: Article size (WP:SIZERULE). https://en.wikipedia.org/wiki/Wikipedia:Article_size
- Wikipedia: Summary style (WP:SUMMARY). https://en.wikipedia.org/wiki/Wikipedia:Summary_style
- Wikipedia: Notability, standalone pages (WP:PAGEDECIDE). https://en.wikipedia.org/wiki/Wikipedia:Notability
- Stanford Encyclopedia of Philosophy, entry guidelines (current mirror and 1999 archive). https://meinong.stanford.edu/guidelines.html ; https://plato.stanford.edu/archives/win1999/guidelines.html
- nLab, HowTo (one page per concept, redirects, merging). https://ncatlab.org/nlab/show/HowTo
- mathlib4, `longFile` linter, `Mathlib/Tactic/Linter/Style.lean` (read at master, 2026-10-30). https://github.com/leanprover-community/mathlib4/blob/master/Mathlib/Tactic/Linter/Style.lean
- Sweller, J., "Element Interactivity and Intrinsic, Extraneous, and Germane Cognitive Load", *Educational Psychology Review* 22(2):123–138, 2010. https://doi.org/10.1007/s10648-010-9128-5
- Pollock, E., Chandler, P., Sweller, J., "Assimilating complex information", *Learning and Instruction* 12(1):61–86, 2002. https://www.sciencedirect.com/science/article/abs/pii/S0959475201000160
- Mayer, R.E., *Multimedia Learning*, 3rd ed., 2021, segmenting principle (d = 0.67, as tabulated in the UNH summary). https://www.unh.edu/teaching-learning-resource-hub/sites/default/files/media/2023-06/itow-research-based-principles-for-designing-multimedia-instruction-mayer.pdf
- Rey, G.D., et al., "A Meta-analysis of the Segmenting Effect", *Educational Psychology Review*, 2019. https://maria-wirzberger.de/wp-content/uploads/2019/01/Rey2019_Article_AMeta-analysisOfTheSegmentingE.pdf
- Catrambone, R., "The subgoal learning model", *J. Exp. Psychol.: General* 127(4):355–376, 1998. https://doi.org/10.1037/0096-3445.127.4.355
- Wozniak, P., "Effective learning: Twenty rules of formulating knowledge", SuperMemo, 1999. https://www.supermemo.com/en/blog/twenty-rules-of-formulating-knowledge
- Bhat, S.R., et al., "Rethinking Chunk Size for Long-Document Retrieval: A Multi-Dataset Analysis", arXiv 2025. https://arxiv.org/abs/2505.21700
- Smith, B., Troynikov, A., "Evaluating Chunking Strategies for Retrieval", Chroma Research, Jul 2024. https://www.trychroma.com/research/evaluating-chunking
- Caspari, L., et al., "When Is Complex Chunking Worth It? A Multi-Objective Evaluation of Chunking Methods at Scale", arXiv Aug 2026. https://arxiv.org/abs/2608.16586
- Basic Memory, knowledge format docs and `memory-notes` skill. https://docs.basicmemory.com/concepts/knowledge-format ; https://github.com/basicmachines-co/basic-memory
- Letta, Memory blocks docs. https://docs.letta.com/guides/core-concepts/memory/memory-blocks/
- HKUDS/LightRAG issues #1323, #485, #1631 (exact-name entity merging). https://github.com/HKUDS/LightRAG/issues/1323
- SmartBear / Cohen, J., *Best Kept Secrets of Peer Code Review*, 2006 (Cisco case study; vendor). https://static0.smartbear.co/support/media/resources/cc/book/code-review-cisco-case-study.pdf
- Already covered in 01/02 and not re-read: Karpathy LLM Wiki gist; A-Mem (NeurIPS 2025); LongMemEval (ICLR 2025); Liu et al., "Lost in the Middle" (TACL 2024); Chroma "Context Rot" (2025); qmd; Graphiti.
