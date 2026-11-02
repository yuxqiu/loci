# 18 — Ideas from compilers, linters and language tooling

Status: research, 2026-11-02. Builds on research 16 (`find` versus grep) and 17
(finding things in a knowledge base). Their findings are cited, not repeated: the
LSP-versus-grep study (Xu 2026) and the small-agent code-graph study are summarised in
research 16 §2 and 17 §3.3; this note adds only the parts about renames and tool use.

Question: loci already behaves like a compiler front end. Which ideas from compilers,
linters and language servers should it borrow, and which not? Seven areas: auto-fix
design, diagnostics design, incremental computation, refactoring safety, testing,
language-server queries for agents, and editions and migrations.

What loci has today (checked in the working tree at 7763d47 plus the staged changes,
2026-11-02):

- **Diagnostics** (`crates/loci-core/src/envelope.rs`): `Diagnostic { code, severity,
  location: Option<Location { path, span: Option<Span { line, col, byte_start,
  byte_end }> }>, message, hint: Option<String> }`; two severities, error (exit 1) and
  warning. Human form `path:line:col: severity[code]: message` then `hint:`. Sorted by
  (path bytes, byte offset, code), byte-identical across runs (m0 §7.1). About 35 core
  codes in m0 §7.3. No lint levels, no configuration of rules (the only `loci.toml`
  lint settings are two `[budgets]`, m0 §11), no inline suppression, no baseline, no
  long-form explanations.
- **No machine-applicable fixes.** A hint is free text. Some hints are exact
  replacements in prose: `link.markdown` says `write [[concepts/a/b]]`
  (`lint.rs`, `fn links`); `fm.field-type` on `prereqs` says `write "[[slug]]"`;
  `link.dangling` says `no concept is named `x`; did you mean [[y]]?` when a slug has
  Jaro-Winkler similarity ≥ 0.85 (`index.rs`, `closest_slug`, line 214).
- **An edit primitive already exists**: `splice::apply(original, Vec<Edit>)`
  (`crates/loci-core/src/splice.rs`), byte-range edits applied in one pass, sorted by
  start, inserts at one offset kept in input order, overlap and off-char-boundary
  edits rejected with `EditError`.
- **Refactorings with verification**: `mv` (m0 §9.5) and `merge` (m1-m4 §1) compute
  link edits, build the post-change tree in memory, re-resolve every link and refuse
  (`mv.verify`, `merge.verify`) unless each previously resolving link resolves to the
  mapped image of its old target (`relink.rs`, `fn verify`, lines 107–160). Writes go
  through temp file + rename. No `--dry-run` on either.
- **Index rebuilt per command** (`index::build`, D1: every index is a rebuildable
  cache; D161 removed the persistent root index).
- **Tests** (m0 §13): fixture vaults with `insta` snapshots of the `--json` envelope
  and, for mutating commands, the resulting tree; proptest for "`mv` keeps resolving
  links resolving", lint determinism, and splice identity / one-pass equivalence
  (`crates/loci/tests/properties.rs`). No property test for `merge`, no fuzzing.
- **Vault updates** (m0 §9.11, `docs/specs/updates.md`): an append-only list compiled
  into the binary, applied by the agent on the user's yes, state as one high-water mark
  `updates = "u0003"` in `loci.toml`. m0 §11 plans a `schema` bump with `loci upgrade`
  and "a `deprecated.*` lint warning with a fix hint, then removal".

Nothing here changes DESIGN, the specs, the rules or the skills. Conventions as in
research 12 to 17: `[unverified]` means I did not confirm the claim in a primary
source (the reason is given); `[design inference]` is my extrapolation; `[gap]` means
I looked and found nothing. **(summary)** marks a number read through a fetch tool
that summarises the page rather than from the raw text or PDF. Every 2026 arXiv paper
cited is a preprint. Source files were read at the commits listed under Sources; line
numbers refer to those commits.

## TL;DR

| Area | What the tools do | For loci | Verdict |
|---|---|---|---|
| 1. Auto-fixes | A fix is a set of non-overlapping (span, replacement) edits plus a **confidence label**: rustc `MachineApplicable / MaybeIncorrect / HasPlaceholders / Unspecified`; Ruff `Safe / Unsafe / DisplayOnly`; ESLint `fix` (applied by `--fix`) versus `suggest` (editor only); clang puts uncertain fix-its on notes, never applied automatically. Application: sort, skip overlaps, **re-run and repeat** (ESLint ≤ 10 passes with cycle detection; `cargo fix` ≤ 4; Ruff ≤ 100, "Failed to converge"), then **verify and back out** (`cargo fix` re-runs rustc and restores the originals on error; Ruff refuses a fix that introduces a syntax error) | loci has the edit primitive (`splice`, same overlap rule as LSP `TextEdit[]`) and the verifier (`relink::verify`), but diagnostics carry prose hints | **Adopt.** Highest value |
| 2. Diagnostics | Stable codes with long-form explanations (`rustc --explain`); primary/secondary labels and notes; lint levels (allow/expect/warn/deny/forbid); inline suppression with a reason and an "unused suppression" check (`#[expect]`, `RUF100`, `--report-unused-disable-directives`); baselines keyed by (file, rule, count or signature) with "unmatched entry" errors (ESLint, PHPStan, detekt); SARIF 2.1.0 for CI | Codes are stable and the JSON is already structured. Rules are format rules an agent can always satisfy; git `HEAD` already serves as a baseline (`material-dropped`) | **Adapt** explanations into the rule table; **skip** levels, suppression, baselines, SARIF |
| 3. Incremental computation | rustc red-green with early cutoff; salsa in rust-analyzer (in memory, revision counter, cancellation) | Measured here: `lint` 0.09 / 0.21 / 2.0 s at 1k / 10k / 100k synthetic notes; `links` 0.03 / 0.24 / 2.45 s; `mv` 0.43 s at 10k | **Skip.** Revisit past ~50k notes, with git blob ids as the cache key, not salsa |
| 4. Refactoring safety | Pre-checks, preview, conflict annotations needing confirmation (LSP `ChangeAnnotation.needsConfirmation`; rust-analyzer flags shadowing renames); binding-preservation as the correctness criterion (Schäfer et al. 2008) | loci's `relink::verify` is exactly binding preservation, done before any write | **Keep**; add `--dry-run` to `mv`/`merge` (cheap) |
| 5. Testing | rustc UI tests: `.stderr` snapshots, inline `//~ ERROR` annotations, `run-rustfix` with a `.fixed` file that must compile; fuzzing; differential testing (Csmith, >325 compiler bugs); generated-program testing of refactoring engines (45 bugs in Eclipse and NetBeans) | Snapshot fixtures and an `mv` property test exist | **Adopt** `.fixed` snapshots and fix properties with (1); property test for `merge`; **adapt** a small `cargo-fuzz` target over parse spans |
| 6. LSP-style queries for agents | definition, references, workspace symbols | `loci links` is already find-references (incoming, outgoing, unlinked mentions); a link target *is* a path; slugs are file names | **Skip** new commands. Agents rarely pick semantic tools for localisation (0–6%) and grep beat LSP on multi-file renames (Xu 2026, **summary**) |
| 7. Editions and migrations | Edition = opt-in, per-crate, interoperable; migration lints in a `rust-20xx-compatibility` group that "should be" `MachineApplicable`; `cargo fix --edition` applies them, re-checks, backs out on failure, and leaves the manifest edit to the user; renamed and removed lints are registered | Updates are agent-applied prose; `deprecated.*` lints are planned with a fix *hint* | **Adapt**: a mechanical vault update becomes a `deprecated.*` lint with a safe fix; its update entry's How runs `loci lint --fix --only <code>` |

**Recommendation (ranked at the end):** give diagnostics structured fixes with a
three-level applicability, add `loci lint --fix` that applies safe fixes in memory,
re-lints to a fixpoint and verifies before writing, and use that machinery for
mechanical vault updates. Add `--dry-run` to `mv` and `merge`, and test fixes the way
rustc tests suggestions. Do not build incremental computation, suppression, baselines,
SARIF or LSP-style query commands now.

## 1. Auto-fix design

### 1.1 How fixes are labelled

- **rustc / clippy `Applicability`** (`compiler/rustc_lint_defs/src/lib.rs`, lines
  66–86, rust-lang/rust at ae36fe22c7):
  - `MachineApplicable`: "The suggestion is definitely what the user intended, or
    maintains the exact meaning of the code. This suggestion should be automatically
    applied." Several `MachineApplicable` suggestions "should be automatically
    applied" together.
  - `MaybeIncorrect`: "may be what the user intended, but it is uncertain. The
    suggestion should result in valid Rust code if it is applied."
  - `HasPlaceholders`: "contains placeholders like `(...)` … cannot be applied
    automatically because it will not result in valid Rust code."
  - `Unspecified`: "The applicability of the suggestion is unknown."
  The rustc dev guide asks authors to "Be conservative when choosing the level"
  (https://rustc-dev-guide.rust-lang.org/diagnostics.html#suggestions, source
  `src/diagnostics.md` line 385ff at a29f828599). In `--error-format json` each span
  carries `suggested_replacement` and `suggestion_applicability` (same page, §"JSON
  diagnostic output", example output).
- **Ruff** (`crates/ruff_diagnostics/src/fix.rs`, lines 14–32, astral-sh/ruff at
  f10755306e): `Applicability::{DisplayOnly, Unsafe, Safe}`. `Safe`: "definitely
  what the user intended, or maintains the exact meaning of the code. User comments
  are preserved, unless the fix removes an entire statement or expression." `Unsafe`:
  "should only be applied with user opt-in … may lead to a change in runtime behavior,
  the removal of user comments, or both." `DisplayOnly`: "should only be displayed for
  manual application … likely to be incorrect". The user docs
  (https://docs.astral.sh/ruff/linter/#fix-safety, `docs/linter.md` lines 336–410):
  only safe fixes apply by default; `--unsafe-fixes` opts in; per-rule promotion and
  demotion via `lint.extend-safe-fixes` / `lint.extend-unsafe-fixes`. Ruff also has
  an `IsolationLevel` (`fix.rs` lines 37–43): `NonOverlapping` (default) or
  `Group(u32)`, "applied as long as no other fixes in the same group have been
  applied".
- **ESLint** (https://eslint.org/docs/latest/extend/custom-rules, source
  `docs/src/extend/custom-rules.md` at 896b8ad4f2): a rule's `fix` is applied by
  `--fix`; `suggest` is for when "a fix potentially changes functionality or if there
  are multiple valid ways to fix a rule" (line 403), and is exposed by editors for a
  person to pick. "Suggestions are applied as stand-alone changes, without triggering
  multipass fixes" (line 433). A rule must declare `meta.fixable` / `meta.hasSuggestions`
  or ESLint throws (lines 61–65). Best practices for fixes (lines 356–360): "Avoid any
  fixes that could change the runtime behavior", "Make fixes as small as possible",
  "Only make one fix per message".
- **clang fix-its** (https://clang.llvm.org/docs/InternalsManual.html, §"Fix-It
  Hints", **summary**): three kinds (insertion, removal, replacement). Rules: since they
  are applied automatically with `-Xclang -fixit`, use them only "when it's very likely
  they match the user's intent"; clang "must recover from errors as if the fix-it had
  been applied"; a fix-it on a warning "must not change the meaning of the code". "If a
  fix-it can't obey these rules, put the fix-it on a note. Fix-its on notes are not
  applied automatically."
- **LSP** (https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/,
  gh-pages at 112d978951): a `CodeAction` carries `diagnostics` it resolves, an optional
  `WorkspaceEdit`, and `isPreferred`: "Preferred actions are used by the `auto fix`
  command … A quick fix should be marked preferred if it properly addresses the
  underlying error" (`language/codeAction.md` lines 355–370).

All five converge on the same two facts: a fix is data (spans and replacement text),
and every fix carries a confidence that decides whether a tool may apply it without a
person.

### 1.2 How fixes are represented

- **rustfix** (`crates/rustfix/src/lib.rs` lines 88–125, rust-lang/cargo at
  6238f6fd61): `Suggestion { message, snippets, solutions }`, `Solution { message,
  replacements }`, `Replacement { snippet: Snippet { file_name, line_range, range },
  replacement }`. A solution's replacements are applied together or not at all
  (`CodeFix::apply_solution` restores on error, lines 251–261).
- **LSP `TextEdit[]`** (`types/textEditArray.md`): "All text edits ranges refer to
  positions in the document they are computed on … Text edits ranges must never
  overlap … it is possible that multiple edits have the same start position: multiple
  inserts, or any number of inserts followed by a single remove or replace edit. If
  multiple inserts have the same position, the order in the array defines the order in
  which the inserted strings appear." A `WorkspaceEdit` adds file create/rename/delete
  operations, executed "in the order in which they are provided"
  (`types/workspaceEdit.md` line 5). A `TextDocumentEdit` names a document version "to
  allow clients to check the text document version before an edit is applied"
  (`types/textDocumentEdit.md`).
- **SARIF 2.1.0** (OASIS, https://docs.oasis-open.org/sarif/sarif/v2.1.0/errata01/os/sarif-v2.1.0-errata01-os-complete.html):
  `result.fixes` (§3.27.30) → `fix` (§3.55) → `artifactChanges` (§3.56) →
  `replacement { deletedRegion, insertedContent }` (§3.57); each `deletedRegion`
  "SHALL specify the location of the replacement in the unmodified artifact".

**loci today.** `splice::Edit { range, new }` and `splice::apply` already implement
the LSP rule: offsets into the original, inserts at one offset in input order, then at
most one replacement, overlap rejected. (rustfix's `replace.rs` is stricter: it rejects
a second insert at the same point, lines 180–186.) What is missing is the label and the
link from a diagnostic to its edits.

### 1.3 How fixes are applied: conflicts and fixpoints

- **ESLint** (`lib/linter/source-code-fixer.js` lines 86–135): fixes sorted by range;
  a fix that starts before the previous one ended "remain[s] … a problem"; then
  `lib/linter/linter.js`: `MAX_AUTOFIX_PASSES = 10` (line 43); after each pass all
  rules run again; it stops when a pass fixes nothing, at 10 passes, or on a "circular
  fix" where the text equals the text two passes ago (lines 1620–1637). The docs say the
  same: "This process will repeat up to 10 times, or until no more fixable problems are
  found" (custom-rules.md line 330). `--fix-dry-run` "has the same effect as `--fix`
  with the difference that the fixes are not saved to the file system"; `--fix-type`
  restricts to `problem`, `suggestion`, `layout` or `directive`
  (https://eslint.org/docs/latest/use/command-line-interface, `command-line-interface.md`
  lines 373–399).
- **`cargo fix`** (`src/ops/cargo_fix/mod.rs`, rust-lang/cargo at 6238f6fd61): module
  docs (lines 26–37): apply suggestions; if some fail to apply (overlap) but some
  succeed, retry "up to 4 times as long as some suggestions succeed"; then "rustc is
  run again to verify the suggestions didn't break anything. The change will be backed
  out if it fails (unless `--broken-code` is used)". The loop (lines 881–950) exists
  because rustc emits overlapping but individually valid suggestions; termination is a
  retry budget (`CARGO_FIX_MAX_RETRIES`, default 4) or no progress. Back-out writes the
  original bytes (lines 763–770). It refuses to start on code that already fails to
  compile (lines 997–1006).
- **rustfix** `apply_suggestions` (`lib.rs` lines 272–294): "The intent of this design
  is that the overall application process should repeatedly apply non-conflicting
  suggestions then rëevaluate the result, looping until either there are no more
  suggestions to apply or some budget is exhausted." An exactly identical duplicate
  replacement is skipped, not an error.
- **Ruff** (`crates/ruff_linter/src/linter.rs`, at f10755306e): `MAX_ITERATIONS: usize
  = 100` (line 374); if the source had no syntax errors before and a fix introduced
  one, it returns `"Fix introduced a syntax error"` and keeps the original (lines
  611–622); if it hits the limit it reports "Failed to converge after {} iterations"
  with the rule codes (lines 649, 671–679).

**Common shape:** collect → filter by applicability → sort → apply the non-overlapping
subset → re-analyse → repeat until nothing applies or a budget runs out → verify the
whole result against the analyser → write or back out.

### 1.4 What loci should do

Mechanically fixable diagnostics in m0 §7.3 today, by my reading `[design inference]`:

| Code | Fix | Applicability |
|---|---|---|
| `link.markdown` | replace the markdown link range with the wikilink (label kept as `\|label` when it differs from the target) | safe |
| `fm.field-type` (unquoted `prereqs` item) | wrap the item in quotes | safe |
| `file.bom` | delete the first three bytes | safe |
| `fm.missing-field` (`id`) | insert `id: <new ULID>` | safe |
| `path.not-nfc` | `mv` to the NFC name (a file operation; `mv` already accepts the NFC spelling) | safe |
| `link.dangling` with a close slug | replace the target with the suggested slug | unsafe (a guess at 0.85 similarity) |
| `link.dangling-heading` with a close heading | replace the fragment | unsafe |
| `fm.id-duplicate`, `block.id-duplicate` | new id on one occurrence | unsafe (which one, and plugins may reference it) |
| `h1.missing` | insert `# <slug>` | display-only (the title is the author's) |
| `inbox.root-file` | move into `drafts/` or `sources/` | display-only (a choice) |
| `size.note`, `prereqs.cycle`, `area.too-long`, `slug.duplicate` | none: judgement | — |

Recommendation: **adopt**, in this shape.

1. `Diagnostic` gains `fixes: Vec<Fix>`, `Fix { message, applicability: Safe | Unsafe |
   DisplayOnly, edits: Vec<(VaultPath, splice::Edit)>, moves: Vec<(VaultPath,
   VaultPath)> }`, serialised in the `--json` envelope as `"fixes": [...]` with byte
   offsets (the envelope already carries exact byte offsets, m0 §8). Three levels, as
   Ruff, not rustc's four: loci has no reason for `Unspecified`, and a fix with a
   placeholder is the same as display-only for an agent. Keep `hint` for prose.
2. `loci lint --fix` applies safe fixes; `--unsafe-fixes` adds unsafe ones; `--only
   <code>` restricts by code; `--dry-run` prints the edits and the diagnostics that
   would remain, writes nothing. Agents get `fixes` in `lint --json` either way and can
   apply an unsafe fix themselves after reading it.
3. Application reuses what exists: per pass, group edits by file, apply the
   non-overlapping subset with `splice::apply` (a fix whose edits conflict is skipped
   whole, as a rustfix `Solution`), re-build the index in memory from the new texts
   (`index::build` with a text closure, as `relink::verify` does), re-lint, repeat;
   stop when no fix applies, at a small pass budget (ESLint's 10 is ample), or on a
   repeated text. Then **verify**: no error diagnostic in the result that was not in
   the input, and every link that resolved still resolves to the same target (the
   `relink::verify` check with an identity map, extended by the moves). On failure,
   write nothing and report which fix broke it (`lint.fix-verify`). Write through
   `relink::apply` (temp file + rename).
4. Fixes whose moves rename files go through the `mv` planner so links follow.

- **Cost:** `envelope.rs` (one field and its JSON), a `fixes` builder in each fixable
  rule in `lint.rs`, a fix driver of perhaps 150–250 lines reusing `splice` and
  `relink`, the CLI flags, fixtures (§5). `[design inference]`
- **Changes:** `loci lint` (m0 §9.2), the envelope (m0 §8; an added optional field,
  which by m0 §11 does not change `schema`), the rules' "fix what lint reports" step.
- **Why it matters for an agent-maintained vault:** today the agent turns a prose hint
  into an edit by hand, through its own edit tool, on byte offsets it has to recompute.
  An exact edit with a label removes that step for the safe cases and leaves judgement
  where it belongs. The nearest agent evidence is about guardrails, not fixes:
  SWE-agent's edit command, which runs a linter and discards an invalid edit ("Invalid
  edits are discarded, and the agent is asked to try editing the file again"), scored
  18.0% against 15.0% without the linter on SWE-bench Lite (Yang et al., NeurIPS 2024,
  §3 and §5.1, https://arxiv.org/abs/2405.15793, **summary**). No study measures
  agents applying linter auto-fixes versus editing from hints `[gap]`.

## 2. Diagnostics design

### 2.1 Codes and long-form explanations

- rustc: most errors have a code "linked to long-form explanations which contains an
  example of how to trigger the error and in-depth details"; viewed with `--explain`
  or the error index (https://doc.rust-lang.org/error_codes/error-index.html). "Give an
  error a code (with an associated explanation) if the explanation would give more
  information than the error itself." Lint diagnostics do not get `E` codes; the lint
  name is their identity (rustc dev guide, `src/diagnostics.md` lines 74–98 and 26–33).
  New codes must ship with an explanation (`src/diagnostics/error-codes.md`).
  Explanations "should help users understand why their code cannot be accepted", not
  give "a quick fix" to copy.
- SARIF has the same idea per rule: `reportingDescriptor.helpUri` and `help`
  (§3.49.12–13).
- The rustc suggestion style guide (`src/diagnostics.md` line 483ff): "Suggestions
  should not be a question. In particular, language like 'did you mean' should be
  avoided … Compare 'did you mean: `Foo`' vs. 'there is a struct with a similar name:
  `Foo`'."

**loci today:** stable dotted codes (`link.dangling`), message templates in m0 §7.3,
and hints. Rules and their reasons live in m0 and DECISIONS, which ship to developers,
not to the agent in a vault.

**Recommendation: adapt, low cost.** No `loci explain` command: the agent reads
`loci rules` at session start, and a separate lookup is one more tool to learn. Instead,
make each diagnostic self-sufficient (the hint already does most of this) and, if
agents are seen misreading a code, add a one-paragraph "why" per code to `loci rules`
output or to the hint. Optional, trivial: phrase the close-slug hint as a statement
("a concept with a similar name exists: [[y]]"), per the rustc style guide. Evidence
for agents: Anthropic advises to "prompt-engineer your error responses to clearly
communicate specific and actionable improvements"
(https://www.anthropic.com/engineering/writing-tools-for-agents, **summary**); there
is no measurement of `--explain`-style lookups by agents `[gap]`.

### 2.2 Primary and secondary labels, notes

rustc diagnostics have one primary span with a label, secondary spans with labels, and
sub-diagnostics (`note`, `help`), with or without a span (`src/diagnostics.md` lines
6–73). LSP has `relatedInformation` (locations with messages) and `tags`
(`Unnecessary`, `Deprecated`) (`types/diagnostic.md` lines 30–118).

**loci today:** vault-level rules "report on the first involved path … and name the
others in the message" (m0 §7.1): `slug.duplicate` lists paths in prose.

**Recommendation: skip for now.** A `related: [{path, span, message}]` list would let
`slug.duplicate`, `fm.id-duplicate` and `prereqs.cycle` point at every involved file
with spans, but agents already get the paths in the message, and nothing has failed
for want of spans. Revisit if fixes (§1) need secondary locations (an unsafe
`fm.id-duplicate` fix would).

### 2.3 Lint levels, configuration and inline suppression

- rustc levels (`rustc_lint_defs/src/lib.rs` lines 156–186): `Allow`, `Expect`,
  `Warn`, `ForceWarn`, `Deny`, `Forbid` ("can't be overwritten"). `#[expect(lint)]`
  suppresses and reports `unfulfilled_lint_expectations` if the lint did not fire; any
  lint attribute takes `reason = "..."` (Rust Reference §"Lint reasons" and §"The
  `#[expect]` attribute", https://doc.rust-lang.org/reference/attributes/diagnostics.html,
  `src/attributes/diagnostics.md` lines 107–150).
- Ruff: `# noqa: F841`, `# ruff: ignore[...]`, block and file forms; `RUF100`
  (`unused-noqa`) flags suppressions that suppress nothing and can remove them with
  `--fix` (`docs/linter.md` lines 457–722).
- ESLint: `eslint-disable` comments; `--report-unused-disable-directives` turns stale
  ones into errors, motivated by a rule bug fixed upstream that leaves the comment
  suppressing nothing (`command-line-interface.md` lines 125, 649–654).
- rustc separates lints (user-controllable) from fixed errors (not silenceable), and
  prefers hard errors where silencing makes no sense ("Borrow checker errors: these are
  fixed errors", `src/diagnostics.md` lines 100–128).

**loci today:** every rule is fixed: no levels, no suppression.

**Recommendation: skip.** loci's errors are format rules (frontmatter shape, link
resolution, layout), closer to rustc's fixed errors than to style lints: an agent can
always satisfy them, and a suppression comment in a note is clutter in the user's
prose. The warnings that are judgement calls (`size.note`, `area.too-long`) are
budgets, already configurable in `[budgets]`. The risk an agent adds is silencing
instead of fixing; no suppression mechanism closes that door. If a false positive ever
needs silencing, copy the safest form: an `expect`-style marker that becomes an error
when it suppresses nothing (rustc `#[expect]`, Ruff `RUF100`), with a required reason.

### 2.4 Baselines for adopting new rules

- ESLint bulk suppressions (https://eslint.org/docs/latest/use/suppressions):
  `--suppress-all` / `--suppress-rule` write `eslint-suppressions.json`, keyed per file
  and rule by a **count** (`lib/services/suppressions-service.js` line 24: `Record<file,
  Record<rule, { count }>>`); only `"error"` rules are suppressed; leftover entries make
  the run fail until `--prune-suppressions`. Recommended with `--fix` first "so that you
  don't suppress violations that can be auto-fixed".
- PHPStan (https://phpstan.org/user-guide/baseline): `--generate-baseline` writes
  `ignoreErrors` entries of (message regex, count, path); unmatched entries are reported
  unless `reportUnmatchedIgnoredErrors: false`. "It's not the best tool when you have
  15,000 errors."
- detekt (https://detekt.dev/docs/introduction/baseline/, **summary**): `baseline.xml`
  with `CurrentIssues` and `ManuallySuppressedIssues`, ids
  `<RuleID>:<FileName>:<Finding_Signature>`, not line numbers.
- SARIF `baselineState` ∈ {new, unchanged, updated, absent} (§3.27.24) with
  `partialFingerprints` for identity (§3.27.17).

All key on something that survives edits (count per file, signature), never on line
numbers, and all fail loudly when a baselined entry disappears.

**loci today:** new rules arrive with releases; lint reports every error. git `HEAD` is
already used as a reference state (`material-dropped`, `sources.changed`, m0 §7.3).

**Recommendation: skip.** A vault has one maintainer (an agent) and new rules are
rare; the right adoption path is a fix (§1) plus a vault update (§7), so the vault
reaches zero errors in one step. A baseline file would be a second state store next to
`loci.toml`. If a future rule cannot be fixed mechanically, ship it as a warning first
(the two-release retirement of m0 §11 already does this for removals).

### 2.5 SARIF

SARIF 2.1.0 is the OASIS interchange format; GitHub code scanning "only supports SARIF
version `2.1.0`" and uses `partialFingerprints` to match results across runs
(https://docs.github.com/en/code-security/code-scanning/integrating-with-code-scanning/sarif-support-for-code-scanning,
**summary**). **Skip**: no consumer of a personal vault's lint output reads SARIF, and
the loci envelope is simpler for an agent. A converter could be written outside loci if
ever wanted.

## 3. Incremental and query-based computation

- **rustc query system** (https://rustc-dev-guide.rust-lang.org/queries/incremental-compilation.html,
  `src/queries/incremental-compilation.md` lines 9–90): results of all queries and the
  query DAG are saved; on the next run a query is green if all inputs are green; and
  even with a changed input, "after executing a query, we always check whether it
  produced the same result as the previous time. If it did, we can still mark the query
  as green" (early cutoff). Queries are fingerprinted by arguments.
- **salsa / rust-analyzer** (`docs/book/src/contributing/architecture.md` lines 138–140,
  377–388, rust-analyzer at beed142fed): salsa as "a key-value store, but it can also
  compute derived values"; a global revision counter; on a change, in-flight queries are
  cancelled. Salsa is used in rust-analyzer, not rustc (rustc dev guide,
  `src/queries/salsa.md`, dated Nov 2022). rust-analyzer keeps its database in a
  long-lived server process.
- **Build systems** (Mokhov, Mitchell & Peyton Jones, "Build systems à la carte", ICFP
  2018, https://doi.org/10.1145/3236774, not re-read): early cutoff, verifying traces
  and content hashes as the general vocabulary.

**Measured here** (release build of this working tree; synthetic vaults of N concepts
in 20 areas, each ~1.8 KB of body text with 5 random bare-slug links, committed; Intel
Core Ultra X7 358H, 16 threads; `/usr/bin/time` wall time, 2–3 runs, spread under
0.03 s; generator in the session scratchpad, not committed):

| Notes | `lint` | `status` | `links <note>` | `mv` (one rename) | `lint` peak RSS |
|---|---|---|---|---|---|
| 1,000 | 0.09 s | 0.04 s | 0.03 s | 0.09 s | 14 MB |
| 10,000 | 0.21 s | 0.10 s | 0.24 s | 0.43 s | 79 MB |
| 100,000 | 2.0 s | 0.62 s | 2.45 s | — | 731 MB |

The brief's "about 1 s at 10k notes" does not match current `lint`; it matches research
16's measurement of the removed `find` (0.95 s, tantivy) `[design inference]`.

**Recommendation: skip.** At personal-vault sizes the per-call rebuild costs a fraction
of a second, under the latency of one agent turn, and D1 (every index is a rebuildable
cache) stays simple with no cache to invalidate. Salsa specifically buys reuse inside a
long-lived process; a CLI invoked per command would need on-disk persistence, which is
the rustc design, and much more machinery. If a real vault passes ~50k notes (where
`lint` approaches 1 s), the cheap step is a per-file parse cache in `.loci/` keyed by
git blob id (the walk already uses `git ls-files`, D123;
`git ls-files -s` prints blob ids for tracked files) with the link-resolution pass
rerun in full: early cutoff at file granularity, no query engine.

## 4. Refactoring safety

- **Pre-checks and conflicts.** rust-analyzer's rename refuses invalid targets with
  explicit errors ("Cannot rename a non-local definition", "Invalid name `{}`: not an
  identifier") and, when a local rename would shadow or be shadowed, attaches a
  `ChangeAnnotation { label: "This rename will change the program's meaning",
  needs_confirmation: true, ... }` (`crates/ide-db/src/rename.rs` lines 99–119, 364–400,
  684–700). LSP gives clients `prepareRename` to test a rename position before asking
  for the new name (`language/rename.md` lines 113–130) and `needsConfirmation` on
  change annotations (`types/textEdit.md` lines 20–66).
- **Correctness criterion.** Schäfer, Ekman & de Moor, "Sound and extensible renaming
  for Java", OOPSLA 2008 (https://doi.org/10.1145/1449764.1449787, not re-read):
  a rename is correct when every name still binds to the same declaration after the
  edit; references that would rebind are rewritten or the rename is refused.
- **Preview.** ESLint's `--fix-dry-run`; LSP clients show a `WorkspaceEdit` before
  applying; the rust-analyzer comment on macro renames: "The correct behavior in such
  cases is probably to show a dialog to the user" (`rename.rs` lines 11–22).

**loci today:** `mv` and `merge` run explicit refusals first (m0 §9.5 lists them), then
compute link edits, then verify binding preservation in memory before writing
(`relink::verify`). This is the Schäfer criterion applied to links, and stronger than
most IDEs, which do not re-resolve after a rename. Two gaps: no preview, and the
verification covers link targets but not fragments ("fragments are lint's concern",
`relink.rs` line 148).

**Recommendation: keep; add `--dry-run` to `mv` and `merge` (adopt, cheap).** The plan
(`moved`, `edited` with edit counts) is computed before writing, so `--dry-run` returns
the same `data` and skips `relink::apply`. It lets an agent show the user what a merge
will touch before doing it. git already gives undo, so the value is moderate. Skip
`prepareRename`: `mv`'s refusals already happen before any write. Optionally have
`verify` also report fragments that resolved before and do not after, as a warning
rather than a refusal `[design inference]`.

## 5. Testing practices from compilers

- **UI tests** (https://rustc-dev-guide.rust-lang.org/tests/ui.html, `src/tests/ui.md`):
  expected output in `.stderr` / `.stdout` next to the test, regenerated with
  `--bless` and reviewed by hand (lines 45–60); in-source annotations `//~ ERROR msg`
  that tie each expected diagnostic to a line and make unexpected ones fail (lines
  185–235); **`run-rustfix`** applies the suggestions, compares with a `.fixed` file,
  and compiles the fixed source, which "is required to succeed";
  `rustfix-only-machine-applicable` restricts to safe suggestions (lines 558–590).
- **Fuzzing:** the rustc guide treats fuzzing as a source of minimal reproducers for
  internal errors (`src/fuzzing.md`); pulldown-cmark ships `cargo-fuzz` targets
  `parse`, `html` and `commonmark_js` (a differential target against commonmark.js)
  (https://github.com/pulldown-cmark/pulldown-cmark/tree/master/fuzz/fuzz_targets).
- **Differential testing:** Csmith generated random C programs, compiled them with
  several compilers and compared outputs; "we reported more than 325 previously unknown
  bugs" (Yang, Chen, Eide & Regehr, PLDI 2011, abstract,
  https://users.cs.utah.edu/~regehr/papers/pldi11-preprint.pdf).
- **Testing refactoring engines:** generated programs plus oracles exposed "21 new bugs
  in Eclipse and 24 new bugs in NetBeans" (Daniel, Dig, Garcia & Marinov, ESEC/FSE 2007,
  abstract, https://doi.org/10.1145/1287624.1287651).

**loci today:** fixture vaults with insta snapshots of the envelope and of the tree after
mutating commands (the UI-test pattern, with `cargo insta review` as `--bless`); proptest
for `mv` link preservation, lint determinism and splice laws.

**Recommendations:**

- **Adopt with §1:** a `.fixed`-style snapshot per fixable fixture (the tree after
  `lint --fix`), and properties over generated vaults: (a) after `--fix`, no safe fix
  remains (fixpoint); (b) `--fix` twice equals once (idempotence); (c) `--fix` never adds
  an error diagnostic and keeps every resolving link resolving; (d) `--dry-run` output
  equals the edits `--fix` writes. This is `run-rustfix` translated.
- **Adopt, small:** a `merge` property alongside the `mv` one (merge rewrites links
  too, and has none).
- **Adapt, small:** one `cargo-fuzz` target over loci's own parse layer, not
  pulldown-cmark (fuzzed upstream): for arbitrary bytes, every span in `Note` lies on
  char boundaries inside the text, frontmatter spans index the original, and a no-op
  splice is the identity. The risk sits in loci's span arithmetic (saphyr offsets, block
  ids, table pipes), which the CJK and CRLF fixtures cover only by example.
- **Skip:** inline `//~`-style annotations (the JSON snapshot already pins every
  diagnostic with its span) and differential testing (no second implementation of loci's
  resolution rules exists to compare against).

## 6. Language-server queries for an agent-facing CLI

LSP's navigation requests are `textDocument/definition`, `textDocument/references` and
`workspace/symbol` (`language/references.md`, `workspace/symbol.md`). For notes:

| LSP request | loci equivalent today |
|---|---|
| definition | a link target is a vault path or a bare slug that is a file name (m0 §5.3); `rg --files -g '<slug>.md'` finds it |
| references | `loci links <note>`: incoming, outgoing and unlinked mentions both ways (`commands/links.rs`) |
| workspace symbols | the concept list is the file tree; `rg --files concepts` |
| rename | `loci mv` |
| diagnostics | `loci lint --json` |
| code actions | none (§1) |

**Agent evidence.** Xu, "Does a Language Server Save Tokens for Coding Agents?" (arXiv
2608.13568, June 2026, https://arxiv.org/html/2608.13568v1, **summary**): with three
Claude models on Python and TypeScript repositories, agents used semantic tools 0–6% of
the time on localisation; on reference-finding tasks the LSP improved F1 (Opus 0.706 →
0.778) through precision, not recall, at +19% tokens; on multi-file renames grep passed
1.00 against 0.67 for a location-only LSP, because the LSP misses comments ("a comment
is not a semantic reference"). The abstract's conclusion: "not LSP-always but an
adaptive router". Research 17 §3.3 adds that small agents chained graph expansions in
only 12–14% of episodes.

**Recommendation: skip new commands.** The one LSP feature with agent evidence,
references, already exists as `loci links`, which (unlike an LSP) also returns unlinked
mentions, the textual hits the rename study found LSPs miss. Definition and symbols are
file names in loci, which agents already reach with `rg` and `ls`. If anything, the LSP
result supports what loci has: a rename tool that edits every reference and is checked
(§4), plus grep for prose.

## 7. Editions and migrations

- **Editions** (https://doc.rust-lang.org/edition-guide/editions/, `src/editions/index.md`
  at ab8544aeed): "Since editions are opt-in, existing crates won't use the changes
  unless they explicitly migrate"; crates of different editions "**must** seamlessly
  interoperate"; migration "is easy and largely automated" through `cargo fix`, which
  "aims to avoid changes to semantics", with "corner cases where manual changes are
  required".
- **Migration lints.** A lint with `FutureIncompatibilityReason::EditionError`
  "will automatically include the lint in the `rust-20xx-compatibility` lint group,
  which is used by `cargo fix --edition` to do migrations. The lint *should* be
  auto-fixable with `Applicability::MachineApplicable`" (`rustc_lint_defs/src/lib.rs`
  lines 388–408). Other reasons distinguish code that will be rejected in a future
  release (`FutureReleaseError`, which "can be turned into hard errors" after a while)
  from code that changes meaning (`EditionSemanticsChange`, line 416). A lint's default
  level can change with the edition (`Edition::Edition2018 => Warn`, rustc dev guide
  §"Edition-gated lints"). Renamed and removed lints are registered so that old names
  warn (`store.register_renamed`, §"Renaming or removing a lint", line 750).
- **`cargo fix --edition`** (`doc/man/cargo-fix.md` lines 45–90; edition guide
  `src/editions/advanced-migrations.md`): runs `cargo check` with the migration lints,
  applies fixes, checks again, "If the fixes fail, then it will back out the changes";
  it does **not** edit the manifest ("must be updated manually after `cargo fix
  --edition` has finished"); then `cargo fix` without `--edition` applies remaining
  suggestions; `--edition-idioms` is a separate, optional step; `--broken-code` keeps a
  failed result for manual work; `--allow-dirty` is needed on a dirty tree.

**loci today:** two mechanisms. Vault updates are prose entries the agent applies
with judgement (`docs/specs/updates.md` §2); `schema` changes are planned with `loci
upgrade` and a two-release retirement, "first a `deprecated.*` lint warning with a fix
hint, then removal" (m0 §11). u0001 ("Concepts no longer have a `digest`") is the case
in point: a purely mechanical removal of one frontmatter line per concept, which an old
vault meets today as `fm.unknown-field` errors with no fix.

**Recommendation: adapt.** Use the edition split.

1. A mechanical change ships as a `deprecated.<what>` lint with a **safe fix** (the
   m0 §11 plan, with "fix hint" upgraded to a fix). Its vault update's **How** becomes
   "run `loci lint --fix --only deprecated.<what>`, show the user the diff", and **Done
   when** becomes "`loci lint` reports no `deprecated.<what>`". The agent keeps the
   offer-and-consent step; the edit becomes exact and verified.
2. Changes needing judgement (a user-edited `AGENTS.md`) stay prose updates, as today:
   the `cargo fix` "manual changes" tail.
3. `loci upgrade` for a `schema` bump follows `cargo fix --edition`: refuse a dirty tree
   (m0 §11 already says so), apply the migration fixes, re-lint, back out on new errors,
   write the new `schema` last. Unlike Cargo, loci can write the `schema` itself, since
   it reads only one schema (m0 §11).
4. When a code is renamed or retired, keep the old code known and map it (as
   `register_renamed`), so that a stale hint or update entry naming it still makes
   sense. This matters only once codes are referenced by `--only` and by update
   entries.

- **Cost:** none beyond §1, plus one `deprecated.*` rule per mechanical change.
- **Changes:** `docs/specs/updates.md` §2 (How for mechanical entries), m0 §11 (fix
  instead of fix hint), the `upgrade` design when it is built.

## Recommendations, ranked by value for an agent-maintained vault

1. **Structured fixes and `loci lint --fix`** (§1): `fixes` on diagnostics with
   `safe / unsafe / display-only`, `--fix`, `--unsafe-fixes`, `--only`, `--dry-run`;
   apply in memory to a fixpoint, verify with the `relink` check, write or refuse.
   Changes `lint`, the envelope, `lint.rs` rules. Moderate cost; removes the most
   error-prone step an agent does today (turning prose hints into byte edits).
2. **Mechanical vault updates as deprecation lints with safe fixes** (§7): the
   edition model. Changes the updates spec and the planned `upgrade`. Small cost once (1)
   exists.
3. **Fix tests in the rustfix style** (§5): `.fixed` tree snapshots, fixpoint,
   idempotence and no-new-error properties; a `merge` property test. Ships with (1).
4. **`--dry-run` for `mv` and `merge`** (§4): returns the plan without writing. Small
   cost, moderate value (git already gives undo).
5. **A parse-span fuzz target** (§5): small cost, guards the byte-offset arithmetic
   every fix and refactoring depends on.
6. **Self-sufficient diagnostics** (§2.1): statement-form close-slug hint; a "why" line
   per code only if agents misread one. Trivial.
7. **Skip for now:** lint levels, inline suppression, baselines (§2.3–2.4), secondary
   labels (§2.2), SARIF (§2.5), incremental computation (§3; revisit past ~50k notes
   with a blob-id parse cache), new LSP-style query commands (§6).

## Gaps

- No study measures agents applying linter auto-fixes against editing from free-text
  hints `[gap]`; the nearest evidence is SWE-agent's linter guardrail on edits.
- No study of `--explain`-style long-form diagnostics for LLM agents `[gap]`.
- Agent use of LSP tools rests on one 2026 preprint read through a summarising fetch
  (**summary**); its abstract and body disagree on two numbers (the fetch noted that a
  location-only LSP "fails three-quarters" of renames in the abstract against pass@1
  0.67 in the table) `[unverified]`.
- clang's fix-it rules, detekt's baseline format and the SARIF/GitHub details were read
  through a summarising fetch (**summary**); the quoted rules are short and consistent
  with the other tools, but spot-check before quoting elsewhere.
- Schäfer et al. 2008 and Mokhov et al. 2018 are cited for their central idea, not
  re-read.
- The timings are one machine and one synthetic vault shape (5 links per note, uniform
  sizes); a real vault with long notes or many links per note would scale differently.

## Sources

Fixes and diagnostics (source read 2026-11-02):
- rustc `Applicability`, `Level`, `FutureIncompatibilityReason`: https://github.com/rust-lang/rust/blob/ae36fe22c7/compiler/rustc_lint_defs/src/lib.rs (lines 66–86, 156–186, 365–440)
- rustc dev guide, "Errors and lints" (diagnostic structure, codes, suggestions, style guide, edition-gated and future-incompatible lints, renaming lints, JSON output): https://rustc-dev-guide.rust-lang.org/diagnostics.html ; source https://github.com/rust-lang/rustc-dev-guide/blob/a29f828599/src/diagnostics.md
- rustc dev guide, error codes: https://rustc-dev-guide.rust-lang.org/diagnostics/error-codes.html
- rustc error index: https://doc.rust-lang.org/error_codes/error-index.html
- Rust Reference, lint attributes, reasons, `#[expect]`: https://doc.rust-lang.org/reference/attributes/diagnostics.html (source at 3989fe91f1)
- rustfix: https://github.com/rust-lang/cargo/tree/6238f6fd61/crates/rustfix/src (`lib.rs`, `replace.rs`)
- `cargo fix`: https://github.com/rust-lang/cargo/blob/6238f6fd61/src/ops/cargo_fix/mod.rs ; man page https://doc.rust-lang.org/cargo/commands/cargo-fix.html (`doc/man/cargo-fix.md`)
- Ruff `Applicability`, `IsolationLevel`: https://github.com/astral-sh/ruff/blob/f10755306e/crates/ruff_diagnostics/src/fix.rs ; fix loop: https://github.com/astral-sh/ruff/blob/f10755306e/crates/ruff_linter/src/linter.rs ; docs: https://docs.astral.sh/ruff/linter/
- ESLint custom rules (fixes, suggestions): https://eslint.org/docs/latest/extend/custom-rules ; CLI: https://eslint.org/docs/latest/use/command-line-interface ; bulk suppressions: https://eslint.org/docs/latest/use/suppressions ; source at 896b8ad4f2: `lib/linter/linter.js`, `lib/linter/source-code-fixer.js`, `lib/services/suppressions-service.js`
- clang Internals Manual, Fix-It Hints: https://clang.llvm.org/docs/InternalsManual.html
- LSP 3.17 (TextEdit[], TextDocumentEdit, WorkspaceEdit, ChangeAnnotation, CodeAction, Diagnostic, rename, prepareRename, references, workspace symbols): https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/ (gh-pages at 112d978951)
- SARIF 2.1.0 errata 01 (§3.27.17, §3.27.23, §3.27.24, §3.27.30, §3.49.12–13, §3.55–3.57): https://docs.oasis-open.org/sarif/sarif/v2.1.0/errata01/os/sarif-v2.1.0-errata01-os-complete.html
- GitHub, SARIF support for code scanning: https://docs.github.com/en/code-security/code-scanning/integrating-with-code-scanning/sarif-support-for-code-scanning
- PHPStan baseline: https://phpstan.org/user-guide/baseline
- detekt baseline: https://detekt.dev/docs/introduction/baseline/

Incremental computation:
- rustc dev guide, incremental compilation: https://rustc-dev-guide.rust-lang.org/queries/incremental-compilation.html ; salsa: https://rustc-dev-guide.rust-lang.org/queries/salsa.html
- rust-analyzer architecture: https://github.com/rust-lang/rust-analyzer/blob/beed142fed/docs/book/src/contributing/architecture.md
- Mokhov, Mitchell & Peyton Jones, "Build systems à la carte", ICFP 2018: https://doi.org/10.1145/3236774 (not re-read)

Refactoring:
- rust-analyzer rename: https://github.com/rust-lang/rust-analyzer/blob/beed142fed/crates/ide-db/src/rename.rs
- Schäfer, Ekman & de Moor, "Sound and extensible renaming for Java", OOPSLA 2008: https://doi.org/10.1145/1449764.1449787 (not re-read)

Testing:
- rustc dev guide, UI tests: https://rustc-dev-guide.rust-lang.org/tests/ui.html ; fuzzing: https://rustc-dev-guide.rust-lang.org/fuzzing.html
- pulldown-cmark fuzz targets: https://github.com/pulldown-cmark/pulldown-cmark/tree/master/fuzz/fuzz_targets
- Yang, Chen, Eide & Regehr, "Finding and Understanding Bugs in C Compilers", PLDI 2011: https://users.cs.utah.edu/~regehr/papers/pldi11-preprint.pdf
- Daniel, Dig, Garcia & Marinov, "Automated testing of refactoring engines", ESEC/FSE 2007: https://doi.org/10.1145/1287624.1287651 (PDF: http://dig.cs.illinois.edu/papers/AutomatedTestingOfRefactoringEngines.pdf)

Agents:
- Xu, "Does a Language Server Save Tokens for Coding Agents?" (2026): https://arxiv.org/abs/2608.13568
- Yang et al., SWE-agent, NeurIPS 2024: https://arxiv.org/abs/2405.15793
- Anthropic, "Writing effective tools for agents — with agents" (2025): https://www.anthropic.com/engineering/writing-tools-for-agents
- Research 16 and 17 (this folder) for the grep-versus-index evidence

Editions:
- Rust Edition Guide, "What are Editions?" and "Advanced migrations": https://doc.rust-lang.org/edition-guide/editions/ ; https://doc.rust-lang.org/edition-guide/editions/advanced-migrations.html (source at ab8544aeed)

Measurement: `target/release/loci` built from this working tree into the session
scratchpad; synthetic vaults generated by a script in the session scratchpad (not
committed).
