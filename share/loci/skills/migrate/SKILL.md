---
name: migrate
description: Bring notes from another notes system (a folder of markdown, an Obsidian or Logseq vault, exported notes) into this vault, one batch at a time. Use when the user asks to migrate or import existing notes.
---

# Migrate

If you have not read them this session, read the framework rules (`loci rules`),
`RULES.md` and `STYLE.md` first.

The old notes stay where they are and are never modified. Migration copies one
batch at a time into the inbox and processes it with the normal `intake` skill.

## 1. Survey the old notes

Before copying anything, look at how the collection is organised, without reading
every note:

- folders and how many files each holds;
- file formats: markdown, plain text, PDFs, images, exports (HTML, JSON), other;
- conventions: link syntax (`[[wikilinks]]`, markdown links, block references),
  tags, frontmatter keys, dated file names, attachment folders, templates;
- the kinds of material, named the way the user names them:
  - notes that explain ideas;
  - dated records (logs, diaries, meeting notes);
  - material written by others (papers, articles, clippings);
  - pages that only arrange other notes (indexes, reading lists, outlines).

Report this to the user in a few lines and propose batches. A batch is a set of
files about one subject, small enough to review in one sitting (start with 10–30
notes).

## 2. Agree on the mapping

Agree with the user, once, where each kind goes:

- notes that explain ideas, and pages that arrange other notes → `inbox/drafts/`
  (intake turns arrangements into views);
- dated records → `inbox/journal/<stream>/`, one stream per kind of record, named
  with the user;
- material written by others → `inbox/sources/`;
- templates, configuration and empty notes → not migrated, unless the user says
  otherwise.

Reuse the mapping for every later batch.

## 3. Copy a batch

Copy the batch's files and the attachments they embed into the agreed inbox
folders, keeping each image next to the draft that embeds it. Do not edit files
while copying. Ask the user to commit.

## 4. Process it

Follow the `intake` skill, with these additions:

- Rewrite the old link syntax as vault links. A link to a note that is not migrated
  yet becomes plain text; list it in the summary. Never link into the old notes.
- Do not carry tags over: a tag becomes the area a concept lives in, a
  name in the concept's text, or a link to the concept it names.
- Keep dated findings dated: they go to a journal entry, not into a concept.

## 5. Review and continue

Summarise what went where, which links were left as plain text, and which files
were skipped. After the first batch, add a few `bench/retrieval.jsonl` cases for
questions the user asks of these notes, and check that the size budgets in
`loci.toml` fit. After the user reviews and commits, take the next batch. When a
later batch brings in a note that earlier summaries listed as a plain-text link,
restore that link.
