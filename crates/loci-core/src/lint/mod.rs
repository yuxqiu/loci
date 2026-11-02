//! Core lint rules (m0 §7): the rule table and the edition gate, here; the checks, by
//! category, in the submodules.

mod areas;
mod concepts;
mod disk;
mod links;
mod notes;
mod paths;

pub use concepts::{CONCEPT_FIELDS, Field};
pub use disk::{DISK_CODES, disk_rules};

use crate::config::{Budgets, Config, Editions, UpdateId};
use crate::envelope::{Diagnostic, sort_diagnostics};
use crate::index::Index;
use crate::parse::Note;

/// A lint rule (m0 §7.3): the codes it reports, the editions it applies to, and its
/// check. Adding a rule is adding a row; a rule a vault update retires gets
/// `Editions::until` that update.
pub struct Rule {
    pub codes: &'static [&'static str],
    pub editions: Editions,
    pub check: fn(&Ctx, &mut Vec<Diagnostic>),
}

/// The index rules, by category (one submodule each); the output is sorted. The rules
/// that read file bytes or git are [`disk_rules`] ([`DISK_CODES`]).
pub const RULES: &[Rule] = &[
    // Paths.
    Rule {
        codes: &["kind.unknown", "sources.layout", "slug.format", "journal.path", "path.not-nfc", "inbox.root-file"],
        editions: Editions::ALWAYS,
        check: paths::paths,
    },
    // Every managed note.
    Rule {
        codes: &["file.bom", "fm.unterminated", "fm.yaml", "fm.not-allowed"],
        editions: Editions::ALWAYS,
        check: notes::note_format,
    },
    Rule { codes: &["h1.missing", "h1.multiple", "h1.not-first"], editions: Editions::ALWAYS, check: notes::h1 },
    Rule { codes: &["block.id-duplicate"], editions: Editions::ALWAYS, check: notes::block_ids },
    Rule { codes: &["size.note"], editions: Editions::ALWAYS, check: notes::note_size },
    // Links.
    Rule {
        codes: &["link.markdown", "link.dangling", "link.path-form", "link.dangling-heading", "link.dangling-block"],
        editions: Editions::ALWAYS,
        check: links::links,
    },
    // Concepts.
    Rule {
        codes: &["fm.unknown-field", "fm.missing-field", "fm.id-format", "fm.field-type"],
        editions: Editions::ALWAYS,
        check: concepts::concept_fields,
    },
    Rule { codes: &[UPDATE_CODES], editions: Editions::ALWAYS, check: concepts::outdated_fields },
    Rule { codes: &["fm.id-duplicate", "slug.duplicate"], editions: Editions::ALWAYS, check: concepts::duplicates },
    Rule { codes: &["prereqs.cycle"], editions: Editions::ALWAYS, check: concepts::prereq_cycles },
    // Areas.
    Rule { codes: &["area.missing", "area.format", "area.too-long"], editions: Editions::ALWAYS, check: areas::areas },
];

/// How [`RULES`] lists the `update.<id>` codes: one per vault update.
const UPDATE_CODES: &str = "update.<id>";

/// The code for the old form of vault update `id` (one a pending update changes).
pub fn update_code(id: UpdateId) -> String {
    format!("update.{id}")
}

/// Whether `code` is a code core lint reports: one in a rule's row, or `update.<id>`
/// for an update id, which the caller checks against the updates it knows.
pub fn known_code(code: &str) -> bool {
    code.strip_prefix("update.").is_some_and(|id| id.parse::<UpdateId>().is_ok())
        || RULES.iter().flat_map(|r| r.codes).chain(DISK_CODES).any(|c| *c == code)
}

/// Every vault update the tables name, for the check that each has its entry.
pub fn update_ids() -> Vec<UpdateId> {
    let editions = RULES.iter().map(|r| r.editions).chain(CONCEPT_FIELDS.iter().map(|f| f.editions));
    let mut ids: Vec<UpdateId> = editions.flat_map(|e| [e.from, e.until]).flatten().collect();
    ids.sort();
    ids.dedup();
    ids
}

/// What a rule sees: the index, the budgets, and the vault's edition (its `updates`
/// mark).
pub struct Ctx<'a> {
    pub index: &'a Index,
    pub budgets: &'a Budgets,
    pub mark: Option<UpdateId>,
}

/// The rules of the vault's edition over the index (§7.3), sorted.
pub fn run(index: &Index, config: &Config) -> Vec<Diagnostic> {
    let ctx = Ctx { index, budgets: &config.budgets, mark: config.updates };
    let mut out = vec![];
    for rule in RULES.iter().filter(|r| r.editions.contains(ctx.mark)) {
        let from = out.len();
        (rule.check)(&ctx, &mut out);
        debug_assert!(
            out[from..].iter().all(|d| rule
                .codes
                .iter()
                .any(|c| *c == d.code || (*c == UPDATE_CODES && d.code.starts_with("update.")))),
            "a rule reports only the codes in its row: {:?}",
            out[from..].iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }
    sort_diagnostics(&mut out);
    out
}

/// The managed notes (§4.1), the notes most rules read.
fn managed<'a>(ctx: &'a Ctx) -> impl Iterator<Item = &'a Note> {
    ctx.index.notes().filter(|n| n.kind.is_managed())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::VaultPath;

    fn lint(files: &[(&str, &str)]) -> Vec<(String, String)> {
        let index = crate::index::from_texts(files);
        run(&index, &Config::parse("schema = 1\n").unwrap())
            .into_iter()
            .map(|d| (d.path().unwrap_or_default().to_string(), d.code))
            .collect()
    }

    #[test]
    fn prereqs_must_be_bare_slugs() {
        for bad in ["[[x#H]]", "[[]]", "[[x|y]]", "[[a/x]]", "[[two words]]"] {
            let note = format!("---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\nprereqs: [\"{bad}\"]\n---\n# X\n");
            let files = [("concepts/a/y.md", note.as_str()), ("concepts/a/x.md", "")];
            let d = lint(&files);
            assert!(d.contains(&("concepts/a/y.md".into(), "fm.field-type".into())), "{bad}: {d:?}");
            let index = crate::index::from_texts(&files);
            assert!(
                index.parsed(&VaultPath::new("concepts/a/y.md".into()).unwrap()).unwrap().links.is_empty(),
                "{bad} is not a link"
            );
        }
    }

    #[test]
    fn escaped_frontmatter_links_are_rejected_not_linked() {
        let note = "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\nprereqs: [\"[[caf\\u00e9]]\"]\n---\n# X\n";
        let files = [("concepts/a/x.md", note), ("concepts/a/AREA.md", "# A\n\nArea.\n"), ("concepts/a/café.md", "")];
        let d = lint(&files);
        let x = |code: &str| d.iter().filter(|(p, c)| p == "concepts/a/x.md" && c == code).count();
        assert_eq!(x("fm.field-type"), 1, "{d:?}");
        let index = crate::index::from_texts(&files);
        assert!(
            index.parsed(&VaultPath::new("concepts/a/x.md".into()).unwrap()).unwrap().links.is_empty(),
            "no link from an escaped scalar"
        );
    }

    const OK: &str = "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\n---\n# A\n\nSee [[journal/r/2025/03/25-03-14]].\n";

    #[test]
    fn clean_vault() {
        let r = lint(&[
            ("concepts/x/a.md", OK),
            ("concepts/x/AREA.md", "# X\n\nThe area.\n"),
            ("journal/r/2025/03/25-03-14.md", "# J\n\n[[a]]\n"),
        ]);
        assert_eq!(r, vec![]);
    }

    #[test]
    fn cycle_reported_once() {
        let c = |p: &str| format!("---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\nprereqs: [\"[[{p}]]\"]\n---\n# T\n");
        let r =
            lint(&[("concepts/x/AREA.md", "# X\n\nP.\n"), ("concepts/x/a.md", &c("b")), ("concepts/x/b.md", &c("a"))]);
        assert_eq!(r.iter().filter(|(_, c)| c == "prereqs.cycle").count(), 1);
    }

    #[test]
    fn tags_and_unquoted_prereqs() {
        let r = lint(&[
            ("concepts/x/AREA.md", "# X\n\nP.\n"),
            ("concepts/x/a.md", "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\ntags: [a]\nprereqs: [[b]]\n---\n# T\n"),
        ]);
        let codes: Vec<&str> = r.iter().map(|(_, c)| c.as_str()).collect();
        assert!(codes.contains(&"fm.unknown-field"));
        assert!(codes.contains(&"fm.field-type"));
    }
}
