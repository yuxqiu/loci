//! Core lint rules (m0 §7): the rule table and the edition gate, here; the checks, by
//! category, in the submodules.

mod areas;
mod code;
mod concepts;
mod disk;
mod links;
mod notes;
mod paths;

pub use code::Code;
use concepts::CONCEPT_FIELDS;
use disk::DISK_CODES;
pub use disk::disk_rules;

use crate::config::{Budgets, Editions, UpdateId};
use crate::envelope::{Diagnostic, sort_diagnostics};
use crate::index::Index;
use crate::parse::Note;

/// A lint rule (m0 §7.3): the codes it reports, the editions it applies to, and its
/// check. Adding a rule is adding a row; a rule a vault update retires gets
/// `Editions::until` that update.
struct Rule {
    codes: Codes,
    editions: Editions,
    check: fn(&Ctx, &mut Vec<Diagnostic>),
}

/// The codes a rule reports.
enum Codes {
    These(&'static [Code]),
    /// `update.<id>`, for the updates the tables name.
    Updates,
}

impl Codes {
    fn contains(&self, code: Code) -> bool {
        match (self, code) {
            (Codes::These(codes), code) => codes.contains(&code),
            (Codes::Updates, Code::Update(id)) => update_ids().contains(&id),
            (Codes::Updates, _) => false,
        }
    }
}

/// The index rules, by category (one submodule each); the output is sorted. The rules
/// that compare with git's HEAD are [`disk_rules`] ([`DISK_CODES`]).
const RULES: &[Rule] = &[
    // Paths.
    Rule {
        codes: Codes::These(&[
            Code::KindUnknown,
            Code::SourcesLayout,
            Code::SlugFormat,
            Code::JournalPath,
            Code::PathNotNfc,
            Code::PathNameClash,
            Code::InboxRootFile,
        ]),
        editions: Editions::ALWAYS,
        check: paths::paths,
    },
    // Notes: every managed note, but `size.note` (concepts).
    Rule {
        codes: Codes::These(&[Code::FileBom, Code::FmUnterminated, Code::FmYaml, Code::FmNotAllowed]),
        editions: Editions::ALWAYS,
        check: notes::note_format,
    },
    Rule {
        codes: Codes::These(&[Code::H1Missing, Code::H1Multiple, Code::H1NotFirst]),
        editions: Editions::ALWAYS,
        check: notes::h1,
    },
    Rule { codes: Codes::These(&[Code::BlockIdDuplicate]), editions: Editions::ALWAYS, check: notes::block_ids },
    Rule { codes: Codes::These(&[Code::SizeNote]), editions: Editions::ALWAYS, check: notes::note_size },
    // Links.
    Rule {
        codes: Codes::These(&[
            Code::LinkMarkdown,
            Code::LinkDangling,
            Code::LinkPathForm,
            Code::LinkDanglingHeading,
            Code::LinkDanglingBlock,
        ]),
        editions: Editions::ALWAYS,
        check: links::links,
    },
    // Concepts.
    Rule {
        codes: Codes::These(&[Code::FmUnknownField, Code::FmMissingField, Code::FmIdFormat, Code::FmFieldType]),
        editions: Editions::ALWAYS,
        check: concepts::concept_fields,
    },
    Rule { codes: Codes::Updates, editions: Editions::ALWAYS, check: concepts::outdated_fields },
    Rule {
        codes: Codes::These(&[Code::FmIdDuplicate, Code::SlugDuplicate]),
        editions: Editions::ALWAYS,
        check: concepts::duplicates,
    },
    Rule { codes: Codes::These(&[Code::PrereqsCycle]), editions: Editions::ALWAYS, check: concepts::prereq_cycles },
    // Areas.
    Rule {
        codes: Codes::These(&[Code::AreaMissing, Code::AreaFormat, Code::AreaTooLong]),
        editions: Editions::ALWAYS,
        check: areas::areas,
    },
];

/// `code` as a code core lint reports, if it is one: in a rule's row, or `update.<id>`
/// for an update the tables name.
pub fn core_code(code: &str) -> Option<Code> {
    code.parse::<Code>().ok().filter(|c| RULES.iter().any(|r| r.codes.contains(*c)) || DISK_CODES.contains(c))
}

/// Every vault update the tables name, for the check that each has its entry.
pub fn update_ids() -> Vec<UpdateId> {
    let editions = RULES.iter().map(|r| r.editions).chain(CONCEPT_FIELDS.iter().map(|f| f.editions));
    let mut ids: Vec<UpdateId> = editions.flat_map(|e| [e.since, e.until]).flatten().collect();
    ids.sort();
    ids.dedup();
    ids
}

/// What a rule sees: the index, the budgets, and the vault's edition (its `updates`
/// mark).
struct Ctx<'a> {
    index: &'a Index<'a>,
    budgets: &'a Budgets,
    mark: Option<UpdateId>,
}

/// The rules of the vault's edition over the index (§7.3), sorted.
pub fn run(index: &Index) -> Vec<Diagnostic> {
    let config = index.vault().config();
    let ctx = Ctx { index, budgets: &config.budgets, mark: config.updates };
    let mut out = vec![];
    for rule in RULES.iter().filter(|r| r.editions.contains(ctx.mark)) {
        let from = out.len();
        (rule.check)(&ctx, &mut out);
        debug_assert!(
            out[from..].iter().all(|d| d.code.parse().is_ok_and(|c| rule.codes.contains(c))),
            "a rule reports only the codes in its row: {:?}",
            out[from..].iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }
    sort_diagnostics(&mut out);
    out
}

/// The managed notes (§3), the notes most rules read.
fn managed<'a>(ctx: &'a Ctx) -> impl Iterator<Item = &'a Note> {
    ctx.index.notes().filter(|n| n.kind.is_managed())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::vp;

    fn lint(files: &[(&str, &str)]) -> Vec<(String, String)> {
        let index = crate::testing::index(files);
        run(&index).into_iter().map(|d| (d.path().map_or_else(String::new, ToString::to_string), d.code)).collect()
    }

    #[test]
    fn prereqs_must_be_bare_slugs() {
        for bad in ["[[x#H]]", "[[]]", "[[x|y]]", "[[a/x]]", "[[two words]]"] {
            let note = format!("---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\nprereqs: [\"{bad}\"]\n---\n# X\n");
            let files = [("concepts/a/y.md", note.as_str()), ("concepts/a/x.md", "")];
            let d = lint(&files);
            assert!(d.contains(&("concepts/a/y.md".into(), "fm.field-type".into())), "{bad}: {d:?}");
            let index = crate::testing::index(&files);
            assert!(index.note_at(&vp("concepts/a/y.md")).unwrap().links.is_empty(), "{bad} is not a link");
        }
    }

    #[test]
    fn escaped_frontmatter_links_are_links() {
        let note = "---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\nprereqs: [\"[[caf\\u00e9]]\"]\n---\n# X\n";
        let files = [("concepts/a/x.md", note), ("concepts/a/AREA.md", "# A\n\nArea.\n"), ("concepts/a/café.md", "")];
        let d = lint(&files);
        assert!(!d.iter().any(|(p, c)| p == "concepts/a/x.md" && c == "fm.field-type"), "{d:?}");
        let index = crate::testing::index(&files);
        let x = index.note_at(&vp("concepts/a/x.md")).unwrap();
        assert_eq!(x.links[0].target.names, crate::parse::Names::Slug("café".parse().unwrap()));
    }

    #[test]
    fn a_lone_prereq_gets_the_quoting_fix() {
        for lone in ["[[x]]", "\"[[x]]\""] {
            let note = format!("---\nid: 01JB9XQ7Z3K4M5N6P7Q8R9S0TV\nprereqs: {lone}\n---\n# Y\n");
            let index = crate::testing::index(&[("concepts/a/y.md", note.as_str()), ("concepts/a/x.md", "")]);
            let diags = run(&index);
            let d: Vec<_> = diags.iter().filter(|d| d.code == "fm.field-type").collect();
            assert_eq!(d.len(), 1, "{lone}: {diags:?}");
            assert_eq!(d[0].fixes.len(), 1, "{lone}: {diags:?}");
            assert_eq!(d[0].fixes[0].message, "put it in a list", "{lone}");
            let fixed =
                crate::fix::Plan::new(&index, &crate::fix::Options { unsafe_fixes: false, only: None }).unwrap();
            let text = &fixed.change.writes[&vp("concepts/a/y.md")].text;
            assert!(text.contains("prereqs:\n  - \"[[x]]\"\n"), "{lone}: {text}");
        }
    }

    #[test]
    fn a_block_link_to_an_id_that_is_no_block_id_says_so() {
        // `^quo` is a valid id, but nothing in a quote has one (§4.6): the block is gone.
        let files = [
            ("views/v.md", "# V\n\nStep one ^step-1\n\nStep two ^step-2a\n\n> quoted ^quo\n\n- item\n\n^list-1\n"),
            ("views/w.md", "# W\n\n[[views/v#^step-1]] [[views/v#^step-2]] [[views/v#^quo]] [[views/v#^list-1]]\n"),
        ];
        let index = crate::testing::index(&files);
        let hints: Vec<String> =
            run(&index).iter().filter(|d| d.code == "link.dangling-block").map(|d| d.hint.clone().unwrap()).collect();
        assert_eq!(hints.len(), 4, "{hints:?}");
        assert!(hints[0].contains("`^step-1` is in `views/v.md` but is no block id"), "{hints:?}");
        assert!(hints[1].contains("removed"), "`^step-2a` is another id: {hints:?}");
        assert!(hints[2].contains("removed"), "`^quo` is a valid id: {hints:?}");
        assert!(hints[3].contains("is no block id"), "an id on its own line: {hints:?}");
    }

    #[test]
    fn a_note_beside_a_file_of_its_short_name() {
        let d = lint(&[("concepts/x/x.png", ""), ("concepts/x/x.png.md", "# X\n"), ("concepts/x/y.md", "# Y\n")]);
        let clashes: Vec<_> = d.iter().filter(|(_, c)| c == "path.name-clash").map(|(p, _)| p.as_str()).collect();
        assert_eq!(clashes, ["concepts/x/x.png.md"]);
        let root = lint(&[("index", ""), ("index.md", "# I\n")]);
        assert!(!root.iter().any(|(_, c)| c == "path.name-clash"), "no path link names a root file: {root:?}");
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
}
