# Spec: the learn plugin (`loci-learn`): cards, review log and estimates

Status: draft, milestone M3. Decisions: see docs/DECISIONS.md.

`loci-learn` is a plugin binary in the loci workspace. It depends only on
`loci-core` and follows the plugin contract of `docs/specs/m0.md` §2.3: it is run as
`loci learn <command>`, prints the core `--json` envelope, requires `LOCI_VERSION`
(`plugin.direct`, `plugin.version`), and reports lint diagnostics with codes
prefixed `learn.`.

The plugin binary never writes notes. It writes only under `plugins/learn/`:
`.gitattributes` and `reviews.jsonl` (§2.4, §3). It reads
`plugins/learn/config.toml` (§2.1), which the user writes.

## 1. Cards

### 1.1 Syntax

Cards are the top-level list items under the `## Cards` H2 of a concept note (to the
next H1 or H2). The plugin reads them from the working tree through `loci-core`:
each item's byte range, from its `Start(Item)` and `End(Item)` offsets, is one card.

- The item's block id, at the end of its leading text (core: `docs/specs/m0.md`
  §4.6), is the card's id. It identifies the whole item.
- The question and answer are split at the first `::` outside inline code that is
  preceded by whitespace and followed by whitespace or a line end.
- A nested list inside the item is part of the answer.

```markdown
- Why does Dijkstra's algorithm fail with negative edge weights? ::
  A settled vertex can later get a shorter path through a negative edge,
  which the algorithm never revisits. ^01JBA2K7N4P8R3T6W9X2Z5C8DF

- Which conditions does Dijkstra's algorithm need? :: ^01JB9XR2M8C4V6T0Q3W5H7K9PA
  - non-negative edge weights
  - a priority queue keyed by tentative distance
```

`loci block-ids <note> --section Cards` places missing ids at the end of each card's
leading text.

### 1.2 Card ids

A card's id is its block id. Card ids are unique across the vault and are ULIDs
(ulid/spec), the form `loci block-ids` generates. Whoever writes a card (the agent or
the user) writes it without an id and runs `loci block-ids <note> --section Cards`.
Moving a card to another note keeps its id. When a card's meaning changes, its block
id is deleted and `loci block-ids` is run again.

## 2. Commands

### 2.1 `plugins/learn/config.toml`

Optional; written by the user, read by `loci learn review`:

| Key | Default | Range | Meaning |
|---|---|---|---|
| `new_per_day` | 20 | integer ≥ 0 | new cards introduced per day |
| `desired_retention` | 0.9 | number, 0 < x < 1 | retrievability at which a card becomes due (§4.1) |

An unknown key, a wrong type or a value out of range is `learn.config` (exit 1).

### 2.2 `loci learn lint`

Read-only. Reads every concept's `## Cards` section and reports:

| Code | Sev | Message (template) | Hint |
|---|---|---|---|
| `learn.card-format` | E | card is not `- question :: answer`, optionally followed by a block id | |
| `learn.no-id` | E | card has no block id | `loci block-ids <note> --section Cards` |
| `learn.id-form` | E | card id `^<id>` is not a ULID | delete the id and run `loci block-ids <note> --section Cards` |
| `learn.id-duplicate` | E | card id `^<id>` is used by `<paths>` (vault-wide) | |

Anything in the section that is not a top-level list item, and a second `## Cards`
H2 in the same note, is `learn.card-format`.
Messages contain no positions; `path` and the card item's byte range are separate
fields. `data` is `null`.

### 2.3 `loci learn skills`

Prints the plugin's skills, compiled into the binary. Every name starts with
`learn-`. `data`: `{"skills": [{"name", "description", "body"}]}`.

### 2.4 `loci learn log`

Reads exactly one JSON object from stdin:

| Key | Required | Meaning |
|---|---|---|
| `card` | one of `card`, `concept` | card id (§1.2) |
| `concept` | one of `card`, `concept` | concept ULID, for a quiz item that has no card |
| `item` | iff `concept` | the prompt of the quiz item |
| `purpose` | yes | `diagnostic`, `practice` or `review` |
| `grader` | yes | `self` or `llm` |
| `grade` | iff `grader` is `self` | `again`, `hard`, `good` or `easy` |
| `verdicts` | iff `grader` is `llm` | `[[bool, ...], ...]`: one array per grader call (one or two calls), one boolean per rubric criterion |

For `grader: llm`, `log` derives `grade` from `verdicts` (§3) and rejects a supplied
`grade`. It adds `t` (now, UTC), validates, and appends the line to
`plugins/learn/reviews.jsonl`.

- Input that does not match the table, or purpose `review` without a `card`:
  `learn.bad-input`.
- A `card` that is not a card id in the working tree: `learn.unknown-card`.
- A `concept` that does not name a concept note in the working tree:
  `learn.unknown-concept`.

**Appending** (shared by `log` and `review`): before the first append, if
`plugins/learn/` or `plugins/learn/.gitattributes` is missing, it is created, the
file containing `reviews.jsonl merge=union`.

**Committing** (shared by `log` and `review`): `git add plugins/learn/reviews.jsonl
plugins/learn/.gitattributes`, then `git commit -m "loci: learn <command>" --
plugins/learn/reviews.jsonl plugins/learn/.gitattributes` (the user's identity).

Each `log` call appends its line and commits. On a git failure the appended line is
truncated away (exit 3). `data`: `{"line": <the appended object>, "commit"}`.

### 2.5 `loci learn state <concept>`

Returns the estimate of a concept (§4.2), named by its path or slug (a slug shared
by several concepts is `concept.ambiguous`). An unknown one is
`learn.unknown-concept`. `data`:

```json
{"concept": "01JB9XQ7Z3K4M5N6P7Q8R9S0TV",
 "estimate": {"kind": "cards", "r": 0.82},
 "lines": [{"t": "2026-10-30T12:00:00Z", "card": "01JBA2K7N4P8R3T6W9X2Z5C8DF",
            "purpose": "review", "grader": "self", "grade": "good"}]}
```

`estimate` is one of `{"kind": "cards", "r"}`, `{"kind": "item", "grade",
"age_days"}` or `{"kind": "unknown"}`. `lines` are the review-log lines the estimate
was computed from (§4.2), in timestamp order; empty for `unknown`.

### 2.6 `loci learn review`

1. Reads every card from the working tree through `loci-core`, and
   `config.toml` (§2.1).
2. Replays each card's FSRS state from `reviews.jsonl` (§4.1). Lines whose card id
   no longer exists are ignored.
3. Selects, in this order:
   1. the due cards (§4.1), by due time ascending, ties by card id bytes;
   2. new cards (cards with no line), by note path bytes, then by position in the
      note, at most `new_per_day` minus the number of cards whose first line has
      today's UTC date.
4. Runs a terminal loop over the selection: show the question, wait, reveal the
   answer, read a self-grade (`again`, `hard`, `good` or `easy`).
5. Appends each grade at once, through the same validation and append code as
   `log` (purpose `review`, grader `self`), without committing.
6. If any line was appended, commits once, at the end of the session or on
   interrupt (SIGINT), as `log` does. If that commit fails, the appended lines stay
   in the working tree and the command exits 3. If no line was appended, nothing is
   committed, `commit` is `null`, and the command exits 0.

`data`: `{"reviewed", "new", "due_left", "commit"}`.

## 3. The review log

`plugins/learn/reviews.jsonl` records every graded recall: card reviews and quiz
items. It is the only learning data stored. Card schedules and estimates are
recomputed from it and from the notes; nothing computed is stored.

- **One JSON object per line,** appended and never edited.
- **Only `loci learn log` and `loci learn review` append to it,** and they commit it
  on its own (per call for `log`, per session for `review`).
- **`plugins/learn/.gitattributes` marks the file `merge=union`,** so appends from
  two machines merge cleanly. Lines are processed in timestamp order.

A line has the keys of the `log` input (§2.4) plus `t` (UTC, RFC 3339) and, for
`grader: llm`, the derived `grade`.

`purpose` is read by the M3 evaluation. The learn skill's questions on the
material, asked at the start of a session and again at its end, are not logged here;
the skill records them in a dated journal entry for the session, written through the
usual workflow. `grader`,
`verdicts` and `item` are read by the M3 evaluation: grader agreement and results by
item.

**Grades from LLM grading.** Each grader call (one array of `verdicts`) gives one
result:

- **pass** if every rubric criterion passes;
- **fail** if none does;
- **partial** otherwise.

Pass maps to `good`, partial to `hard` and fail to `again`. If two grader calls give
different results, the grade is `hard`.

**Disputes.** The skill logs a grade only after the user has seen it. If the user
disputes it, a fresh grader re-judges the answer, and the skill logs one line with
that grader's single `verdicts` array.

**Changed or deleted notes.**

- Lines whose card or concept is absent from the working tree are ignored. Nothing
  is logged when a note or card is deleted.
- Moving a card or renaming a concept changes nothing.

## 4. Estimates

### 4.1 Cards (scheduling)

Each card's state comes from the `fsrs` crate 6.6.x with its default parameters
(`FSRS::new(&DEFAULT_PARAMETERS)`):

- **Memory state:** `memory_state(FSRSItem { reviews }, None)`, where `reviews` has
  one `FSRSReview { rating, delta_t }` per line with that `card`, in timestamp
  order: `rating` is 1 (`again`), 2 (`hard`), 3 (`good`) or 4 (`easy`); `delta_t`
  is the difference, in days, between the UTC calendar dates of this line and of the
  card's previous line (0 for the first line).
- **Due time:** the time of the last line plus `next_interval(Some(stability),
  desired_retention, rating)` days, with `desired_retention` from `config.toml`.
  This is when the card's predicted retrievability falls to `desired_retention`.
- **Retrievability** at time t: `current_retrievability(state, days_elapsed,
  FSRS6_DEFAULT_DECAY)`.

`loci learn review` schedules from this state alone.

### 4.2 Concepts

A concept has no FSRS state of its own. Its estimate is derived:

- **With at least one reviewed card:** the mean of the retrievability R(t) of its
  reviewed cards.
- **Otherwise, with quiz items** (lines with that `concept`): the grade of its last item, read as fail (`again`), partial (`hard`)
  or pass (`good`, `easy`), and how long ago the item was.
- **Otherwise:** `unknown`.

## 5. Tests

| Fixture | Asserts |
|---|---|
| `cards` | cards in a tight list and in a loose list parse the same; a multi-line card; a card with a nested list in its answer and its id before the nested list is valid; `std::vector` and `::` inside inline code do not split; `q ::` at a line end splits; missing `::` → `learn.card-format`; a card without a block id → `learn.no-id` with the `block-ids` hint; a hand-written short id → `learn.id-form`; the same id in two notes → `learn.id-duplicate`; a second `## Cards` H2 → `learn.card-format`; all reported through `loci lint`; `lint` writes nothing and returns `data: null` |
| `config` | without a file, `review` uses `new_per_day` 20 and `desired_retention` 0.9; a file overrides them; an unknown key, a wrong type, a negative `new_per_day` and a `desired_retention` of 0 or 1 are each `learn.config` |
| `version` | an incompatible `LOCI_VERSION` is `plugin.version`; an unset `LOCI_VERSION` is `plugin.direct` |
| `log` | an unknown purpose is `learn.bad-input`; the first line ever creates `plugins/learn/` and its `.gitattributes`, and the line and `.gitattributes` are committed alone; nothing outside `plugins/learn/` is written; later lines likewise; other uncommitted and intent-to-add changes stay untouched; `llm` input derives `grade` from `verdicts` and rejects a supplied `grade`; `learn.bad-input`, `learn.unknown-card`, `learn.unknown-concept` |
| `state` | each `estimate` kind; mean R over reviewed cards only; the lines behind the estimate; an unknown slug |
| `review` | due cards by due time (a card becomes due when its predicted retrievability reaches `desired_retention`), then at most `new_per_day` new cards in note-path and position order; a line for a deleted card is ignored; grades are appended one by one and committed once at the end, and once on interrupt; a failed final commit leaves the lines and exits 3; a session with no grades commits nothing and exits 0; no note is written |
