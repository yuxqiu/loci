# Changelog

Changes that users and agents notice: commands, lint rules, vault layout, skills
and framework rules. Each change adds a line under "Unreleased"; a release moves
them under its version.

## Unreleased

### Added

- `loci links <note>`: a note's links and the links pointing at it.
- `loci done`: agents finish every task with it. It lints, then records the agent's
  result as a local git ref (`refs/loci/agent/<commit>`), so the `improve` skill
  can see exactly what you changed afterwards. `loci status` counts the records.
- The `improve` skill: reviews how loci is working for you and proposes changes
  to your vault rules, your workflows, or loci itself, tested where cheap.
- Framework rules: how to keep `RULES.md` lessons short and current, and when to
  offer an issue draft for loci itself.

### Changed

- The `--json` diagnostics nest their location: `"location": {"path", "span":
  {"line", "col", "byte_start", "byte_end"}}`, or `null`, instead of five flat
  fields that could be partly `null`.
- The framework rules say when to link: search before writing a concept, link the
  first mention of every concept that has a note, and link a new concept from the
  notes that mention it.
- The root digest outlines each area's sub-folders with their concept counts
  (`Sub-folders: graphs (6), strings (3)`; JSON `subfolders`). `AREA.md`'s paragraph
  is a scope note: what the area holds and which look-alike topics go to a
  neighbouring area. Concept sub-folders may nest at any depth.
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
- `loci find` reports `search.index` when it cannot build its index.
- `vault.not-git` now includes git's own message, so a missing or broken git is no
  longer reported as "not a git work tree" alone. `lint` reports `git.failed`, and
  `loci done` reports `done.record`, when git cannot tell whether there is a commit.
- Files in a merge conflict are no longer ignored by `loci status`, the inbox check
  and `material-dropped`.
- `loci bench --agent` no longer waits forever when the agent leaves a process
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
- `loci find` searches the formed notes (concepts, areas, views, journal) by default;
  sources, inbox drafts, rule files and markdown outside the known folders only with
  `--kind` (`source`, `inbox`, `rules`, `unknown`).
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
- `loci merge` no longer fails when it removes two adjacent prerequisites; `loci mv`
  can rename a file or folder whose name is not NFC; `loci bench` matches slugs
  case-insensitively.
- A line in `plugins/learn/reviews.jsonl` that does not parse is `learn.log` with
  exit 1 (was exit 3).
- Commands that take a note accept the same path forms everywhere (`./`, a trailing
  `/`, with or without `.md`) and refuse paths outside the vault (`path.outside`).

### Removed

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
  `merge.frontmatter`. Other names for a concept go in its title, digest or text,
  which `loci find` searches. `loci mv` and `loci merge` rewrite links only and no
  longer edit the renamed or kept note, so renaming a concept back now works.
