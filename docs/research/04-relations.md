# 04 — Relations between concept notes

Date: 2026-10-30. Scope: how loci should type, store, evolve and lint relations between
concept notes. Builds on 01/02 (SKOS, discourse graphs, Graphiti, HippoRAG) without
repeating them. Claims I could not check against a primary source are marked
**[unverified]**; claims taken from a secondary summary are marked *(secondary)*.

Current design (DESIGN.md §4.3): body wikilinks are untyped; `prereqs` is the one typed
relation, read by `loci closure`; "a field exists only if a tool or procedure reads it";
no free-form tags.

**Bottom line.** Keep `prereqs` as the only typed relation for now. Add a small
`relations.toml` registry that every typed-relation field must be declared in, with a
definition, algebra (inverse, transitive, acyclic), who may assert it, and the procedure
that reads it. `loci lint` enforces the registry as hard errors. New types enter only
through a promotion rule tied to a reading procedure, and retire through an explicit
migration. Anything an edge needs to *say* (conditions, an argument, a source) goes into
a note, not onto the edge.

---

## 1. What history says about typed relations

### 1.1 Hypertext: typed links were tried first, and users avoided typing

- **Textnet** (Trigg, PhD thesis, U. Maryland 1983, https://dl.acm.org/doi/book/10.5555/911585;
  Trigg & Weiser, *ACM TOIS* 4(1), 1986, https://dl.acm.org/doi/10.1145/5401.5402) proposed a
  taxonomy of link types meant to capture "the essential relationship between pieces of text
  comprising scientific text and commentary". It had roughly eighty types *(secondary:
  https://www.ps.uni-saarland.de/~duchier/pub/hypertext_review/chapter1.html)*.
- **NoteCards** (Halasz, *CACM* 31(7), 1988, first at Hypertext '87) had user-extensible
  typed links. Marshall's 2001 commentary (https://dl.acm.org/doi/pdf/10.1145/507317.507325)
  notes that Halasz's "NoteCards in Use" section found links often just placed at the end of
  a card's text, and frames the field's recurring tension as formal vs informal structure.
  The claim that most NoteCards links stayed untyped is **[unverified]**: the only hint I
  found is an `<Unspecified>` link type in the paper's example figure
  (https://mprove.de/visionreality/text/2.1.6_notecards.html).
- **"Formality Considered Harmful"** (Shipman & Marshall, *CSCW* 8(4):333–352, 1999,
  https://doi.org/10.1023/A:1008716330212) is the core lesson. Many problems come from
  "users' unwillingness or inability to make structure, content, or procedures explicit".
  Designers should match the level of formality to the users' goals.

For loci, the "user" who must formalise is partly an LLM agent. Unlike a human, it will
happily type every link, and that is its own failure: confident structure that no one
checked.

### 1.2 PKM tools: typed links are wanted, proliferate, and fork syntax

- The Obsidian request "Add support for link types" has been open since July 2020 and has
  more than 200 replies
  (https://forum.obsidian.md/t/add-support-for-link-types-link-info-link-metadata/6994). In
  that thread users raise ambiguous direction ("married to" reads both ways) and
  proliferation ("the link network will be awash with them"). Those are forum anecdotes,
  not studies.
- Each plugin uses its own syntax:
  - **Juggl**: list items such as `- painted [[The Night Watch]]`. Only one-word types, no
    edge properties, and the current note must be the source. The docs admit "it is
    metadata, and not inline" (https://juggl.io/link-types.html).
  - **Dataview**: `key:: value`, `[key:: value]`, `(key:: value)` inline, or frontmatter
    (https://blacksmithgu.github.io/obsidian-dataview/annotation/add-metadata/).
  - **Breadcrumbs**: user-defined edge fields, automatic inverses (up/down), and
    user-defined transitive rules (https://github.com/SkepticMystic/breadcrumbs).
  - **ExcaliBrain**: infers parent, child and friend from plain links (forward link = child,
    backlink = parent, mutual = friend) unless Dataview fields override them
    (https://github.com/zsviczian/excalibrain).

  The same edge can therefore be written three ways, and forum users report one syntax
  rendering while another silently does not
  (https://forum.obsidian.md/t/how-to-create-juggl-typed-links-edge-annotation/44477).
- **Logseq** moved from untyped `key:: value` file properties to the DB version, where
  properties have a declared type, a cardinality and, for node-valued properties, allowed
  tags. It ships versioned schema migrations (`schema-version->updates`) and a converter
  from file graphs (https://github.com/logseq/docs/blob/master/db-version.md;
  https://discuss.logseq.com/t/logseq-db-changelog/30013). **Tana** supertags define fields
  on a type template, can extend other supertags, and the docs advise checking a tag's usage
  count and merging duplicates before deleting it
  (https://outliner.tana.inc/learn/features/supertags).
- **Semantic MediaWiki** uses `[[property::value]]`. Each property has its own page that
  declares its datatype, and an undeclared property defaults to `Page`. Changing a type
  triggers a change-propagation job that locks the property until done
  (https://www.semantic-mediawiki.org/wiki/Help:Property_declaration;
  https://www.semantic-mediawiki.org/wiki/Help:Changing_property_types).

**Pattern.** The tools that lasted put a *declaration* of each relation somewhere (a
property page, a tag template, a typed schema) and treat changes to it as migrations.
Undeclared inline types (Juggl's one-word types, free Dataview keys) are where synonyms
and syntax drift pile up.

### 1.3 Curated vocabularies: governance is the product

- **SKOS** (W3C Rec 2009, https://www.w3.org/TR/skos-reference/): `skos:broader` is "not a
  transitive property", but is a sub-property of the transitive `skos:broaderTransitive`
  (S24). `skos:related` is symmetric (S23) and not transitive. Integrity condition **S27**
  says `related` is disjoint with `broaderTransitive`. The lesson for loci is to store the
  asserted edge and *compute* the closure, and to state disjointness rules that lint can
  check.
- **Wikidata**: a new property is proposed and discussed, and a property creator or admin
  creates it once there is "reasonable consensus"
  (https://www.wikidata.org/wiki/Wikidata:Property_proposal;
  https://www.wikidata.org/wiki/Wikidata:Property_creators). Constraints such as
  single-value, value-type, symmetric, inverse, one-of, conflicts-with and allowed
  qualifiers are attached to the property itself. They are "hints, not firm restrictions":
  violations are reported, not blocked, and exceptions can be listed
  (https://www.wikidata.org/wiki/Help:Property_constraints_portal). Statements carry
  qualifiers and references, which is edge-level provenance.
- **schema.org**: new terms enter through a `pending` area and are promoted or dropped
  "depending on usage, adoption, and discussion" (https://schema.org/docs/extension.html).
  Retired terms move to the **attic** so that old links keep resolving
  (https://schema.org/docs/attic.home.html). `supersededBy` relates a term "to one that
  supersedes it" (https://schema.org/supersededBy).
- **OBO Relation Ontology**: shared relations such as `part of` (BFO:0000050) are reused
  across ontologies (https://oborel.github.io/). The founding paper defines each relation
  formally, with axioms such as the transitivity of part_of. It also warns that part_of is
  really two relations (between continuants, and between processes), and that
  transitivity must not cross that divide (Smith et al., *Genome Biology* 6:R46, 2005,
  https://doi.org/10.1186/gb-2005-6-5-r46). Obsolete terms keep their IDs, get
  `owl:deprecated true`, and point to `term replaced by` (IAO:0100001, at most one) or to
  `consider` (any number). The OBO dashboard flags inconsistencies
  (https://obofoundry.org/principles/fp-019-term-stability.html).
- **Mathematics**:
  - Lean **blueprints** declare dependency with `\uses{...}`, separately in a statement and
    in its proof, and build the dependency graph from it.
    `leanblueprint checkdecls` checks that every referenced Lean name exists
    (https://github.com/PatrickMassot/leanblueprint). That is one typed relation, read by
    one tool, checked by a linter: exactly the loci principle.
  - In mathlib itself, dependency is the import and declaration graph that Lean computes;
    no one asserts it by hand.
  - nLab pages typically end with an untyped "Related concepts" list **[unverified as a
    convention]**.

**Pattern.** Every durable vocabulary has (a) a written definition per relation, (b) a
declared algebra (inverse, transitivity, disjointness), (c) a staging area or proposal
step, and (d) deprecation that keeps the old ID resolvable with a pointer to the
replacement. Wikidata keeps constraints soft because it has millions of editors. loci has
one user and a CLI, so the same checks can be hard errors.

---

## 2. Representing relations in plain text

| Option | Example | Verdict for loci |
|---|---|---|
| Frontmatter list | `prereqs: [[a]], [[b]]` | **Use.** One parse location, diffable, and the CLI can rewrite it safely. |
| Inline field in body | `prereq:: [[a]]` (Dataview/Logseq) | Avoid. The same edge can then live in two places, and a sentence edit can change graph semantics. |
| List-item typed link | `- uses [[a]]` (Juggl, basic-memory) | Avoid. Ambiguous with ordinary prose lists. |
| Link-internal type | `[[uses::a]]` (Typed Links plugin) | Avoid. Breaks markdown-oxide and other standard wikilink parsers. |
| Reified note | `concepts/master-theorem.md` linking both ends | **Use when the edge has content** (§4.3). |

**Edge attributes and n-ary relations.** The W3C note on n-ary relations (Working Group
Note, 12 Apr 2006, https://www.w3.org/TR/swbp-n-aryRelations/) gives the standard
answer. When a relation has attributes (a probability, a condition) or several
participants with no distinguished owner, make the relation a node with one property per
participant. The note also lists the cost: inverses and restrictions get harder.
RDF 1.2 adds triple terms and reifiers (`rdf:reifies`) so a statement can be described
without being asserted; it is still a Candidate Recommendation, snapshot of 7 Apr 2026
(https://www.w3.org/TR/rdf12-concepts/). Property graphs put key/value pairs directly on
edges.

At loci's scale (thousands of notes, one user, git) neither stack is needed. A note *is*
the reifier: it has an ID, prose, sources and git history. Edge provenance (who asserted
it, when, reviewed or not) already comes from `git blame` plus the agent-branch merge
gate (DESIGN §4.4), so per-edge `source`/`confidence` fields would fail the "a tool reads
it" test.

---

## 3. How reliable is automatic relation typing?

**Humans first.**
- **Prerequisites.** LectureBank reports Cohen's κ = 0.7 for binary prerequisite labels on
  208 NLP topics, after tightening criteria. The earlier ternary scheme on the same pairs
  had κ ≈ 0.3 (Li et al., AAAI 2019, https://arxiv.org/abs/1811.12181). Alzetta et al.
  (*Digging Into Prerequisite Annotation*, CEUR-WS Vol-2674, 2020,
  https://ceur-ws.org/Vol-2674/paper03.pdf; workshop **[unverified]**) state that prerequisite
  annotations "rarely achieve high agreement values unless the annotation task is
  extremely guided". A major source of disagreement is transitivity: one rater writes
  A<B<C, another writes A<C. Lexical/taxonomic relations were most frequent among
  high-agreement pairs.
- **Fine-grained scientific relations.** SciERC uses seven relation types (Used-for,
  Feature-of, Hyponym-of, Part-of, Compare, Conjunction, Evaluate-for). Expert κ was 76.9%
  for entities but **67.8% for relations** (Luan et al., EMNLP 2018,
  https://aclanthology.org/D18-1360/). Fine-grained "uses/part-of/feature-of" typing is
  only moderately reliable even for experts.

**LLMs.**
- **Prerequisite.** Zero-shot GPT-4 reaches F1 ≈ 0.78 on LectureBankCD (Graphusion,
  https://arxiv.org/abs/2410.17600 *(secondary read, from 02)*).
- **Is-a.** On binary hypernym checks, GPT-4o scored 87.5%. Definition-based prompts gave
  the highest accuracy, and structural prompts did not reliably help (LLM-Hype, VLDB 2025
  LLM+Graph workshop,
  https://www.vldb.org/2025/Workshops/VLDB-Workshops-2025/LLM+Graph/LLMGraph-4.pdf). The
  test sets are small (5 datasets × 20 positive + 20 negative pairs).
- **Wider gaps.** Across hyponymy, holonymy, meronymy, antonymy and synonymy, a
  human–model gap remains for all relations; antonymy is the only one where models do well
  (*Language Resources & Evaluation* 2025, https://arxiv.org/abs/2412.01131). Those models
  are mostly encoder PLMs, not frontier LLMs.
- **Generative relation extraction** improves with fine-tuning and chain-of-thought (Wadhwa
  et al., ACL 2023, https://aclanthology.org/2023.acl-long.868/), but I found no per-type
  frontier-LLM results on SciERC-style concept relations **[gap]**.

**Implications.**
1. is-a/broader and prerequisite are the two relation types reliable enough for an agent
   to *propose* (≈0.75–0.88). Neither is reliable enough to merge without review.
   uses/part-of/feature-of/contrasts are not reliably typed even by humans.
2. Agreement depends on the definition. Put the registry definition, with examples, into
   the agent's prompt (LLM-Hype's definition-prompt result) and into the human's review
   view.
3. Store **direct** prerequisites only and compute the closure. This removes the A<C vs
   A<B<C disagreement by construction.

---

## 4. Recommendation

### 4.1 Registry: `relations.toml` at the vault root

```toml
version = 1                       # bumped by every migration; see 4.5

[prereqs]
definition = """
B prereqs A: A is a direct prerequisite of B. B's definition or central argument cannot be
followed without A. List only immediate prerequisites; transitive ones are computed."""
examples   = ["dijkstra -> priority-queue", "dijkstra -> edge-relaxation"]
counter    = ["dijkstra -/-> a-star (used with it, not needed to understand it)"]
domain     = "concept"            # source must be a concept note
range      = "concept"            # target must be a concept note
transitive = true                 # closure is computed, never stored
acyclic    = true
inverse    = "enables"            # display name only; never stored
assert_by  = ["user", "agent-branch"]   # agent edges enter only via a reviewed branch
read_by    = ["loci closure", "skill:learn step 2", "skill:learn step 5"]
status     = "active"             # active | deprecated
since      = "2026-10-30"
```

Fields beyond `definition` are included only because lint or a command reads them:

| Field | Read by |
|---|---|
| `domain`, `range` | domain/range lint |
| `transitive`, `acyclic` | closure and cycle lint |
| `inverse` | backlink panes |
| `assert_by` | merge-gate lint |
| `read_by` | dead-relation lint |
| `status`, `replaced_by`, `consider` | migration lint |

This is the SMW property page, Wikidata constraints and OBO deprecation, collapsed into
one file that git versions.

### 4.2 Starting set: just `prereqs`

No current procedure reads anything else. Candidates, each with the procedure that would
justify it. Do not add one until that procedure exists.

| Candidate | Justifying procedure | Algebra | Notes |
|---|---|---|---|
| `broader` (is-a / instance-of / specialises) | "show siblings and generalisations" in learn plans, or card generation for discrimination questions | non-transitive asserted, transitive closure (SKOS) | Hub-concept backlinks already cover "theme membership". Add only if a procedure must tell is-a apart from mention. LLM is-a proposals are the most reliable kind (§3). |
| `contrasts` | review: generate "A vs B" discrimination cards | symmetric, stored once (lexicographic source) | Not reliably extractable. User-asserted only. |
| `supersedes` | retrieval must hide or deprioritise the older concept | acyclic, not transitive in display | Probably better as a note-level `status`. |

Explicitly rejected for now: `uses`, `part-of`, `related`. `related` is what an untyped
body link already means. `uses` and `part-of` have κ ≈ 0.68 for experts and no reader.

### 4.3 When to reify a relation as its own note

Make it a note, not an edge, when **any** of these holds:

1. The relation needs **conditions or assumptions**. Example: "A reduces to B under C" or
   "X is sound if the hash is a random oracle".
2. It needs an **argument or construction** (a proof, a reduction, a transform).
3. It has **more than two participants** or no natural source (W3C n-ary pattern 3).
4. It needs its **own sources**, or it may be **disputed or superseded**.
5. Explaining it takes more than one sentence.

In mathematics and computer science most interesting relations pass test 2: a reduction, a
transform or a theorem is itself a concept (`master-theorem`, `cook-levin`). The reified
note links to its endpoints, and may itself list them in `prereqs`. The endpoints then see
it through backlinks. Do not add `reduces_to` as a field type.

### 4.4 Promoting a body link into a typed relation

A *specific link* becomes a typed edge when the relation type already exists and the link
satisfies its definition. The agent proposes this on a branch, quoting the definition. A
*new relation type* is created only when all of these hold:

1. **A reader exists.** A named procedure or command would produce a wrong or worse result
   without it. That procedure goes in `read_by`.
2. **Untyped links are not enough.** Show at least one concrete case where the procedure
   cannot tell the needed links from other body links by text or position alone.
3. **The definition is operational.** It has at least two positive examples and one
   counter-example drawn from the vault, and its algebra is declared.
4. **A pilot passes.** Have the agent label about 20 existing body links with the new
   type, and have the user review them. If the user would overturn more than about 1 in 5,
   the definition is not ready. This is a heuristic echoing §3, not a validated threshold.
5. **It is recorded.** Add a DECISIONS.md entry, bump the registry version, and run the
   migration that back-fills edges on an agent branch.

This is schema.org's *pending → core* and Wikidata's *proposal → creation*, sized for one
person.

### 4.5 Evolution: migrations like database migrations

`loci rel` subcommands are idempotent. Each one is a single commit that bumps `version`
and appends to DECISIONS.md.

- `loci rel add NAME`: writes the registry entry. Lint fails until `read_by` names
  something real.
- `loci rel rename OLD NEW`: rewrites every frontmatter key and keeps `OLD` as
  `status = "deprecated", replaced_by = "NEW"` for one release. Lint errors on new uses of
  `OLD` (OBO `term replaced by`; schema.org attic).
- `loci rel retire NAME [--into-body]`: removes the field everywhere. By default each
  removed edge turns into a plain body link, so no information is lost and git keeps the
  history. The entry stays as `deprecated` with `consider = [...]`, so old commits still
  explain themselves.
- `loci rel split OLD NEW1 NEW2`: no automatic mapping is possible. The command opens an
  agent branch that proposes a label per edge for review (OBO `consider`).

Logseq's versioned `schema-version->updates` list and SMW's locked change propagation
are the same idea: the schema version is data, and changing it is an explicit job.

### 4.6 Lint checks (hard errors unless noted)

1. **Unknown relation key.** A frontmatter key that is neither a fixed field nor a
   registered relation is an error. This blocks tag-like drift.
2. **Dangling target** and **domain/range violations**. For example, a `prereqs` target
   must be in `concepts/`, never in `sources/` (this mirrors the no-self-citation rule).
3. **Self-loop.**
4. **Cycle** in an `acyclic` relation. Print the cycle.
5. **Redundant transitive edge** (warning). A→C is asserted while A→B→C holds. Enforces
   "direct only".
6. **Stored inverse or duplicate symmetric edge.** Inverses are computed; a symmetric
   relation is stored on one canonical side.
7. **Disjointness**, once `broader` exists: SKOS S27, a pair cannot be both `broader*`
   and in another declared-disjoint relation.
8. **Dead relation.** A `read_by` entry naming a command or skill that does not exist is an
   error. A relation with zero uses for 90 days is a warning and a retirement candidate.
9. **Deprecated relation in use.** Error, with the `replaced_by` fix in the message.
10. **Merge gate.** On `main`, edges added by `loci-agent` commits must come through a
    merged agent branch, per `assert_by`. Checked with git, not stored per edge.
11. **Typed edge without a body mention** (warning, optional). A `prereqs` target that the
    body never mentions suggests an unexplained edge. Adopt this only if the closure turns
    out noisy.

---

## 5. Open questions

- No study found on maintenance cost of typed links in personal vaults over years; evidence
  is forum anecdote plus Shipman & Marshall's general argument **[gap]**.
- Per-type frontier-LLM accuracy on concept relations other than prerequisite and is-a is
  unmeasured **[gap]**. A 50-edge pilot from the user's own vault (prereqs on technical
  topics) is the cheapest way to get a real number.
- Whether `broader` earns its place depends on whether learn plans need "siblings". Decide
  after the first few `learn X` runs.
