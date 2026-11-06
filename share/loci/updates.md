# loci vault updates

Append-only: the newest entry is at the bottom, ids only increase, and a released
entry is never edited or deleted (a correction is a new entry). Each entry says what changes
(Offer), what the agent does if the user says yes (How), and what the result must
meet (Done when). `loci updates` prints the waiting ones with the procedure.

## u0001 · 2026-11-02 · Concepts no longer have a `digest`

- **Offer:** Concepts no longer have a `digest:` field; until this update is handled,
  lint warns about each one (`update.u0001`). Remove it from every concept. Declining
  keeps the fields, which lint then reports as unknown fields (errors) until they are
  removed by hand.
- **How:** If a digest says something the note's text does not, ask the user whether
  to add it to the text first. Then run
  `loci lint --fix --allow-dirty --unsafe-fixes --only update.u0001`, which deletes
  every `digest:` field, and show the diff.
- **Done when:**
  - `loci lint` reports no `update.u0001`;
  - anything the user asked to keep from a digest is in that note's text;
  - `loci lint` passes.

## u0002 · 2026-11-02 · Agents read the framework rules from `loci rules`

- **Offer:** `loci status` no longer prints the framework rules. Update
  `AGENTS.md` so agents read them, with `RULES.md` and `STYLE.md`, at the start of a
  session.
- **How:** Compare `AGENTS.md` with the stub a fresh `loci init` writes (run it in
  an empty folder). Keep every line the user added; replace the rest with the stub.
- **Done when:**
  - `AGENTS.md` sends agents to `loci rules`, `RULES.md` and `STYLE.md`;
  - every line the user had added to it is still there.

## u0003 · 2026-11-02 · Benchmark cases are `bench/cases.jsonl`, with shell checks

- **Offer:** `loci bench` now reads `bench/cases.jsonl`, where each case is a
  question or task and a shell check, instead of `bench/retrieval.jsonl` with
  expected notes. Convert the cases.
- **How:** If there is no `bench/retrieval.jsonl`, there is nothing to do. Otherwise
  write each case as `{"q": ..., "check": ...}` in `bench/cases.jsonl`, the check
  passing when the agent's reply names an expected note, for example
  `grep -qiE '\[\[(stack|queue)(\]\]|\||#)' "$LOCI_BENCH_OUTPUT"`, then delete
  `bench/retrieval.jsonl`.
- **Done when:**
  - there is no `bench/retrieval.jsonl`;
  - every line of `bench/cases.jsonl` has exactly `q` and `check`, and each check
    passes for the notes the old case expected and fails for others.
