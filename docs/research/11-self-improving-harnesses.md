# 11 — Self-improving harnesses and agents that learn from their user

Status: research, 2026-10-31. Question: what do the 2026 papers on agents that
rewrite their own harness, and the work on learning a user's preferences from their
edits, teach the `improve` skill (D109)? Four sub-questions: (a) how `improve` can
tell the user's corrections from agent edits without authorship tracking (D73);
(b) mining those corrections into `STYLE.md`; (c) guarding rule and skill changes
against overfitting and tampering; (d) review while the user is away.

This note extends research/10, which covers Reflexion, ExpeL, Voyager, AWM, MemGPT,
Generative Agents, ACE, SICA, the Darwin Gödel Machine, DSPy, Claude Code memory,
AGENTS.md and Harness². Conventions as there: `[unverified]` means not confirmed
in a primary source; `[design inference]` means my extrapolation.

## TL;DR

| Topic | Recommendation | Why |
|---|---|---|
| (a) | A local agent snapshot ref, `refs/loci/agent/<time>`, written by an explicit end-of-task step (`loci lint --snapshot`, name tentative). No agent hooks (per-agent, not general; the user ruled them out). `improve` diffs it against the user's next commit. No commit trailer | The Linux kernel is dropping `Assisted-by` because a voluntary tag gave no signal. A ref needs no user action and is never pushed by default |
| (b) | An `improve` step that turns snapshot→commit prose edits into candidate `STYLE.md` lines, kept only when two or more independent edits support them | CIPHER (edit-derived preferences) and PROSE (verify across samples) |
| (c) | One mechanism per proposal; a proposal never edits the checks it is tested by; accept only with no regression on any case and a gain on one; a diff-aware lint warning when a budget is raised or a bench case removed | RRSI, Self-Harness, HSI's frozen anchor, the harness-tampering audit |
| (d) | No scheduler. `loci status` prints a one-line count of unreviewed snapshots | Sleep-time compute and MetaClaw need a running service. Letta's background memory edits and MetaClaw's skills ship with no user approval step |

## 1. Sources

All papers below were checked against their arXiv abstract and, where noted, the
HTML full text. "Keep" is the evidence that decides whether an edit stays.

| Source | What it does | Keep | Safeguards | Human role |
|---|---|---|---|---|
| Recursive Harness Self-Improvement, RHI (2607.15524) | Harness as a prompt-level spec of the agent loop, revised from an LLM's pairwise preferences between the current and previous harness's outputs | No explicit accept/reject step. Stops when the new harness wins on less than a fraction ε of tasks. About 2 iterations suffice | The optimizer sees the preference history, not the judge's prompt. No rollback described in the main text | None in the loop. Tasks are LLM-generated |
| Harness-R1 (2608.02276) | A 9B "harness engineer", trained with RL, turns batches of failures into code hooks at four lifecycle points | Mean reward change when the frozen target reruns every task in the batch, "including tasks that originally succeeded", so regressions score negative | Patch schema validated before install; hooks never touch the policy; prompt forbids "task-specific answers, indices, or identifiers". Held-out tasks: +8.9±1.5 points | None |
| Hierarchical Self-Improvement, HSI (2608.08466) | Three scopes: task harness, evolver, meta-evolver | Lower confidence bound μ − 0.5σ/√n during search; export by validation score; a test split "inaccessible throughout" | Meta-evolver loaded from "an immutable initialization template" (the frozen anchor); a fixed task-injection interface; "evaluation signals and data splits remain outside the agent's control". Discarded candidates roll back with their notes. The selection policy is itself editable, a gap the paper does not discuss | None |
| Auditing Harness Tampering (2609.00069) | Taxonomy of misaligned harness edits: role (execution, evaluation, selection, recording, propagation) × obligation (measurement validity, fidelity, procedure, authorization, protected artifacts, provenance, completeness) | n/a (audit) | Real runs tamper often: per-iteration rates 18% (AFlow) to 85% (ADAS); best DGM agents keep 3.5 findings, HyperAgents 5.2. Example: two lines overwrite accuracy with 1.0 before saving. LLM auditors classify at about 90% F1. Calls for "tamper-proof evaluation channels" and continuous audits | Manual seeding and spot checks only |
| RRSI (2609.24972) | Regularised harness evolution | Must clear several non-compensatory gates: score ≥ incumbent − δ (δ measured by rerunning the unchanged harness), a cost gate, then a shaped rule for gains inside the noise band | Cosine-annealed limit on edits per candidate; a critic (an LLM) rejects diffs that "encode task names, entity names, task-specific values, answers"; a pruner flags components with no gain over a window as deletion candidates. Ablation on one benchmark: unregularised evolution scores higher on the evolve split (92.8 vs 90.5) but lower out of distribution (40.3 vs 43.6) | None |
| Self-Improvements survey (2607.13104) | Taxonomy by update target (weights vs scaffold) and signal | n/a | §9.1: treat the self-improving agent as "untrusted code"; "layered gating"; "If an agent conflates the roles of proposing updates and accepting them, it collapses into a self-confirming loop"; evolving critics "gated by human audit trails" | §8.1: human oversight "compromises the 'self'"; report it as a cost |
| Self-Harness (2606.09498) | Weakness mining → minimal proposals tied to one failure → validation | Accept only if held-in and held-out pass counts both do not fall and one rises; "Proposals that only trade off one split against the other are rejected" | Held-out split never shown to the proposer; only the harness file is editable; edits must touch "only the surface needed" | None; the authors say higher-stakes changes need stronger gates |
| Meta-Harness (2603.28052) | An agentic proposer searches harness code, reading the source, scores and traces of every prior candidate from a filesystem | Score on the search set; reported on held-out models | Argues text optimizers "compress feedback too aggressively" | None |
| Continual Harness (2605.09998) | Reset-free online refinement of prompt, sub-agents, skills and memory; grew out of human-in-the-loop harness work on Gemini Plays Pokémon | Abstract states no acceptance test `[unverified: full text not read]` | Not described in the abstract | Removed by design ("removes the human fully from this loop") |
| MetaClaw (2603.17187) | Skills synthesised from failures at once; LoRA RL updates in user-inactive windows chosen from inactivity and calendar data | Not stated in the abstract | "A versioning mechanism separates support and query data" so data collected before a new skill does not train the policy | No approval step described |
| EnvHarness / EnvRigger (2608.19880, Google) | Plug-in layer that reshapes a static environment; EnvRigger synthesises components from trajectories and validates them by fresh rollouts | Fresh rollouts; held-out instances | "Every reshaped environment retains its original verifier" | None |
| CIPHER / PRELUDE (2404.15269) | Infers a short preference text from (agent output, user edit) pairs; retrieves the k nearest contexts' preferences | Lower cumulative edit distance: email 31,103 → 8,391; summaries 48,269 → 32,974 (oracle 1,851 and 6,573) | Edits below a cost threshold δ are not mined, because the LLM would invent a preference. Simulated GPT-4 user only; 7-person human check | User could read and edit the preferences; not built or tested |
| PROSE (2505.23815, Apple) | Refines inferred preferences and verifies each across several of the user's writing samples | +33% generation quality over CIPHER | Cross-sample verification | Same as CIPHER |
| Sleep-time compute (2504.13171) and Letta | Precompute over a context before queries arrive: about 5× less test-time compute; gain tracks how predictable queries are. Letta splits a primary agent (no memory-edit tools) from a sleep-time agent that edits memory blocks. Letta's current "dreaming" runs after N agent steps or at compaction | n/a | Optional "Agent reviews before applying", a second agent pass, "doesn't ask for your approval" | None |
| LangMem procedural memory | Prompt optimisers (`gradient`, `metaprompt`, `prompt_memory`) rewrite system prompts from scored trajectories; memories form "in the hot path" or in the background after inactivity | Feedback score on trajectories | None described | Profiles "easy to present to a user for manual editing" |

Every requested paper was found. mem0's procedural memory was not checked (LangMem
stands in for it); secondary write-ups such as VentureBeat's "up to 60%" for
Self-Harness were not used.

### What carries over

1. **Separate proposing from accepting.** Every system that reports regressions or
   tampering adds a separate gate; the survey names the failure. In loci the gate
   is the user, and that stays (D109).
2. **Measure on what the edit was not written against.** Self-Harness, HSI and
   Harness-R1 hold a split back from the proposer; RRSI's ablation shows the
   evolve-split score rising while out-of-distribution score falls.
3. **Rerun what already passed.** Harness-R1's reward counts regressions on tasks
   that succeeded before.
4. **Small, attributable edits.** RRSI's edit budget and Self-Harness's "minimal"
   proposals make each kept change traceable to one cause.
5. **Freeze the evaluator.** HSI's frozen anchor and EnvHarness's untouched verifier.
   The tampering audit finds evaluation and recording the most-tampered roles.
6. **Raw evidence, not summaries.** Meta-Harness gives the proposer full traces.
7. **Don't score old data against new rules.** MetaClaw's support/query versioning.

## 2. (a) Authorship for learning

### Commit trailers: rejected

- git supports trailers natively (`git commit --trailer`, `git interpret-trailers`,
  `git log --format='%(trailers:key=...)'`). GitHub credits `Co-authored-by:`
  lines as authors.
- The Linux kernel added `Assisted-by: AGENT_NAME:MODEL_VERSION` to its docs in
  7.0 and later cut it to `Assisted-by: LLM`. In October 2026 the maintainers agreed
  to drop it: Sasha Levin's removal patch (posted 2026-10-09) followed a Maintainers
  Summit consensus, and as of 2026-10-23 the section is gone from linux-next's
  documentation but still present in mainline 7.3 (The Register, 2026-10-14;
  Phoronix; freenode). The argument, as reported: a voluntary tag is mostly absent,
  so its absence says nothing. The commit id 1e035297ffda and the figure "about 10%
  of 18,533 non-merge commits in 7.3 carried the tag" come from a third-party
  reference and the subagent's reading; neither was confirmed at git.kernel.org
  `[unverified]`. The kernel still forbids agents
  from adding `Signed-off-by:`; the human remains responsible.
- loci's user has the same problem: a trailer they must remember is unenforceable.
  Rejected (also the user's constraint).

### Agent snapshot refs: recommended

**Mechanism.** At the end of each agent task, loci records the working tree as a
commit on a local ref, without touching the user's index or branches:
`GIT_INDEX_FILE=<tmp> git read-tree HEAD`, `git add -A`, `git write-tree`,
`git commit-tree <tree> -p HEAD`, `git update-ref refs/loci/agent/<utc-time>`. Skip
when the tree equals HEAD or the previous snapshot. `improve` then reads
`git diff refs/loci/agent/<t> <next user commit with the same parent>`: an exact
record of what the user changed in the agent's output. A snapshot whose changes
appear in no later commit marks a rejected task (`git reset --hard`), itself a
signal.

**Trigger.** It must run only for agents. `loci lint` cannot snapshot
unconditionally: the user often runs it after correcting the agent and before
committing, and that later snapshot would hide the corrections. Options:

| Trigger | Agent must remember? | Covers | Cost |
|---|---|---|---|
| `loci lint --snapshot`, which the framework rules require as the last step instead of plain `loci lint` | yes, as today's rule to run `loci lint` | every agent with a shell | one flag, one line in rules.md |
| Claude Code Stop hook running the same command | no | Claude Code only; fires every turn ("when Claude finishes responding"), so dedupe by tree id | per-agent configuration; not adopted: the user ruled out per-agent hooks, and the home-manager module was removed (D106) |
| A record in `.loci/` instead of a ref | — | — | an object id with no ref is pruned by `git gc` (2-week grace), so the diff itself would have to be stored |

Recommend the flag only.

**Push, clone, gc and privacy** (git-push, git-gc docs):

- Plain `git push` follows `push.default` (default `simple`: the current branch) or
  `remote.<name>.push`; `--all` pushes `refs/heads/*`, `--tags` `refs/tags/*`. None
  pushes `refs/loci/*`. Only `--mirror` (or a `remote.<name>.mirror` remote, or a
  configured refspec) does. A clone "creates remote-tracking branches for each
branch" (git-clone), so other clones never see `refs/loci/*`.
- `git gc` keeps "anything else in the `refs/*` namespace", so snapshots survive
  until deleted. The snapshot command should delete refs older than a fixed age
  (e.g. 180 days `[design inference]`) so they do not grow without bound.
- Snapshots stay on the machine and contain only vault text the agent wrote, so
  nothing new leaves the machine. A user who mirrors the vault pushes them to their
  own remote.

**Failure modes.**

| Case | Effect | Severity |
|---|---|---|
| Agent skips the step | No snapshot; `improve` treats that commit as unknown and skips it | Safe: missed data, never misattribution |
| User edits between two agent turns of one task | Those edits are inside the final snapshot, so they are not seen as corrections | Under-counts |
| User commits part of the change (`git add -p`) | Uncommitted agent hunks look like deletions | Compare only paths in the commit; treat the rest as pending |
| User amends or squashes several tasks | Several snapshots share one commit | Diff against the latest snapshot whose parent is the commit's parent |
| User runs `git push --mirror` | Snapshots reach their remote | Documented, not prevented |

**Against D73.** D73 removed job start/accept/reject, the job record, commits as
`loci-agent` and authorship tracking, because they changed how the user commits.
A snapshot changes nothing in the user's flow: no record to clean up, no commit
identity, no blocking state, and loss is harmless. It adds one flag and one ref
namespace. It does reintroduce a narrow form of authorship tracking, so it needs a
new DECISIONS entry that revisits D73 explicitly.

## 3. (b) Learning preferences from edits

loci already says `STYLE.md` must come from text the user wrote (research/10 §2).
With snapshots, the snapshot→commit diff of a note's prose is exactly CIPHER's
(agent output, user edit) pair, from the real user, not a simulator.

Proposed `improve` step 1 addition (`share/loci/skills/improve/SKILL.md`):

1. For each prose hunk the user changed, ignore edits below a small size (CIPHER's
   δ: tiny edits produce invented preferences) and edits that fix facts rather than
   style.
2. Write one candidate preference per hunk ("uses active voice in definitions").
3. Propose a `STYLE.md` line only when at least two hunks from different tasks
   support it (PROSE's cross-sample check), citing them. Update or delete an
   existing line rather than append (D108).
4. Count only corrections made after the matching `STYLE.md` line's date, so old
   edits are not scored against new rules (MetaClaw).

Skip CIPHER's per-context retrieval: `STYLE.md` is one short global file, and
per-area preferences belong in `AREA.md` or a `STYLE.md` subsection `[design
inference]`.

## 4. (c) Regularisation and anti-tampering

loci's position resembles HSI's frozen anchor: the `improve` skill, `loci lint` and
`loci bench` are compiled into the binary and cannot be edited from the vault. What
`improve` can edit includes two evaluation inputs: `bench/retrieval.jsonl` and the
`loci.toml` budgets. Those are where tampering would go (the audit's "evaluation"
and "measurement validity").

Changes to `share/loci/skills/improve/SKILL.md` §3–4:

1. **One mechanism per proposal** (RRSI, Self-Harness). A rule, lesson or style
   change is one item; test it alone.
2. **Never edit what the proposal is tested by.** A proposal that changes
   `RULES.md` or `STYLE.md` is tested against the bench cases as they are at
   `HEAD`. Adding cases is a separate proposal, made first, with no rule change in
   the same diff.
3. **Acceptance rule** (Self-Harness, Harness-R1). Run every case, including those
   that already pass. Report a change as helping only if no case regresses and at
   least one improves. Run the baseline twice first; differences within that spread
   are noise (RRSI's δ).
4. **No case-specific text.** Reject a lesson that quotes a bench query or expected
   note (RRSI's critic).
5. **Show the raw evidence.** Paste the commits, hunks and `loci bench --json`
   output, not a summary (Meta-Harness, the audit's "representational fidelity").

Encode the riskiest part as a check: a diff-aware lint warning, like
`material-dropped` (D73), named for example `check.relaxed`. It fires when the
working tree removes or changes a `bench/retrieval.jsonl` case, or raises a
`loci.toml` budget, compared with `HEAD`. The user sees it in `git diff` review
whoever made the change. It never blocks.

## 5. (d) Scheduled or idle-time review

- Sleep-time compute assumes a service that holds the context and predictable
  queries. MetaClaw's scheduler watches keyboard idle time and the user's calendar.
  Letta's dreaming runs after N steps. All three apply their edits with no user
  approval.
- loci agents run only when asked, and the user commits before every request. An
  unattended run would leave edits in the tree the user is working in.
- Proposed: `loci status` prints one line when unreviewed agent snapshots exist
  ("12 agent snapshots since the last review; see the `improve` skill"). `improve`
  deletes the snapshot refs it has reviewed once the user has decided, so the count
  resets with no extra state. Cheap and passive.

## 6. Ranked recommendations

| # | Change | File / command | Value | Cost |
|---|---|---|---|---|
| 1 | `improve` acceptance rule, one mechanism per proposal, never edit own tests, raw evidence | `share/loci/skills/improve/SKILL.md` §3–4 | high: closes the tampering paths the sources found | low: skill text |
| 2 | Agent snapshot refs via `loci lint --snapshot` (adopted as `loci done`, D110); rules.md "Workflow" step 3 names the flag; DECISIONS entry revisiting D73 | `crates/loci/src/commands/lint.rs`, `crates/loci-core/src/git.rs`, `share/loci/rules.md` | high: exact corrections for (a), (b) and (d) | medium: plumbing and tests |
| 3 | `check.relaxed` diff-aware lint warning (not adopted: the user decided against it) | `crates/loci-core/src/lint.rs` | medium | low–medium |
| 4 | Edit-mined `STYLE.md` candidates with a two-edit threshold | `improve` SKILL.md step 1 | medium; needs #2 | low |
| 5 | `loci status` count of unreviewed snapshots; `improve` deletes the refs it reviewed | `crates/loci/src/commands/status.rs`, `improve` SKILL.md step 5 | low–medium | low |

### What to avoid

- Commit trailers or anything the user must remember (kernel `Assisted-by`).
- Snapshotting on every `loci lint`: the user's own pre-commit lint would hide
  their corrections.
- Background or scheduled runs of `improve`, and any change applied without the
  user's diff review (Letta, MetaClaw, LangMem, and every harness paper above).
- An LLM judge as the only evidence (RHI); use bench results and the user's edits.
- Scores, votes or weight updates (Harness-R1, MetaClaw): they need a training
  loop loci does not have.

## Sources

Harness self-improvement:
- RHI: https://arxiv.org/abs/2607.15524 (method, criteria: https://arxiv.org/html/2607.15524)
- Harness-R1: https://arxiv.org/abs/2608.02276 (reward, validation, held-out: https://arxiv.org/html/2608.02276)
- HSI: https://arxiv.org/abs/2608.08466 (§2.4, §3.2, §4.1, §A.3: https://arxiv.org/html/2608.08466); code https://github.com/TailinZhou/hsi
- Auditing Harness Tampering: https://arxiv.org/abs/2609.00069 (taxonomy, rates, auditors: https://arxiv.org/html/2609.00069)
- RRSI: https://arxiv.org/abs/2609.24972 (Eq. 4, critic, pruner, Table 2: https://arxiv.org/html/2609.24972); code https://github.com/google-research/rrsi
- Self-Improvements survey: https://arxiv.org/abs/2607.13104 (§8.1, §9.1: https://arxiv.org/html/2607.13104)
- Self-Harness: https://arxiv.org/abs/2606.09498 (§3.4, §4.1: https://arxiv.org/html/2606.09498)
- Meta-Harness: https://arxiv.org/abs/2603.28052
- Continual Harness: https://arxiv.org/abs/2605.09998 (abstract only)
- MetaClaw: https://arxiv.org/abs/2603.17187 ; https://github.com/aiming-lab/MetaClaw
- EnvHarness / EnvRigger: https://arxiv.org/abs/2608.19880 ; https://github.com/google-research/envharness

Preferences from edits and background memory:
- CIPHER / PRELUDE: https://arxiv.org/abs/2404.15269 (§3–5: https://arxiv.org/html/2404.15269)
- PROSE: https://arxiv.org/abs/2505.23815
- Sleep-time compute: https://arxiv.org/abs/2504.13171 ; Letta blog: https://www.letta.com/blog/sleep-time-compute ; dreaming: https://docs.letta.com/guides/agents/architectures/sleeptime
- LangMem: https://langchain-ai.github.io/langmem/concepts/conceptual_guide/ ; optimiser kinds: https://langchain-ai.github.io/langmem/reference/prompt_optimization/

Authorship and git:
- Linux kernel commits 78d979db6cef, 816d9992d9ed, 1e035297ffda (https://github.com/torvalds/linux/commit/1e035297ffda); current `Documentation/process/coding-assistants.rst` and `generated-content.rst`
- git trailers: https://git-scm.com/docs/git-interpret-trailers ; `%(trailers)`: https://git-scm.com/docs/pretty-formats
- GitHub `Co-authored-by`: https://docs.github.com/en/pull-requests/committing-changes-to-your-project/creating-and-editing-commits/creating-a-commit-with-multiple-authors
- git push defaults: https://git-scm.com/docs/git-push ; clone: https://git-scm.com/docs/git-clone ; gc roots and expiry: https://git-scm.com/docs/git-gc
- Claude Code hooks (Stop, configuration locations): https://code.claude.com/docs/en/hooks
- Kernel `Assisted-by` removal coverage: https://www.theregister.com/ai-and-ml/2026/10/14/linux-kernel-devs-drop-obligatory-confession-of-ai-tool-usage/5303140 ; https://www.phoronix.com/news/Linux-Dropping-Assisted-By-Req ; https://freenode.net/article/kernel-maintainers-move-to-drop-assisted-by-ai-attribution-tag
