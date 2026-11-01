# 12 — Hierarchy depth and per-folder description files

Status: research, 2026-11-01. Question: how do mature note-taking and knowledge
systems handle (1) how deep a folder or topic hierarchy may go, and (2) files that
describe a folder or area? And what does the evidence say about depth and
retrieval, for people and for LLM agents? The goal is to decide, for loci:

- **(a)** whether concept sub-folders below an area may go to any depth, and how
  `loci digest` should show that depth;
- **(b)** whether `AREA.md` should stay required, become optional, or extend to
  sub-folders.

Current loci design (m0 §3, §6.3, §10; D78): concepts live at
`concepts/<area>/**/<slug>.md` at any depth; slugs are unique vault-wide and links
are bare `[[slug]]`. Each top-level area has a required `AREA.md` (an H1 and one
paragraph). The index has two levels: `loci digest` lists areas with their paragraph
and concept count; `loci digest --area X` lists all of that area's concepts, grouped
flat by sub-folder path (`graphs`, `graphs/shortest`). Sub-folders have no
description file.

Nothing here changes DESIGN or the specs; it informs decisions. Conventions:
`[unverified]` means I did not confirm the claim in a primary source;
`[design inference]` means my extrapolation, with no study behind it; `[gap]` means
I looked and found no evidence.

## TL;DR

| Question | Recommendation | Why |
|---|---|---|
| (a) depth on disk | Keep any depth. Add no lint for it. Tell the agent, in the `concepts` skill and `rules.md`, to use sub-folders sparingly (one level is the norm, two is rare) and to split an area with `loci mv` when it outgrows its digest | Bare-slug links make depth free for link stability (as in Logseq, Dendron, Obsidian). People keep folders shallow, and each level costs time (Bergman et al. 2010: +2.2 s per level, mean depth 2.86). Johnny.Decimal caps at two levels, Ango uses no nested folders |
| (a) digest | Keep two levels (root, area) and the flat `## <relative-dir>` groups. Add no `--area X/sub` third level | An agent that reads one area digest sees every concept in the area, whatever the depth. Anthropic's skill guidance: keep references "one level deep" because nested ones get partial reads. llms.txt sections are flat H2 lists too |
| (a) optional | Consider listing each area's sub-folder paths on the root digest, under the paragraph `[design inference]` | It is a cheap second-level hint, like llms.txt section names. Measure with `loci bench --agent` before adopting |
| (b) | Keep `AREA.md` required for top-level areas, and only for them (D78 stands) | The root paragraph is the only text the agent sees when choosing an area, like a skill `description`, which Anthropic calls "critical for skill selection". Inside an area the concept digests already describe the leaves, so a sub-folder description would repeat them. Every system surveyed makes sub-folder descriptions optional or omits them |
| (b) docs gap | `share/loci/rules.md` describes `concepts/<area>/<slug>.md` and does not mention sub-folders. Say that they exist, that they have no `AREA.md`, and when to create one | The agent cannot follow a convention it is never told about |

## 1. Findings per system

### Obsidian

- **Nesting:** ordinary file-system folders, nested to any depth. The help page
  documents "New folder" on a folder and "Move folder to…" and gives no depth limit
  (https://obsidian.md/help/plugins/file-explorer).
- **Folder description:** none in core. The file explorer's folder menu has no
  folder-note entry (same page). Community plugins add one:
  - LostPaul's *Folder notes*: "attach notes to folders so that you can click on the
    name of a folder to open the note", "like in the app Notion"; it can
    auto-create a note for each new folder, convert existing folders, and exclude
    folders by pattern (https://github.com/LostPaul/obsidian-folder-notes,
    https://lostpaul.github.io/obsidian-folder-notes/). Its docs mention a
    "parent folder" storage location
    (https://lostpaul.github.io/obsidian-folder-notes/Folder%20overview/). The other
    naming options (`{{folder_name}}`, `index`) are `[unverified]`.
  - xpgo's *Folder Note*: "three methods of creating description note for a
    folder: Inside-Folder, Outside-Folder and Index-File" (e.g. `_overview_` or
    `index`); Inside-Folder is the default; a note is created on demand by
    Ctrl+Click on a folder (https://github.com/xpgo/obsidian-folder-note-plugin).
    Its docs advise against naming the note after the folder, because a renamed
    folder then needs a manual rename (search result summary; `[unverified]` on the
    exact wording).
  - So folder notes are **opt-in per folder**, and three plugins disagree on where
    the file lives.
- **Practitioner stance:** Obsidian's CEO, Steph Ango: "I use very few folders. I
  avoid folders because many of my entries belong to more than one area of
  thought"; "I do not use nested sub-folders"; "Most of my notes are in the root of
  the vault"; "My notes are primarily organized using the `categories` property",
  shown through Bases; "I mostly navigate using the quick switcher, backlinks, or
  links within a note" (https://stephango.com/vault).
- **MOCs (Nick Milo, Linking Your Thinking):** a Map of Content is "a special note
  called a Map of Content (also known as an MOC…)". You collect links in it when you
  reach a "Mental Squeeze Point": "when your unsorted knowledge becomes so messy it
  overwhelms and discourages you"
  (https://www.linkingyourthinking.com/ideaverse/youre-already-doing-it-sort-of).
  So an MOC is written when needed, not required per folder. Whether MOCs nest (a
  home note linking to MOCs) is not on that page `[unverified]`.

### Logseq

- **Nesting:** pages, not folders. A namespace page `a/b/c` is still one flat file
  in `pages/`. With the default `:file/name-format :triple-lowbar` (default for new
  graphs since 0.8.9) the `/` becomes `___`, e.g. `pages/a___b___c.md`
  (https://github.com/logseq/logseq/releases/tag/0.8.9; a Logseq issue reports that
  renaming a page to `A/B` and getting an `A/` folder is a bug:
  https://github.com/logseq/logseq/issues/6155). I could not load
  https://docs.logseq.com (the page was too large to fetch), so the namespace
  semantics come from the release note, the issue and the forum
  (https://discuss.logseq.com/t/namespace-separator-2f-in-one-repo-triple-underscore-in-the-other/27623).
  A depth limit is `[unverified]` (I found none).
- **Description:** the namespace parent `a` is an ordinary page and may hold text.
  Nothing requires it.

### Org-roam

- **Nesting:** "Org-roam searches recursively within `org-roam-directory` for
  notes" (manual §5.3). The manual recommends neither a flat nor a nested layout;
  sub-directories appear only in examples (dailies, a developer example that takes a
  "namespace" from the parent directory, §17.2.2).
- **Organisation:** "Org-roam allows for effortless non-hierarchical note-taking"
  (§1). Grouping is by links and by tags: file-level tags come from `#+filetags`
  (§8.3) (https://www.orgroam.com/manual.html).
- **Description:** none. There is no folder-level file.

### Dendron

- **Nesting:** a hierarchy of "`.` ("dot" or "period") delimited Markdown files",
  e.g. `project1.tasks.task1.md` is a child of `project1.tasks.md`. All files sit in
  one folder on disk. A note "can have data but also contain other notes"; missing
  parents are *stubs*, so a deep path needs no empty files. There is no depth limit
  (https://wiki.dendron.so/notes/f3a41725-c5e5-4851-a6ed-5f541054d409/).
- **Description:** each vault has a generated `root.md`. Schemas are "an optional
  type system for your notes that describe the hierarchy of your data", in
  `{name}.schema.yml`, with a `desc` field: "human readable description of the
  schema node" (https://wiki.dendron.so/notes/c5e5adde-5459-409b-b34d-a0d75cbb1052/).
  Both are optional.
- **The founder's case for hierarchy:** Kevin Lin: "Having the hierarchy in the
  filename made it easy to find information using the hierarchy"; he can "find any
  specific note within seconds even with thousands of existing notes"; his example
  goes four levels deep (`lang.python.data.boolean`), and each hierarchy's root
  `schema` note works as its table of contents
  (https://blog.dendron.so/notes/3dd58f62-fee5-4f93-b9f1-b0f0f59a9b64/). He called
  Dendron "anti-roam": "every note is exactly in one well defined place (which you
  can change over time)"
  (https://kevins8.medium.com/dendron-an-open-source-local-first-anti-roam-note-taking-tool-ca1ba65d3058).
- **Status:** in maintenance mode since February 2023
  (https://github.com/dendronhq/dendron/discussions/3890).

### Foam

- **Nesting:** plain Markdown files in a VS Code workspace, so any folder layout
  works. The docs offer tags ("Organize and filter content"), smart folders, and a
  recipe titled "Keep your notes in a subfolder". I found no guidance on depth and
  no folder-description convention (https://docs.foam.md/) `[gap]`.

### Notion

- **Nesting:** "There are no folders in Notion. Instead, you can organize pages
  inside pages... inside pages"; "You can create any number of subpages for a page".
  The sidebar shows subpages under their parent, and breadcrumbs "show you what page
  you're in, and where that page lives"
  (https://www.notion.com/help/create-a-subpage). No depth limit is documented
  `[unverified]`.
- **Description:** the parent *is* a page, so every container can hold text, but
  nothing requires it to.

### TiddlyWiki

- **Nesting:** all tiddlers are flat. "Tagging is a way of organising tiddlers into
  categories", and a tag is itself a tiddler that "can have tags of its own"
  (https://tiddlywiki.com/static/Tagging.html). The table-of-contents macros build a
  tree from tags: "The top level of the tree consists of the tiddlers that carry a
  particular tag, known as the root tag. Tiddlers tagged with any of those make up
  the next level down, and so on"
  (https://tiddlywiki.com/static/Table-of-Contents%2520Macros.html).
- **Description:** the tag tiddler can hold text, and its `list` field orders its
  children. The tree shows a `caption` field if present. Both are optional.

### Zettelkasten (Luhmann and zettelkasten.de)

- **Luhmann rejected topical hierarchy:** "it is most important that we decide
  against the systematic ordering in accordance with topics and sub-topics and
  choose instead a firm fixed place"; a topical outline "would bind us to a certain
  order for decades in advance"; "The fixed filing place needs no system". Fixed
  numbers allow "arbitrary internal branching": 57/12 continues as 57/13 and branches
  as 57/12a, 57/12a1. The collection has "not hierarchy, and most certainly no linear
  structure like a book". Because there is no systematic order, "we need a register
  of keywords that we constantly update" (Luhmann 1981, *Kommunikation mit
  Zettelkästen*, tr. Manfred Kuehn: https://luhmann.surge.sh/communicating-with-slip-boxes).
- **So Folgezettel IDs nest without limit, but they are positions, not topics.**
  Real IDs reach seven or more characters (e.g. 21/3d7a6;
  https://forum.zettelkasten.de/discussion/3425/visualizing-luhmanns-folgezettel-in-logseq).
  Luhmann also said "It is not important where you store a Zettel as long as you
  can reference it from every other point" (quoted at
  https://zettelkasten.de/posts/luhmann-folgezettel-truth/).
- **Modern practice:** "Don't use categories. Use tags instead"; structure comes
  from "Structural Layers … through links": structure notes, then main structure
  notes (https://zettelkasten.de/posts/overview/). "Creating categories is a
  top-down process"; "Topic clusters emerge by themselves"
  (https://zettelkasten.de/posts/no-categories/). Research note 03 found that
  Luhmann's hub cards listed up to 25 references, which is a structure note.

### Johnny.Decimal

- **Depth is capped by design:** "everything in a Johnny.Decimal structure is only
  two levels deep. No more endless clicking down into nested-folder hell"; "A
  maximum of 10 areas", "A maximum of 10 categories … in each area", "A maximum of
  100 IDs … in each category" (https://johnnydecimal.com/documentation/the-numbers).
  "Files are saved in IDs, never in areas or categories"
  (https://johnnydecimal.com/documentation/saving-files).
- **Sub-folders inside an ID:** older third-party summaries say there are none.
  The creator, Johnny Noble, later wrote on the forum: "You should not be afraid to
  use subfolders. **But** they must be structured"
  (https://forum.johnnydecimal.com/t/breaking-the-rules-exceptions-allowed-subfolders-what-do-other-people-do/1618;
  quoted from a search summary, `[unverified]` verbatim).
- **Description:** the JDex is required: "the master record of every ID in a
  system"; "Creating the note *is* creating the new ID"
  (https://johnnydecimal.com/10-19-concepts/11-core/11.05-the-jdex/). It indexes
  IDs. Whether areas and categories get JDex entries is not on that page
  `[unverified]`.

### PARA (Tiago Forte)

- **Nesting:** "Four top-level folders – Projects, Areas, Resources, and
  Archives", each with "a small number of subfolders dedicated to each active
  project, area of responsibility, resource, and archive"
  (https://fortelabs.com/blog/para/). The post sets no depth limit. Its only
  relevant guidance is that the system "has to be so simple that it frees up your
  attention".
- **Description:** none prescribed.

### Docs sites (contrast)

- **MkDocs:** "You may nest sections as deeply as you like", with the warning "be
  careful that you don't make it too difficult for your users to navigate". By
  convention the homepage is `index.md`, and `README.md` is accepted ("the
  `index.md` file is used and the `README.md` file is ignored" when both exist). A
  nav section "cannot have a page assigned to it". Index pages are a convention, not
  a requirement (https://www.mkdocs.org/user-guide/writing-your-docs/).
- **Docusaurus:** each folder becomes a sidebar category. An optional
  `_category_.json`/`.yml` sets `label`, `position`, `link` and more. A doc named
  `index`, `README` or the folder's own name becomes the category's index. `link:
  {type: 'generated-index', title, description, …}` generates a page that "displays
  all the direct children of this category". `link: null` generates none. No depth
  limit is given (https://docusaurus.io/docs/sidebar/autogenerated,
  https://docusaurus.io/docs/sidebar/items).
- Docs sites are where per-folder description files are most common, and they are
  still optional there. They exist because a reader browses a deep nav tree one
  level at a time.

## 2. Comparison

| System | Depth | On disk | Folder/area description | Required? | Stance on hierarchy |
|---|---|---|---|---|---|
| Obsidian core | any | nested folders | none in core | — | neutral; CEO avoids folders and nesting |
| Obsidian folder-note plugins | any | note inside folder, beside it, or `index` | folder note | opt-in per folder | — |
| MOC (LYT) | n/a (links) | ordinary note | the MOC itself | written at a "squeeze point" | links over folders |
| Logseq | any (namespaces) | flat `pages/a___b.md` | namespace parent page | no | hierarchy as page names |
| Org-roam | any (recursive) | free | none | — | "non-hierarchical"; links and tags |
| Dendron | any | flat `a.b.c.md` | `root.md`, schema `desc` | schemas optional | hierarchy first |
| Foam | any | free | none documented | — | neutral |
| Notion | any | pages in pages | parent page body | no | nesting is the model |
| TiddlyWiki | any (tags) | flat tiddlers | tag tiddler text, `caption` | no | tags build trees |
| Luhmann | unbounded IDs | fixed position | keyword register, hub cards | register yes | against topical hierarchy |
| Johnny.Decimal | **2** levels (+ ID) | folders | JDex entry per ID | **yes** (per ID) | capped by design |
| PARA | unspecified | 4 top folders | none | — | keep it simple |
| MkDocs | any | folders | `index.md`/`README.md` | convention | "don't make it too difficult" |
| Docusaurus | any | folders | `_category_.json`, index doc | optional | — |
| llms.txt | flat H2 lists | one file per path | H1 required; summary blockquote optional | H1 only | flat, short |
| Agent Skills | refs "one level deep" | folders | `description` in frontmatter | **yes** | shallow |
| **loci now** | any below area | folders | `AREA.md` per area; none below | **yes** (area) | two-level index |

Pattern: almost every tool allows any depth. The systems that cap depth are the
ones built for retrieval (Johnny.Decimal; Anthropic's skill guidance). The
descriptions that are required sit at the **entry point** where a reader or agent
chooses among siblings without seeing their contents: a JDex entry, a skill
`description`, the llms.txt H1. Descriptions of internal folders are optional
everywhere.

## 3. Evidence: does depth help or hurt retrieval?

### People

- **Bergman, Whittaker et al. (2010), *The effect of folder structure on personal
  file navigation*, JASIST 61(12).** 296 participants retrieved 1,131 of their own
  files (5,035 navigation steps). "Folder structures were found to be shallow (files
  were retrieved from mean depth of 2.86 folders)", with a mean of 11.82 files and
  10.64 sub-folders per folder; 94% success in 14.76 s on average. "Retrieval time
  and success depended on folder size and depth", so the users' choice "to avoid
  both deep structure and large folders" was "adaptive". Their model: *Retrieval
  Time = 4.956 + 2.236 × Depth + 0.106 × Folder Size*. One level costs about as
  much as 21 items in a folder, hence their "up to 21" heuristic: past about 21
  items, add a level (full text:
  https://www.tau.ac.il/education/muse/publications/101.pdf; DOI
  https://doi.org/10.1002/asi.21415). The study measures click navigation in a file
  browser, not reading an index.
- **Larson & Czerwinski (CHI 1998), web breadth vs depth:** "while increased depth
  did harm search performance on the web, a medium condition of depth and breadth
  outperformed the broadest shallow web structure overall"
  (https://www.microsoft.com/en-us/research/publication/web-page-design-implications-memory-structure-scent-information-retrieval/).
  The three structures of 512 leaves are commonly given as 8×8×8, 16×32 and 32×16
  `[unverified]`; I read only the abstract.
- Bergman et al. also cite Miller (1981): one flat menu of 64 items was slower and
  more error-prone than two levels of 8 (as cited on p. 30 of the PDF above; I did
  not read Miller).
- **No controlled comparison of PKM methods** (Zettelkasten, PARA, MOC vs folders)
  exists; research note 01 reached the same conclusion `[gap]`.

### LLM agents

- **Anthropic, Agent Skills best practices:** only each skill's `name` and
  `description` are preloaded; "The description is critical for skill selection".
  Under "Avoid deeply nested references": "Claude might partially read files when
  they're referenced from other referenced files … might use commands such as
  `head -100` … resulting in incomplete information. **Keep references one level
  deep from `SKILL.md`**." Reference files over 100 lines should start with a table
  of contents. "Organize for discovery: Structure directories by domain or feature"
  (https://platform.claude.com/docs/en/agents-and-tools/agent-skills/best-practices).
- **Anthropic, context engineering:** agents keep "lightweight identifiers (file
  paths, stored queries, web links, etc.)" and load data "at runtime using tools",
  which "enables progressive disclosure"; "Folder hierarchies, naming conventions,
  and timestamps all provide important signals"
  (https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents).
  So folder paths are useful metadata, separate from how many hops the agent takes.
- **Claude Code CLAUDE.md hierarchy:** files above the working directory load at
  launch; "Files in subdirectories load on demand", once Claude reads or edits a
  file there; target "under 200 lines per CLAUDE.md file"; `@` imports go at most
  "four hops" deep (https://code.claude.com/docs/en/memory). Per-directory context
  exists here because code in sub-trees has its own rules, not to describe what a
  folder contains.
- **AGENTS.md:** "Place another AGENTS.md inside each package"; "the closest one
  takes precedence"; "the main OpenAI repo has 88 AGENTS.md files"
  (https://agents.md/). Same purpose: scoped instructions, not content summaries.
- **llms.txt:** an H1 ("the only required section"), an optional blockquote
  summary, then H2 sections of `[name](url): notes` lists. The format has no nested
  sections; depth comes from placing more files at sub-paths, and "where more than
  one file applies, agents should use the most specific one". Motivation: "Context
  windows … are still too small for most websites in their entirety"
  (https://llmstxt.org/).
- **LATTICE (arXiv 2510.13217):** an LLM navigates a semantic tree whose index is
  built from "multi-level document summaries", reaching 46.7 nDCG@10 on BRIGHT with
  one off-the-shelf LLM. The authors had to handle "noisy, slate-dependent LLM
  scores" when comparing across branches and levels
  (https://arxiv.org/abs/2510.13217). This supports summaries at internal nodes
  **when the navigator cannot see the leaves**. It also shows that every extra
  level adds a noisy decision.
- **No study measures LLM agents navigating personal-knowledge folder trees of
  different depths** `[gap]`. The guidance above is vendor practice plus one
  retrieval paper.

### What this means for loci `[design inference]`

Depth has two separate costs: **hops** (how many reads before the agent sees the
target's description) and **labels** (whether the path tells it anything). loci's
digest separates them. However deep the folders go, the agent takes two hops (root,
then area) and then sees every concept's slug and digest, with the folder path as a
heading. That keeps the benefit of a path as a label and avoids the hop cost
measured by Bergman et al. and warned about in the skill guidance. A third digest
level (`--area X/sub`) would put the hop cost back.

Both digest levels have budgets, and those bound breadth. With the default
`budgets.area_index = 24000` and lines of about 200 characters (slug, 160-character
digest, markup), an area holds about 120 concepts before `index.over-budget` fires.
With `budgets.root_index = 8000` and 600-character paragraphs, the root holds about
12 areas (arithmetic, not measured). This is close to Johnny.Decimal's "10 areas"
and gives a breadth-first shape like the "medium" structure in Larson & Czerwinski.

## 4. Recommendations for loci

### (a) Depth of concept sub-folders, and the digest

1. **Keep "any depth" on disk.** Links are bare slugs and slugs are unique across
   the vault, so moving a note deeper or shallower never breaks a link, and
   `loci mv` rewrites any path-form links. Logseq and Dendron hold hierarchy in
   names over flat storage for the same reason. A depth cap would refuse layouts
   that are legal in every other tool surveyed and would buy no link safety. Add no
   `depth` lint: no source gives a threshold for an index the agent reads in one
   piece. Bergman's 2.86 and Johnny.Decimal's 2 describe click navigation.
2. **Keep the digest at two levels with flat path groups,** as m0 §10 specifies:
   loose concepts first, then one `## <relative-dir>` per directory that holds
   concepts, sorted by path bytes. Do **not** add `loci digest --area X/sub`, and do
   not nest groups as `###`/`####`. Flat full paths match llms.txt's flat H2
   sections, can be grepped, and keep the agent at one level of reference below the
   root, which is what the skill guidance recommends.
3. **Treat the area budget, not depth, as the signal to restructure.** When an
   area goes over `budgets.area_index`, the fix is to split it into areas with
   `loci mv`. Adding sub-folders does not help, because the area digest still lists
   every concept. The `concepts` skill already says to restructure when an area
   "grows too mixed"; add that `index.over-budget` on an area means split it.
4. **Write the convention down.** `share/loci/rules.md` describes concepts as
   `concepts/<area>/<slug>.md`, and the `concepts` skill does not mention
   sub-folders. Add one line to each, roughly: "An area may have sub-folders to
   group related concepts; they have no `AREA.md`, and `loci digest --area` shows
   them as `##` groups. Create one only when it will hold several concepts, and
   rarely go below one level." "Several" is deliberately vague: Bergman's "up to
   21" is the nearest number, and it was measured for a different task.
5. **Optional, to be measured:** list each area's sub-folder paths on the root
   digest, under the paragraph (e.g. `Sub-folders: graphs, graphs/shortest,
   sorting`). This gives the agent a second-level hint for choosing an area at
   little root-budget cost. Adopt it only if `loci bench --agent` shows better area
   choice; otherwise it is an extra field in `digest --json` that nothing needs
   `[design inference]`.

### (b) `AREA.md`

1. **Keep `AREA.md` required for every top-level area that contains a concept**
   (`area.missing`, `area.format`, `area.too-long` as they are). At the root, the
   paragraph is the only content signal the agent gets when it chooses an area.
   That is the role of a skill `description`, which Anthropic's docs make
   mandatory and call "critical for skill selection", and of a JDex entry, which
   Johnny.Decimal requires before the folder may exist. Without it the root shows
   only a title and a count, and the agent must fall back to `loci find` or open
   areas to guess. Making it optional would weaken the step that most decides
   whether the agent opens the right area.
2. **Do not extend it to sub-folders** (D78 stands). In the area digest the agent
   already sees every concept's digest, so a sub-folder description would summarise
   text that is on screen. LATTICE-style internal summaries pay off only when the
   navigator cannot see the leaves. No surveyed system requires per-sub-folder
   descriptions. Obsidian's plugins, MkDocs, Docusaurus, Dendron and Notion all make
   them optional, and the Obsidian plugins disagree on where the file even goes. A
   required file per sub-folder would add a lint target, a `mv` edge case (renaming
   or merging folders) and budget pressure, for content that repeats the digests.
3. **If a sub-folder seems to need a description, use what loci already has.** A
   sub-folder big or distinct enough to need its own paragraph should become an
   area (`loci mv concepts/a/sub concepts/sub`). A reading order or overview is a
   curated view in `views/`, as with MOCs written at a "squeeze point". Both exist
   today, and neither adds a file kind.
4. **Leave sub-folder `AREA.md` as it is now: a concept that fails `slug.format`**
   (m0 §3). This is a loud failure, so an agent that copies the area pattern into a
   sub-folder is told immediately. `loci mv` already has a precise message for a
   destination that would be an area's `AREA.md` (D145).

## Sources

PKM tools:
- Obsidian file explorer: https://obsidian.md/help/plugins/file-explorer
- Steph Ango, "How I use Obsidian": https://stephango.com/vault
- Folder notes (LostPaul): https://github.com/LostPaul/obsidian-folder-notes ; docs https://lostpaul.github.io/obsidian-folder-notes/ ; https://lostpaul.github.io/obsidian-folder-notes/Folder%20overview/
- Folder Note (xpgo): https://github.com/xpgo/obsidian-folder-note-plugin
- Nick Milo, MOC and Mental Squeeze Point: https://www.linkingyourthinking.com/ideaverse/youre-already-doing-it-sort-of
- Logseq 0.8.9 release (file name format): https://github.com/logseq/logseq/releases/tag/0.8.9 ; issue 6155: https://github.com/logseq/logseq/issues/6155 ; forum: https://discuss.logseq.com/t/namespace-separator-2f-in-one-repo-triple-underscore-in-the-other/27623
- Org-roam manual: https://www.orgroam.com/manual.html
- Dendron hierarchies: https://wiki.dendron.so/notes/f3a41725-c5e5-4851-a6ed-5f541054d409/ ; schemas: https://wiki.dendron.so/notes/c5e5adde-5459-409b-b34d-a0d75cbb1052/ ; Kevin Lin, "A Hierarchy First Approach to Note Taking": https://blog.dendron.so/notes/3dd58f62-fee5-4f93-b9f1-b0f0f59a9b64/ ; "anti-roam": https://kevins8.medium.com/dendron-an-open-source-local-first-anti-roam-note-taking-tool-ca1ba65d3058 ; maintenance mode: https://github.com/dendronhq/dendron/discussions/3890
- Foam docs: https://docs.foam.md/
- Notion subpages: https://www.notion.com/help/create-a-subpage
- TiddlyWiki tagging: https://tiddlywiki.com/static/Tagging.html ; TOC macros: https://tiddlywiki.com/static/Table-of-Contents%2520Macros.html
- Luhmann, *Communicating with Slip Boxes* (tr. Kuehn): https://luhmann.surge.sh/communicating-with-slip-boxes ; zettelkasten.de: https://zettelkasten.de/posts/luhmann-folgezettel-truth/ , https://zettelkasten.de/posts/overview/ , https://zettelkasten.de/posts/no-categories/ ; Folgezettel examples: https://forum.zettelkasten.de/discussion/3425/visualizing-luhmanns-folgezettel-in-logseq
- Johnny.Decimal: https://johnnydecimal.com/documentation/the-numbers , https://johnnydecimal.com/documentation/saving-files , JDex: https://johnnydecimal.com/10-19-concepts/11-core/11.05-the-jdex/ , forum on sub-folders: https://forum.johnnydecimal.com/t/breaking-the-rules-exceptions-allowed-subfolders-what-do-other-people-do/1618
- PARA: https://fortelabs.com/blog/para/

Docs sites:
- MkDocs: https://www.mkdocs.org/user-guide/writing-your-docs/
- Docusaurus: https://docusaurus.io/docs/sidebar/autogenerated , https://docusaurus.io/docs/sidebar/items

Retrieval evidence:
- Bergman, Whittaker, Sanderson, Nachmias, Ramamoorthy (2010), JASIST 61(12), https://doi.org/10.1002/asi.21415 ; full text: https://www.tau.ac.il/education/muse/publications/101.pdf
- Larson & Czerwinski (CHI 1998): https://www.microsoft.com/en-us/research/publication/web-page-design-implications-memory-structure-scent-information-retrieval/
- LATTICE, LLM-guided Hierarchical Retrieval: https://arxiv.org/abs/2510.13217

Agent context files:
- Agent Skills best practices: https://platform.claude.com/docs/en/agents-and-tools/agent-skills/best-practices
- Effective context engineering for AI agents: https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents
- Claude Code memory: https://code.claude.com/docs/en/memory
- AGENTS.md: https://agents.md/
- llms.txt: https://llmstxt.org/
