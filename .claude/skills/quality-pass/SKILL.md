---
name: quality-pass
description: Run a code-quality pass on loci (review, verify, apply, check, record). Use when asked to review the codebase for smells or bad design, make it more readable or robust, shrink it, or drop unneeded rules and dirty tricks.
---

# Quality pass

A pass makes loci easier to understand and more robust while making it smaller. It ends when the
re-review finds nothing worth applying and the checks pass. It does not end when the code merely looks better.

## 1. Measure the baseline

Record these before any edit, so the result is measured, not asserted:

```sh
python3 .claude/skills/quality-pass/fnlen.py        # functions, over 40/60 lines, longest, mean
git ls-files -co --exclude-standard 'crates/*/src/*.rs' 'crates/*/src/**/*.rs' | xargs cat | wc -l
```

Runtime invariant checks in production code (test modules excluded):

```sh
for f in $(git ls-files -co --exclude-standard 'crates/*/src/*.rs' 'crates/*/src/**/*.rs'); do
  awk '/#\[cfg\(test\)\]/{exit} {print}' "$f"; done | grep -cE '\.expect\(|panic!\(|unreachable!\('
```

Compare only counts taken with these exact commands.

## 2. Review

Spawn reviewer agents (read-only, in the background), one per lens, each briefed with the scope and the
rules below. For a small scope, one agent covering all lenses is enough.

- **Design and boundaries:** internals exposed across modules or crates (pub fields, raw paths or
  strings that should be types); free functions that belong as methods; an Option, bool or String
  standing in for an enum; one concept modelled twice; lookups repeated after a type could carry the result.
- **Shrink:** dead or single-use code, speculative flexibility, rules and special cases that can go,
  hand-rolled code where std or an existing dependency does it, duplicate tests, dirty tricks
  (re-slicing text to recover what the parser knew, hand-counted offsets, magic constants).
- **Robustness:** silent fallbacks and skips, unchecked preconditions, check-then-act file races, writes
  that follow symlinks, `expect`s a type could make impossible, errors that lose what failed, edge cases
  (empty input, no frontmatter, CRLF, BOM, non-UTF-8 names). Prove each with a concrete input.
- **Readability:** names narrower or wider than the thing, comments the code no longer matches, control
  flow a reader must simulate, state spread over variables that one type would hold.
- **Code versus docs:** `docs/specs/*.md`, `docs/RUNBOOK.md` and recent `docs/DECISIONS.md` entries
  against the code; stale module docs and refusal-code lists; decisions naming functions that no longer exist.

Rules for reviewers:
- Every finding cites a file:line the reviewer actually read, and gives a concrete fix.
- Length alone is not a defect. Do not propose splitting a function unless its parts are separate ideas
  and the split reads better.
- Search `docs/DECISIONS.md` before proposing to undo something, and cite the D-number.
- Findings hold no vault content and use neutral examples (D53, D62).

## 3. Verify each finding yourself

Open the cited code before acting. Each finding gets one of these outcomes:
- **Apply:** the claim holds and the fix is simpler or more correct.
- **Ask the user:** the fix changes user-visible behaviour, or undoes a recorded decision.
  The user's decision outranks a reviewer's finding.
- **Decline with a reason:** the claim is wrong, already handled, or the fix is not an improvement.
  Recheck the reason before reporting it; a wrong decline reason is a defect too.

## 4. Apply

- One finding at a time. Run the tests after each group of related edits.
- A bug fix gets a test that fails without it.
- A behaviour change also updates the spec line that describes it.
- Splits: keep a function whole when its steps belong together; extract only a separate idea.
- When a change makes something unused, remove it. Leave unrelated dead code alone and mention it instead.
- Propose any new abstraction (a type, a trait, a constructor) in two or three sentences before building it.
- Do not stage or commit. Run `git add -N` on new files only, so the flake sees them.

## 5. Check

All of these must pass before reporting:

```sh
cargo fmt && cargo clippy -q --all-targets -- -D warnings
cargo test -q
RUSTDOCFLAGS="-D warnings" cargo doc -q --no-deps --workspace
nix flake check
```

When the parser changed, run the parse-span fuzz at 30k cases. Temporarily set `with_cases(30000)` in
`crates/loci-core/tests/parse_spans.rs`, then run `cargo test --release -p loci-core --test parse_spans`,
then restore the 2000. `PROPTEST_CASES` does not override it. To confirm the larger count ran, check that
the test recompiled and that its time grew.

## 6. Re-review and record

- Re-run step 2 on the changed code. Stop when it finds nothing worth applying. Do not stop because
  findings were left unexamined; every finding gets an outcome from step 3.
- Measure again (step 1) and report before and after.
- Append a D-entry to `docs/DECISIONS.md`. It is append-only: supersede an old entry, never edit it.
  The entry covers what changed, what was declined and why, and the numbers.
