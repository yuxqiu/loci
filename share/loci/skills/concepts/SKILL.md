---
name: concepts
description: Write, revise, split, merge and organise concept notes and areas. Use when editing knowledge, answering "explain this note", or making a view.
---

# Concepts

## A concept note

```markdown
---
id: <assigned by loci new>
aliases: [other-name]
prereqs: ["[[prerequisite-slug]]"]
sources:
  - "sources/papers/<key>#p3"
digest: One sentence that summarises the concept.
---
# Title

Explanation, linking related concepts by bare slug.

## Example: <what it shows>
## Open questions
```

- `prereqs` lists direct prerequisites only, each quoted.
- `digest` is one sentence with no links; it is what `loci digest` shows.
- One note per concept. An idea gets its own note once it has its own definition
  and other notes need to link to it on its own; otherwise it is a heading in its
  parent, with its name in the parent's `aliases`.

## Revising

- New material that contradicts a concept: decide whether it is a duplicate, a
  refinement or a contradiction, and make the change with both sources cited.
- Never cite another concept as evidence; cite the sources it cites.
- Possible duplicates: tell the user and propose a merge; do not merge on your own.

## Areas

Every area folder has `AREA.md`: an H1 (the area title) and exactly one paragraph
saying what belongs there. Restructure areas with `loci mv` when an area grows too
mixed; links are rewritten for you.

## Views

A curated view is a file in `views/`: an ordered list of concept links with
connecting text (a reading path, a project overview, a learning plan). Build
one from `loci digest`, `loci find` and the concepts' `prereqs`.
