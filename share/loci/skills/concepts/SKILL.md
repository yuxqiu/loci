---
name: concepts
description: Write, revise, split, merge and organise concept notes and areas. Use when editing knowledge, answering "explain this note", or making a view.
---

# Concepts

## A concept note

```markdown
---
id: <assigned by loci new>
prereqs: ["[[prerequisite-slug]]"]
digest: One sentence that summarises the concept.
---
# Title

Explanation, linking related concepts by bare slug and citing evidence next to the
claim it supports ([[sources/papers/<key>.pdf#p3]]).

## Example: <what it shows>
## Open questions
```

- `prereqs` lists direct prerequisites only, each quoted.
- `digest` is one sentence with no links; it is what `loci digest` shows.
- Other names (an abbreviation, a spelling, a translation) go in the title, the
  digest or the text, where `loci find` finds them. There is no alias field.
- One note per concept. An idea gets its own note once it has its own definition
  and other notes need to link to it on its own (`loci links <note>` lists what
  links to a note); otherwise it is a heading in its parent.

## Revising

- New material that contradicts a concept: decide whether it is a duplicate, a
  refinement or a contradiction, and make the change with both sources cited.
- Never cite another concept as evidence; cite the sources it cites.
- Possible duplicates: tell the user and propose a merge (`loci links` on both notes
  shows what the merge will touch); do not merge on your own.

## Areas

Every area folder has `AREA.md`: an H1 (the area title) and exactly one paragraph,
a scope note:

- what kinds of concept the area holds;
- where a neighbouring area could be mistaken for it, which topics go there
  instead, linked as `[[concepts/<other>/AREA]]`; add the reverse note to the other
  area's `AREA.md`;
- optionally, in a few words, key topics that have no sub-folder.

Do not list sub-folders (`loci digest` outlines them), summarise concepts or record
history there.

Group related concepts in sub-folders of an area (`concepts/<area>/<group>/`),
usually one level deep. Sub-folders have no `AREA.md`; if one needs its own scope
note, make it an area. Restructure with `loci mv` when an area grows too mixed;
links are rewritten for you.

## Views

A curated view is a file in `views/`: an ordered list of concept links with
connecting text (a reading path, a project overview, a learning plan). Build
one from `loci digest`, `loci find` and the concepts' `prereqs`.
