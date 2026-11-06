# Changelog

Changes that users and agents notice: commands, lint rules, vault layout, skills
and framework rules. Each change adds a line under "Unreleased"; a release moves
them under its version.

## Unreleased

### Added

- loci builds and is tested on Windows; a plugin there is `loci-<name>.exe` on
  `PATH`. CI checks Linux, macOS and Windows.
- `path.name-clash` (warning): a note `x.md` in a folder beside a file `x`, which
  `[[x]]` names.
- `loci lint --fix`: applies the fixes that keep the meaning (a missing id, a
  markdown link to a vault file, an unquoted prereq, a byte-order mark), checks
  that nothing got worse, then writes. `--unsafe-fixes` adds likely fixes (a
  misspelt slug, duplicate ids), `--only <code>` keeps one code, `--dry-run` writes
  nothing. A name not in NFC is renamed with
  `loci mv`, which its diagnostic's hint gives. Like `cargo fix`, it refuses a working
  tree with uncommitted changes unless given `--allow-dirty`. `lint --json` shows each diagnostic's
  fixes as exact edits.
- `update.<id>` warnings: what a waiting vault update changes (u0001's `digest:`),
  with its fix, which `loci lint --fix --unsafe-fixes --only update.<id>` applies.
  Once the update is handled, what is left is an ordinary lint error.
- `loci mv --dry-run` and `loci merge --dry-run` report the change without writing.
- `loci links <note>` (or any vault file by its path): a note's links and the links
  pointing at it.
- `loci done`: agents finish every task with it. It lints, then records the agent's
  result as a local git ref (`refs/loci/agent/<commit>`), so the `improve` skill
  can see exactly what you changed afterwards. `loci status` counts the records.
- The `improve` skill: reviews how loci is working for you and proposes changes
  to your vault rules, your workflows, or loci itself, tested where cheap.
- Framework rules: how to keep `RULES.md` lessons short and current, and when to
  offer an issue draft for loci itself.
- `prereqs.unread` (warning, `loci closure`): a concept whose prerequisites cannot
  all be read (broken frontmatter, or an item that is no `"[[slug]]"`), so the
  closure may miss some.

### Changed

- On Windows, a path argument starting at a root or drive (`/etc/passwd`, `C:x`) is
  `path.outside`; it was read as a vault path (`etc/passwd`).
- A plugin exiting with a code outside 0–255 (possible on Windows, as when it
  aborts) is `plugin.failed`, instead of a wrapped exit code (possibly 0).
- On Windows, paths in messages and `LOCI_VAULT` have no `\\?\` prefix.
- Files loci creates in the vault (a default `loci export`, its images) get the
  usual mode (0666 less the umask), not owner-only 0600.
- `loci export` names images that differ only in letter case apart (`Graph.png`,
  `graph-1.png`), so they stay two files on macOS and Windows.
- `loci init --vault <dir>` creates `<dir>` when it does not exist, instead of failing
  in `git init`.
- `loci lint --fix` also fixes a lone prereq outside a list (`prereqs: [[x]]` or
  `prereqs: "[[x]]"`), rewriting it as `["[[x]]"]`.
- `link.dangling-block` says when the target holds the id but it is no block id
  (`^step-1`: ids are letters, digits and `_`), instead of "the block was removed".
- `loci merge` no longer leaves a note with the same prereq twice when it named both
  concepts as prerequisites.
- `loci export` no longer uses an embed's size (`![[x.pdf|300]]`) as the text of a
  link it writes; the file name or note title stands instead.
- `loci learn state` and `loci learn review` no longer crash when a very low
  `desired_retention` gives a card an interval of tens of thousands of years; the
  card is due at the last representable time.
- Framework rules: "Answering questions": an answer from the vault names the note
  behind each claim, keeps outside knowledge under "Outside the vault", and says
  what the vault does not cover.
- The `concepts` skill: a contradiction keeps both claims, says what would settle
  it, and is never settled by recency; two notes that disagree on a fact are shown
  to the user instead of proposed as a merge. The `intake` skill processes inbox
  items one at a time, never in parallel subagents.
- `loci closure --depth N` orders the concepts at depth N after their prerequisites
  and reports a cycle through them (`prereqs.cycle`); they were listed as if they had
  no prerequisites.
- A `loci` command whose stdout or stderr is closed early (`loci lint 2>&1 | head`)
  keeps its exit code instead of crashing; a plugin killed by a signal gets a `plugin.failed` error
  (with `--json`, an envelope) instead of no output.
- `loci bench` gives each copy of the vault a repository of its own, also when the
  vault is a linked worktree or a submodule, whose agent commits used to land in the
  vault's repository.
- `loci bench` shows the agent's stderr and reports each case's `agent_exit` (`null`
  on timeout or a signal), so an agent that fails to start is visible.
- `loci done` warns (`done.record`) in a repository with no commit yet, where it
  cannot record the work or its `--note`s.
- A vault whose `HEAD` holds a name that is not UTF-8 or has a control character is
  refused (`file.unreadable`) until the fix is committed, also when the file is
  deleted and the deletion staged; such a change used to be left out of `loci
  status` and of `lint --fix`'s dirty-tree check. Such names are shown escaped
  (`"n\xe9.md"`).
- `loci learn` I/O errors name the file or folder that failed. A `learn` command whose
  `git add` fails (an ignored log) leaves nothing staged.
- `loci export` copies images to `<stem>.assets/` beside the output (was `assets/`),
  so two exports written with `-o` to one folder no longer overwrite each other's
  images. An `-o` path whose file name is not UTF-8 is `io`.
- Unlinked mentions (`loci links`) fold case and find word edges beyond ASCII: `sum`
  is no longer found in `résumé`, and `École` matches `école`. Chinese, Japanese,
  Korean, Thai, Lao, Khmer and Myanmar names still match inside running text, now
  also next to a digit.
- `learn.config` names `plugins/learn/config.toml` and, for a TOML error, the line.
- `loci learn review` and `log` refuse while two cards share an id
  (`learn.id-duplicate`); review asked both and logged two grades for one id.
- `loci export` percent-encodes an image's file name in its link, so a name with
  spaces, brackets, `%` or `?` stays an image of that file.
- A vault with a sparse index opens; git's hints are no longer read as warnings that
  refuse it.
- `loci new --title` refuses a blank title or one with a line break (`new.title`), and
  writes a closing ` #` after a title ending in `#`, so the H1 reads back whole.
  `loci export` keeps a multi-line setext heading on one line in a note's inserted
  title, in a link label inside it, and when the text ends in `#` (also after a link
  at a heading's end becomes text); a link target, file path or image name written as
  text is escaped, so `a_b_c` stays literal; `block-ids
  --section` and the learn plugin's `## Cards` find a multi-line H2 by its one-line
  text. An io error from a `loci` command names the file or folder that failed, and a deleted working
  directory is an `io` refusal, not a crash.
- Block ids: a `^id` inside a block quote or a footnote is text, not a block id (as in
  Obsidian); a link to one is `link.dangling-block`. In a tight list item, the text
  after a nested block can end in a block id, as in a loose one. A task box (`- [ ] `)
  is part of the item's marker, not its text (a card `- [ ] Q :: A` asks `Q`), and an
  item or heading that starts with an escaped character (`\#`) keeps its backslash.
- A note argument that names no note is `note.unknown`, and a note of the wrong kind
  `note.kind`, in every command (replacing `closure.unknown`, `export.unknown`,
  `links.unknown`, `export.kind`, `block-ids.kind`, `merge.not-concept`, and
  `learn.unknown-concept` for `learn state`'s argument).
- Path links name the file at exactly that path, else the note with `.md` added, so
  every file is reachable by its full path (`[[x.png]]` beside a note `x.png.md`); a
  bare link names a concept by its file name, with or without `.md`. Note arguments
  read the same way.
- `link.markdown`'s fix escapes the `|` inside a table, and is offered only when the
  URL names a vault file in a folder and the link text can be a wikilink label;
  `lint --fix` refuses a fix that loses a link.
- Rewritten files keep their permissions, `mv` and `merge` stage every write before
  renaming anything, `loci export` refuses a symlink in `.loci/export/`
  (`file.symlink`), `loci block-ids` refuses ids that would change the blocks they mark
  (`block-ids.changed`), and `loci learn log` starts a new line after a log line that
  lost its line break.
- `lint --json` shows a frontmatter fix as the change it makes (`{"path",
  "frontmatter": {"rename-prereq": {"from": "queu", "to": "queue"}}}`), not as a
  rewritten block, and `lint --fix` applies all of a note's frontmatter fixes at once.
- `loci lint --fix` says when fixes are left after its ten passes (`"unapplied"` in
  `--json`, and advice to run it again), and lists every diagnostic one shared fix
  resolved (such as each unquoted prereq of a note), not only the first.
- When `loci mv`, `loci merge` or `loci lint --fix` changes a concept's frontmatter,
  the whole block is rewritten in a canonical form: block lists
  (`prereqs:\n  - "[[bfs]]"`), comments and layout not kept. A `prereqs` item may be
  any YAML string `[[slug]]` (single quotes and escapes count). `loci links` shows
  a prereq as written (`raw`: `"[[bfs]]"`); lint messages name it by its slug.
- A file name holding a control character (a newline, a tab) refuses the vault
  (`file.unreadable`), so every message names a path on one line; such a path
  argument is `path.control`.
- A file git tracks that has become a FIFO, socket or device refuses the vault
  (`file.unreadable`) instead of being skipped.
- `link.dangling-heading` lists the target note's headings in its hint, so a renamed
  heading can be relinked without opening the note.
- `link.dangling-block` says what likely happened (the block was removed or its id
  changed) and how to relink.
- `loci learn log` refuses a `card` or `concept` that is not a ULID
  (`learn.bad-input`), and a review-log line with such an id is `learn.log`.
- A concept whose file name is not a slug (`a_b.md`, `BFS.md`, already
  `slug.format`) can no longer be named by a bare link or a slug argument; link it
  by path until it is renamed.
- A card's `::` is found as markdown reads the item: a lone backtick is literal text
  (`` what is ` :: x `` is now a card), and `::` inside math does not split it.
- `loci bench` refuses a cases file with no cases (`bench.none`) instead of
  reporting a pass rate of 0.
- Slugs are matched exactly: `[[BFS]]` no longer names `bfs.md` (lint says
  `link.dangling` and suggests `[[bfs]]`); the same for slug arguments and
  `prereqs` items.
- The `--json` diagnostics nest their location: `"location": {"path", "span":
  {"line", "col", "byte_start", "byte_end"}}`, or `null`, instead of five flat
  fields that could be partly `null`.
- The framework rules give one ordered procedure for adding a concept: search under
  every name, file it next to what the search found (checked against the areas'
  scope notes), create it and link first mentions, then link it from the notes that
  mention it, including notes the user wrote.
- `AREA.md`'s paragraph is a scope note: what the area holds and which look-alike
  topics go to a neighbouring area. The agent reads it when filing a concept.
  Concept sub-folders may nest at any depth.
- `loci status` reports only what changes: uncommitted changes, agent work to
  review, and the inbox, counted by folder with at most ten uncommitted files
  named. The framework rules, `RULES.md`, `STYLE.md`, the index and the plugin
  list are no longer printed. The agent reads the rules at session start, as the
  `AGENTS.md` that `loci init` writes says, through the new `loci rules`.
- `loci links` lists **unlinked mentions** both ways: notes that name the note
  (title, parenthesised alias, or file name: a concept's slug) without linking it, and concepts the note
  names without linking them, and marks prerequisites
  `(prereq)`. `loci new` runs `git add -N` on the file it creates and, on a slug
  collision, names the note that has the slug. A dangling `[[slug]]` suggests the
  closest concept. `loci status` ends with the next step. Every argument has a
  `--help` description.
- The `improve` skill records the proposals the user declines and does not offer
  them again without new evidence.
- **Vault updates:** `loci updates` lists the changes a release asks of a vault,
  with the procedure an agent follows to offer them and apply the accepted ones;
  `loci updates --handled <id>` records them in `loci.toml` (`updates`). `loci status`
  says when updates are waiting, and `loci init` starts a vault current.
- Every file name git lists in a vault must be UTF-8: a name that is not, anywhere
  (`plugins/` included) and even when the file was deleted but is still tracked,
  refuses the vault with `file.unreadable` until it is renamed or the deletion
  committed.
- A path argument such as `a//b`, `concepts/./a/x.md` or `concepts/b/../a/x.md`
  is read as `a/b` and `concepts/a/x.md`; only an absolute path or one that climbs
  above the vault root is refused (`path.outside`), and the root itself is
  `path.root`.
- `loci export --to html` no longer needs pandoc: loci renders the page itself,
  with math as MathML and nothing loaded from the network. `--to pdf` is removed
  (print the HTML from a browser), and anchors follow GitHub's rule, so links in a
  markdown export work on GitHub.
- `fm.id-format` also rejects an `id` above the ULID range (a first character past
  `7`), which was accepted before.
- `loci done --note "<text>"` records what the agent found hard in the task, in
  its local record; the `improve` skill reads these notes.
- Every command exits 1 when it reports an error diagnostic, not only `lint`:
  `loci skill` now exits 1 when a plugin's `skills` hook fails.
- Source types and journal streams are any slug you choose; nothing is declared
  in `loci.toml`.
- Journal entries may use any folders and file names inside `journal/<stream>/`.
- loci packages its agent skills but no longer installs them; install them the way
  your agent expects. `loci init` no longer adds `.gitignore` lines for skill
  folders.
- An unsupported `schema` in `loci.toml` is refused with a message naming the
  supported one.
- `vault.not-git` now includes git's own message, so a missing or broken git is no
  longer reported as "not a git work tree" alone. `lint` reports `git.failed`, and
  `loci done` reports `done.record`, when git cannot tell whether there is a commit.
- Files in a merge conflict are no longer ignored by `loci status`, the inbox check
  and `material-dropped`.
- `loci bench` no longer waits forever when the agent leaves a process
  holding its output open; the 600-second timeout applies.
- `plugin.failed` includes what the plugin printed on stderr.
- `loci mv` refuses a destination that breaks a path rule lint checks
  (`sources.layout`, `kind.unknown`, `slug.format`, `journal.path`), with that rule's
  code, and `loci new` a concept path whose slug fails (`slug.format`); a source
  can be renamed but not moved out of `sources/` or given another extension, and a
  file that becomes a concept must have an unused slug.
- A source is one file, `sources/<type>/<slug>.<ext>`, with no folder or
  `source.yaml`, filed with `loci mv` from the inbox or downloaded into place;
  where it came from is not recorded. Lint
  reports a source that differs from its committed version (`sources.changed`).
- Concepts cite evidence with links in their text, next to the claim.
- `loci done` reports a failure to delete old records as `done.prune`.
- A source's extension is lowercase letters and digits with at least one letter;
  `loci mv` keeps a source's extension and refuses to move a source you have edited
  (`mv.source-changed`).
- A permission-only change to a file is not reported as a change, and a stray file
  under `sources/` is reported only by `sources.layout`.
- The vault's files are the ones git sees: a tracked file is read even if
  `.gitignore` matches it, and your global excludes file is respected.
- `loci mv` and `loci new` refuse a destination that, or a folder on whose path,
  differs from an existing name only in letter case (`path.case-twin`), and one git
  ignores (`path.ignored`): a case twin cannot be handled safely on macOS and makes
  twin folders elsewhere. `mv` says to use `git mv -f` for a case-only rename of the
  source, to move the whole folder when it is the source's own, and otherwise to use
  the existing spelling.
- Clearer refusals: `mv` names a destination that would become an area's
  `AREA.md`, and refuses a note losing its `.md` extension or another file gaining
  one.
- Every file loci reads must be readable: a vault holding a `.md` file that is not
  valid UTF-8, a file or folder loci cannot read, or a file whose name is not UTF-8
  is refused by every command with `file.unreadable`, naming each, like a symlink.
  Before, each command skipped such a file and could mistake it for a missing one
  (a duplicate slug through `loci new`, links left unrewritten by `loci mv`).
  A folder git cannot list refuses the vault wherever it is, `plugins/` included; a
  tracked file gone from disk does not.
- When git cannot list the vault (for example a corrupt index), every command,
  `loci status` included, stops with `git` (exit 3) instead of running on an empty
  file list.
- loci does not follow symlinks: a vault that holds one (a file or a folder) is
  refused by every command with `file.symlink`, naming each link, until it is
  replaced with the file or folder itself.
- `loci mv` refuses a directory move that would leave a moved file ignored by git.
- A `loci learn log` or `review` whose commit fails (a hook refuses it) leaves
  nothing staged; `review` says how many grades stay uncommitted.
- `loci learn log`, `state` and `review` refuse a symlink on the way to
  `plugins/learn/`'s files (`file.symlink`), even one git ignores; `loci bench`
  names a symlink in the vault it cannot copy.
- Frontmatter is split off by loci, not read by pulldown-cmark: it runs from a first
  line `---` to the next `---` line (trailing spaces and tabs allowed on both). `...`
  no longer closes it, and a `...` inside it is `fm.yaml`. An empty block has no
  fields (`--fix` can add its id). A later `---`, text, `---` in the body is a rule
  and a heading, as CommonMark reads it, so its links count.
- `loci export -o /` (a path that names no file) is `io`, not a crash.
- A rewritten frontmatter block (`mv`, `merge`, `lint --fix`) keeps the note's
  `\r\n` line breaks instead of mixing in `\n`.
- A `loci mv` or `loci merge` rename that fails partway (a read-only folder) undoes
  the renames made before it; the error names both ends.
- `loci merge` no longer fails when it removes two adjacent prerequisites; `loci mv`
  can rename a file or folder whose name is not NFC.
- A line in `plugins/learn/reviews.jsonl` that does not parse is `learn.log` with
  exit 1 (was exit 3).
- Commands that take a note accept the same path forms everywhere (`./`, a trailing
  `/`, with or without `.md`) and refuse paths outside the vault (`path.outside`).
- A `#Heading` fragment matches a heading ignoring letter case only: a `-` no longer
  stands for a space, so `[[x#hello-world]]` no longer names `## Hello World`.
- A note's headings are only those at the top level of its body: a heading inside a
  block quote, a list item or a footnote is no longer a title, a section or a
  `#Heading` link target (as in Obsidian), and export leaves it as written.
- A multi-line setext heading's text is read on one line (each line ending, with the
  spaces and tabs around it, as one space): `[[x#Title more]]` now names it, lint
  hints list it on one line, and export writes it as one ATX line.
- `loci new` refuses a symlink on the way to the new note (`file.symlink`), even an
  ignored one.
- `loci lint`, `status` and `mv` report a `git status` name with a control character
  (a file whose deletion is staged) as a git error instead of crashing.
- A vault whose folder name ends in a space opens.
- Removing an emptied frontmatter block takes only blank lines (spaces and tabs) after
  it, not lines of other whitespace.
- `loci export` writes an empty HTML comment for a block id whose removal would change
  the note's blocks (a line left as `===` was read as a heading underline, `***` as a
  rule); other ids are still dropped.
- A vault opens when git tracing is on (`GIT_TRACE`, `GIT_TRACE2`, …, or trace2
  targets in git config) or an fsmonitor hook prints messages; that output was read
  as warnings and refused the vault.
- `loci mv` and `loci new` refuse a destination under a file (`mv.exists`,
  `new.exists`); `mv --dry-run` reported it would work, and both failed with an I/O
  error.
- `loci updates --handled` keeps `loci.toml`'s CRLF line breaks, and an invalid
  `loci.toml` error names the file.
- The default `loci export` output is `.loci/export/<note path>/<stem>.<ext>`
  (`.loci/export/views/study.md/study.md`), with its images in that folder's
  `assets/`, so one export no longer overwrites another's images.
- The framework rules say `loci lint --fix` fixes the whole vault, and that
  `--unsafe-fixes --only <code>` applies every unsafe fix of that code.

### Removed

- `loci find`: agents search with `rg`, as the framework rules show (titles, words
  with any separator, several names at once, output size). Every
  production coding agent searches this way, and in published comparisons grep
  beat BM25 and embedding search inside an agent loop
  (`docs/research/16-find-vs-grep.md`). The binary no longer contains tantivy or
  ICU4X, and `THIRD_PARTY_NOTICES` is gone. `loci bench` runs only the agent level:
  `loci bench '<agent command>'` reports the pass rate (no recall@k, MRR or `-k`).
  Each case is now a question or task and a shell check (`{"q", "check"}`) on the
  agent's reply (`LOCI_BENCH_OUTPUT`) or the vault it left, run on a fresh copy of
  the vault; the file is `bench/cases.jsonl` (was `bench/retrieval.jsonl`, with
  `expect`), or `--cases <file>`.
- The concept `digest` frontmatter field, with `digest.multiline`, `digest.has-link`
  and `digest.too-long`; the `loci digest` command, the root and area indexes and
  `index.over-budget`; the `digest`, `root_index` and `area_index` budgets; and the
  glossary of `loci export`; and the `digest` key of `loci closure --json` members.
  Research found no evidence that a one-line summary helps
  beyond the title (`docs/research/15-digests-vs-search.md`). To update a vault:
  delete `digest`, `root_index` and `area_index` from `[budgets]` in `loci.toml`
  (otherwise every command refuses with `config.invalid`), then run `loci updates`:
  the rest (the `digest:` lines, the `AGENTS.md` stub, the benchmark cases) are
  vault updates the agent offers.
- The home-manager module, and the skill checks in `loci status`.
- Lint's `file.encoding`, and `file.unreadable` as a lint rule: a file loci cannot
  read now refuses the vault (`file.unreadable`).
- `purpose`, `grader`, `verdicts` and `item` from `loci learn log` input and
  review-log lines: a line is now the card or concept, the grade and the time.
- `loci fetch`, and with it loci's only network access (the `ureq` dependency).
  File a source with `loci mv` from the inbox, or download it into
  `sources/<type>/<slug>.<ext>` with the agent's own tools.
- The `sources:` frontmatter field and `sources.unresolved`.
- `source.yaml` and SHA-256 checks: `sources.no-provenance`, `sources.bad-meta`,
  `sources.uncovered` and `sources.hash-mismatch`.
- The `aliases` frontmatter field, with `alias.duplicate`, `mv.frontmatter` and
  `merge.frontmatter`. Other names for a concept go in its title or text,
  where a search finds them. `loci mv` and `loci merge` rewrite links only and no
  longer edit the renamed or kept note, so renaming a concept back now works.
