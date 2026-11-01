# loci

A personal knowledge base that an AI agent helps you maintain. What you know lives in one note per concept; papers, logs and reading paths are kept alongside as sources, journal entries and views. A command-line tool (`loci`) checks the structure, such as links, names and sizes, and agent skills handle the writing and judgement calls. It works with any coding agent that can run shell commands.

The name: *loci communes* were the topic headings of a commonplace book; the method of loci is a memory technique.

## Setup

1. Install `loci`, and the `loci-learn` plugin if you want review cards. This
   repository is a Nix flake with packages `loci` and `loci-learn`; without Nix,
   build them with `cargo install --path crates/loci` (and `crates/loci-learn`).
2. Install the agent skills the way your agent expects. They are the folders in
   `share/loci/skills/` (and `crates/loci-learn/skills/` for the plugin), also
   installed by the Nix packages under `share/loci/skills/`. An agent without them
   can read them with `loci skill show`.
3. In the folder for your notes, run `loci init`. It creates the vault, the git
   repository and an `AGENTS.md` that tells agents to run `loci status` first.
   Agents read `AGENTS.md` as follows:

   | Agent | Reads `AGENTS.md` |
   |---|---|
   | Codex, Cursor, Copilot, opencode | yes |
   | Claude Code | only when there is no `CLAUDE.md` |
   | Gemini CLI | once its context file name is set to `AGENTS.md` |
   | any other agent with a shell | tell it to run `loci status` first |

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
  | combine two notes about the same concept | copy what you want to keep into the note that stays, then run `loci merge <old> <kept>`: it points every link at the kept note and deletes the old note |
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
