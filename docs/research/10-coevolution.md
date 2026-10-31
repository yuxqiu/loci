# 10 — Co-evolution: adapting to the user, upgrading safely, improving from use

Status: research, 2026-10-31. Question: how can loci change with its user over time,
in three senses:

- **(a)** the system adapts to the user (style, rules, corrections, budgets, benchmark
  cases, area structure);
- **(b)** the software and the vault format upgrade without breaking or silently
  rewriting notes;
- **(c)** loci itself improves from patterns seen in use, without private content
  leaving the user's machine.

Nothing here changes DESIGN or the specs; it informs decisions. Conventions:
`[unverified]` means I did not confirm the claim in a primary source;
`[design inference]` means my extrapolation, with no study behind it.

## TL;DR

| Sense | Top recommendation | Why |
|---|---|---|
| (a) | Treat `RULES.md` Lessons as a short list of dated, specific bullets that the agent edits or deletes, never only appends; give it a size budget and a lint warning | Every system that keeps learned text bounds it and removes entries (Reflexion, ExpeL, MemGPT, Claude Code). Long or contradictory instruction files cut compliance (IFScale, Claude Code docs) |
| (a) | Turn user corrections into structure first: a `bench/retrieval.jsonl` case, a `loci.toml` budget change, a `STYLE.md` line, and only then a Lesson | Voyager adds a skill only after verifying it; context files add cost without clear gains (AGENTS.md study) |
| (b) | Keep `schema` bumps rare. When one is needed, ship `loci upgrade`: it rewrites the working tree only, never commits, refuses a dirty tree, and is idempotent; the user reviews `git diff` | git refuses unknown repository formats; org-roam, Taskwarrior and Logseq show what goes wrong without a reviewable, repeatable path |
| (b) | Deprecate in stages: a lint warning with a fix hint for several releases, then an error | Hugo (INFO→WARN→ERROR) and Obsidian (1.4 deprecate → 1.9 drop) |
| (c) | Encode recurring lessons as a command, a lint rule or a fixture before prose; move lessons upstream only by the user's explicit action, rewritten with neutral examples | D107 (`loci links`) is already this path; Rust's metrics plan and Homebrew/VS Code show local-first, opt-in reporting |

## 1. What the sources store, keep and discard

| System | Stores | Keeps / discards | Evidence it helps |
|---|---|---|---|
| Reflexion (2303.11366) | verbal self-reflections after each failed trial | bounded buffer of Ω = 1–3 reflections, a sliding window, "to adhere to max context LLM limitations" | 91% pass@1 on HumanEval vs 80% for GPT-4 |
| ExpeL (2308.10144) | natural-language insights from success/failure pairs, plus trajectories | LLM applies ADD, EDIT, UPVOTE, DOWNVOTE; a new insight starts at count 2; at 0 it is removed, because "even successful trajectories can be suboptimal and mislead the generated insights" | HotpotQA: ReAct 28.0%, hand-written insights 32.0%, ExpeL 39.0%. Insights beat trajectory retrieval on HotpotQA (36/31%) but not ALFWorld (50/55%) |
| Voyager (2305.16291) | skills as executable code, keyed by an embedding of a description | a skill is added only after self-verification succeeds | removing self-verification: −73% items found; without the skill library, progress plateaus |
| Agent Workflow Memory (2409.07429) | reusable workflows induced from past tasks, offline or online | (not described in the abstract) | +24.6% and +51.1% relative success on Mind2Web and WebArena |
| MemGPT (2310.08560) | a fixed-size "working context" the agent edits by function calls; recall and archival storage outside the window | at a memory-pressure warning (e.g. 70% of the window) the agent saves what matters; at the flush threshold messages are evicted and a recursive summary is kept | deep memory retrieval: GPT-4 32.1% → 92.5% |
| Generative Agents (2304.03442) | a stream of all observations; reflections synthesised from it | retrieval scores recency (decay 0.995), importance and relevance; reflection runs when summed importance exceeds 150 | TrueSkill 29.89 full vs 26.88 without reflection |
| ACE (2510.04618) | a "playbook" of itemised bullets, each with an id and helpful/harmful counters | incremental delta edits, not rewrites; embedding-based de-duplication | +10.6% on agent benchmarks; a monolithic rewrite collapsed one context from 18,282 to 122 tokens and accuracy fell from 66.7 to 57.1, below the 63.7 no-adaptation baseline |

Common pattern: learned text is **bounded**, **itemised** and **edited in place**,
with an explicit way to drop an entry. Whole-file rewrites lose detail ("context
collapse", ACE).

## 2. (a) The system adapting to the user

### What loci already has

- `STYLE.md` (the user's style, built by the agent from the user's writing, D20) and
  `RULES.md` with a `## Lessons` section the agent maintains (rules.md "Writing
  concept notes"). Both are printed verbatim by `loci status` (m0 §6.5, §9.6) and
  override the framework rules.
- Every agent change goes through `git diff` review, so a lesson the agent writes is
  seen before it lands (rules.md "Workflow").
- `loci.toml` `[budgets]` with defaults; the `migrate` skill says to check budgets and
  add `bench/retrieval.jsonl` cases after the first batch; DESIGN §8 says to add a
  case whenever the agent misses something that exists.
- Area restructuring with `loci mv`; `index.over-budget` and `area.too-long` warn when
  an area outgrows its digest.
- Missing: a size limit or format for Lessons, a rule for removing or merging them,
  and any check for lessons that contradict the framework rules or each other.

### What the sources suggest

- **Short and specific beats long.** Claude Code: "target under 200 lines per
  CLAUDE.md file. Longer files consume more context and reduce adherence"; "if two
  instructions contradict each other, Claude may pick one arbitrarily", so review
  them "periodically to remove outdated or conflicting instructions". It ships an
  audit (`/doctor prompt-audit`) that looks for references to files or commands that
  no longer exist and for contradictions, and proposes edits without applying them.
  Cursor: "Keep rules under 500 lines", split large rules, "Avoid vague guidance".
- **Instruction count hurts.** IFScale (2507.11538): the best of 20 models reach about
  68% accuracy at 500 simultaneous instructions, with a bias towards earlier
  instructions. Lost in the Middle (2307.03172): performance "significantly degrades"
  when relevant information is in the middle of a long context.
- **Generated context files are not free.** Evaluating AGENTS.md (2602.11988):
  context files "do not generally improve task success rates" and raise inference
  cost "by over 20% on average"; they are useful "for specifying non-standard coding
  practices". AGENTS.md itself sets no size or fields and asks maintainers to treat it
  as "living documentation".
- **Do not store what can be derived.** Claude Code's auto memory skips "anything it
  can derive from the codebase" and anything CLAUDE.md already says; it keeps an
  index under 200 lines / 25 KB, warns when the index nears the limit, and stamps a
  `modified` time so staleness is visible.
- **Verify before keeping.** Voyager adds only verified skills; ExpeL drops insights
  that later evidence contradicts.
- Cursor's "Memories" (auto-generated rules, approved by the user) appear only in
  forum posts quoting older docs; the current docs page does not mention them
  `[unverified]`.

### Recommendations (ranked by value)

1. **Lesson hygiene in `share/loci/rules.md`.** Replace "Record reusable lessons in
   the `## Lessons` section of `RULES.md`" with a few rules:
   - one bullet per lesson, one sentence, specific enough to check ("Name journal
     entries in `journal/reading/` as `<citekey>.md`", not "be careful with names");
   - before adding, search Lessons for one to edit; when a correction contradicts a
     lesson, edit or delete that lesson in the same diff;
   - put a lesson about writing in `STYLE.md`, not Lessons;
   - prefix each bullet with the date it was added or last confirmed
     (`- 2026-10-31: …`), so stale ones are visible (Claude Code's `modified`).
2. **Budget and warning.** Add `rules = 4000` (characters; `[design inference]` for
   the value, roughly the 200-line guidance at short lines) to `[budgets]` in
   `loci.toml`, covering `RULES.md` + `STYLE.md`, and a lint warning
   `rules.too-long`, like `size.note`. A warning is the trigger for consolidation;
   loci never shortens the file itself.
3. **Prefer structure to a lesson** (rules.md, `intake` and `migrate` skills): when
   the user corrects a retrieval miss, add a `bench/retrieval.jsonl` case; when the
   user keeps notes above `size.note`, propose a new `note` budget; when the
   correction is a vault convention lint could check, say so to the user (it feeds
   §4). Only what fits none of these becomes a Lesson.
4. **Lesson audit after an upgrade or on request.** A short section in the `concepts`
   skill (or a new small skill): read `RULES.md`, `STYLE.md` and the framework rules
   from `loci status`; report lessons that repeat a framework rule, contradict one,
   name a command or path that no longer exists (`loci skill show`, `ls`), or
   contradict each other; propose deletions as a diff. This mirrors
   `/doctor prompt-audit`, with the user's review as the gate.
5. **Budgets from the vault, not guesses.** After a migration batch, the agent runs
   `loci lint --json` and proposes `loci.toml` budgets only when warnings that the
   user has kept across commits cluster on one code. Keep defaults otherwise.
6. **Areas follow interests.** When `index.over-budget` fires for an area, or an area
   keeps receiving concepts that link mostly elsewhere (`loci links`), the agent
   proposes a split or merge with `loci mv` and updated `AREA.md` paragraphs. Never
   restructure unasked (rules.md: "Propose merges to the user").

### What to avoid

- Appending lessons without ever deleting them, and regenerating `RULES.md` in one
  rewrite (ACE's collapse).
- Repo overviews or facts derivable from `loci digest` in Lessons (2602.11988;
  Claude Code auto memory).
- Counters, votes or importance scores on lessons (ExpeL, ACE): they need a log and a
  reader loci does not have (DESIGN principle 5). Dates and the user's diff review do
  the same job at this scale `[design inference]`.
- Learning style from agent-written text: `STYLE.md` should be derived from text the
  user wrote, or the agent learns its own habits.

## 3. (b) Upgrading software and vault format safely

### What loci already has

- `loci.toml` has `schema = 1`; an unknown schema, unknown key or wrong type is
  `config.invalid` (m0 §11; `crates/loci-core/src/config.rs` rejects `schema = 2`
  and unknown sections via `deny_unknown_fields`). The `--json` envelope has its own
  `schema` (m0 §8).
- Plugins refuse to run unless `LOCI_VERSION` is semver-compatible by Cargo's rule
  (m0 §2.3.6). Under that rule `^0.2.3` means `>=0.2.3, <0.3.0`, so before 1.0 every
  minor release breaks plugin compatibility. All crates share one version and ship
  together, so this holds in practice.
- Framework rules and core skills are compiled into the binary (m0 §12), so they
  change exactly when the binary does; copies the user installed can lag (D106).
- `plugins/learn/config.toml` and `reviews.jsonl` lines use `deny_unknown_fields`
  and have no version field.
- No upgrade command (D55, written about skill installation), no changelog, no
  deprecation policy.

### What the sources suggest

- **Refuse what you do not understand; bump rarely.** git: an implementation that
  does not understand a repository's format version "MUST NOT operate on that
  repository; doing so risks not only producing wrong results, but actually losing
  data". Version 1 adds `extensions.*`; an unknown extension also stops the operation.
  "Version bumps should be kept to an absolute minimum": prefer versioning individual
  files, or new data that older clients ignore.
- **Major vs minor.** nbformat: backward-compatible changes (new fields, new cell or
  output types) bump the minor version; incompatible ones the major. Unknown cell and
  output types "will be preserved". `nbformat.read(..., as_version=)` converts
  between majors one step at a time, both up and down; `NO_CONVERT` reads as-is.
- **Staged deprecation.** Hugo logs INFO for 3 minor releases, WARN for 12 more, then
  ERROR and fails the build; code is removed six or more releases after that.
  Obsidian deprecated `tag`/`alias`/`cssclass` in 1.4 and dropped them in 1.9; its
  property editor converts a key when it touches a note, and the Format converter
  plugin converts the vault at once.
- **What went wrong elsewhere.**
  - *Org-roam v1→v2:* `org-roam-migrate-wizard` asks for confirmation, copies the
    directory to `org-roam.bak`, then adds IDs, renames keywords and rewrites every
    file link as an ID link. The manual tells users to treat v2 "as entirely new
    software". A user reports a first run that converted only 20–30 notes; deleting
    `org-roam.bak` and rerunning worked `[unverified: single blog report]`. An older
    standalone script left `roam:` links and tags unconverted (Discourse thread).
  - *Taskwarrior 3.0:* storage moved from `*.data` files to SQLite; users who
    upgraded through a system update found their tasks no longer recognised
    (discussions #3324, #3358). 3.3 added `task import-v2`, and Taskwarrior warns on
    every run while the old files exist.
  - *Logseq file → DB:* not in place. The importer builds a new DB graph and "loudly
    error[s] or warn[s] when something doesn't fully import"; file graphs were split
    into a separate app (logseq/og); Org files and old flashcard SRS data are not
    imported.
- **Report, don't fail, for future changes.** Cargo prints a one-line notice when
  dependencies trigger future-incompatibility warnings and keeps a full report that
  `cargo report future-incompatibilities` shows.

### Recommendations (ranked by value)

1. **Write the compatibility rule down** (DESIGN §7 or m0 §11): a new optional key
   with a default, a new lint warning, or a new command is not a schema change. A
   change in meaning, a removed key, a new required field, or a layout change that old
   binaries would misread bumps `schema`. This keeps bumps rare, as git advises.
2. **When a bump is needed, ship `loci upgrade`** (revisit D55, which was about skill
   installation). Behaviour:
   - refuses when the working tree is not clean, as intake refuses uncommitted inbox
     files: `git reset --hard` is then the backup (no `.bak` folder, unlike org-roam);
   - edits the working tree only, through the splice API, and never commits;
   - converts one schema step at a time (nbformat) and is idempotent, so a partial
     run can be repeated (the org-roam failure);
   - finishes by running `loci lint` and printing what it changed per file;
   - never rewrites note prose: only frontmatter keys, link forms, paths and
     `loci.toml`.
   An older binary meeting a newer `schema` keeps refusing with `config.invalid`; a
   newer binary meeting an older one refuses with a hint "run `loci upgrade`"
   (Taskwarrior's lesson: never leave the user's data silently unread).
3. **Deprecate in stages.** A frontmatter key, link form or command being retired
   first produces a lint warning `deprecated.<what>` whose hint names the fix
   (`loci upgrade`, `loci mv`), for at least two minor releases, then becomes an
   error. Record each in DECISIONS with the release it starts and ends.
4. **A `CHANGELOG.md` with an "Agent-visible changes" section** for each release:
   changes to `share/loci/rules.md`, skills, lint codes and command flags. After an
   upgrade the agent runs the lesson audit (§2 rec. 4) against it, so vault Lessons
   that the new rules make redundant or contradictory are proposed for removal.
5. **Golden tests for upgrades** (DESIGN §10): fixture vaults at each past schema,
   `loci upgrade`, compared with the expected tree; a property test that a second run
   changes nothing and every link still resolves.
6. **Plugin data versions belong to the plugin.** Add one line to the plugin contract
   (m0 §2.3): a plugin must refuse data under `plugins/<name>/` it cannot read and
   must provide its own upgrade path. `reviews.jsonl` is append-only history: new
   fields should be optional (nbformat minor), so old lines stay valid forever.
7. **Installed skill copies.** Keep D106; have the `AGENTS.md` stub name
   `loci skill show` as the authoritative copy.

### What to avoid

- Migrating on read, or rewriting a note whenever it is touched (Obsidian's editor):
  unrelated diffs mix with the user's task.
- Migrations that commit, keep backups outside git, or stop half-way without being
  rerunnable (org-roam).
- Changing on-disk formats through a routine package update with no command and no
  loud error (Taskwarrior 3.0).
- Dropping data that has no target in the new format (Logseq's SRS data): the learn
  review log must survive any core or plugin upgrade.
- A per-vault pin of rules or skill versions: the compiled-in copy already matches
  the binary, and a pin is a second source of truth.

## 4. (c) loci improving from patterns found in use

### What loci already has

- `DECISIONS.md` records why loci changed. Several entries came from use: D107 added
  `loci links` because "agents had to search the files by hand" to find backlinks;
  D97 came from a smoke test; D98–D99 from reviews, each fix covered by a test.
- DESIGN §10: "Every bug found on a real vault becomes a fixture with no private
  content". Private maintainer notes are kept apart from public docs.
- Signals that already exist in each vault: `RULES.md` Lessons, git history of what
  the user changed after the agent, `loci lint --json`, `bench/retrieval.jsonl`.
- No collection mechanism and no network access, which is right.

### What the sources suggest

- **Self-improving agents need a metric, an archive and a guard.** SICA (2504.15228)
  edits its own code, keeps an archive of versions with benchmark results, picks the
  best by a utility of accuracy, cost and time (0.5/0.25/0.25), and runs an LLM
  overseer that can cancel runs; SWE-Bench Verified subset 17% → 53%. It reports path
  dependence: early ideas steer later ones. Darwin Gödel Machine (2505.22954):
  SWE-bench 20.0% → 50.0%, validated empirically, sandboxed, with human oversight; it
  also found *objective hacking*: one agent removed the logging that the
  hallucination checker read, scoring well without fixing the problem, and hacking
  was more frequent when the checking functions were visible to the agent.
- **Optimise prompts against a metric and traces.** DSPy (2310.03714) compiles a
  pipeline "to maximize a given metric" from examples (over 25% better than few-shot
  prompting for GPT-3.5). GEPA (2507.19457) reflects in natural language on traces and
  beats GRPO with up to 35× fewer rollouts. Both need a scored set of cases, which for
  loci is `loci bench` and the procedure goldens.
- **Edit, don't rewrite.** ACE keeps itemised bullets and applies deltas (§1).
- **Local-first feedback.** Rust's Metrics Initiative: "NO TELEMETRY, NO NETWORK
  CONNECTIONS"; metrics are written locally and sharing is "opt-in, clear, and
  manual". Cargo writes future-incompatibility reports to disk and shows them on
  request. Homebrew: `brew gist-logs` uploads build logs only when the user runs it;
  its analytics are opt-out, with a first-run notice and no user id. VS Code: one
  `telemetry.telemetryLevel` setting, and the user can view every event sent
  (`Developer: Show Telemetry`, `telemetry.log`).

### How patterns get detected

| Pattern | Where it shows | How to read it (all local) |
|---|---|---|
| repeated agent mistake | the user's next commit edits what the agent wrote; the same Lesson appears in several vaults | `git log -p` on the touched notes; compare Lessons across the maintainer's own vaults |
| lint finding the user overrides | the warning stays in committed `HEAD` across many commits | `loci lint --json` on a clean tree; count codes |
| command emulated by hand | agent transcripts show the same shell pattern (e.g. `grep -rn '\[\[<slug>'` before D107) | the agent's own local transcripts (Claude Code keeps them under `~/.claude/projects/` until `cleanupPeriodDays`) |
| retrieval miss | the user finds a note the agent did not | a new `bench/retrieval.jsonl` case; `loci bench` recall drops on it |

### Recommendations (ranked by value)

1. **An encoding ladder for every recurring lesson** (a DECISIONS entry, and a
   checklist in `docs/RUNBOOK.md`): (i) a command or a change to one, (ii) a lint
   rule whose hint names the fix, (iii) a fixture or golden test, (iv) skill text,
   (v) framework-rule prose, last. Each fix starts with a failing fixture built from
   neutral examples (DESIGN §10). D107 is the model.
2. **A neutral upstream path, triggered only by the user.** When the agent writes a
   Lesson about loci's behaviour rather than the vault ("`loci mv` does not update X",
   "lint flags Y wrongly"), the rules ask it to also tell the user that this may be a
   loci issue and to offer a draft issue text that restates the lesson with a generic
   example (sorting algorithms, not the user's notes). The user decides whether to
   file it. Add an issue template asking for: loci version, the command, the lint code,
   a minimal neutral reproduction.
3. **Retire upstreamed lessons.** When a release encodes a lesson (CHANGELOG
   "Agent-visible changes"), the lesson audit (§2 rec. 4) proposes deleting the vault
   copy, so vaults do not carry rules the binary now enforces.
4. **Measure skill and rule changes before shipping.** Changes to `share/loci/rules.md`
   or a SKILL.md run `loci bench --agent` and the procedure goldens on the sample
   vault (`examples/sample-vault/`), as DSPy/GEPA optimise against a metric. Keep a
   held-out set of cases the change was not written against (DGM's hidden checks).
5. **Optional, later: `loci lint --summary` or a `status` section** printing counts per
   lint code and the loci version, with no paths or text, for the user to paste into
   an issue. Like `cargo report`, it is computed locally and never sent
   `[design inference]`. Add only when the issue template proves insufficient.

### What to avoid

- Any automatic upload, telemetry or network call, even opt-out. Nothing leaves the
  machine unless the user copies it (Rust metrics principle).
- An agent that files issues, opens PRs, or edits loci's skills from inside a vault.
  The vault's only writable instruction files are `RULES.md` and `STYLE.md`.
- Paths, slugs, note titles or note text in anything meant for upstream; examples
  must be rewritten in neutral terms.
- Optimising skills against `loci bench` alone: the agent learns the cases (DGM's
  objective hacking). Keep held-out cases and read the diffs.

## Addendum: Harness² (Google Research, 2026)

Harness² improves an agent without retraining by rewriting its harness (system
prompt, guardrails, memory, tool guidance, subagents, skills, scripts, plugins) in
two loops: per task, Propose–Probe–Compose keeps the edits whose effect shows in
contrasts between runs, with no answer key; across tasks, a reflector carries
forward only "evidence-supported lessons". Its README describes no human review
step and no safeguards against regressions or tampering (repository:
https://github.com/google-research/harness2; paper: https://arxiv.org/abs/2610.14224).

loci adopts the loop with the user as the gate, as the `improve` skill (D109):
evidence from corrections in `git log -p`, lessons and repeated hand work;
proposals at the vault, workflow or upstream level; probes with
`loci bench --agent` where cheap; adoption only by the user's review; upstream
changes only as issue drafts the user files.

## Sources

Agent memory and self-improvement:
- Reflexion: https://arxiv.org/abs/2303.11366 (bound Ω: §3, §4 of https://arxiv.org/html/2303.11366)
- ExpeL: https://arxiv.org/abs/2308.10144 (operators §4.2, results §5: https://arxiv.org/html/2308.10144)
- Voyager: https://arxiv.org/abs/2305.16291 (§2.2–2.3, §3.4: https://arxiv.org/html/2305.16291)
- Agent Workflow Memory: https://arxiv.org/abs/2409.07429
- MemGPT: https://arxiv.org/abs/2310.08560 (tiers, pressure, Table 2: https://arxiv.org/html/2310.08560v2)
- Generative Agents: https://arxiv.org/abs/2304.03442 (scoring, threshold 150, ablation: https://arxiv.org/html/2304.03442v2)
- ACE: https://arxiv.org/abs/2510.04618 (roles, bullets, collapse: https://arxiv.org/html/2510.04618)
- SICA: https://arxiv.org/abs/2504.15228 (utility, overseer, path dependence: https://arxiv.org/html/2504.15228)
- Darwin Gödel Machine: https://arxiv.org/abs/2505.22954 (objective hacking: https://arxiv.org/html/2505.22954v3)
- DSPy: https://arxiv.org/abs/2310.03714
- GEPA: https://arxiv.org/abs/2507.19457

Instruction files and drift:
- Claude Code memory (CLAUDE.md, auto memory, prompt audit): https://code.claude.com/docs/en/memory
- AGENTS.md: https://agents.md/
- Cursor rules: https://cursor.com/docs/context/rules
- Cursor Memories, forum only `[unverified]`: https://forum.cursor.com/t/0-51-memories-feature/98509
- Evaluating AGENTS.md: https://arxiv.org/abs/2602.11988
- IFScale, "How Many Instructions Can LLMs Follow at Once?": https://arxiv.org/abs/2507.11538
- Lost in the Middle: https://arxiv.org/abs/2307.03172

Format changes in note tools:
- Obsidian 1.4 changelog: https://obsidian.md/changelog/2023-07-26-desktop-v1.4.0/ ; Properties help (dropped in 1.9): https://obsidian.md/help/Editing+and+formatting/Properties
- Org-roam manual (migration steps): https://www.orgroam.com/manual.html ; wizard source (confirm, `org-roam.bak`): https://github.com/org-roam/org-roam/blob/main/org-roam-migrate.el ; Discourse: https://org-roam.discourse.group/t/the-org-roam-v2-great-migration/1505 ; partial first run (blog): https://oleksii.shmalko.com/migrating-to-org-roam-v2/
- Logseq DB importer: https://github.com/logseq/docs/blob/master/db-version.md ; changes (logseq/og split, SRS not imported): https://github.com/logseq/docs/blob/master/db-version-changes.md

Versioning of file-based data:
- git repository format: https://git-scm.com/docs/gitrepository-layout
- nbformat: https://nbformat.readthedocs.io/en/latest/format_description.html ; API (`as_version`, `NO_CONVERT`): https://nbformat.readthedocs.io/en/latest/api.html
- Hugo deprecation policy: https://gohugo.io/troubleshooting/deprecation/
- Taskwarrior 3 upgrade: https://taskwarrior.org/docs/upgrade-3/ ; user reports: https://github.com/GothenburgBitFactory/taskwarrior/discussions/3324 , https://github.com/GothenburgBitFactory/taskwarrior/discussions/3358
- Cargo caret requirements: https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html ; future-incompat report: https://doc.rust-lang.org/cargo/reference/future-incompat-report.html

Private feedback:
- Rust Metrics Initiative: https://goals.rust-lang.org/2025h1/metrics-initiative.html
- Homebrew analytics: https://docs.brew.sh/Analytics ; `brew gist-logs`: https://docs.brew.sh/Manpage
- VS Code telemetry: https://code.visualstudio.com/docs/configure/telemetry
