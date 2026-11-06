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
| All flake checks (packages with their tests, `loci-core`'s included; Clippy with warnings denied; formatting; on x86_64-linux also `windows`: Clippy for `x86_64-pc-windows-gnu` and the unit tests under wine, `nix/windows.nix`) | `nix flake check` (test output: `nix log .#checks.<system>.<name>`) |
| CI (GitHub Actions, `.github/workflows/ci.yml`) | `nix flake check` on x86_64-linux, aarch64-linux and aarch64-darwin; `cargo clippy -D warnings` (all targets) and `cargo test` on Windows. |
| Windows build, lints and unit tests from Linux (MinGW, wine; no rustup) | `nix develop .#windows`, then `cargo clippy --target x86_64-pc-windows-gnu --workspace --all-targets -- -D warnings` or `cargo test --target x86_64-pc-windows-gnu --workspace --lib --bins` (integration tests need Git for Windows: Windows CI only) |
| Code-quality pass (review, verify, apply, check, record) | the `quality-pass` skill, `.claude/skills/quality-pass/SKILL.md` |
| Function-length stats | `python3 .claude/skills/quality-pass/fnlen.py` |
| Agent evals (calls a real agent; costs money; not in CI) | `REPS=3 evals/run.sh <suite> <model>...`, e.g. `evals/run.sh finding sonnet haiku` |

New source files must be known to git (`git add -N <file>`) before Nix can see
them.

Test helpers shared by both crates' integration tests are in `crates/loci-testkit`.
Integration tests run the built binaries against fixture vaults in
`tests/fixtures/`, with `PATH` holding only `git`, plus `loci-stub` in the plugin
tests and the tools a bench case needs (`sh`, `grep`, `head`) in the bench tests.
OS differences go in `platform.rs` (`loci-core`, `loci-testkit`), never as `cfg` in a
test file; a test of what only Unix has goes in `crates/loci/tests/unix.rs` (core's
unit tests: `platform.rs`'s `unix_tests`; loci-learn's one test stays in `learn.rs`). On
Windows, symlink tests need Developer Mode (CI's runners have it).

A change that asks something of existing vaults (a stub, a removed field, a new
convention) also gets an entry at the bottom of `share/loci/updates.md` (Offer, How,
Done when; the next id; today's date). Entries are never edited or deleted.

Every change that users or agents notice (commands, lint rules, vault layout,
skills, framework rules) adds a line to `CHANGELOG.md` under "Unreleased".
