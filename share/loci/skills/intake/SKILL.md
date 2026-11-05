---
name: intake
description: Process the inbox — dissolve drafts into concept notes, file journal entries and sources. Use when the user asks to process the inbox or drops material in it.
---

# Intake

## Before you start

If you have not read them this session, read the framework rules (`loci rules`),
`RULES.md` and `STYLE.md` first. Run `loci status`. If `inbox/` has uncommitted files, stop and ask the user to
commit them: a dissolved draft must be in git so it can be recovered. A concept's
origin is found with `git log -p -- <concept path>`, not by citing the draft.

Process inbox items one at a time. Do not split related items across parallel
subagents: each one's search misses the notes the others are creating, and
they write duplicates.

## Drafts (`inbox/drafts/`)

For each draft:

1. Read it fully. List the concepts it explains, the results it reports (dated,
   may go stale) and the open questions it raises.
2. Add each concept as the framework rules say ("Adding a concept"): new
   material for an existing note goes into that note. Give an idea its own note
   only when it has its own definition and other notes will need to link to it on
   its own; otherwise make it a heading in its parent concept.
3. Write in the user's style (`STYLE.md`). Keep every formula, image and code block
   the draft contains, moving images into the concept area's `attachments/` and
   embedding them by vault path.
4. Results go into a journal entry in `journal/<stream>/` (named as the framework
   rules say);
   open questions go into the concept's `## Open questions` or that journal entry.
5. If the draft is mainly an ordered arrangement of existing concepts (a reading
   path, an outline), keep that arrangement as a view in `views/`.
6. Delete the draft.

## Journal entries (`inbox/journal/<stream>/`)

Move each file into `journal/<stream>/` with `loci mv`, keeping its name unless the
stream's other entries follow a pattern it does not. Extract concepts from it as for drafts, but keep the entry. Journal
entries have no frontmatter; add concept links in the body only.

## Sources (`inbox/sources/`)

1. File it as the framework rules say (`loci rules`, "Filing a source").
2. For an annotated PDF, the annotations are the user's: process highlights and
   comments like a draft. If the paper is already filed, keep the filed copy,
   process only the new annotations, and delete the new copy.
3. Extract concepts, citing next to each claim `[[sources/<type>/<slug>.pdf#p<N>]]`
   (PDF page) or `[[sources/<type>/<slug>#<heading>]]` (markdown).

## Finish

Run `loci done`. Fix every error you introduced (then run it again), and check that
no `material-dropped` warning reports material you lost. Then summarise for the user
what went where.
