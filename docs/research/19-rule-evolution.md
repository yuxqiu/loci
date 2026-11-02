# 19 — Evolving lint rules and the vault format together with vault updates

Status: research, 2026-11-02. Builds on research 18 (compiler ideas), whose §1 (fixes)
and §7 (editions) are the starting point and are cited, not repeated.

Question: what is the leanest, most readable way for loci to add, change, deprecate
and remove lint requirements and vault-format rules over time, together with its
agent-applied vault updates? The user found the first attempt (a side table
`REMOVED_FIELDS` in `lint.rs`) neither scalable nor readable. A draft proposal,
modelled on Rust editions, adds a central rule table with `since: Option<UpdateId>`,
severity gated by the vault's mark, update lint groups derived from those tags, and
`loci lint --fix --update uNNNN`. This note checks that draft against how seven
other tools handle the problem, then adapts it to loci.

What loci has today (checked in the working tree at 60facf7 plus the staged changes,
2026-11-02):

- **Rule codes are string literals at their call sites.** `lint.rs` builds each
  diagnostic with `Diagnostic::error("code", …)`, `Diagnostic::warning(…)` or a local
  closure `d("code", …)`. One traversal emits several codes (`concept_fields` emits
  `fm.removed-field`, `fm.unknown-field`, `fm.missing-field`, `fm.id-format` and
  `fm.field-type`). The only list of codes is the m0 §7.3 table, about 40 rows, which
  no test ties to the code. About 30 more codes are refusals (`mv.exists`,
  `updates.order`, …).
- **The deprecation side table** (`lint.rs` lines 17–22):
  `const CONCEPT_FIELDS: [&str; 2] = ["id", "prereqs"]` and, separately,
  `const REMOVED_FIELDS: [(&str, &str); 1] = [("digest", "u0001")]`. In
  `concept_fields` (lines 161–177) a field in `REMOVED_FIELDS` becomes
  `fm.removed-field` ("field `digest` was removed (vault update u0001)", hint
  "see `loci updates --all` for u0001", an unsafe fix that deletes the lines);
  any other field outside `CONCEPT_FIELDS` is `fm.unknown-field`. The update id is an
  untyped string, though `config::UpdateId` exists (D185).
- **Fixes** (D192, `envelope.rs`, `fix.rs`): `Fix { message, applicability: Safe |
  Unsafe | DisplayOnly, edits, moves }`; `loci lint --fix [--unsafe-fixes]
  [--only <code>] [--dry-run]`. `--only` takes any string (`main.rs` line 50,
  `fix.rs` `Options::pick`): a code that does not exist silently matches nothing.
- **Vault updates** (m0 §9.11, `docs/specs/updates.md`, D175–D177): an append-only
  list in `share/loci/updates.md` (id, date, title; Offer, How, Done when), one
  high-water mark `updates = "uNNNN"` in `loci.toml`, waiting entries shown by
  `loci status` and printed by `loci updates`. u0001's Offer still says lint reports
  `digest` as `fm.unknown-field`; since D192 it is `fm.removed-field` (stale text).
  updates.md §2 says a mechanical change's How is `loci lint --fix --only <code>`.
- **`schema`** (m0 §11, `config.rs` lines 66–75): `schema = 1`; `Config::parse`
  refuses any other value, and every vault command opens the config first
  (`Vault::open`, `vault.rs` line 123). m0 §11 plans `loci upgrade` and a retirement in
  two releases, "first a `deprecated.*` lint warning with a fix hint, then removal",
  and updates.md §2 says "A schema change is an entry whose How runs `loci upgrade`".
- No release yet: m0 §11 says that before the first release "the format changes
  without a schema bump or upgrade", and updates.md entries are not yet "released".

Nothing here changes DESIGN, the specs, the rules or the skills. Conventions as in
research 12 to 18: `[unverified]` means I did not confirm the claim in a primary
source (the reason is given); `[design inference]` is my extrapolation; `[gap]` means
I looked and found nothing. Source files were read at the commits listed under
Sources; line numbers refer to those commits.

## TL;DR

| Tool | Where lifecycle metadata lives | How old users are told | Renamed or removed codes | Fit for loci |
|---|---|---|---|---|
| 1. Rust editions, rustc, clippy | On the lint declaration: `@future_incompatible = FutureIncompatibleInfo { reason: fcw!(EditionError 2018 …) }`, `@edition Edition2024 => Warn`; edition groups are **derived** from those fields. Renames and removals in separate tables (`register_renamed`, clippy's `DEPRECATED`/`RENAMED`) | Migration lints are mostly `Allow`; `cargo fix --edition` forces them on. Old editions stay valid forever, so there is no deadline | Old **names in configuration** warn (`renamed_and_removed_lints`) and are redirected | The derived-group idea is good; per-crate gating suits an opt-in format loci does not have |
| 2. Ruff | On each rule struct: `#[violation_metadata(stable_since / preview_since / deprecated_since / removed_since = "…")]` (it was a `RuleGroup` column in the central `codes.rs` table in 0.5.0); ~100 selector redirects in `rule_redirects.rs` | Deprecated: a warning when selected; skipped when selected by prefix; an error under preview. Removed: an error when selected | Redirect map for selectors | Moving status onto the rule, with a version, is the trend; preview mode has no use for one user |
| 3. ESLint | `meta.deprecated: boolean \| DeprecatedInfo { message, url, replacedBy, deprecatedSince, availableUntil }` on the rule; `conf/replacements.json` for removed core rules | Deprecated rules keep running; reported as `usedDeprecatedRules` in results. A removed rule in config is an error naming its replacement | `replacements.json` | Metadata on the rule; config migration is a separate one-shot tool |
| 4. Django | Deprecation metadata **on the deprecated thing** (`Field.system_check_deprecated_details` / `system_check_removed_details`); one generic check reads it | Same number, warning then error: `fields.W903` "is deprecated", then `fields.E903` "is removed". Runtime `RemovedInDjangoXXWarning` silent by default; removal after two feature releases | Check ids are never reused; `SILENCED_SYSTEM_CHECKS` by id | **Closest match** for removed vault fields: lifecycle on the field list, one generic rule |
| 5. Terraform SDK | `SchemaVersion` + `StateUpgraders []StateUpgrader { Version, Type, Upgrade }`, ordered, consecutive, one step each; `Deprecated string` on a schema attribute | Attribute: a warning diagnostic. State: upgraded silently and exactly | — | Two mechanisms, matching loci's lint versus `upgrade` split |
| 6. Alembic | Linear chain by `down_revision`; head recorded in `alembic_version`; `stamp head` for a new database; `alembic check`, `current --check-heads` | `current --check-heads` fails when behind | — | loci's mark already is this design |
| 7. markdownlint | One object per rule (`names: [code, alias]`, `description`, `tags`, `function`), listed in `lib/rules.mjs` | MD002 deprecated in 0.13.0, MD006 in 0.19.0, both removed in 0.32.0; numbers not reused | Alias names beside stable codes | One declaration per rule, check included, at ~50 rules |

**Recommendation (details at the end):** keep the link between an update and its
diagnostics **on the diagnostic** (`Diagnostic.update: Option<UpdateId>`, set where the
rule fires, shown in JSON), and keep format lifecycle **in the format declaration**
(the concept field list holds `digest` as removed by u0001), Django-style, instead of a
side table. Add `loci lint --fix --update <id>`, which picks fixes by that field, so an
update entry names its own immutable id and never a lint code. Do **not** gate severity
by the mark, and do **not** add `since` on rules with derived groups or a rule registry
with check functions. Add a code list for loud `--only` validation and, after the first
release, a renamed-codes table, only when the first rename happens.

## 1. Rust editions, rustc lints and clippy

### 1.1 How lints and migrations are declared

- **One static per lint, metadata in the declaration.** `declare_lint!` builds a
  `Lint { name, default_level, desc, edition_lint_opts, future_incompatible,
  feature_gate, … }` (`compiler/rustc_lint_defs/src/lib.rs` lines 258–313, macro lines
  664–700, rust-lang/rust at ae36fe22c7). `edition_lint_opts: Option<(Edition, Level)>`
  ("Starting at the given edition, default to the given lint level", lines 287–289) is
  read by `default_level(edition)` (lines 536–541).
- **Migration lints** carry `@future_incompatible = FutureIncompatibleInfo { reason:
  fcw!(EditionError 2018 "slug-of-edition-guide-page") }` (rustc dev guide, "Editions",
  §"Migration lints", `src/guides/editions.md` lines 164–193, rustc-dev-guide at
  a29f828599). They are implemented with a `MachineApplicable` suggestion that compiles
  "in both the previous and the next edition" (lines 168–171). The `fcw!` macro
  (`lib.rs` line 770ff) builds a `FutureIncompatibilityReason`: `FutureReleaseError`
  ("After some period of time, lints with this variant can be turned into hard errors
  (and the lint removed)", lines 365–380), `EditionError` (line 408),
  `EditionSemanticsChange`, combined variants and `Custom`.
- **Groups are derived, not listed.** `LintStore::register_lints`
  (`compiler/rustc_lint/src/context.rs` lines 205–240): a lint whose reason has an
  edition is pushed into that edition's `rust-20xx-compatibility` group; any other
  future-incompatible lint goes into `future_incompatible`. No author maintains the
  group's membership.
- **A changed rule is a new lint.** The 2024 keyword change is `keyword_idents_2024`,
  next to the older `keyword_idents` (`editions.md` lines 196–212).

### 1.2 How existing users are told, and when it becomes an error

- "Migration lints can be either `Allow` or `Warn` by default … **Most migration lints
  are `Allow`**" (`editions.md` lines 215–218). `Warn` is for changes "important for
  everyone to be aware of"; "new warn-by-default lint that hit many projects can be very
  disruptive" (lines 220–227). `cargo fix --edition` passes `--force-warn
  rust-20xx-compatibility` and `--cap-lints=allow` so only migration lints show (lines
  192–194).
- An `@edition` level change (`Edition2024 => Warn`) "should generally be used
  sparingly" (lines 233–250). The documented path for a phase-out:
  `anonymous_parameters` was an `Allow` migration lint, then, "three years later, it was
  changed to `Warn` for all previous editions" (lines 267–279).
- The 2018 "idiom lints" group "did not seem to work very well, and it is unlikely that
  we will use the idiom groups in the future" (lines 308–316).
- The premise: an old edition stays supported forever, so a migration lint can stay
  silent; nothing forces a crate to move.

### 1.3 Renamed and removed lints

- rustc: `store.register_renamed(old, new)` / `store.register_removed(name, reason)` in
  `register_builtins` (rustc dev guide, `src/diagnostics.md` lines 750–765; 87 such calls
  in `compiler/rustc_lint/src/lib.rs`, for example line 415 and lines 714–728). The store
  maps the old name to `TargetLint::Renamed(new, id)` or `Removed(reason)`
  (`context.rs` lines 95–113, 301–312); `register_renamed` panics if the target does not
  exist (line 305). Using an old name in an attribute fires `renamed_and_removed_lints`
  (Warn): "To fix this, either remove the lint or use the new name"
  (`rustc_lint_defs/src/builtin.rs` lines 1428–1448).
- clippy: `clippy_lints/src/deprecated_lints.rs` (rust-clippy at 9fd09bc3fb), "managed
  by `cargo dev rename_lint` and `cargo dev deprecate_lint`" (line 1), holds two tables
  of string pairs with a `#[clippy::version]` each: `DEPRECATED` (17 entries, name and
  reason, for example `"lint has proved problematic"`) and `RENAMED` (76 entries, old and
  new). `clippy_lints/src/lib.rs` lines 452–456 feed them to `register_renamed` and
  `register_removed`. The list of live lints, `declared_lints.rs` (847 lines), "was
  generated by `cargo dev update_lints`… do not edit by hand" (lines 1–3).

**Note for loci.** Renames and removals in rustc and clippy matter only where a lint
name is **input** (attributes, `-W` flags). Output carries the current name; nothing
redirects it.

### 1.4 Readability

Each lint is one declaration with its lifecycle beside it; edition membership is
computed. Renames and removals sit in one small table each. At ~800 clippy lints the
registry is generated by a tool, the price of keeping scattered declarations and a
central list in sync.

## 2. Ruff

- **Status on the rule, with a version.** Each rule struct carries
  `#[violation_metadata(stable_since = "…" | preview_since | deprecated_since |
  removed_since, category = …)]` (`crates/ruff_linter/src/rules/ruff/rules/test_rules.rs`
  lines 73, 233, 267, 338, astral-sh/ruff at f10755306e; `CONTRIBUTING.md` lines 205–212:
  new rules start with `preview_since = "NEXT_RUFF_VERSION"`, filled in at release). It
  becomes `RuleStatus { Stable | Preview | Deprecated | Removed { since } }`
  (`crates/ruff_linter/src/codes.rs` lines 212–224). The central `code_to_rule` table
  (line 238ff) maps `(Linter, code)` to the rule only. In Ruff 0.5.0 the status was a
  column in that central table: `(Pycodestyle, "E101") => (RuleGroup::Stable, …)`
  (`codes.rs` at tag 0.5.0, lines 50–70). Ruff moved the metadata from the central
  table onto each rule.
- **What users see** (`RuleStatus` docs, `codes.rs` lines 219–223; `rule_selector.rs`
  lines 303–321): a deprecated rule gives "warnings … during selection in stable and
  errors … if used with preview mode enabled", and is excluded when selected by prefix
  or category ("Deprecated rules are excluded by default unless explicitly selected");
  a removed rule "errors will be displayed on use". `docs/preview.md` §"Deprecated
  rules" (lines 184–187) says the same.
- **Redirects** (`crates/ruff_linter/src/rule_redirects.rs`): a `HashMap` of about 100
  old-to-new selector pairs (`("R501", "RET501")`, `("SIM111", "SIM110")`), followed by
  `RuleSelector::parse` (`rule_selector.rs` lines 196–225), again on input only.
- **Fixes graduate.** "Fixes for rules may be introduced at a lower applicability, then
  promoted to a higher applicability. Reducing the applicability of a fix is not a
  breaking change" (`docs/versioning.md` §"Fix stabilization").
- **Versioning** (`docs/versioning.md`): minor releases for breaking changes, including
  "A rule is deprecated", "The behavior of a stable rule is changed", "A safe fix for a
  rule is promoted to stable"; patch releases for "A rule is added in preview" and "An
  unsafe fix for a rule is added". New rules stay in preview "for at least one minor
  release".

**Readability:** one place per rule (struct, docs, status, fix availability), one
central code map, one redirect map. Preview exists "to collect community feedback"
(`versioning.md` §"Preview mode"), a reason a single-user tool lacks.

## 3. ESLint

- **Rule metadata.** `meta.deprecated: boolean | DeprecatedInfo`; the top-level
  `meta.replacedBy` is itself deprecated (`docs/src/extend/custom-rules.md` lines
  108–111, eslint at 896b8ad4f2). `DeprecatedInfo { message?, url?, replacedBy?:
  ReplacedByInfo[], deprecatedSince?, availableUntil?: string | null }`, where
  `availableUntil: null` means "frozen" (`packages/core/src/types.ts` lines 220–291,
  eslint/rewrite at 9aa97f2d08; `docs/src/extend/rule-deprecation.md`). `meta.fixable`
  is mandatory for a rule that fixes, or "ESLint will throw an error"
  (`custom-rules.md` lines 59–61).
- **Reporting.** A deprecated rule keeps running; results carry `usedDeprecatedRules:
  { ruleId, replacedBy, info }[]` (`docs/src/integrate/nodejs-api.md` lines 480–482;
  computed in `lib/eslint/eslint.js` lines 139–208). `lib/cli.js` contains no
  deprecation message, so display is left to formatters and editors (my reading of the
  source). Policy (`docs/src/use/rule-deprecation.md`): core rules are removed only when
  replaced by a core rule or a plugin rule; deprecated rules get no more fixes and "You
  can continue to use deprecated rules indefinitely".
- **Removed rules.** `conf/replacements.json` maps removed core rules to replacements;
  a config naming one fails with `Rule "x" was removed and replaced by "y"`
  (`lib/config/config.js` lines 98–101; `lib/linter/file-report.js` lines 94–96).
- **Config migration** is a one-shot external tool, `npx @eslint/migrate-config
  .eslintrc.json`, whose output "isn't guaranteed to work in all cases"
  (`packages/migrate-config/README.md`, eslint/rewrite).

**Readability:** deprecation on the rule; removals in one JSON file; migration of the
user's config outside the linter.

## 4. Django

- **Check ids** follow `applabel.X001`, X in `CEWID` for the severity
  (`docs/ref/checks.txt` lines 53–57, django at b501b8493f). Errors stop management
  commands; warnings can be silenced with `SILENCED_SYSTEM_CHECKS`, "identifiers … that
  you wish to permanently acknowledge and ignore" (`docs/topics/checks.txt` lines 17–21;
  `docs/ref/settings.txt` lines 2909–2920). Ids are emitted inline in each check; the
  complete list is the reference page.
- **Deprecated and removed model fields, one number, two letters**
  (`docs/ref/checks.txt` lines 239–270): `fields.W903` "`NullBooleanField` is
  deprecated … will be removed in Django 4.0. *This check appeared in Django 3.1 and
  3.2*", then `fields.E903` "`NullBooleanField` is removed except for support in
  historical migrations". Same for W900/E900, W901/E901, W904/E904, W905/E905.
- **Where that metadata lives: on the deprecated field.** `Field` has
  `system_check_deprecated_details = None` and `system_check_removed_details = None`
  (`django/db/models/fields/__init__.py` lines 155–156). One generic check,
  `_check_deprecation_details` (lines 548–573), turns `system_check_removed_details`
  into `checks.Error(msg, hint, id)` and, failing that, `system_check_deprecated_details`
  into `checks.Warning`. A removed field class only sets the
  dict (lines 1369, 2269, 2387).
- **Timeline for code** (`docs/internals/release-process.txt` lines 94–116): deprecated
  in A.x, warns through A.x, removed in B.0 (or B.1), "over at least 2 feature
  releases"; the `RemovedInDjango51Warning` class names the removal version; "The
  warnings are silent by default". A `# RemovedInDjangoXXXXWarning.` comment marks code
  to delete when the deprecation ends (`docs/internals/contributing/writing-code/submitting-patches.txt`
  lines 304–311, 348–352). `docs/internals/deprecation.txt` lists every pending removal
  by version.
- **Migrations** name their `dependencies` (`docs/topics/migrations.txt` line 349), and
  Django "will refuse to run migrations" when the recorded state contradicts them
  (line 441).

**Readability:** for a format element, the deprecation sits next to the element's
definition, and one rule turns it into a diagnostic. The pairs of ids make the
transition visible in the list.

## 5. Terraform plugin SDK (chosen over Kubernetes for its versioned data)

- `Resource.SchemaVersion int`: "Provider authors can increment this version number
  when Schema semantics change in an incompatible manner. If the state's SchemaVersion is
  less than the current SchemaVersion, the MigrateState and StateUpgraders functionality
  is executed" (`helper/schema/resource.go` lines 78–90, terraform-plugin-sdk at
  ea3d95550b).
- `StateUpgraders []StateUpgrader`: "The registered versions are expected to be
  ordered, consecutive values" (lines 117–127); `StateUpgrader { Version, Type, Upgrade }`
  "upgrades the state one single schema version" (lines 782–798). The old `MigrateState`
  is kept only for state older than the first upgrader (lines 98–115).
- User-written configuration is handled differently: `Schema.Deprecated string`
  "defines warning diagnostic details to display when practitioner configurations use
  this attribute" (`helper/schema/schema.go` lines 329–356), on the attribute itself.

**For loci:** a program converts stored state exactly and silently, step by step; the
human-written part gets a warning on the deprecated element. These map onto `loci
upgrade` (exact, by `schema`) and lint plus updates (content an agent edits with
consent).

## 6. Alembic

- Each revision names its parent in `down_revision`; Alembic "composes a list based on
  how the `down_revision` identifiers link together" (`docs/build/tutorial.rst` lines
  810–824, alembic at 1bcba93ea0). The database records one version in
  `alembic_version`; `upgrade head` computes the path from it (lines 862–871).
- A new database is created whole and stamped with `head`, so "the purpose of the
  migration scripts is relegated just to movement between versions on out-of-date
  databases" (`docs/build/cookbook.rst` lines 15–45).
- Checks tie to versions in two ways: `alembic current --check-heads` fails when the
  database "is not on all head revisions" (`cookbook.rst` lines 1492–1506), and `alembic
  check` fails when the models have changes no revision covers
  (`docs/build/autogenerate.rst` lines 992–1020).

**For loci:** `updates` is Alembic's single version, `loci init` writing the newest id
is `stamp head`, and the `loci status` line is `--check-heads` (updates.md §3 already
says so). The useful addition is the `alembic check` idea: a test that every update id
named in code exists in `share/loci/updates.md`.

## 7. Closer to loci: markdownlint (Vale, Obsidian, Logseq not found)

- Each rule is one module exporting `{ names: ["MD001", "heading-increment"],
  description, tags, parser, function }` (`lib/md001.mjs` lines 7–12,
  DavidAnson/markdownlint at 5dd44fe444), imported into one list (`lib/rules.mjs`).
  Configuration keys may be a code or its alias (`README.md` lines 369, 474).
- Lifecycle by changelog: "Deprecate MD002" (0.13.0), "Deprecate MD006/ul-start-left"
  (0.19.0), "Remove deprecated MD002/MD006" (0.32.0) (`CHANGELOG.md` lines 362–365,
  295–300, 120–122). The README's rule list skips MD002 and MD006, so numbers are not
  reused. What a config naming MD002 does now: `[unverified]` (not traced in source).
- Vale, Obsidian and Logseq: I found no primary source describing a rule or format
  lifecycle and did not survey them `[gap]`.

**Readability:** at ~50 rules, one declaration per rule including its check, plus a
plain list, is enough; deprecation was announced in the changelog only.

## 8. Adapting this to loci

Two different things evolve, and the surveyed tools keep them apart:

1. **The vault format**: frontmatter fields, files, layout. Here a change can make an
   existing vault wrong. Django puts the lifecycle on the field; Terraform on the
   attribute (for human-written configuration) or in a versioned upgrader chain (for
   stored state).
2. **The rule codes**: names that agents, `--only` and plugins read. Here a change only
   breaks a consumer of the name. rustc, clippy, Ruff and ESLint handle this with
   rename and remove tables consulted **on input**.

The current `REMOVED_FIELDS` mixes both: format knowledge (which field is gone) in a
second table beside `CONCEPT_FIELDS`, and a code whose message carries the update id as
a string. The draft also mixes them, putting format history (`since: uNNNN`) on rules.

### 8.1 (a) Where should the link between an update and its rules live?

| Option | Who uses it | Pros | Cons |
|---|---|---|---|
| **On the diagnostic**, set where the rule fires (`Diagnostic.update`) | `--update <id>` picks fixes by it; agents read it in JSON | The knowledge is already at the emission site (the rule knows `digest` came from u0001). Works for generic rules where one code serves several updates (`fm.removed-field` for `digest` now, another field later). Needs no registry or derived groups. | The id is data on each diagnostic, not visible in a rule list |
| On the rule (`since: Option<UpdateId>`, draft; rustc, Ruff) | `--update` derives a group from the tags | Visible in one table; rustc derives edition groups this way (`context.rs` lines 214–237) | One rule per update. A generic `fm.removed-field` cannot carry two updates, so either codes multiply (`fm.digest-removed`, …), each a new public name to retire later, or the table lies. It needs a registry. |
| On the update (an extra `Lint:` field in `updates.md`) | — | Nothing new in code | Duplicates the knowledge that has to be in code anyway (lint must know what is removed); couples markdown parsing to codes; the entry is immutable once released, so a renamed code breaks it |
| Nowhere (prose updates; generic `fm.unknown-field` with an unsafe "delete field" fix) | How names `--only fm.unknown-field` | Least code | Scope is wrong: the fix would also delete a mistyped `prereq:`. The message no longer says why the field is wrong. The immutable entry names a mutable code. |

There is also a scoping bug in today's design, and only the diagnostic or rule options
fix it: u0001's How (updates.md §2) is `loci lint --fix --only fm.removed-field`. Once
a second update removes another field, running u0001's How also deletes that field,
before the user has seen the second offer. `--update u0001` picks only u0001's fixes.

**Recommendation: on the diagnostic.** Rust's real lesson here is not that the tag
sits on the lint declaration. It is that **the user names the migration, not the
lints**: `cargo fix --edition`, not a list of lint names. `--update u0001` does the
same, and update ids never change, so an append-only entry can name one safely. Ruff
moved status from the central table onto each rule (§2), which agrees with putting
lifecycle data at its point of use, not in a parallel table.

### 8.2 (b) Gate severity by the vault's mark?

The draft: a warning before the update is handled, full severity after. Rust's
equivalent (`@edition`, `Allow` migration lints) exists because an old edition stays
valid forever (§1.2). loci reads exactly one format (m0 §11), so `digest` is wrong in
every vault this binary reads, whatever the mark says.

Against gating, for loci:

- **The window is about zero turns.** AGENTS.md has the agent run `loci status` before
  each task, and `status` lists waiting updates (updates.md §5). Gating only changes
  what happens between a binary upgrade and the next `status`.
- **It breaks the simplest Done when.** With fixed severity, "`loci lint` passes" proves
  the update is done. With gating, a waiting update's leftovers are warnings and lint
  passes anyway.
- **Declining is not exempting either way.** With one high-water mark, a declined update
  is "handled", so under gating a decline turns the warnings into errors at once, and
  without gating they were errors all along. For an enforced format change, "no" can only
  mean "not now". The Offer should say so; no severity rule can change that.
- **Cost.** Lint would need the config (it now takes only `Index` and `Budgets`), two
  severities per rule, and fixtures for both states.

The real risk gating would address is **consent**: the rules tell the agent to fix what
`loci done` reports and to apply an unsafe fix "when it is right" (`share/loci/rules.md`
lines 32–36). That path could delete digests whose text the user wanted kept. The
cheapest guard sits at the point of action: a fixed hint on every update-tagged
diagnostic, for example "vault update u0001 made this change; if `loci status` lists it
as waiting, offer it with `loci updates` before fixing". Research 18 §2.1 cites
Anthropic's advice to put actionable guidance in error responses. The guard needs no
mark lookup.

**Recommendation: do not gate.** Ranked: (1) a fixed severity and a fixed hint naming the
update; (2) the same hint, phrased differently when the update is waiting (one
comparison against the mark in the lint command); (3) severity gating (the draft). If
loci ever needs Django's warning-then-error phase after a release (m0 §11's "two
releases"), edit the severity in the declaration at the second release. That needs no
machinery.

### 8.3 (c) A rule registry?

What a central list of codes would buy loci, and what it would cost:

- **Validation of codes given as input.** Today `--only fm.removd-field` silently fixes
  nothing. Every surveyed tool fails loudly on an unknown or removed name (rustc
  `renamed_and_removed_lints`, Ruff "Removed selector", ESLint's config error). This
  needs a list of known codes, nothing more.
- **Spec and code agreement.** A test can check that m0 §7.3 lists exactly the declared
  codes (the table is the only list today).
- **One severity per code.** Each call site picks `error` or `warning` itself now.

What it should **not** include: a check function per rule. loci's rules share
traversals (one pass over a concept's frontmatter emits five codes). A
function-per-rule design needs a visitor or dispatch layer like rustc's lint passes or
Ruff's AST checker. markdownlint (one function per rule) can do it because each rule
walks tokens on its own. Ruff and clippy need code generation (`map_codes`,
`cargo dev update_lints`) to keep ~900 and ~800 rules in sync; loci has about 40 lint
codes and gains nothing from that.

No `since` either, if (a) puts the update on the diagnostic.

**Recommendation:** the minimum is one `static` per code, `Code { name, severity }`,
which call sites pass to `Diagnostic::new`, plus `ALL: &[&Code]` for `--only`
validation and the m0 test. It is a separate, small change, not needed for the
deprecation design. Do it with the first rename or now, if loud `--only` failure is
wanted.

### 8.4 (d) Renamed and removed codes

- **Before the first release:** rename freely and record it in the CHANGELOG, as m0 §11
  already allows for the format.
- **After it:** codes are read as input in two places, `--only` and text written by
  agents (lessons in `RULES.md`). With (a), update entries name update ids, not codes,
  so the append-only file is no longer a consumer. A rename then costs one row in
  `RENAMED: &[(&str, &str)]`: `--only old` warns and uses the new code, as rustc
  warns and redirects. A removal costs one row in `REMOVED: &[(&str, &str)]` (code,
  reason): `--only old` is refused with the reason, like Ruff's removed rules and
  clippy's `DEPRECATED`. Never reuse a code (Django, markdownlint).
- **Output is never redirected.** No surveyed tool emits old names. A plugin or agent
  matching on an old code learns from the CHANGELOG. Plugins emit only their own
  prefixed codes (m0 §2.3) and do not read core codes.

### 8.5 (e) `schema`, `updates` and `loci upgrade`

- `updates` is the vault's position in the list of content changes (Alembic's head).
- `schema` answers a different question: can this binary load the vault at all?
  `Config::parse` refuses any other value before any command runs.
- **A contradiction in the current specs:** updates.md §2 says "A schema change is an
  entry whose How runs `loci upgrade`". But a binary with a new `schema` refuses an
  old vault in `Vault::open` (`config.invalid`), so `loci updates` cannot run there and
  the entry can never be shown. When a schema bump comes, the refusal message has to
  name `loci upgrade` itself. `upgrade` then rewrites `schema` and moves the mark past
  the matching entry, as Terraform's upgraders run before the provider reads state
  (§5) `[design inference]`.
- **Redundancy to remove:** m0 §11 lists "a renamed or removed … frontmatter field" as a
  reason to change `schema`. Under this design a removed or renamed field does not stop
  the binary reading the vault; lint reports it with an update-tagged fix, as u0001 shows.
  Narrow the schema rule to changes the binary cannot load or would **silently misread**
  (a removed or renamed `loci.toml` key, which `deny_unknown_fields` refuses, D176; a
  field whose meaning changed). Replace m0 §11's "first a `deprecated.*` lint warning
  with a fix hint, then removal" with "a lint with a fix whose diagnostics name the
  update". Do not build `loci upgrade` until the first real schema bump.
- Keep `schema`: it is one integer, already built, and the only guard against a vault
  being misread. It is not redundant with `updates`, because it must be checked before
  `updates` can be read.
- Unrelated but easy to confuse: `envelope::SCHEMA` is the JSON envelope's version, a
  third "schema" with nothing to do with either.

### 8.6 (f) What agents consume

- **JSON:** `"update": "u0001"` on each diagnostic it applies to (omitted when absent,
  like `fixes`, so no envelope change by m0 §11). An agent can group diagnostics by
  update, see that the errors are expected after an upgrade, and leave them to the
  update procedure.
- **Hint:** the fixed text of §8.2, generated in one place, so every update-tagged
  diagnostic tells the agent the same thing.
- **Command:** `loci lint --fix --update <id> [--unsafe-fixes] [--dry-run]`. An unknown
  id is `updates.unknown` (the refusal `loci updates --handled` already uses).
- **Update entry:** How is "check X with the user, then `loci lint --fix --unsafe-fixes
  --update u0001`, show the diff"; Done when includes "`loci lint` passes", which is
  exact because severity is fixed.
- Nothing new in `loci updates` or `loci status`: they already list waiting entries. Do
  not list codes per update; lint reports them.

## 9. Recommended design

### 9.1 Data structures

`crates/loci-core/src/envelope.rs`:

```rust
pub struct Diagnostic {
    pub code: String,
    pub severity: Severity,
    pub location: Option<Location>,
    pub message: String,
    pub hint: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fixes: Vec<Fix>,
    /// The vault update that made this a problem; `loci lint --fix --update` picks by it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update: Option<UpdateId>,
}

impl Diagnostic {
    /// Marks this diagnostic as caused by update `id`, with the standard hint.
    pub fn from_update(self, id: UpdateId) -> Self { … }
}
```

A plugin envelope that sets `update` is invalid (`plugin.failed`): updates belong to
core (updates.md §7) `[design inference]`.

`crates/loci-core/src/config.rs`: `impl UpdateId { pub const fn new(n: u32) -> Self }`,
so code can name an update as a typed constant.

`crates/loci-core/src/lint.rs`, the concept fields in one table, replacing both
`CONCEPT_FIELDS` and `REMOVED_FIELDS`:

```rust
/// A concept frontmatter field (m0 §6.1). A removed field stays listed with the vault
/// update that removed it, so lint can say so and offer the deletion.
enum Field { Current, Removed(UpdateId) }

const CONCEPT_FIELDS: &[(&str, Field)] = &[
    ("id", Field::Current),
    ("prereqs", Field::Current),
    ("digest", Field::Removed(UpdateId::new(1))),
];
```

A one-off change with no generic rule (a new requirement, a tightened rule) writes its
own check, as any rule does, and calls `.from_update(UpdateId::new(N))` on what it
emits. A tightened rule gets a **new code** for the new condition, so that only the
new failures carry the update (rustc's `keyword_idents_2024`, §1.1).

`crates/loci-core/src/fix.rs`: `Options { unsafe_fixes, only, update: Option<UpdateId> }`;
`pick` skips a diagnostic whose `update` differs when `update` is set.
`crates/loci/src/main.rs`: `--update <ID>` (requires `--fix`, like `--only`).

### 9.2 Policy (for updates.md §2 and m0 §11)

- Any change that can make a clean vault fail lint (a new requirement, a removed or
  renamed field, a stricter rule) ships with an update entry, and the diagnostics it
  newly causes carry that update id. A change that only relaxes a rule needs no entry.
- An enforced change's Offer says that lint reports the old form until the change is
  made, so declining means "later".
- Update entries name update ids, never lint codes.
- A test (Alembic's `check`, §6) asserts that every `UpdateId` used in core exists in
  `share/loci/updates.md`. Cheapest form: a `const` list of the ids used, checked
  against `updates::all()` `[design inference]`.

### 9.3 The `digest` / u0001 example

- `lint.rs`: the `("digest", Field::Removed(UpdateId::new(1)))` row. `concept_fields`
  matches on `Field`: `Removed(u)` emits `fm.removed-field` "field `digest` was removed
  (vault update u0001)", `.from_update(u)`, with the existing unsafe line-deletion fix;
  a key not in the table is `fm.unknown-field`, as today.
- JSON: `{"code": "fm.removed-field", "severity": "error", …, "hint": "vault update
  u0001 made this change; if `loci status` lists it as waiting, offer it with `loci
  updates` before fixing", "fixes": [{"applicability": "unsafe", …}], "update": "u0001"}`.
- `share/loci/updates.md` u0001 (unreleased, so still editable): Offer "The `digest:`
  field is gone; lint reports it as `fm.removed-field` until it is removed." How "If a
  digest says something the note's text does not, ask the user whether to add it to the
  text first. Then run `loci lint --fix --unsafe-fixes --update u0001` and show the
  diff." Done when "anything the user asked to keep is in the note's text; `loci lint`
  passes."

### 9.4 What a new deprecation costs

| Change | Files touched |
|---|---|
| Remove a concept field | `share/loci/updates.md` (entry), `lint.rs` (one table row), m0 §6.1 (one line), `CHANGELOG.md`. The generic rule is already tested |
| Rename a field | as above, with `Field::Renamed(new, UpdateId)` and a safe fix that rewrites the key (a second variant, added only when the first rename happens) |
| New or stricter requirement | `updates.md`, `lint.rs` (check + `.from_update` + fix), m0 §7.3 (row), a fixture or unit test, `CHANGELOG.md` |
| Rename a code (after the first release) | the `RENAMED` row, m0 §7.3, `CHANGELOG.md` |
| Remove a requirement | `lint.rs` (delete the check), m0 §7.3, `CHANGELOG.md`; after the first release, also a `REMOVED` row |

The draft's per-deprecation cost is the same or higher (a registry row with `since`
in addition), and it gives up generic rules.

### 9.5 Delete from the current code and specs

- `REMOVED_FIELDS` and the separate `CONCEPT_FIELDS` array, merged into the field
  table above (the side table the user objected to).
- The string update id `"u0001"` in `lint.rs` and its hand-written hint "see `loci
  updates --all` for u0001", replaced by `UpdateId` and `from_update`.
- updates.md §2's "the How is `loci lint --fix --only <code>`" (the scoping bug and the
  immutable code reference), replaced by `--update <id>`.
- updates.md §2's "A schema change is an entry whose How runs `loci upgrade`"
  (unreachable, §8.5).
- m0 §11's "first a `deprecated.*` lint warning with a fix hint, then removal" and the
  "removed frontmatter field" schema trigger (§8.5).
- u0001's stale "lint reports it as `fm.unknown-field`".
- `crates/loci/tests/fix.rs` line 147's `--only fm.removed-field`, to use
  `--update u0001`.

### 9.6 Do not build

| Not building | Why |
|---|---|
| Severity gated by the mark | The window before `loci status` offers the update is about zero turns; it would make "`loci lint` passes" a false Done when; declining exempts nothing anyway; a fixed hint gives the consent guard (§8.2) |
| `since` on rules and derived update groups | The update on the diagnostic gives the same `--update` without a registry and keeps generic rules possible (§8.1) |
| A rule registry with check functions, or a lint-pass framework | Rules share traversals; ~40 codes; Ruff and clippy need code generation at 20× the size (§8.3) |
| One code per deprecation (`deprecated.digest`) | Every format change would add a public code to retire later; one generic code plus `update` says the same |
| A `Lint:` field in `updates.md` | Duplicates what lint must know in code; couples the prose list to codes (§8.1) |
| Redirect tables now | Before the first release renames are free; after it, add `RENAMED`/`REMOVED` with the first rename (§8.4) |
| Preview or stable rule states (Ruff), `availableUntil` dates (ESLint), `RemovedInX` warnings tied to versions (Django) | They serve many users and a release cadence; loci has one user per vault, no release, and an offer step that already announces each change |
| Silencing deprecated diagnostics (`SILENCED_SYSTEM_CHECKS`) | An agent would silence instead of fix (research 18 §2.3) |
| `loci upgrade` now | No schema bump is planned; build it with the first one, with the refusal message pointing to it (§8.5) |
| Codes listed per update in `loci updates` output | Lint already reports them, with the update id |

## Alternatives, ranked

1. **Recommended (§9):** update on the diagnostic, format lifecycle in the format
   declaration, `--update <id>`, fixed severity and a fixed hint. Least new structure;
   one place per kind of format element; generic rules keep working; the update entry
   names only its own immutable id. Trade-off: no single table lists every rule tied to
   an update. `rg 'UpdateId::new\(1\)'` finds them, and Django needs the same kind of
   grep (`RemovedInDjangoXXXXWarning` comments, §4).
2. **The draft, trimmed:** a code registry with `since: Option<UpdateId>`, groups
   derived from it, and `--update`; no severity gating. Readable as one table and close
   to rustc. It costs a registry plus one code per deprecation (or a generic code that
   cannot carry two updates). Choose it if, later, most format changes turn out to be
   one-off checks, not entries in a generic field or file table.
3. **The draft as written (with mark gating):** adds per-vault severity, which breaks
   "lint passes" as a Done when and protects a window that `loci status` already closes.
4. **Prose only:** generic `fm.unknown-field` with a delete fix and `--only`. The
   fewest lines, but the fix's scope is wrong (typos, later updates) and the immutable
   entry names a mutable code.

Separately, and independent of the choice: a code list (`Code { name, severity }`
statics plus `ALL`) so `--only` fails loudly on an unknown code, with a test that m0 §7.3
matches it. Small, and worth doing with the first rename or sooner.

## Gaps

- Vale, Obsidian and Logseq lifecycles were not surveyed: no primary source found
  `[gap]`.
- What markdownlint does today when a config names the removed MD002 `[unverified]`.
- That ESLint's CLI prints no deprecation warning by itself comes from searching
  `lib/cli.js` for "deprecat"; formatters were not read `[unverified]`.
- No source measures whether agents respect a hint like "offer the update first" over
  their standing instruction to fix lint errors `[gap]`. The §8.2 ranking assumes they
  usually do. The updates eval suite (updates.md §8) could test it with a case where the
  update is waiting and the task is unrelated.
- The cost tables are my estimates from the current files `[design inference]`.

## Sources

Rust (source read 2026-11-02):
- rustc dev guide, "Editions" (migration lints, edition-specific lints, idiom lints): https://rustc-dev-guide.rust-lang.org/guides/editions.html ; source https://github.com/rust-lang/rustc-dev-guide/blob/a29f828599/src/guides/editions.md
- rustc dev guide, "Errors and lints" (future-incompatible lints, edition-gated lints, renaming or removing a lint): https://rustc-dev-guide.rust-lang.org/diagnostics.html ; source https://github.com/rust-lang/rustc-dev-guide/blob/a29f828599/src/diagnostics.md (lines 667–765)
- `Lint`, `FutureIncompatibleInfo`, `FutureIncompatibilityReason`, `declare_lint!`, `fcw!`: https://github.com/rust-lang/rust/blob/ae36fe22c7/compiler/rustc_lint_defs/src/lib.rs
- `renamed_and_removed_lints`: https://github.com/rust-lang/rust/blob/ae36fe22c7/compiler/rustc_lint_defs/src/builtin.rs (lines 1428–1448)
- `LintStore::register_lints`, `register_renamed`, `register_removed`, `TargetLint`: https://github.com/rust-lang/rust/blob/ae36fe22c7/compiler/rustc_lint/src/context.rs
- `register_builtins`: https://github.com/rust-lang/rust/blob/ae36fe22c7/compiler/rustc_lint/src/lib.rs
- clippy deprecated and renamed lints: https://github.com/rust-lang/rust-clippy/blob/9fd09bc3fb/clippy_lints/src/deprecated_lints.rs ; registration https://github.com/rust-lang/rust-clippy/blob/9fd09bc3fb/clippy_lints/src/lib.rs (lines 452–456) ; generated list https://github.com/rust-lang/rust-clippy/blob/9fd09bc3fb/clippy_lints/src/declared_lints.rs
- `cargo fix --edition`: research 18 §7 and its sources

Ruff:
- `RuleStatus`, `code_to_rule`: https://github.com/astral-sh/ruff/blob/f10755306e/crates/ruff_linter/src/codes.rs ; at 0.5.0 (`RuleGroup`): https://github.com/astral-sh/ruff/blob/0.5.0/crates/ruff_linter/src/codes.rs
- `violation_metadata` examples: https://github.com/astral-sh/ruff/blob/f10755306e/crates/ruff_linter/src/rules/ruff/rules/test_rules.rs
- Selection and redirects: https://github.com/astral-sh/ruff/blob/f10755306e/crates/ruff_linter/src/rule_selector.rs ; https://github.com/astral-sh/ruff/blob/f10755306e/crates/ruff_linter/src/rule_redirects.rs
- Versioning, preview, contributing: https://docs.astral.sh/ruff/versioning/ ; https://docs.astral.sh/ruff/preview/ ; sources `docs/versioning.md`, `docs/preview.md`, `CONTRIBUTING.md` at f10755306e

ESLint:
- Custom rules (`meta.deprecated`, `meta.fixable`): https://eslint.org/docs/latest/extend/custom-rules ; DeprecatedInfo: https://eslint.org/docs/latest/extend/rule-deprecation ; sources at https://github.com/eslint/eslint/tree/896b8ad4f2/docs/src/extend
- `DeprecatedInfo` type: https://github.com/eslint/rewrite/blob/9aa97f2d08/packages/core/src/types.ts (lines 220–291)
- Rule deprecation policy: https://eslint.org/docs/latest/use/rule-deprecation
- `usedDeprecatedRules`: https://eslint.org/docs/latest/integrate/nodejs-api ; `lib/eslint/eslint.js` at 896b8ad4f2
- Removed rules: https://github.com/eslint/eslint/blob/896b8ad4f2/conf/replacements.json ; `lib/config/config.js`, `lib/linter/file-report.js`
- Config migrator: https://github.com/eslint/rewrite/blob/9aa97f2d08/packages/migrate-config/README.md ; migration guide https://eslint.org/docs/latest/use/configure/migration-guide

Django (docs and source at b501b8493f):
- System check framework: https://docs.djangoproject.com/en/dev/topics/checks/ ; reference https://docs.djangoproject.com/en/dev/ref/checks/ (`docs/ref/checks.txt` lines 53–57, 239–270)
- `SILENCED_SYSTEM_CHECKS`: https://docs.djangoproject.com/en/dev/ref/settings/#silenced-system-checks
- `Field._check_deprecation_details`: https://github.com/django/django/blob/b501b8493f/django/db/models/fields/__init__.py (lines 155–156, 548–573)
- Deprecation policy: https://docs.djangoproject.com/en/dev/internals/release-process/#deprecation-policy ; timeline https://docs.djangoproject.com/en/dev/internals/deprecation/ ; deprecating a feature https://docs.djangoproject.com/en/dev/internals/contributing/writing-code/submitting-patches/#deprecating-a-feature
- Migrations: https://docs.djangoproject.com/en/dev/topics/migrations/

Terraform:
- `SchemaVersion`, `StateUpgraders`, `StateUpgrader`: https://github.com/hashicorp/terraform-plugin-sdk/blob/ea3d95550b/helper/schema/resource.go
- `Schema.Deprecated`: https://github.com/hashicorp/terraform-plugin-sdk/blob/ea3d95550b/helper/schema/schema.go

Alembic (at 1bcba93ea0):
- Tutorial (`down_revision`, `alembic_version`, `upgrade head`): https://alembic.sqlalchemy.org/en/latest/tutorial.html
- Cookbook (stamp head, `--check-heads`): https://alembic.sqlalchemy.org/en/latest/cookbook.html
- `alembic check`: https://alembic.sqlalchemy.org/en/latest/autogenerate.html

markdownlint (at 5dd44fe444):
- README (rules and aliases, configuration): https://github.com/DavidAnson/markdownlint/blob/5dd44fe444/README.md
- CHANGELOG: https://github.com/DavidAnson/markdownlint/blob/5dd44fe444/CHANGELOG.md
- Rule module and list: `lib/md001.mjs`, `lib/rules.mjs` at 5dd44fe444

loci (working tree at 60facf7 plus staged changes): `crates/loci-core/src/{lint,envelope,fix,config,vault}.rs`,
`crates/loci/src/commands/updates.rs`, `crates/loci/src/main.rs`,
`crates/loci/tests/fix.rs`, `share/loci/{updates,rules}.md`,
`docs/specs/updates.md`, `docs/specs/m0.md` §2.3, §6.1, §7, §8, §9.2, §9.11, §11,
`docs/DECISIONS.md` D175–D177, D185, D192; research 18.
