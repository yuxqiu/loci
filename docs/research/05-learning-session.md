# Designing the "learn X" session: diagnosis, teaching, grading, review

Researched 2026-10-30. Scope: one advanced learner (graduate level, technical domain) working in a terminal with an LLM. The pipeline is fixed upstream: resolve X → prerequisite closure from concept notes → join with the learner model (`unseen` / `encountered` / `known`, where only graded recall makes `known`) → diagnostic → plan → teach and write notes → closing check → FSRS cards. This note answers the open questions: how many diagnostic items, which item types, grading and disputes, session structure, and teaching style. It builds on `01-literature-review.md` §4 and `02-deep-dive.md` §4 (knowledge tracing, open learner models, FSRS, LLM card generation) and does not repeat them.

Conventions: **[unverified]** = I did not confirm this against the primary source. **[design inference]** = my extrapolation, with no study behind it.

---

## 1. Adaptive diagnosis: how few items are enough?

| Source | Venue / year | Finding relevant here |
|---|---|---|
| Babcock & Weiss, "Termination criteria in CAT" — https://doi.org/10.7333/1212-0101001 | J. Computerized Adaptive Testing 1, 2012 | In simulation, the stop-when-SE(θ)-falls-below-threshold rule did better than minimum-information and θ-convergence rules. They recommend combining rules and adding a **minimum length** (as reported by citing papers). I did not check which SE cutoffs they tested **[unverified]**. |
| Liu & Weiss — https://doi.org/10.1177/00131644261453945 | Educ. Psych. Measurement, 2026 | With high-information item banks, SE-based and fixed-length rules gave the lowest conditional RMSE (abstract only). |
| Matayoshi et al., "A practical perspective on KST: ALEKS and its data" — https://jmatayoshi.github.io/publications/JMP2021_KST_ALEKS_preprint.pdf | J. Math. Psych., 2021 | ALEKS runs until one knowledge state has very high probability. Its structures can have about 2^77 states, yet the initial assessment is **capped at 30 questions (29 adaptive)**, a cap set from user feedback. It gets "a fairly accurate picture" early and spends the remaining items on fine-tuning. |
| Doignon & Falmagne — https://arxiv.org/abs/1511.06757 | arXiv 2015 (theory: *Learning Spaces*, Springer 2011) | Efficient assessment in about 25–35 questions. In a learning space, knowledge states are closed under prerequisites, so one answer gives information about many items. |
| Eppstein, "Learning sequences" — https://arxiv.org/abs/0803.4030 | arXiv 2008 | If each answer roughly halves the feasible states, the number of questions is about log₂(#states). |
| LLM-run CAT (AgentCAT) — https://arxiv.org/abs/2606.21832 | arXiv 2026 | Uses LLM agents for item selection with knowledge-graph exploration. This is simulation work, not a learner study. Most 2024–26 "LLM + CAT" papers point the other way and use CAT to evaluate LLMs (e.g., ATLAS, https://arxiv.org/abs/2511.04689). |

**Implications.** (a) IRT/CAT stopping rules depend on calibrated item parameters from large populations. A single learner with freshly generated items has none, so SE-based stopping cannot be used literally. (b) The KST idea does transfer: the prerequisite DAG *is* a surmise relation. A correct answer on concept *c* is evidence for all of *c*'s prerequisites, and a wrong answer is evidence against everything that depends on *c*. So item selection is a binary search over the DAG, and the information bound is about log₂ of the number of prerequisite-closed states in the frontier sub-DAG. A frontier of 6–10 concepts with typical chain-plus-branch structure has tens to low hundreds of closed states, i.e. roughly **4–8 well-chosen binary items** **[design inference]**. (c) ALEKS caps its assessment for fatigue reasons, not information reasons. A cap is a legitimate design choice.

## 2. What tutoring evidence says about style

| Source | Venue / year | Finding |
|---|---|---|
| Bloom, "The 2 Sigma Problem" | Educational Researcher 13(6), 1984 — https://doi.org/10.3102/0013189X013006004 | One-to-one tutoring combined with mastery learning ≈ +2 SD. |
| VanLehn, "Relative effectiveness of human tutoring, ITS, …" | Educational Psychologist 46(4), 2011 — https://doi.org/10.1080/00461520.2011.611369 | Re-analysis: human tutoring d = 0.79 and step-based ITS d = 0.76, not 2.0. Tutoring that interacts at the **step** level gets most of the benefit. |
| Kulik, Kulik & Bangert-Drowns, mastery learning meta-analysis | Rev. Educ. Research 60(2), 1990 — https://doi.org/10.3102/00346543060002265 | 108 evaluations. Effect ≈ 0.5 SD (exact value not confirmed **[unverified]**). Higher mastery thresholds work better. Mastery programs increase time on task. |
| Adesope, Trevisan & Sundararajan, practice-testing meta-analysis | Rev. Educ. Research 87(3), 2017 — https://doi.org/10.3102/0034654316689306 | Testing vs. restudy +0.51. Effects are larger at 1–6-day delays (+0.82) than under 1 day (+0.56), and they hold for transfer (0.53). |
| Roediger & Karpicke, test-enhanced learning | Psychological Science 17(3), 2006 — https://doi.org/10.1111/j.1467-9280.2006.01693.x | Retrieval beats restudy on delayed tests, even though restudy looks better on immediate tests. |
| Richland, Kornell & Kao, pretesting effect | J. Exp. Psych.: Applied 15(3), 2009 — https://doi.org/10.1037/a0016496 | Failed retrieval attempts *before* study improve later learning. The diagnostic itself therefore has teaching value. |
| Bisra et al., self-explanation meta-analysis | Educ. Psych. Review 30, 2018 — https://doi.org/10.1007/s10648-018-9434-x | Prompting learners to self-explain: g = 0.55 [0.45, 0.65] across 69 effects. |
| Kalyuga, Ayres, Chandler & Sweller, expertise reversal | Educational Psychologist 38(1), 2003 — https://doi.org/10.1207/S15326985EP3801_4 | Worked examples help novices, but the advantage disappears and can reverse as expertise grows. Guidance should fade (cf. Renkl & Atkinson, same issue). |
| Brunmair & Richter, interleaving meta-analysis | Psychological Bulletin 145(11), 2019 — https://doi.org/10.1037/bul0000209 | g = 0.42 overall and about 0.34 for math. Interleaving helps most when categories are similar and easy to confuse. It hurts for some materials (word lists). |
| Kestin et al., Harvard physics AI-tutor RCT | Scientific Reports 15:17458, 2025 — https://pmc.ncbi.nlm.nih.gov/articles/PMC12179260/ | N = 194, crossover design. The AI tutor beat in-class active learning: median post 4.5 vs 3.5, gains more than doubled, median 49 min vs 60. Design features: **pre-written step-by-step solutions in the prompt** ("avoided relying solely on GPT-4 to generate solutions"), the platform sequenced problem parts (the prompt alone let the model go out of order), plus cognitive-load and growth-mindset guidelines. Limitations: immediate post-test with a ceiling effect, one course, feedback only when asked. |
| Bastani et al., "GenAI without guardrails can harm learning" | PNAS 122(26), 2025 — https://doi.org/10.1073/pnas.2422633122 | About 1,000 high-school students. Unguarded GPT-4 raised practice scores by 48% but **lowered exam scores by 17%** once access was removed. "GPT Tutor" (teacher solutions plus hints, told not to give answers) removed the harm but gave no gain. Students in both arms **overestimated** how much they had learned. |
| De Simone et al., Nigeria GPT-4 after-school RCT | World Bank PRWP 11125, 2025 — https://doi.org/10.1596/1813-9450-11125 | 6 weeks of 90-min sessions: +0.31 SD overall, about 0.23 SD English. Sensitive to excluding one school (0.156). |
| LearnLM Team, "Improving Gemini for Learning" | Google tech report, 2024 — https://arxiv.org/abs/2412.16429 | Treats pedagogy as *instruction following*: the system prompt states the desired pedagogical behaviours. Measured by expert preference, not learning outcomes. |

**Socratic vs. direct, for experts.** I found no RCT that compares Socratic and direct-explanation LLM tutoring for advanced learners. The K-12 evidence summarized by Stanford SCALE (https://scale.stanford.edu/sites/default/files/The%20Evidence%20Base%20on%20AI%20in%20K-12%20Report.pdf) is mixed: Socratic bots raise engagement but are rated less helpful, and gains on tests are inconsistent. Expertise reversal points toward **direct, compact explanation plus an active check** for a graduate-level learner, rather than drawn-out questioning. The guardrail lesson from Bastani applies only to *checks*: the system must never solve the check item for the learner.

## 3. LLM grading reliability

| Source | Venue / year | Finding |
|---|---|---|
| Ma et al., ProofBench / ProofGrader | ICLR 2026 — https://arxiv.org/abs/2510.13888 | 145 competition problems and 435 proofs scored 0–7 by experts. The best evaluator (strong reasoning model + **reference solution + marking scheme** + simple ensemble) reaches MAE 0.926 on the 0–7 scale. Context and ensembling mattered. |
| "Can LLMs Reliably Grade Olympiad Proofs?" | arXiv 2026 — https://arxiv.org/abs/2610.36767 | Even with full grading guidelines, models stay below 0.70 agreement, versus 0.76 between experts (from the search abstract) **[secondary read]**. |
| "Cost-Effective Automated Judging of NL Proofs" | arXiv 2026 — https://arxiv.org/abs/2608.00004 | For **pass/fail against a reference**, cheap judges match frontier judges. Judges are less strict on their own model family's proofs **[secondary read]**. |
| Short-answer grading surveys (e.g., Ferreira Mello et al., LAK 2025, https://dl.acm.org/doi/10.1145/3706468.3706481) | 2024–26 | GPT-4-class models typically reach κ/QWK ≈ 0.6–0.87, and results swing with rubric design. A dental-education study found near-zero κ for zero-shot rubric scoring (https://link.springer.com/article/10.1186/s12903-026-07665-4). Agreement degrades on **mid-quality answers** (https://arxiv.org/abs/2605.07647). |
| Kim & Khashabi, "Challenging the Evaluator: LLM Sycophancy Under User Rebuttal" | Findings of EMNLP 2025 — https://aclanthology.org/2025.findings-emnlp.1222/ | LLM evaluators readily accept user counterarguments. Rebuttals that contain reasoning get *correct* reversals more often. Casual assertiveness causes indiscriminate flips, both correct and incorrect. |
| Bull & Kay, SMILI☺ open learner models | IJAIED 26(1), 2016 — https://doi.org/10.1007/s40593-015-0090-8 | The learner should be able to inspect and contest the model (already cited in 02 §4). |

**Implications.** Reliability is acceptable for **binary, criterion-level** judgments made against a **pre-committed reference and rubric**. It is weaker for holistic scores and for partially correct answers, which are exactly the ones that decide `encountered` vs `known`. Disputes are a known sycophancy attack surface, so a regrade must not see the learner's assertion as a status signal.

## 4. Item types and cards

The revised Bloom taxonomy (Anderson & Krathwohl, 2001; book, not checked online) gives the levels. Kestin's post-tests were 41% *apply*, 33% *analyze*, 21% *understand* and 4% *remember*, which is a reasonable target mix for a technical domain.

| Item type (algorithms example) | Bloom level | Discriminates |
|---|---|---|
| **Define/state** ("State the soundness property of a Σ-protocol.") | Remember | `unseen` vs `encountered`. Cheap and easy to bluff. |
| **Explain in own words / why** ("Why does Dijkstra's algorithm need non-negative edge weights?") | Understand | Surface vs. structural understanding. Self-explanation evidence applies. |
| **Predict / compute** ("What happens to proof size if the FRI blowup factor doubles?") | Apply | Usable knowledge. Easy to grade against a reference. |
| **Counterexample / break it** ("Give an input on which greedy coin change is not optimal.") | Analyze | Strong mastery signal. Needs a reference attack to grade. |
| **Derivation step / fill the gap** (one missing step in a correctness proof or a complexity argument) | Apply/Analyze | Step-level interaction, in the spirit of VanLehn. A natural faded worked example. |
| **Transfer** (same idea, new setting: "Where does this bottleneck appear in a database join?") | Analyze/Evaluate | Best evidence of `known`. Most expensive to grade reliably. |

**FSRS cards.** Matuschak's prompt properties (https://andymatuschak.org/prompts/): focused, precise, consistent, tractable, effortful. 01 §4 already established that LLMs pick good targets but construct cards poorly (Kirkby & Matuschak 2026: about 36% of generated prompts unusable, LLM judges below 70% accuracy). So cards should be drafted *from items the learner already answered in the session*, one idea per card, and the learner approves each one.

## 5. Session structure, attention, confidence

- **Length.** The "attention drops after 10–15 min" claim has little empirical support (Wilson & Korn, Teaching of Psychology 34(2), 2007, https://doi.org/10.1080/00986280701291291). Observed effective formats: about 49 min median (Kestin) and 90-min sessions (De Simone). Spacing across sessions has stronger support than any within-session length: the optimal gap grows with the retention interval (Cepeda et al., Psych. Bulletin 132(3), 2006, https://doi.org/10.1037/0033-2909.132.3.354).
- **Immediate checks overstate retention.** Roediger & Karpicke and Adesope's delay moderator both show this, and Bastani's learners overestimated their learning. A same-session check is weak evidence for durable `known`.
- **Confidence ratings.** The hypercorrection effect says high-confidence errors are corrected *more* readily once feedback is given (Butterfield & Metcalfe, JEP:LMC 27(6), 2001, https://doi.org/10.1037/0278-7393.27.6.1491). However, those errors can come back later (Butler, Fazio & Marsh, PB&R 2011, https://doi.org/10.3758/s13423-011-0173-y). Using self-reported confidence as an input to the *learner model* has thin evidence: one iBKT study (https://ojs.istp-press.com/jait/article/view/319) and one preprint (https://arxiv.org/abs/2606.28881) report better prediction, with no replication found. Certainty-based marking (Gardner-Medwin) is an established practice in medical education, but I did not read its calibration data **[unverified]**.

---

## 6. Recommended protocol

### 6.1 Diagnostic (target 4–8 items, ≤ 12 min)

1. **Frontier** = the concepts in the closure of X that are not `known`, plus `known` concepts whose FSRS retrievability is below 0.8 and that lie on X's critical path. Skip any `unseen` concept that has an `unseen` prerequisite, because it is inferred `unseen` for free (KST closure).
2. **Selection.** Pick the frontier concept whose outcome splits the remaining prerequisite-closed states most evenly. Usually that is the middle of the longest chain. Pass → mark it and its ancestors "likely ok". Fail → mark it and its descendants "needs teaching". Recurse.
3. **Item type per state.** `encountered` → one *predict/compute* or *counterexample* item (Apply/Analyze), not a definition. `known`-but-decaying → its existing FSRS card or a short *why* item. `unseen` → at most one *predict* question used as a **pretest**: it is not scored toward state, and it is shown again after teaching (pretesting effect).
4. **Stopping rule.** Stop when every concept on the plan's critical path is either directly tested or implied by closure, **or** after 8 items. Always ask at least 3 items unless the frontier is smaller. "Don't know" is a valid answer: score it as a fail and do not penalize it beyond that. *Validation status: the closure logic is KST and established. The specific numbers are design inference.*

### 6.2 Grading and dispute

- **Commit before asking.** Each item is generated together with a reference answer and a 2–4-criterion rubric of binary checks ("names the binding problem", "correct direction of the inequality"). These are stored before the learner sees the item. Grounding in the cited source (paper section, textbook) plays the role of Kestin's pre-written solutions.
- **Grade per criterion** as pass/fail, quoting the learner's text as evidence for each pass. Outcome: `correct` if all criteria pass, `partial` if some pass, `incorrect` otherwise. Only `correct` is promotion evidence. Use one grader call plus a second independent call. If they disagree, the result is `partial` (ensemble, following ProofGrader).
- **Dispute.** The learner writes a *technical* argument. A **fresh-context** grader sees the item, the rubric, the reference, the original answer, and the argument presented as an addendum to the answer. It does not see the prior verdict or any tone. It may also find that the **reference or rubric is wrong**, which matters for niche technical material where the generator may be wrong. Possible outcomes: `upheld`, `overturned`, `rubric_fixed`, `contested`. A `contested` result never promotes. The concept gets a new item next session. *Why: Kim & Khashabi show that reasoning-bearing rebuttals produce correct flips and assertive ones produce arbitrary flips.*

### 6.3 Teaching loop (per concept, ~8–12 min; 2–4 new concepts per session)

1. If a pretest exists, show the learner's pretest answer.
2. **Direct, compact explanation**: formal definition, the one intuition that matters, and a link to the prerequisite it builds on. Give one worked example only if the concept is `unseen` (expertise reversal).
3. **Active step**: a faded worked example (learner supplies the key step) *or* a self-explanation prompt ("why is this step needed?"). Graded with the same rubric machinery, and never solved for the learner. Hints escalate, but the full answer is given only after a real attempt (Bastani guardrail).
4. **Note writing.** The LLM drafts the concept note, the learner edits or approves, and the note records its source. Matuschak's argument is that the learner doing the writing is elaboration. Optional: the learner writes the one-line summary.
5. The learner can say "skip, I know this". That routes to a single Apply-level item, and a pass promotes (graded recall).

### 6.4 Closing check (≤ 10 min)

- One **new** item per concept taught (not a repeat of an in-session item), interleaved across concepts, preferably Apply/Analyze or transfer, with a confidence rating (1–3) per item.
- `correct` → `known`. Record the evidence and *the fact that it was same-session*. The FSRS card's first review (typically 1–3 days later) is the real durability test. A lapse there demotes to `encountered` **[design choice: the alternative, promoting only after the first spaced review, is more defensible by the delay evidence; pick one and record it in DECISIONS.md]**.
- High-confidence errors get explicit corrective feedback right away (hypercorrection) and an extra card.

### 6.5 Cards

Generate 2–5 cards per concept from closing-check and active-step items, following Matuschak's properties: at least one *why/when* card, not only definitions. The learner approves each card. The first FSRS rating comes from the closing-check result.

### 6.6 Logged learner events (append-only)

`session_start{X, frontier}` · `item_posed{id, concept, type, bloom, purpose: diagnostic|pretest|active|closing, ref_hash, rubric}` · `answer{item, text, confidence?, latency_s}` · `graded{item, per_criterion, outcome, grader_ids}` · `dispute{item, argument}` → `dispute_resolved{outcome, rubric_diff?}` · `inferred{concept, from_item, direction}` (closure inference, kept separate from direct evidence) · `state_change{concept, from, to, evidence:[items]}` · `self_report{concept, "skip"}` · `note_written{concept, path, source}` · `card_created{concept, card_id, approved}` · `session_end{duration, concepts_taught}`. Inferred and direct evidence must stay distinguishable so the open learner model can show *why* it believes something.

## 7. Validated vs. speculative

| Claim | Status |
|---|---|
| Retrieval > restudy; delay matters; immediate tests overstate | Validated (meta-analyses) |
| Self-explanation and step-level interaction help | Validated (Bisra; VanLehn) |
| Worked examples help novices and fade for experts | Validated (lab studies, mostly non-LLM) |
| Structured LLM tutors with pre-written solutions can beat active learning | One RCT (Kestin), immediate post-test, undergraduates |
| Unguarded answer-giving harms learning, and learners overestimate their learning | One large RCT (Bastani), high school |
| Prerequisite closure cuts diagnostic length to about log₂(states) | Validated in KST/ALEKS with expert-built structures. With LLM-extracted prereq edges it is **untested** (02 §4: edge F1 ≈ 0.78) |
| 4–8 items, 3-item minimum, 8-item cap | Design inference |
| Reference + rubric + ensemble improves LLM grading | Validated on olympiad proofs; untested on graduate-level free response in specialised fields |
| Blind fresh-context regrade resists sycophancy | Inferred from Kim & Khashabi; not tested as a protocol |
| Confidence ratings improve the learner model | Weak (two non-replicated studies). Log them, don't gate on them |
| Socratic vs. direct explanation for graduate-level learners | No direct evidence; direct + active check inferred from expertise reversal |
| Session ≈ 45–60 min, 2–4 new concepts | Design inference from Kestin/De Simone timings; no dose-response evidence |
