# Runbook

Development commands, run from the repository root. `nix develop` provides the
toolchain (cargo, clippy, rustfmt, cargo-insta).

| Task | Command |
|---|---|
| Unit and integration tests | `cargo test --workspace` |
| Lints | `cargo clippy --workspace --all-targets` |
| Format (config: `rustfmt.toml`) | `cargo fmt` (check: `cargo fmt --check`) |
| Review changed snapshots | `cargo insta review` |
| Build the packages | `nix build .#loci .#loci-learn` |
| All flake checks (packages with their tests, `loci-core`'s included; formatting) | `nix flake check` |
| Code-quality pass (review, verify, apply, check, record) | the `quality-pass` skill, `.claude/skills/quality-pass/SKILL.md` |
| Function-length stats | `python3 .claude/skills/quality-pass/fnlen.py` |
| Agent evals (calls a real agent; costs money; not in CI) | `REPS=3 evals/run.sh <suite> <model>...`, e.g. `evals/run.sh finding sonnet haiku` |

New source files must be known to git (`git add -N <file>`) before Nix can see
them.

Test helpers shared by both crates' integration tests are in `crates/loci-testkit`.
Integration tests run the built binaries against fixture vaults in
`tests/fixtures/`, with `PATH` holding only `git`, plus `loci-stub` in the plugin
tests and the tools a bench case needs (`sh`, `grep`, `head`) in the bench tests.

A change that asks something of existing vaults (a stub, a removed field, a new
convention) also gets an entry at the bottom of `share/loci/updates.md` (Offer, How,
Done when; the next id; today's date). Entries are never edited or deleted.

Every change that users or agents notice (commands, lint rules, vault layout,
skills, framework rules) adds a line to `CHANGELOG.md` under "Unreleased".
