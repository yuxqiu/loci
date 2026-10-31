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

### Removed

- The home-manager module, and the skill checks in `loci status`.
