# loci

A personal knowledge base that an AI agent helps you maintain. What you know lives in one note per concept; papers, logs and reading paths are kept alongside as sources, journal entries and views. A command-line tool (`loci`) checks the structure, such as links, names and sizes, and agent skills handle the writing and judgement calls. It works with any coding agent that can run shell commands.

The name: *loci communes* were the topic headings of a commonplace book; the method of loci is a memory technique.

## Setup

1. Install `loci` (and the `loci-learn` plugin if you want review cards). With
   Nix, use this flake's packages, or its home-manager module:

   ```nix
   # flake input `loci`; import loci.homeManagerModules.default
   programs.loci = {
     enable = true;
     vaults = [ "notes" ];               # paths relative to $HOME
     plugins = [ loci.packages.${system}.loci-learn ];
     claudeSkills = true;                # also link .claude/skills
   };
   ```

   The module links the agent skills into each vault as `.agents/skills/`.
   Without Nix, put the binaries on `PATH` and copy `share/loci/skills/` into the
   vault as `.agents/skills/`.
2. In the folder for your notes, run `loci init`. It creates the vault, the git repository
   and an `AGENTS.md` that tells agents to run `loci status` first.
3. Point your agent at the vault:

   | Agent | Setup |
   |---|---|
   | Codex, Cursor, Copilot, opencode | none; they read `AGENTS.md` and `.agents/skills/` |
   | Claude Code | link `.claude/skills` (`claudeSkills = true`); it reads `AGENTS.md` only when there is no `CLAUDE.md` |
   | Gemini CLI | set the context file name to `AGENTS.md` |
   | any other agent with a shell | tell it to run `loci status` and `loci skill show` |

## Using it

Notes are plain markdown in a git repository; edit them with any editor.

- **Hand edits need no sync.** The index and search are computed from the files
  on each command, and the agent acts only when asked. Edit anything except
  `sources/` and `plugins/`:

  | Change | What to do |
  |---|---|
  | edit text | nothing; `loci lint` flags broken links |
  | create a concept | start with `loci new concepts/<area>/<slug>.md` (it writes the id) |
  | rename or move a note | use `loci mv`, which rewrites every link to it |
  | combine two notes about the same concept | copy what you want to keep into the note that stays, then run `loci merge <old> <kept>`: it points every link at the kept note, adds the old name to its aliases, and deletes the old note |
  | delete a note | fix the links `loci lint` reports |
  | add review cards | `loci block-ids <note> --section Cards` |
  | write a journal entry | write it in `journal/<stream>/`; ask the agent if you want concepts extracted |
  | change how the agent writes | edit `STYLE.md` or `RULES.md` |

- **Material to process** goes in `inbox/` (`drafts/`, `journal/<stream>/`,
  `sources/`). Commit, then ask the agent to process the inbox; it turns the
  material into concept notes, journal entries and sources.
- **Review everything the agent does** with `git diff`, then `git commit` to keep
  it or `git reset --hard` to discard it. The agent never commits.

## Acknowledgements

Thanks to my friend [chenyue42](https://github.com/chenyue42), whose project carrel inspired much of this design.

## License

MIT. See `LICENSE`. The binary includes ICU4X word segmentation code and data;
its notices are in `THIRD_PARTY_NOTICES`.
