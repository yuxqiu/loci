# 2026-11-01: linking suite, `loci links` pointers on and off

- **Question:** do steps 3 and 4 of "Adding a concept" pointing at `loci links`'
  unlinked mentions (D165, D172) help an agent link a new concept both ways?
- **Setup:** the sample vault (39 concepts, with unlinked mentions of four concepts
  that do not exist yet), the 4 tasks of `suites/linking.jsonl` (add A* search,
  red-black tree, momentum, determinant), checked by `checks/new_concept.py` (one new
  concept in the right folder, the named notes link it, no unlinked mentions either
  way, lint clean). Arms: `current` and `nolinks` (`arms/make_arms.py`: the same rules
  without the `loci links` pointers). Claude Code headless, Sonnet and Haiku, 3
  repeats: 48 runs.
- **Decision rule (set before running):** keep the pointers if `current` beats
  `nolinks` beyond run-to-run noise for some model; otherwise they are prose with no
  measured effect.

| model | current | nolinks |
|---|---|---|
| Haiku | 11/12 | 7/12 |
| Sonnet | 12/12 | 12/12 |

Per task, Haiku: A* search 3/3 against 0/3, red-black tree 2/3 against 1/3,
momentum and determinant 3/3 in both.

**Result: suggestive for the small model, not conclusive.** Sonnet links fully either
way. Haiku did better with the pointers (11/12 against 7/12; Fisher's exact test
p = 0.16 overall, p = 0.10 on A* search alone): the gap sits in the two tasks whose
concept is mentioned in two existing notes, where a missed link back is easiest. With
3 repeats this does not meet the decision rule; the pointers stay (they cost a few
lines) and the question stays open. **Next:** more repeats on the two tasks that
separate the arms, and `loci bench` keeping each check's output so failures can be
read (today only pass or fail is recorded).
