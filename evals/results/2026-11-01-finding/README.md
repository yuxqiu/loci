# 2026-11-01: finding suite, three rule variants

- **Question:** do the framework rules' "Finding things" patterns (D166, D167) help an
  agent find notes, file a concept and follow links, compared with a bare "search
  with `rg`"?
- **Setup:** the sample vault (`evals/vault/make_vault.py`, 39 concepts), the 15
  cases of `suites/finding.jsonl` (4 known-item, 4 vocabulary-gap, 4 related, 3
  filing), the arms `bare`, `patterns` and `current` (`arms/make_arms.py`), Claude Code
  headless (`agent.sh`) with Sonnet and Haiku, 3 repeats: 270 runs, about $10.
- **Run with:** the scratchpad harness that became `evals/`, before D169; the vault
  then had a `CLAUDE.md` saying "Follow `AGENTS.md`." (no longer needed: Claude Code
  reads `AGENTS.md` by itself). Cases then
  used `expect` lists scored by the built-in matcher; `suites/finding.jsonl` holds
  the same cases as checks, which score the same replies the same way.
- **Decision rule (set before running):** adopt a variant if it beats the others on
  some kind of case by more than run-to-run noise.

| model | bare | patterns | current |
|---|---|---|---|
| Haiku | 45/45 | 45/45 | 45/45 |
| Sonnet | 45/45 | 45/45 | 44/45 |

**Result: no difference; the suite is at its ceiling.** At 39 notes an agent can
list every file and read the titles, so the rules do not matter; even the bare arm
with the small model passed everything. The one miss (Sonnet, `current`, case 4) is
noise. **Next:** a vault of thousands of notes, where listing everything stops
working, and harder cases: absent notes, exhaustive linking, intake tasks checked on
the vault the agent leaves.
