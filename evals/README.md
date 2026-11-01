# Agent evals

Measure how well an agent maintains a loci vault, to decide changes to the framework
rules and skills on evidence. Runs call a real agent: they cost money and vary from
run to run, so they are run on demand, never in CI.

- `vault/make_vault.py`: builds the sample vault (neutral content) with `loci new`.
- `suites/<name>.jsonl`: cases for `loci bench --cases`, one per line,
  `{"q": "<question or task>", "check": "<shell command>"}`. The check runs in the
  agent's copy of the vault after it finishes and passes on exit 0; it reads the
  reply from `$LOCI_BENCH_OUTPUT` or inspects the vault (m1-m4 §3).
- `arms/make_arms.py`: rule variants to compare, each a function of
  `share/loci/rules.md`, served by a `loci` shim for `loci rules`.
- `agent.sh`: the agent, Claude Code headless, without the user's own settings.
- `run.sh <suite> <model>...`: every arm × model × `REPS` (default 3), into
  `results/<date>-<suite>/`, then `summarize.py`.
- `results/<date>-<suite>/README.md`: what was asked, the setup, the decision rule
  set before running, and the outcome.

Example: `REPS=3 evals/run.sh finding sonnet haiku`.
