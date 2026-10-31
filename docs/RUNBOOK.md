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
| All flake checks (packages with their tests, formatting) | `nix flake check` |

New source files must be known to git (`git add -N <file>`) before Nix can see
them.

Integration tests run the built binaries against fixture vaults in
`tests/fixtures/`, with `PATH` holding only `git` (and `loci-stub` in the plugin
tests).
