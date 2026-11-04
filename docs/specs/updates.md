# Spec: vault updates

Status: implemented (D176); the contract is m0 §9.11. The idea comes from
mattabate/life-distributable's `ADDENDA.md`; the state and growth rules follow
migration practice in mature projects (a single high-water mark, as Alembic and
git's repository format version; new installs start from the current state, as
Rails loads `schema.rb`).

## 1. Problem

A loci release can change what a vault should contain: a new `AGENTS.md` stub, a
renamed file (`bench/retrieval.jsonl` → `bench/cases.jsonl`), a removed field
(`digest:`), a convention the agent should follow from now on. Today these reach a
vault only as CHANGELOG lines the user has to notice and apply by hand (D162).
m0 §11 plans `loci upgrade` for schema conversions, but most of these changes are
not mechanical: the user may have edited `AGENTS.md`, a removed field may carry text
worth moving, a convention needs judgement. They suit an agent, with the user
deciding each one.

## 2. Updates

loci ships an append-only list of updates, compiled into the binary like the
framework rules (`share/loci/updates.md`):

```markdown
## u0002 · 2026-11-02 · Agents read the framework rules from `loci rules`

- **Offer:** `loci status` no longer prints the framework rules. Replace
  `AGENTS.md` with the new stub so agents read them at the start of a session.
- **How:** Show the user the difference between their `AGENTS.md` and the stub
  (`loci init` writes it in an empty folder). Keep any lines the user added; replace
  the rest with the stub.
- **Done when:**
  - `AGENTS.md` sends agents to `loci rules`, `RULES.md` and `STYLE.md`;
  - every line the user had added to it is still there;
  - `loci lint` passes.
```

- **Id:** `u` and four digits, only increasing. An entry is never edited once
  released; a correction is a new entry.
- **Offer:** one or two sentences for the user: what changes and why.
- **How:** what the agent does if the user says yes, within what the Offer
  describes. If the work goes beyond it, the agent stops and asks.
- **Done when:** the criteria the result must meet, mechanical or not. After the
  How, the agent goes through each one, running a command where one fits (`rg`,
  `loci lint`) and reading the files where it does not, and reports each as met or
  not, with the evidence; a criterion it cannot confirm is reported as not met. The
  user's review of the diff stays the final gate. Criteria describe the new state
  ("`AGENTS.md` sends agents to `loci rules`"), not the absence of the old one, which
  a vault the user rewrote would also satisfy.
- No entry is skipped automatically: every waiting entry is offered. An entry that
  does not apply to a vault, or that is already done, says so to the user when the
  agent finds it from reading the vault.
- Only changes that need action in a vault become entries; the CHANGELOG still lists
  every change. A schema change is not an entry: a vault on an old schema cannot be
  opened, so it is converted by `loci upgrade` (m0 §11).
- **A change lint can find is marked in lint's edition tables** (m0 §7.3; research
  18 §7, research 19): the row of what the update removes gets `until` the update.
  Before the vault handles it, lint reports each old form as `update.uNNNN` (a
  warning, with a fix where one can be made); after, the current rules judge the
  vault. The How runs `loci lint --fix --only update.uNNNN` (with `--unsafe-fixes`
  when the fix needs a look) and shows the diff; `--fix` wants a clean tree, so the
  diff is the update's. The Done when names `update.uNNNN`, since a pending update's
  diagnostics are warnings and "`loci lint` passes" alone does not show them gone; the
  Offer says what declining means (the old forms become errors of the current rules).
  A unit test checks all three for every update the tables name. An update that
  needs judgement has no row.

## 3. State

The vault records the last update it has handled, applied or declined, as one
optional key in `loci.toml` (optional, so no schema change, m0 §11):

```toml
updates = "u0003"
```

- **One high-water mark, no list of declined entries.** Git history shows what an
  update changed; a declined one left no change. An entry at or below the mark is
  never offered again.
- `loci init` writes the newest id the binary knows: a new vault starts current and
  never sees older entries.
- A vault without the key has handled none.
- A key newer than any id the binary knows means the binary is older than the vault:
  `loci status` warns (`updates.newer`) and offers nothing.

## 4. Growth

The list only grows: no entry is ever deleted, and the binary ships all of them. An
entry is a few hundred bytes, so a few dozen a year cost a few kilobytes; a floor
that dropped old entries would add a build step and a refusal for old vaults for no
measurable gain. What keeps the agent's work short is §3: a new vault starts at the
newest id, and a vault sees only the entries after its mark.

## 5. Commands

- **`loci status`** gains an `## Updates` section when updates are waiting:
  ``2 waiting (u0004, u0005); read them with `loci updates` `` (m0 §9.6).
- **`loci updates`** prints the procedure (§6) as a short header, then the waiting
  entries in full; `--all` prints every entry the binary knows. It writes nothing.
  The procedure lives in the command's output, so neither the framework rules nor a
  skill carries it: an agent meets it exactly when updates are waiting.
- **`loci updates --handled <id>`** records that every entry up to and including
  `<id>` is handled, by setting the key. It refuses an id that is below the oldest
  waiting one (`updates.order`), so none is skipped silently.

## 6. The procedure (printed by `loci updates`)

```text
1. Show the user the Offer of every waiting update, together, noting any that look
   done already, or that do not apply here, and why; ask which to apply.
2. Apply the accepted ones oldest first: do the How in the working tree (stop and
   ask if it goes beyond the Offer), then go through its Done when, reporting each
   criterion as met or not, with evidence. Show the diff.
3. Record them all, accepted or not: `loci updates --handled <newest id>`. Never
   commit.
```

The user is asked once, about every waiting update together, and sees every one: a
missed update would need both the agent and the user to overlook it. The accepted
ones are applied oldest first because a later entry may build on an earlier one (a
file one renames, another edits), and because the single mark (§3) means
"everything up to here is handled".

## 7. What it replaces and what it does not

- **Replaces:** the CHANGELOG's manual "To update a vault" steps (D161, D162). At
  the first release they become the first entries.
- **Keeps:** `loci upgrade` for conversions a program must do exactly (m0 §11); an
  entry points to it.
- **Not:** plugins. A plugin's own updates are its concern.

## 8. Testing

- **Unit and integration tests:**
  - parsing the list (Offer, How and Done when each present), and the id order;
  - waiting entries against the key;
  - `--handled` order and its refusal;
  - `init` writing the newest id;
  - the `status` line and `updates.newer`;
  - the procedure header.
- **An eval suite** (planned, not yet written; `evals/suites/updates.jsonl`): a vault in an old state, the task
  "process the waiting updates; the user says yes to each", checked by `bench`
  checks written for the suite and `loci lint`; a case where the user says no,
  checked by an unchanged tree and the key moved; and a case where an entry is
  already done, checked by the agent's reply saying so.
