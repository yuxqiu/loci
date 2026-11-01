---
name: learn-session
description: Learn material the user does not understand ("learn X") — find the missing concepts out to the boundary of what they know, teach them, and write cards. Use when the user asks to learn or understand a paper, chapter, link or question.
---

# Learn session

If you have not read them this session, read the framework rules (`loci rules`),
`RULES.md` and `STYLE.md` first.

The input is material the user does not understand: a paper, a chapter, a link or a
question. Build the knowledge needed to understand it, searching outward until every
path reaches something the user already knows.

## Recording

- Grades go to the review log with `loci learn log` (one JSON object on stdin):
  `{"card": "<card id>", "grade": "<grade>"}` for a card, or
  `{"concept": "<concept id>", "grade": "<grade>"}` for a question on a concept that
  has no card. A grade is `again`, `hard`, `good` or `easy`.
- The session's questions on the material, their answers and grades, are recorded in
  a dated journal entry for the session in `journal/learn/` (named as the framework
  rules say), not in the review log.
- `loci learn state <slug>` shows what is known about a concept.

## Grading

Before showing a question, fix its reference answer and a short rubric. Grade the
answer against the rubric: every criterion met is `good`, some met is `hard`, none
met is `again`. Show the grade before logging it. If the user disputes it, re-judge
from scratch without looking at the first grade, and log only the final grade.

## Steps

0. **Questions.** Draft questions covering the claims the material cannot be
   understood without; the user may edit, add or drop them. The user answers them
   before any teaching. Record questions, answers and grades in the session's journal
   entry.
1. **Identify.** File the material as a source (framework rules: `loci rules`,
   "Filing a source"). List the concepts it uses, with
   the passages that use them. Each is load-bearing (the material cannot be
   understood without it) or peripheral (it gets a one-line definition where it
   appears).
2. **Sort.** Pre-fill each load-bearing concept as know, shaky or don't know, from
   the knowledge base (slug, `loci find`) and `loci learn state`. The user
   corrects each. Know and shaky are the boundary; don't-know concepts are expanded.
   For a concept already in the knowledge base, offer its `prereqs` as the next level.
3. **Expand.** For each don't-know concept, find external material that explains it
   (the introducing paper, a textbook chapter, documentation), file it as a source,
   list its load-bearing prerequisites and sort them as in step 2.
   Repeat until every branch reaches the boundary or the user stops. New concept
   notes record their `prereqs`. Watch for synonyms (duplicate expansion) and
   homonyms (a false "know").
4. **Plan and diagnose.** Write the plan to `views/learn/<slug>.md`, ordered from
   the boundary inward. Create notes with `loci new` for every new or shaky concept
   that has none. Diagnose each shaky concept until it is settled: a clear pass makes
   it known, a clear fail makes it don't-know, a mixed result gets another item from a
   different angle. The user may settle any concept directly.
5. **Teach,** from the boundary inward, in plan order until the user ends the session
   (the plan keeps the place): a short direct explanation, a worked example only for
   unseen material, then a step the user does themselves. Never do that step for
   them. New notes cite the fetched sources.
6. **Return to the material.** Link its passages to the new concepts. Ask the step-0
   questions again and record the new answers and grades in the journal entry.
7. **Cards.** Write cards in each concept's `## Cards` section until every idea worth
   remembering is covered, drawing on items the user answered; the user approves
   each. Split a proof into key-step and why cards. Write cards without ids, then run
   `loci block-ids <note> --section Cards`. The step-0 questions become cards on the
   closest concept.

Finish with `loci done`.
