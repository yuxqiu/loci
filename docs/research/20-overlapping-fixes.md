# 20 — Overlapping fixes and whole-block rewrites

Status: research, 2026-11-02. Builds on research 18 §1 (fixes) and the fix design in
D192; cited, not repeated.

Question: how do mature linters and fixers handle several fixes that edit the same
region, especially whole-block or whole-file rewrites, when fixes are applied pass
after pass? And what should `loci lint --fix` do about frontmatter fixes, which all
rewrite the whole YAML block?

What loci has today (working tree at 3b44af9, 2026-11-02):

- **Fixes are text edits.** `Fix { message, applicability: Safe | Unsafe |
  DisplayOnly, edits: Vec<TextEdit { path, byte_start, byte_end, new }> }`
  (`crates/loci-core/src/envelope.rs` lines 35–61).
- **The driver** (`crates/loci-core/src/fix.rs`): at most `PASSES = 10` passes
  ("ESLint's limit", line 18). In each pass, `apply_pass` takes every allowed fix (the
  first fix per diagnostic that `--only` and `--unsafe-fixes` allow, `Options::pick`)
  whose edits neither overlap nor touch an edit already taken
  (`r.start <= e.range.end && e.range.start <= r.end`). It applies them with
  `splice::apply`, then rebuilds the index and lints again. After the loop, `verify`
  refuses (`lint.fix-verify`) if any error code has more diagnostics than before or if
  a link that resolved now reaches another file. If the limit is reached with fixes
  still pending, nothing says so: the leftover diagnostics are simply reported as
  remaining.
- **Every frontmatter fix replaces the whole block.** `parse::rewrite_frontmatter`
  (`crates/loci-core/src/parse/frontmatter.rs` lines 156–181) loads the block with
  saphyr, runs a closure over the `Mapping`, and returns one `Edit` that replaces the
  block's span with `YamlEmitter`'s output (D215). The callers are `fm.missing-field`
  (add an id), `fm.field-type` (quote every unquoted prereq, `quote_prereqs`), the
  outdated-field warning (remove a key), `fm.id-duplicate` (new ids for the other
  notes, one edit per note), all in `lint/concepts.rs`, and `link.dangling` on a
  prereq (rename one prereq, `lint/links.rs` lines 56–62). So two frontmatter fixes on
  one note always overlap, and each pass applies one of them. A note with more than 10
  such fixes is left partly fixed, and nothing reports it.
- The `fm.field-type` quote fix is already batched: each unquoted-prereq diagnostic
  carries the same fix, which quotes all of them. The second copy then overlaps the
  first and is skipped, and its diagnostic is gone after the next lint.

Conventions as in research 12 to 19: `[unverified]` means I did not confirm the claim
in a primary source; `[design inference]` is my extrapolation; `[gap]` means I looked
and found nothing. Source files were read at the commits listed under Sources; line
numbers refer to those commits. Examples use neutral concepts (`bfs`, `queue`).

## TL;DR

| Tool | Unit of a fix | Overlap rule | What happens to the loser | Passes |
|---|---|---|---|---|
| ESLint | one `{range, text}`; several from one `fix()` are merged into one span | sorted by range; skip if `lastPos >= start` (touching counts) | stays a problem; next pass | 10, plus a circular-fix check |
| Ruff | `Fix { edits, applicability, isolation_level }` | sort by rule tie-breaks, then start; skip if it overlaps, or if its isolation group already applied; identical edits are deduplicated | skipped; next pass | 100, then "Failed to converge … This indicates a bug in Ruff" |
| rustfix / cargo fix | suggestion → solutions → replacements | rejected if overlapping (touching allowed); identical conflicts silently skipped | saved as an error; retried next round; reported as `ReplaceFailed` if still failing | 4 (`CARGO_FIX_MAX_RETRIES`) |
| clang-tidy | per-diagnostic `Replacements` | sweep line over all fixes | **all** overlapping fixes dropped (a containing fix survives), with a note | 1 |
| clang-apply-replacements | replacements from many TUs | identical replacements from other TUs deduplicated; others merged into one `AtomicChange` | conflict → exit 1, nothing applied | 1 |
| LSP | `TextEdit[]` in a `WorkspaceEdit` | "must never overlap" | — | fix-all servers run the CLI's multipass fixer, then **diff** the result into minimal edits |
| rust-analyzer | `SyntaxEditor` changes on a tree | edits from several sources merged into one editor; nested edits remapped; same-depth overlaps make the whole plan invalid | invalid plan → no change, logged | 1; text edits come from a tree **diff** |
| zizmor (`yamlpatch`) | route-addressed YAML ops | none needed: each op re-finds its target in the current document | a fix that no longer applies fails as "conflict after applying previous fixes" | 1, sequential |
| OpenRewrite, LibCST | visitors and transforms over a lossless tree | recipes run one after another on the evolving tree | — | OpenRewrite: up to 3 cycles |

**Recommendation (details at the end):** (c). Give frontmatter fixes a structural
operation as well as their text edit. In each pass, the driver folds all the
operations taken for one note into one `rewrite_frontmatter` call. Selection by
`--only` and applicability stays per fix, and `--json` still shows each fix as its own
exact text edit. Also report loudly when the pass limit is reached with fixes still
pending (Ruff, cargo), and deduplicate identical edits (Ruff, rustfix, clang).
Format-preserving YAML (d) would reverse D215 for a Rust ecosystem that is still
young. Batching (b) and a higher limit (f) only reduce the symptom. Isolation levels
(e) solve the opposite problem.

## 1. ESLint

- **A fix is one range.** `ruleFixer` methods (`insertTextAfter`, `replaceText`,
  `replaceTextRange`, `remove`, …; `lib/linter/rule-fixer.js` lines 84–191) each
  return a `{range, text}`. A `fix()` may return several. `normalizeFixes` merges them
  with `mergeFixes` (`lib/linter/file-report.js` lines 253–291, 301–312) into **one**
  fix spanning from the first start to the last end, with the original text between
  them copied in. They must not overlap (`assert … "Fix objects must not be
  overlapped in a report."`, line 274). So a multi-part fix blocks every other fix
  in its whole span, gaps included.
- **Overlap rule.** `SourceCodeFixer.applyFixes` (`lib/linter/source-code-fixer.js`
  lines 61–140) sorts fixes by range and applies each unless `lastPos >= start`
  ("Remain it as a problem if it's overlapped", lines 91–94). Touching ranges count as
  overlapping, as in loci. The skipped problem stays in the messages.
- **Passes.** `MAX_AUTOFIX_PASSES = 10` (`lib/linter/linter.js` line 43).
  `verifyAndFix` (lines 1522–1660) lints, fixes and repeats "until … No more fixes
  have been applied [or] Ten passes have been made". It stops early if the text
  equals the text from two passes ago ("Circular fixes detected", line 1629, which
  emits a warning). If the last pass applied fixes, it lints once more so the reported
  problems are current. Reaching 10 is not reported (my reading of lines 1637–1660).
- **Guidance on fix size.** "Make fixes as small as possible. Fixes that are
  unnecessarily large could conflict with other fixes, and prevent them from being
  applied" (`docs/src/extend/custom-rules.md` line 359). But "in some cases it may be
  correct to extend the range of the fix in order to intentionally prevent other
  rules from making fixes in a surrounding range in the same pass" (line 376). A fix
  "may not be applied at all if there are conflicts with other fixes … This process
  will repeat up to 10 times" (line 330). `meta.fixable` is mandatory (line 61).
- **Suggestions** (`meta.hasSuggestions`, `suggest: [{desc, fix}]`) are never applied
  by `--fix`. They "are applied as stand-alone changes, without triggering multipass
  fixes" (lines 403–433), the equivalent of loci's `DisplayOnly`.

**For loci:** loci's driver is ESLint's, applied to a whole vault. A loci frontmatter
fix acts like ESLint's deliberately widened fix: it blocks every other change in the
block for that pass. ESLint accepts this cost because its passes are cheap, and it
does not report reaching the limit.

## 2. Ruff

- **Types** (`crates/ruff_diagnostics/src/fix.rs` lines 8–56): `Applicability {
  DisplayOnly, Unsafe, Safe }` (loci copied this) and `IsolationLevel { Group(u32),
  NonOverlapping (default) }`. `Fix { edits (sorted by start), applicability,
  isolation_level }`; `Fix::isolate(level)` (lines 167–171).
- **Applying** (`crates/ruff_linter/src/fix/mod.rs` `apply_fixes`, lines 54–128):
  fixes are sorted by `cmp_fix`, then for each fix:
  1. edits identical to edits already applied are filtered out (`!applied.contains(edit)`,
     line 76), so two fixes that propose the same edit both count as applied;
  2. a `Group(id)` fix is skipped if another fix in that group was applied in this pass
     (lines 83–87);
  3. a fix whose first new edit starts at or before the end of the last applied edit is
     skipped (`last_pos >= first.start()`, line 90).
- **What isolation is for** (`checkers/ast/mod.rs` lines 581–591): "to ensure that we
  don't delete all statements in a given indented block, which would cause a syntax
  error … delete at most one statement per indented block per fixer pass". It is a
  restriction for fixes whose edits do **not** overlap but must not apply together.
  It never lets overlapping fixes apply together.
- **Ordering** (`cmp_fix`, lines 131–170): `RedefinedWhileUnused` always first (a
  comment explains why it is global, to keep the order transitive), then by start
  offset, then hard-coded tie-breaks for specific rule pairs (`MissingTrailingPeriod`
  before `NewLineAfterLastParagraph`, `IfElseBlockInsteadOfDictGet` before
  `IfElseBlockInsteadOfIfExp`).
- **Passes** (`crates/ruff_linter/src/linter.rs`): `MAX_ITERATIONS = 100` (line 374);
  `lint_fix` loops "until the source code stabilizes" (lines 563–650). If fixes are
  still pending after 100 passes, it prints `report_failed_to_converge_error`
  (lines 671–705): "Failed to converge after 100 iterations … This indicates a bug in
  Ruff", with the rule codes involved. It keeps the code fixed so far. If a fix
  introduces a syntax error into a file that parsed, Ruff returns an error and keeps
  the original ("Fix introduced a syntax error", lines 614–623).
- **Whole-block rewrites.** isort's `I001` (`rules/isort/rules/organize_imports.rs`
  lines 86–190) replaces the entire import block, plus surrounding whitespace, with
  `Fix::safe_edit(Edit::range_replacement(expected, fix_range))` (line 183). It
  coexists with the other fixes only through the overlap rule and passes: an
  `F401` unused-import fix inside the block waits for a later pass, or the reverse.
  The rule does not fix at all when the block shares a line with other statements
  ("too hard to get right … flag but don't fix", lines 103–110).

**For loci:** Ruff's answer to whole-block rewrites is the same as loci's (overlap,
pass again), with 10× the pass budget and a loud error when it runs out. Two cheap
lessons: deduplicate identical edits, and fail loudly at the limit. Isolation groups
would add restrictions, which loci has no need for (§7, option e).

## 3. rustfix and `cargo fix`

- **Applicability** (`crates/rustfix/src/diagnostics.rs` lines 52–78):
  `MachineApplicable` ("should be automatically applied … In case of multiple
  `MachineApplicable` suggestions … all of them should be automatically applied"),
  `MaybeIncorrect`, `HasPlaceholders`, `Unspecified`. `cargo fix` uses
  `Filter::MachineApplicableOnly` unless `__CARGO_FIX_YOLO` is set
  (`src/ops/cargo_fix/mod.rs` lines 1010–1013).
- **Conflicts** (`crates/rustfix/src/replace.rs` `replace_range`, lines 143–191):
  a replacement that starts before the previous one ends, or ends after the next one
  starts, is rejected with `AlreadyReplaced { is_identical, range }`. Touching ranges
  are allowed (strict `<`). Two inserts at the same point conflict. `CodeFix::apply`
  (`lib.rs` lines 237–248) is all-or-nothing per suggestion (`restore()` on error).
  `apply_suggestions` skips a suggestion whose conflict is identical to a prior
  replacement (lines 272–296). Its doc comment: "The intent of this design is that the
  overall application process should repeatedly apply non-conflicting suggestions then
  rëevaluate the result, looping until either there are no more suggestions to apply
  or some budget is exhausted."
- **Loop** (`src/ops/cargo_fix/mod.rs` lines 880–967): rustc is rerun because
  overlapping spans are "difficult in the compiler to **not** have" (example
  `::foo::<::Bar>()`, lines 885–900). Suggestions that failed are discarded and
  rustc is rerun. It stops when a round with errors made no change, or after
  `CARGO_FIX_MAX_RETRIES` (default **4**, lines 914–918). Errors still left are
  "probably bugs in Cargo and/or rustfix" and are posted as
  `Message::ReplaceFailed` (lines 959–966). Identical conflicts are not reported
  (lines 1113–1121).

**For loci:** same model (passes plus overlap), a smaller budget, and a loud report of
whatever did not apply.

## 4. clang-tidy and clang-apply-replacements

- **clang-tidy `--fix`**, one pass: `removeIncompatibleErrors`
  (`clang-tools-extra/clang-tidy/ClangTidyDiagnosticConsumer.cpp` lines 642–779) runs
  a sweep line over all fixes' intervals. Touching intervals are not an overlap (end
  events sort before begin events). Any interval that begins or ends while another is
  open is marked inapplicable. Its fix is cleared, and the note "this fix will not be
  applied because it overlaps with another fix" is added. Tracing the sweep: a fix that
  contains another survives and the contained one is dropped; two partly overlapping
  fixes are **both** dropped `[design inference from the source]`. No rerun: the user
  runs clang-tidy again.
- **clang-apply-replacements** (`clang-apply-replacements/lib/Tooling/ApplyReplacements.cpp`):
  `groupReplacements` (lines 110–180) deduplicates identical replacements that come
  from different translation units (a header fixed from several TUs). `mergeAndDeduplicate`
  (lines 184–239) adds every replacement of a file to one `AtomicChange`. A conflict
  prints the error and sets `ConflictDetected`, and the tool exits 1 before applying
  anything (`tool/ClangApplyReplacementsMain.cpp` lines 138–139).
  `--ignore-insert-conflict` shifts an insert past earlier ones when its length is
  unchanged.

**For loci:** dedup of identical edits again. Failing as a whole is the same choice as
loci's `verify`. There is no iteration, which suits batch tools over many TUs but not
fixes that enable further fixes.

## 5. LSP: `TextEdit[]` and `source.fixAll`

- **Spec** (LSP 3.17, "TextEdit[]"): edits "refer to positions in the document they
  are computed on … Text edits ranges must never overlap … However, it is possible
  that multiple edits have the same start position: multiple inserts, or any number of
  inserts followed by a single remove or replace edit." `CodeActionKind.SourceFixAll =
  'source.fixAll'`: "automatically fix errors that have a clear fix that do not require
  user input. They should not suppress errors or perform unsafe fixes" (3.17,
  "Code Action Request").
- **vscode-eslint** (`server/src/eslintServer.ts`): the source fix-all
  (`computeAllFixes`, lines 748–812) does not compose the published per-problem fixes.
  "Don't use any precomputed fixes since neighbour fixes can produce incorrect results"
  (line 790). It runs `lintText` with `fix: true` (ESLint's multipass loop), then
  `stringDiff(originalContent, fixedContent)` turns the result into **minimal**
  `TextEdit`s (lines 803–811). The quick-fix menu's "Fix all `rule` problems" (lines
  705–730) takes only that rule's fixes that do not overlap (`Fixes.overlaps`,
  `server/src/eslint.ts` lines 477–479), in one round.
- **Ruff server** (`crates/ruff_server/src/fix.rs` `fix_all`, lines 26–140): "We need
  to iteratively apply all safe fixes onto a single file and then create a diff between
  the modified file and the original source to use as a single workspace edit. If we
  simply … applied fixes individually, there's a possibility they could overlap"
  (lines 66–71). It calls `lint_fix` and turns the result into one replacement
  (`Replacement::between`).
- **eslint-plugin-prettier** goes the other way. It turns one whole-file reformat into
  many small fixes: `generateDifferences(source, prettierSource)` and one report with a
  `replaceTextRange` fix per difference (`eslint-plugin-prettier.js` lines 146–166,
  336–339).

**For loci:** what a fix *displays* (its own edit against the original text) and what
the fixer *applies* (the composed result of several passes) are separate everywhere.
No tool shows the composed result as per-fix edits. Diffing a whole-block result back
into small edits is an established trick (§7, option g).

## 6. Structural editing

- **rust-analyzer** (`crates/syntax/src/syntax_editor.rs`, "Inspired by Roslyn's
  `SyntaxEditor`", lines 1–5): an editor records `insert`, `delete`, `replace`, …
  against an immutable tree. `merge(other)` appends another editor's changes for the
  same tree (lines 85–98). `SourceChangeBuilder::add_file_edits` merges every editor
  for a file (`crates/ide-db/src/source_change.rs` lines 238–245). `commit` calls
  `finish()` and then `diff(old_root, new_root).into_text_edit(…)` (lines 259–292), so
  **text edits are derived from a tree diff**. `EditPlan::build`
  (`syntax_editor/edit_algo.rs` lines 63–93) sorts the changes. It remaps a change
  nested inside a replaced node onto the replacement, or discards it under a deletion.
  If replacements at the same depth intersect, the plan is invalid: `apply_edits`
  logs the intersecting changes and returns the tree **unchanged** (lines 668–692).
  `ted.rs` does not exist at this commit.
- **LibCST**: a "lossless CST" that "keeps all formatting details (comments,
  whitespaces, parentheses, etc)" (README). Codemods are "higher-order transforms that
  consist of other, simpler transforms". When chaining them, "the context holds the
  state that needs to be passed between transforms", and `transform_module` handles
  "multi-pass logic" (codemods docs, read through a summarising fetch).
- **OpenRewrite**: a composite recipe runs "each of the building blocks … as a step
  within the pipeline", which "maintains and manages the intermediate state of the
  source files". "Another cycle runs when a recipe changed something during the
  cycle", but only for recipes that opt in with `causesAnotherCycle()`. "Cycles are
  capped (by default the cap is 3)", and results are git-style diffs per file (docs,
  "Recipes", read through a summarising fetch). The LST can be "printed out to
  reconstitute the original source code without clobbering formatting" ("Lossless
  Semantic Trees").
- **recast** (jscodeshift's printer): "reprints only those parts of the syntax tree
  that you modify" by linking each copied node to its original, and "falls back to
  using a generic pretty printer" when it cannot (README, summarising fetch).
- **comby**: matches at the top level. Nested matches need more context in the
  pattern, or rerunning comby on the output (docs, "Basic usage"). The docs say
  nothing about overlapping rewrites `[gap]`.

**For loci:** every structural tool composes edits **on the tree** within a run and
serializes once (rust-analyzer: then diffs). Whole-tree rewrites that conflict at the
text level are not a problem there, because they never meet as text. loci's
frontmatter fixes are already closures over a `Mapping`. Only the driver's interface
(text edits) forces them to meet as text.

## 7. YAML and TOML editors

- **toml_edit**: "parse and modify toml documents, while preserving comments, spaces
  *and relative order* of items … primarily tailored for cargo-edit". It does not keep
  the order of dotted keys or a missing trailing newline (`crates/toml_edit/README.md`
  lines 8–12, 40–43; version 0.25.17). Cargo's manifest editor (`cargo add`) holds a
  `toml_edit::DocumentMut` (`src/workspace/editor/manifest.rs` line 87).
- **ruamel.yaml** round-trip mode keeps comments, hand-named anchors, merges and
  blank lines after block scalars. But it "doesn't preserve individual indentations of
  block sequence items", and inconsistent indentation "cannot be preserved. The first
  round-trip will make it consistent" (yaml.dev "Details", summarising fetch).
- **yq** (mikefarah): "Keeps yaml formatting and comments when updating (though there
  are issues with whitespace)"; "attempts to preserve comment positions and whitespace
  as much as possible, but it does not handle all scenarios" (README).
- **Renovate** does not reserialize. `doAutoReplace`
  (`lib/workers/repository/update/branch/auto-replace.ts` lines 219–505) finds the old
  value with `indexOf` (line 277) and splices with `replaceAt` (lines 392, 420, 453,
  486). Then `confirmIfDepUpdated` re-extracts the file (lines 14–21) to check the
  edit. On failure it tries the next match, and finally throws
  `WORKER_FILE_UPDATE_FAILED`.
- **Dependabot** (Cargo) edits manifest text with regex `gsub` on the declaration line
  (`cargo/lib/dependabot/cargo/file_updater/manifest_updater.rb` lines 131–149).
- **Prettier YAML** is a formatter: it rewrites the whole file and is not a fixer.
  Its ESLint bridge is §5's diff trick.
- **Rust, format-preserving YAML** (crates.io, 2026-11-02):
  - `yamlpatch` 1.32.0 (zizmor) offers route-addressed ops (Replace, Add, Remove,
    MergeInto, Append, comment ops, RewriteFragment) on tree-sitter-yaml via
    `yamlpath`. Its README says: "not a substitute for comprehensive YAML processing
    libraries … targeted modifications", "best-effort". `apply_yaml_patches` applies
    ops in order, each against the current document: "designed to operate
    symbolically without absolute byte positions, so operations should not invalidate
    each other unless they actually conflict" (`crates/yamlpatch/src/lib.rs` lines
    190–219). zizmor's `apply_fixes` applies each finding's fix to the evolving
    document and records a failure as "conflict after applying previous fixes"
    (`crates/zizmor/src/output/fix.rs` lines 86–106). Its version follows zizmor's
    releases, and its scope is GitHub workflow files.
  - `yaml-edit` 0.3.6 (jelmer): a lossless rowan tree with error recovery, which runs
    the YAML test suite. First published 2025-08-10, 12 versions, still 0.x.
  - Smaller and newer: `yaml-splice`, `yamled`, `yaml-rt-core`, `oneyaml`, all with
    a few thousand downloads or fewer. I did not assess them `[unverified]`.

**For loci:** the mature format-preserving editors are TOML (toml_edit) and Python
(ruamel). In Rust YAML, the most-used option is a scoped, best-effort patcher, and the
general one is 0.x. Renovate and Dependabot avoid the problem with targeted splices
plus re-extraction, which is what loci did before D215. The useful idea from zizmor is
not format preservation. It is **symbolic fixes that find their target again in the
current document**, so fixes compose without a byte-overlap rule.

## 8. Options for loci

The facts that matter: frontmatter fixes overlap only because each one carries the
whole block as text. Their real changes (add `id`, set `id`, remove a key, quote
prereqs, rename one prereq) touch different keys or items and commute. D215 made
canonical rewriting the rule on purpose.

| | Option | Cost | Fit | Precedent |
|---|---|---|---|---|
| (c) | **Structural frontmatter ops composed per pass.** Next to its text edit (unchanged, for `--json` and plugins), a frontmatter fix carries an internal `FmOp` (`AddId(Ulid)`, `SetId(Ulid)`, `RemoveKey(String)`, `QuotePrereqs`, `RenamePrereq { from, to }`), not serialized. `apply_pass` still selects fixes one by one (`--only`, applicability unchanged). A fix whose edits are all frontmatter rewrites with ops is grouped by note instead of overlap-checked against other such fixes. Each note's ops are folded into one `rewrite_frontmatter` call, which gives one edit; non-frontmatter edits are checked against it as now. | ~80–120 lines: an enum plus `apply` (the closures already exist), the grouping in `apply_pass`, tests. A second representation of each fix must agree with its text edit: a unit test applies each op alone and compares the result with the fix's edit. | Removes the problem: all frontmatter fixes on a note apply in one pass. Keeps per-code filtering, per-fix applicability, D215's canonical output and the exact per-fix edits in JSON. `verify` still guards the result. | rust-analyzer (merge editors, serialize once, diff), zizmor (symbolic ops on the evolving document), OpenRewrite and LibCST (sequential transforms on one tree) |
| (f) | Raise `PASSES` (Ruff uses 100), keep ESLint's circular check, and **report** when the limit is reached with fixes pending | 2–10 lines | Fixes the silent partial fix. Each pass rebuilds and re-lints the whole vault, so 30 fixes on one note cost 30 vault lints. Does not remove the serialization | Ruff, cargo (loud report); ESLint (cycle check) |
| (b) | Batch per rule per note (a rename fix for all close-slug prereqs of a note, like `quote_prereqs`) | ~20 lines per rule | Helps within one rule only; different rules still go one per pass. Each diagnostic carries the batch fix, so `--only` still works, but the fix's message and diagnostic no longer match one-to-one ("rename 3 prereqs" on each of three diagnostics) | loci's own quote fix; vscode-eslint "Fix all `rule` problems" |
| (g) | Keep whole-block rewrites, but **diff** each fix's output against the original block and emit only the changed lines as edits | ~40 lines (a line diff; `similar` crate or by hand) | Once a block is canonical, fixes on different keys give disjoint edits and apply together. On a block that is not yet canonical, every fix's diff includes the canonicalization, so they still overlap. Composition is a textual merge, not a YAML one, so correctness rests on `verify`. Touching line edits still clash under loci's touch rule | vscode-eslint and Ruff server fix-all, eslint-plugin-prettier |
| (a) | Keep as is | 0 | Works. Leaves silent partial fixes beyond 10 and pays one vault re-lint per frontmatter fix | ESLint |
| (d) | Format-preserving YAML editing, so frontmatter fixes become small text edits | A dependency (`yaml-edit` 0.x or `yamlpatch`, best-effort) or a hand-written editor, and a reversal of D215, with the per-form rules it removed | Contradicts a decision made the same week for stated reasons. Rust YAML editors are young (§7). Small edits on one item or key line would still touch | toml_edit, ruamel, yamlpatch |
| (e) | Ruff-like isolation groups | ~15 lines | Wrong direction: isolation **forbids** fixes that do not overlap from applying together. It cannot let overlapping ones apply together | Ruff |

Two small changes that are worth doing whichever option is chosen:

- **Report the limit.** If the last pass still applied fixes, re-lint and, if any
  allowed fix remains, say so: a warning diagnostic or a field in the plan,
  "`--fix` stopped after 10 passes with N fixes left; run it again". This follows Ruff's
  "Failed to converge" and cargo's `ReplaceFailed`. It matches the operating rule to
  fail loudly.
- **Deduplicate identical edits** within a pass, as Ruff (`applied.contains(edit)`),
  rustfix (`is_identical`) and clang-apply-replacements do. Then the copies of a batch
  fix (the quote fix on every unquoted prereq) are all recorded as applied in the
  same pass, instead of one applied and the rest silently gone.

## Recommendation

1. **(c), structural ops composed per pass.** It is the only option that removes the
   cause: fixes that commute in YAML meet as overlapping bytes. It keeps every
   property the question names: `--only` and safe or unsafe selection stay per fix,
   JSON shows each fix's exact standalone edit, and D215's canonical output is kept.
   Every structural tool surveyed does the same: compose on the tree, serialize once
   (rust-analyzer, zizmor, OpenRewrite). Keep it narrow. The ops exist only for
   `rewrite_frontmatter` fixes, are internal (not in the envelope, so plugins are
   unaffected), and the driver needs no general merge layer. Two frontmatter fixes on
   one note compose, and everything else keeps the overlap rule. Before building it,
   propose the `FmOp` enum and the grouping rule to the user, per the
   new-abstraction rule.
2. **With it or before it: report the pass limit and deduplicate identical edits.**
   Both are a few lines and fix the silent part now.
3. Do not raise the limit as the fix (f): it hides the cost (one whole-vault re-lint
   per frontmatter fix) rather than removing it. If (c) is declined, (f) plus the
   report is the fallback, and (b) for `link.dangling` renames is a cheap addition.
4. Do not reopen D215 for (d), and do not add isolation levels (e).

## Gaps

- How often a real note has more than one frontmatter fix pending is not measured
  `[gap]`. Vault updates that remove a field across many notes give one fix per note,
  not several, so the >10 case needs an unusual note.
- ESLint: that reaching 10 passes is not reported comes from reading `verifyAndFix`
  and its warning service calls only `[unverified]` beyond that function.
- LibCST, OpenRewrite, recast, comby, ruamel.yaml: read through a summarising fetch
  of their docs, not their source.
- The clang-tidy outcome for partial overlaps (both dropped) is my trace of the sweep,
  not a documented rule.
- The cost estimates in §8 are mine `[design inference]`.

## Sources

ESLint (eslint/eslint at 896b8ad4f2):
- `lib/linter/source-code-fixer.js` (lines 26–28, 61–140), `lib/linter/linter.js` (lines 43, 1522–1660), `lib/linter/file-report.js` (lines 253–312), `lib/linter/rule-fixer.js`: https://github.com/eslint/eslint/tree/896b8ad4f2/lib/linter
- Custom rules (fixes, best practices, suggestions): https://github.com/eslint/eslint/blob/896b8ad4f2/docs/src/extend/custom-rules.md (lines 59–65, 330–376, 403–440); https://eslint.org/docs/latest/extend/custom-rules

Ruff (astral-sh/ruff at 08cab22a15):
- `Fix`, `Applicability`, `IsolationLevel`: https://github.com/astral-sh/ruff/blob/08cab22a15/crates/ruff_diagnostics/src/fix.rs
- `apply_fixes`, `cmp_fix`: https://github.com/astral-sh/ruff/blob/08cab22a15/crates/ruff_linter/src/fix/mod.rs
- `MAX_ITERATIONS`, `lint_fix`, `report_failed_to_converge_error`: https://github.com/astral-sh/ruff/blob/08cab22a15/crates/ruff_linter/src/linter.rs
- `Checker::isolation`: https://github.com/astral-sh/ruff/blob/08cab22a15/crates/ruff_linter/src/checkers/ast/mod.rs (lines 581–591)
- isort `I001`: https://github.com/astral-sh/ruff/blob/08cab22a15/crates/ruff_linter/src/rules/isort/rules/organize_imports.rs
- Server fix-all: https://github.com/astral-sh/ruff/blob/08cab22a15/crates/ruff_server/src/fix.rs

rustfix and cargo (rust-lang/cargo at 2be5575ba3):
- `replace_range`, `AlreadyReplaced`: https://github.com/rust-lang/cargo/blob/2be5575ba3/crates/rustfix/src/replace.rs ; `CodeFix`, `apply_suggestions`, `Filter`: https://github.com/rust-lang/cargo/blob/2be5575ba3/crates/rustfix/src/lib.rs ; `Applicability`: https://github.com/rust-lang/cargo/blob/2be5575ba3/crates/rustfix/src/diagnostics.rs
- `rustfix_crate` loop, `CARGO_FIX_MAX_RETRIES`, `ReplaceFailed`: https://github.com/rust-lang/cargo/blob/2be5575ba3/src/ops/cargo_fix/mod.rs (lines 846–1124)

clang (llvm/llvm-project at ac2333f7dc):
- `removeIncompatibleErrors`: https://github.com/llvm/llvm-project/blob/ac2333f7dc/clang-tools-extra/clang-tidy/ClangTidyDiagnosticConsumer.cpp (lines 642–779)
- `groupReplacements`, `mergeAndDeduplicate`: https://github.com/llvm/llvm-project/blob/ac2333f7dc/clang-tools-extra/clang-apply-replacements/lib/Tooling/ApplyReplacements.cpp ; exit on conflict: `clang-tools-extra/clang-apply-replacements/tool/ClangApplyReplacementsMain.cpp` (lines 138–139)

LSP and editors:
- LSP 3.17 `TextEdit[]`: https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textEditArray ; source `_specifications/lsp/3.17/types/textEditArray.md` at gh-pages 112d978951
- `source.fixAll`: same spec, "Code Action Request"; source `_specifications/lsp/3.17/language/codeAction.md` (lines 254–263)
- vscode-eslint (microsoft/vscode-eslint at c999730b19): `server/src/eslintServer.ts` (lines 705–812), `server/src/eslint.ts` (lines 477–560): https://github.com/microsoft/vscode-eslint/tree/c999730b19/server/src
- eslint-plugin-prettier (at e6dd27a4e4): https://github.com/prettier/eslint-plugin-prettier/blob/e6dd27a4e4/eslint-plugin-prettier.js (lines 146–166, 336–339)

Structural editors:
- rust-analyzer (rust-lang/rust-analyzer at beed142fed): https://github.com/rust-lang/rust-analyzer/blob/beed142fed/crates/syntax/src/syntax_editor.rs ; https://github.com/rust-lang/rust-analyzer/blob/beed142fed/crates/syntax/src/syntax_editor/edit_algo.rs ; https://github.com/rust-lang/rust-analyzer/blob/beed142fed/crates/ide-db/src/source_change.rs ; `tree_diff::diff`: `crates/ide-db/src/syntax_helpers/tree_diff.rs` (lines 45–51)
- LibCST: https://github.com/Instagram/LibCST/blob/main/README.rst ; https://libcst.readthedocs.io/en/latest/codemods.html
- OpenRewrite: https://docs.openrewrite.org/concepts-and-explanations/recipes ; https://docs.openrewrite.org/concepts-and-explanations/lossless-semantic-trees
- recast: https://github.com/benjamn/recast/blob/master/README.md
- comby: https://comby.dev/docs/basic-usage

YAML and TOML:
- toml_edit (toml-rs/toml at 70243195af): https://github.com/toml-rs/toml/blob/70243195af/crates/toml_edit/README.md ; cargo manifest editor: https://github.com/rust-lang/cargo/blob/2be5575ba3/src/workspace/editor/manifest.rs
- ruamel.yaml: https://yaml.dev/doc/ruamel.yaml/detail/
- yq: https://github.com/mikefarah/yq/blob/master/README.md
- Renovate (at 6b681a60cf): https://github.com/renovatebot/renovate/blob/6b681a60cf/lib/workers/repository/update/branch/auto-replace.ts
- Dependabot (at 7ff94db9a3): https://github.com/dependabot/dependabot-core/blob/7ff94db9a3/cargo/lib/dependabot/cargo/file_updater/manifest_updater.rb
- yamlpatch and zizmor fixes (zizmorcore/zizmor at 21bd300700): https://github.com/zizmorcore/zizmor/blob/21bd300700/crates/yamlpatch/README.md ; https://github.com/zizmorcore/zizmor/blob/21bd300700/crates/yamlpatch/src/lib.rs ; https://github.com/zizmorcore/zizmor/blob/21bd300700/crates/zizmor/src/output/fix.rs ; `crates/zizmor/src/finding.rs` (lines 102–111)
- yaml-edit (jelmer/yaml-edit at 780a686e5b): https://github.com/jelmer/yaml-edit/blob/780a686e5b/README.md ; crates.io API (versions, downloads), queried 2026-11-02

loci (at 3b44af9): `crates/loci-core/src/{fix,envelope}.rs`, `crates/loci-core/src/parse/frontmatter.rs`,
`crates/loci-core/src/lint/{concepts,links}.rs`, `docs/DECISIONS.md` D192, D215; research 18.
