//! `loci lint --fix` (m0 §9.2; research 18 §1 and §5): safe and unsafe fixes, a second
//! run that changes nothing, a dry run that writes nothing, and verification that
//! refuses fixes that make the vault worse.

mod common;

use common::*;
use insta::assert_snapshot;

const IDS: [&str; 4] = [
    "01JB9XQ7Z3K4M5N6P7Q8R9S0A1",
    "01JB9XQ7Z3K4M5N6P7Q8R9S0A2",
    "01JB9XQ7Z3K4M5N6P7Q8R9S0A3",
    "01JB9XQ7Z3K4M5N6P7Q8R9S0A4",
];

/// One fixable problem per file.
fn broken() -> TestVault {
    TestVault::with_files(
        BIN,
        &[
            ("concepts/a/other.md", concept(IDS[0], "", "# Other\n\n## Section\n\nText ^b1\n\nMore ^b1\n")),
            ("concepts/a/dup.md", concept(IDS[0], "", "# Dup\n")),
            ("concepts/a/no-id.md", "---\nprereqs: []\n---\n# No id\n".to_string()),
            ("concepts/a/bare.md", "# Bare\n".to_string()),
            ("concepts/a/bom.md", format!("\u{feff}{}", concept(IDS[1], "", "# Bom\n"))),
            ("concepts/a/unquoted.md", concept(IDS[2], "prereqs:\n  - [[other]]\n", "# Unquoted\n")),
            ("concepts/a/attachments/d.png", "png".to_string()),
            (
                "views/v.md",
                "# V\n\nSee [Other](concepts/a/other.md) and ![diagram](concepts/a/attachments/d.png), [[othr]], [[other#Sectoin]].\n"
                    .to_string(),
            ),
            ("views/untitled.md", "No title here.\n".to_string()),
        ],
    )
}

fn fixed_codes(env: &serde_json::Value) -> Vec<String> {
    let mut codes: Vec<String> =
        env["data"]["fixed"].as_array().unwrap().iter().map(|f| f["code"].as_str().unwrap().to_string()).collect();
    codes.sort();
    codes
}

#[test]
fn safe_fixes_apply_and_a_second_run_changes_nothing() {
    let v = broken();
    let (_, before) = v.run(&["lint"]);
    let (code, env) = v.run(&["lint", "--fix"]);
    assert_eq!(code, 1, "unsafe and display-only problems remain: {env:#}");
    assert_eq!(
        fixed_codes(&env),
        ["file.bom", "fm.field-type", "fm.missing-field", "fm.missing-field", "link.markdown", "link.markdown"]
    );
    assert!(env["data"]["errors"].as_u64() < before["data"]["errors"].as_u64());
    assert_snapshot!(v.tree());
    // Fixed to a fixpoint: nothing is left to apply.
    v.commit_all("fixed");
    let tree = v.tree();
    let (_, again) = v.run(&["lint", "--fix"]);
    assert_eq!(fixed_codes(&again), Vec::<String>::new());
    assert_eq!(v.tree(), tree);
}

#[test]
fn unsafe_fixes_apply_only_when_asked_and_display_only_never() {
    let v = broken();
    let (_, env) = v.run(&["lint", "--fix", "--unsafe-fixes"]);
    let codes = fixed_codes(&env);
    for code in ["block.id-duplicate", "fm.id-duplicate", "link.dangling"] {
        assert!(codes.contains(&code.to_string()), "{code} in {codes:?}");
    }
    assert!(!codes.contains(&"h1.missing".to_string()), "display-only fixes are never applied");
    let view = v.read("views/v.md");
    assert!(view.contains("[[concepts/a/other|Other]] and ![[concepts/a/attachments/d.png|diagram]]"), "{view}");
    assert!(view.contains("[[other]], [[other#Sectoin]]"), "{view}");
    assert_eq!(v.read("views/untitled.md"), "No title here.\n");
    let (_, after) = v.run(&["lint"]);
    assert_eq!(with_code(&after, "h1.missing").len(), 1);
    let mut left: Vec<&str> =
        after["diagnostics"].as_array().unwrap().iter().map(|d| d["code"].as_str().unwrap()).collect();
    left.sort();
    assert_eq!(
        left,
        ["area.missing", "h1.missing", "link.dangling-heading"],
        "only problems without an applicable fix are left"
    );
    // `--only` restricts the fixes to one code.
    let v = broken();
    let (_, env) = v.run(&["lint", "--fix", "--only", "file.bom"]);
    assert_eq!(fixed_codes(&env), ["file.bom"]);
}

#[test]
fn a_dry_run_writes_nothing_and_reports_the_same_fixes() {
    let v = broken();
    let tree = v.tree();
    let (_, dry) = v.run(&["lint", "--fix", "--dry-run"]);
    assert_eq!(dry["data"]["dry_run"], true);
    assert_eq!(v.tree(), tree);
    let (_, real) = v.run(&["lint", "--fix"]);
    assert_eq!(fixed_codes(&dry), fixed_codes(&real));
}

#[test]
fn fixes_that_make_the_vault_worse_are_refused() {
    // The unsafe fix writes `[[views/my%20note]]`, which names no file.
    let v = TestVault::with_files(
        BIN,
        &[("views/my note.md", "# My note\n"), ("views/v.md", "# V\n\n[x](views/my%20note.md)\n")],
    );
    let tree = v.tree();
    let (code, env) = v.run(&["lint", "--fix"]);
    assert_eq!((code, fixed_codes(&env)), (1, vec![]), "an unsafe fix is not applied by default");
    let (code, env) = v.run(&["lint", "--fix", "--unsafe-fixes"]);
    assert_eq!((code, error_code(&env)), (1, "lint.fix-verify"), "{env:#}");
    assert!(env["error"]["message"].as_str().unwrap().contains("`link.dangling`"), "{env:#}");
    assert_eq!(v.tree(), tree);
}

#[test]
fn a_name_not_in_nfc_is_fixed_by_mv_not_lint() {
    let nfd = "concepts/a/cafe\u{301}.md";
    let v = TestVault::with_files(
        BIN,
        &[(nfd, concept(IDS[3], "", "# Café\n")), ("views/v.md", format!("# V\n\n[[{}]]\n", &nfd[..nfd.len() - 3]))],
    );
    let (_, env) = v.run(&["lint"]);
    let d = with_code(&env, "path.not-nfc");
    assert!(d[0]["hint"].as_str().unwrap().contains("loci mv concepts/a/caf\u{e9}.md"), "{env:#}");
    assert!(d[0].get("fixes").is_none(), "renames are `mv`'s, not `--fix`'s");
}

#[test]
fn a_misspelt_prereq_is_named_by_its_slug_and_fixed_in_the_frontmatter() {
    let v = TestVault::with_files(
        BIN,
        &[
            ("concepts/a/queue.md", concept(IDS[0], "", "# Queue\n")),
            ("concepts/a/x.md", concept(IDS[1], "prereqs: ['[[queu]]'] # why\n", "# X\n")),
        ],
    );
    let (_, env) = v.run(&["lint"]);
    let d = with_code(&env, "link.dangling");
    assert_eq!(d[0]["message"], "`queu` does not resolve", "{env:#}");
    let (_, env) = v.run(&["lint", "--fix", "--unsafe-fixes", "--only", "link.dangling"]);
    assert_eq!(fixed_codes(&env), ["link.dangling"], "{env:#}");
    assert_eq!(v.read("concepts/a/x.md"), concept(IDS[1], "prereqs:\n  - \"[[queue]]\"\n", "# X\n"));
}

/// A concept with u0001's removed `digest` field and another unknown field.
fn with_digest() -> TestVault {
    TestVault::with_files(
        BIN,
        &[
            ("concepts/a/AREA.md", "# A\n\nAlgorithms.\n".to_string()),
            ("concepts/a/x.md", concept(IDS[0], "digest: \"one line\"\nnote: x\n", "# X\n")),
        ],
    )
}

#[test]
fn before_an_update_its_old_form_is_valid_but_reported_with_its_fix() {
    // No `updates` key: no update handled, so u0001 is pending.
    let v = with_digest();
    let (code, env) = v.run(&["lint"]);
    let old = with_code(&env, "update.u0001");
    assert_eq!(old.len(), 1, "{env:#}");
    assert_eq!(old[0]["severity"], "warning");
    assert!(old[0]["hint"].as_str().unwrap().contains("loci lint --fix --only update.u0001"));
    // `digest` is a field in this edition; only `note` is unknown.
    let unknown = with_code(&env, "fm.unknown-field");
    assert_eq!(unknown.len(), 1, "{env:#}");
    assert!(unknown[0]["message"].as_str().unwrap().contains("`note`"));
    assert_eq!(code, 1, "the unknown `note` is an error");
    let (_, env) = v.run(&["lint", "--fix"]);
    assert_eq!(fixed_codes(&env), Vec::<String>::new(), "the field may hold text worth keeping");
    let (_, env) = v.run(&["lint", "--fix", "--unsafe-fixes", "--only", "update.u0001"]);
    assert_eq!(fixed_codes(&env), ["update.u0001"], "{env:#}");
    assert_eq!(v.read("concepts/a/x.md"), concept(IDS[0], "note: x\n", "# X\n"));
}

#[test]
fn after_an_update_its_old_form_is_an_ordinary_error() {
    let v = with_digest();
    v.write("loci.toml", "schema = 1\nupdates = \"u0001\"\n");
    let (_, env) = v.run(&["lint"]);
    assert!(with_code(&env, "update.u0001").is_empty(), "{env:#}");
    let unknown = with_code(&env, "fm.unknown-field");
    assert_eq!(unknown.len(), 2, "`digest` is now unknown too: {env:#}");
}

#[test]
fn fixes_refuse_a_dirty_tree_unless_allowed() {
    let v = with_digest();
    v.write("views/scratch.md", "# Scratch\n");
    let (code, env) = v.run(&["lint", "--fix"]);
    assert_eq!((code, error_code(&env)), (1, "lint.dirty"));
    let (_, env) = v.run(&["lint", "--fix", "--unsafe-fixes", "--only", "update.u0001", "--dry-run"]);
    assert_eq!(fixed_codes(&env), ["update.u0001"], "a dry run may run on a dirty tree");
    let (_, env) = v.run(&["lint", "--fix", "--unsafe-fixes", "--only", "update.u0001", "--allow-dirty"]);
    assert_eq!(fixed_codes(&env), ["update.u0001"], "{env:#}");
}

#[test]
fn only_refuses_a_code_core_lint_does_not_report() {
    let v = broken();
    for bad in ["link.dangle", "update.u9999", "update.x"] {
        let (code, env) = v.run(&["lint", "--fix", "--only", bad]);
        assert_eq!((code, error_code(&env)), (1, "lint.unknown-code"), "{bad}");
    }
    let (_, env) = v.run(&["lint", "--fix", "--only", "file.bom"]);
    assert_eq!(fixed_codes(&env), ["file.bom"]);
}
