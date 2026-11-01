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

- Source types and journal streams are any slug you choose; nothing is declared
  in `loci.toml`.
- Journal entries may use any folders and file names inside `journal/<stream>/`.
- loci packages its agent skills but no longer installs them; install them the way
  your agent expects. `loci init` no longer adds `.gitignore` lines for skill
  folders.
- An unsupported `schema` in `loci.toml` is refused with a message naming the
  supported one.
- `loci search` reports `search.index` when it cannot build its index.
- `vault.not-git` now includes git's own message, so a missing or broken git is no
  longer reported as "not a git work tree" alone. `lint` reports `git.failed`, and
  `loci done` reports `done.record`, when git cannot tell whether there is a commit.
- Files in a merge conflict are no longer ignored by `loci status`, the inbox check
  and `material-dropped`.
- `loci bench --agent` no longer waits forever when the agent leaves a process
  holding its output open; the 600-second timeout applies.
- `plugin.failed` includes what the plugin printed on stderr.
- `loci mv` refuses to move `sources/` other than as a whole source folder, and a
  file that becomes a concept must have a valid, unused slug.
- `loci done` reports a failure to delete old records as `done.prune`.
- A line in `plugins/learn/reviews.jsonl` that does not parse is `learn.log` with
  exit 1 (was exit 3).
- Commands that take a note accept the same path forms everywhere (`./`, a trailing
  `/`, with or without `.md`) and refuse paths outside the vault (`path.outside`).

### Removed

- The home-manager module, and the skill checks in `loci status`.
- The `aliases` frontmatter field, with `alias.duplicate`, `mv.frontmatter` and
  `merge.frontmatter`. Other names for a concept go in its title, digest or text,
  which `loci find` searches. `loci mv` and `loci merge` rewrite links only and no
  longer edit the renamed or kept note, so renaming a concept back now works.
