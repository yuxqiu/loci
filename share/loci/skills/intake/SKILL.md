---
name: intake
description: Process the inbox — dissolve drafts into concept notes, file journal entries and sources. Use when the user asks to process the inbox or drops material in it.
---

# Intake

## Before you start

Run `loci status`. If `inbox/` has uncommitted files, stop and ask the user to
commit them: a dissolved draft must be in git so it can be recovered. A concept's
origin is found with `git log -p -- <concept path>`, not by citing the draft.

## Drafts (`inbox/drafts/`)

For each draft:

1. Read it fully. List the concepts it explains, the results it reports (dated,
   may go stale) and the open questions it raises.
2. For each concept, find whether it already exists (`loci digest`, `loci find`,
   aliases). Merge new material into the existing note; create a note with
   `loci new` only when the idea has its own definition and other notes will
   need to link to it on its own. Otherwise make it a heading in its parent
   concept and add its name to the parent's `aliases`.
3. Write in the user's style (`STYLE.md`). Keep every formula, image and code block
   the draft contains, moving images into the concept area's `attachments/` and
   embedding them by vault path.
4. Results go into a journal entry (`journal/<stream>/YYYY/MM/YY-MM-DD-<slug>.md`);
   open questions go into the concept's `## Open questions` or that journal entry.
5. If the draft is mainly an ordered arrangement of existing concepts (a reading
   path, an outline), keep that arrangement as a view in `views/`.
6. Delete the draft.

## Journal entries (`inbox/journal/<stream>/`)

Move each file to `journal/<stream>/YYYY/MM/` with `loci mv`, renaming it to the
journal pattern. Extract concepts from it as for drafts, but keep the entry. Journal
entries have no frontmatter; add concept links in the body only.

## Sources (`inbox/sources/`)

1. Choose the type (`papers`, `docs`, `clippings`) and a slug (a citekey for
   papers), then `loci fetch inbox/sources/<file> <type>/<slug>`.
2. For an annotated PDF, the annotations are the user's: process highlights and
   comments like a draft. If the paper is already filed, keep the filed copy,
   process only the new annotations, and delete the new copy.
3. Extract concepts, citing `sources/<type>/<slug>#p<N>` (PDF page) or
   `#<heading>` (markdown).

## Finish

Run `loci lint`. Fix every error you introduced, and check that no
`material-dropped` warning reports material you lost. Then summarise for the user
what went where.
