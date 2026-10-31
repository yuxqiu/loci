# loci

An agent-maintained personal knowledge base. Concept notes are canonical; papers, logs and curated reading paths are sources, journal entries and views. A Rust CLI (`loci`) enforces the invariants, and agent skills do the judgement. It works with coding agents that have a shell; per-agent setup is in `docs/DESIGN.md` §7a.

The name: *loci communes* were the topic headings of a commonplace book; the method of loci is a memory technique.

## Using it

Notes are plain markdown in a git repository; edit them with any editor.

- **Hand edits need no sync.** The index and search are computed from the files
  on each command. Use `loci new` to create a concept, `loci mv` to rename (it
  rewrites links) and `loci lint` to check links. `docs/DESIGN.md` §5.1 has the
  full list.
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