---
name: improve
description: Review how loci is working for the user and propose improvements to their vault rules, their workflows, or loci itself, each tested where cheap and adopted only with the user's approval. Use when the user asks to review or improve how loci works for them.
---

# Improve

If you have not read them this session, read the framework rules (`loci rules`),
`RULES.md` and `STYLE.md` first.

Find gaps from evidence, propose changes at the right level, test them where that
is cheap, and leave every decision to the user. Change nothing outside the vault.

## 1. Gather evidence

Look at a recent stretch of work (ask the user how far back; default: the last
month of commits):

- **Corrections.** Each `loci done` left a ref `refs/loci/agent/<base>`, where
  `<base>` is the commit the agent started from (`git for-each-ref refs/loci/agent/`).
  For each ref whose base is an ancestor of the current branch:
  - what the agent did: `git diff <base> <ref>`;
  - what the user kept: the files the agent changed (`git diff --name-only <base>
    <ref>`), compared from the ref to the end of the review window, which runs from
    the base to the next recorded base on the branch, or to `HEAD`:
    `git diff <ref> <window end> -- <files>`. No agent change surviving means the
    task was discarded;
  - later fixes: later commits that change the lines the agent wrote.

  Skip refs whose base is not an ancestor (rewritten history). Show each correction
  with its diff, and ask before learning from one: an unrelated edit to the same
  file is not a correction.
- **Lessons.** The `## Lessons` section of `RULES.md`, and `STYLE.md`. A lesson
  that keeps being restated or broken is a gap.
- **Repeated work.** Steps done by hand again and again: the same searches, edits
  or checks that a command, lint rule or view could do.
- **Health.** `loci lint` warnings that are always present, and `loci bench` results
  if `bench/retrieval.jsonl` has cases.

List each finding with its evidence (commits, lines). Drop findings with a single
occurrence unless the user points to them.

## 2. Propose changes at the right level

For each finding, propose the strongest fix that fits, at the level that owns it:

| Level | Fixes |
|---|---|
| the user's vault | a `bench/retrieval.jsonl` case, a `loci.toml` budget, a `RULES.md` lesson or a `STYLE.md` line (following the framework rules on lessons), a view, an area change |
| the user's workflow | a new habit to try, for example "drop rough notes in `inbox/drafts/` instead of editing concepts directly", with what it saves |
| loci itself | an issue draft for the loci repository (below) |

Prefer, in order: a command, a lint rule, a test, skill text, rule prose. A check
that cannot be skipped beats an instruction that can.

## 3. Test before adopting, where cheap

- **A rule, lesson or style change:** if the vault has benchmark cases and the user
  agrees to the cost, run `loci bench --agent '<agent command>' --json` with and
  without the change and compare pass rates. Do not tune the change on the same
  cases you report.
- **A workflow or structure change:** try it on one real example and show the diff.
- **Untestable:** say so; it is adopted on the user's judgement alone.

## 4. Let the user decide

Present the proposals as a short list: finding, evidence, proposed change, test
result. Apply only what the user accepts, as working-tree edits they review with
`git diff`. Never commit.

**Issue drafts for loci** are text for the user to file, never filed by you. Each
has: the loci version (`loci --version`), what falls short, a minimal reproduction
with neutral examples (no vault content, paths or names), the proposed fix and its
level from the order above, and the evidence it addresses.

## 5. Record what held

Delete the refs you reviewed (`git update-ref -d <ref>`). Keep a lesson only if the
evidence or the test supported it. Delete lessons that an adopted check or command
now makes unnecessary. Summarise what changed, what was
rejected, and what is waiting on an upstream fix.
