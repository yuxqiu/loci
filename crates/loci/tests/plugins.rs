//! Plugin dispatch and the plugin contract (m0 §2.2, §2.3) with a stub plugin
//! `loci-stub` first on `PATH` (fixture `dispatch`).

mod common;

use common::*;
use serde_json::{Value, json};

fn vault_with(stub: Stub) -> TestVault {
    let v = TestVault::committed(BIN, "dispatch");
    v.install_stub(&stub);
    v
}

fn plugin_failed(env: &Value) -> Vec<String> {
    with_code(env, "plugin.failed").iter().map(|d| d["message"].as_str().unwrap().to_string()).collect()
}

#[test]
fn outside_a_vault_is_vault_not_found() {
    let v = vault_with(Stub::default());
    let outside = v.home();
    let (code, env) = v.run_in(&outside, &["stub"]);
    assert_eq!((code, error_code(&env)), (1, "vault.not-found"));
    let (code, env) = v.run_in(&outside, &["skill", "show"]);
    assert_eq!((code, error_code(&env)), (1, "vault.not-found"));
}

#[test]
fn dispatch_passes_args_env_and_exit_code() {
    let v = vault_with(Stub::default());
    let (code, out, _) = v.run_raw(&["stub", "a", "b"]);
    assert_eq!(code, 7);
    assert_eq!(out, format!("args=a b\nvault={}\nversion={}\n", v.root.display(), env!("CARGO_PKG_VERSION")));
    // `--json` before the name is appended.
    let (code, out, _) = v.run_raw(&["--json", "stub", "x"]);
    assert_eq!(code, 7);
    assert!(out.starts_with("args=x --json\n"), "{out}");
    // Run from a subdirectory: LOCI_VAULT is still the absolute root.
    let (_, out, _) = v.run_raw_in(&v.root.join("concepts"), &["stub"]);
    assert!(out.contains(&format!("vault={}\n", v.root.display())), "{out}");
}

#[test]
fn unknown_command_lists_core_commands_and_plugins() {
    let v = vault_with(Stub::default());
    let (code, env) = v.run(&["nosuch"]);
    assert_eq!((code, error_code(&env)), (1, "cmd.unknown"));
    let msg = env["error"]["message"].as_str().unwrap();
    for c in ["init", "lint", "rules", "mv", "status", "new", "skill", "block-ids"] {
        assert!(msg.contains(c), "{msg}");
    }
    assert!(msg.contains("stub"), "{msg}");
}

#[test]
fn core_command_names_are_never_dispatched() {
    let v = vault_with(Stub::default());
    // A `loci-lint` on PATH is not a plugin and never runs.
    let fake = Stub {
        lint: envelope_json("lint", Value::Null, json!([stub_diag("lint.fake", "x.md")]), Value::Null),
        ..Stub::default()
    };
    v.install_stub(&fake);
    std::fs::rename(v.bin().join("loci-stub"), v.bin().join("loci-lint")).unwrap();
    let (code, env) = v.run(&["lint"]);
    assert_eq!(code, 0, "{}", diag_lines(&env));
    assert_eq!(env["data"]["plugins"], json!([]));
    assert!(with_code(&env, "lint.fake").is_empty());
}

#[test]
fn lint_merges_and_counts_plugin_diagnostics() {
    let v = vault_with(Stub::default());
    let (code, env) = v.run(&["lint"]);
    assert_eq!(code, 0, "{}", diag_lines(&env));
    assert_eq!(env["data"]["plugins"], json!(["stub"]));
    let stub = with_code(&env, "stub.note");
    assert_eq!(stub.len(), 1);
    assert_eq!(path_of(stub[0]), "concepts/a/x.md");
    let total = diags(&env).len();
    assert_eq!(
        env["data"]["warnings"].as_u64().unwrap() as usize + env["data"]["errors"].as_u64().unwrap() as usize,
        total
    );
    assert_eq!(env["data"]["errors"], 0);
    // A plugin error counts as an error and fails lint.
    let mut d = stub_diag("stub.bad", "concepts/a/x.md");
    d["severity"] = json!("error");
    let v = vault_with(Stub {
        lint: envelope_json("stub lint", Value::Null, json!([d]), Value::Null),
        lint_exit: 1,
        ..Stub::default()
    });
    let (code, env) = v.run(&["lint"]);
    assert_eq!(code, 1);
    assert_eq!(env["data"]["errors"], 1);
    assert_eq!(with_code(&env, "stub.bad").len(), 1);
}

#[test]
fn lint_plugin_failures_drop_the_output() {
    let diag = json!([stub_diag("stub.note", "concepts/a/x.md")]);
    let cases = [
        (
            "non-null error",
            Stub {
                lint: envelope_json(
                    "stub lint",
                    Value::Null,
                    diag.clone(),
                    json!({"code": "stub.oops", "message": "no"}),
                ),
                lint_exit: 1,
                ..Stub::default()
            },
        ),
        (
            "unprefixed code",
            Stub {
                lint: envelope_json(
                    "stub lint",
                    Value::Null,
                    json!([stub_diag("stub.note", "a.md"), stub_diag("other.code", "a.md")]),
                    Value::Null,
                ),
                ..Stub::default()
            },
        ),
        ("exit code 2", Stub { lint_exit: 2, ..Stub::default() }),
        ("invalid envelope", Stub { lint: "not json".into(), ..Stub::default() }),
    ];
    for (what, stub) in cases {
        let v = vault_with(stub);
        let (code, env) = v.run(&["lint"]);
        assert_eq!(code, 1, "{what}: {}", diag_lines(&env));
        let failed = plugin_failed(&env);
        assert_eq!(failed.len(), 1, "{what}: {}", diag_lines(&env));
        assert!(failed[0].contains("stub") && failed[0].contains("lint"), "{what}: {failed:?}");
        if matches!(what, "exit code 2" | "invalid envelope") {
            assert!(failed[0].contains(STUB_LINT_STDERR), "{what}: the plugin's stderr is reported: {failed:?}");
        }
        assert!(with_code(&env, "stub.note").is_empty(), "{what}: output not dropped");
        assert!(with_code(&env, "other.code").is_empty(), "{what}: output not dropped");
    }
}

#[test]
fn skill_show_lists_plugin_skills() {
    let v = vault_with(Stub::default());
    let (code, env) = v.run(&["skill", "show"]);
    assert_eq!(code, 0, "{env:#}");
    assert_eq!(env["command"], "skill show");
    let skills = env["data"]["skills"].as_array().unwrap();
    let stub: Vec<_> = skills.iter().filter(|s| s["plugin"] == "stub").collect();
    assert_eq!(stub, [&json!({"name": "stub-hello", "description": "Say hello from the stub.", "plugin": "stub"})]);
    assert!(skills.iter().any(|s| s["plugin"].is_null()), "core skills listed");
    let (code, env) = v.run(&["skill", "show", "stub-hello"]);
    assert_eq!(code, 0);
    assert_eq!(env["data"]["body"], STUB_SKILL_BODY);
    assert_eq!(env["data"]["plugin"], "stub");
    let (code, raw, _) = v.run_raw(&["skill", "show", "stub-hello"]);
    assert_eq!((code, raw.as_str()), (0, STUB_SKILL_BODY));
    let (code, env) = v.run(&["skill", "show", "nope"]);
    assert_eq!((code, error_code(&env)), (1, "skill.unknown"));
    assert!(env["error"]["message"].as_str().unwrap().contains("stub-hello"));
}

#[test]
fn skills_plugin_failures_drop_the_output() {
    let skills = json!({"skills": [{"name": "stub-hello", "description": "d", "body": "b"}]});
    let cases = [
        (
            "non-null error",
            Stub {
                skills: envelope_json(
                    "stub skills",
                    Value::Null,
                    json!([]),
                    json!({"code": "stub.oops", "message": "no"}),
                ),
                skills_exit: 1,
                ..Stub::default()
            },
        ),
        (
            "unprefixed code",
            Stub {
                skills: envelope_json(
                    "stub skills",
                    skills.clone(),
                    json!([stub_diag("other.code", "a.md")]),
                    Value::Null,
                ),
                ..Stub::default()
            },
        ),
        (
            "unprefixed skill",
            Stub {
                skills: envelope_json(
                    "stub skills",
                    json!({"skills": [{"name": "hello", "description": "d", "body": "b"}]}),
                    json!([]),
                    Value::Null,
                ),
                ..Stub::default()
            },
        ),
        ("exit code 3", Stub { skills_exit: 3, ..Stub::default() }),
        ("invalid envelope", Stub { skills: "{".into(), ..Stub::default() }),
    ];
    for (what, stub) in cases {
        let v = vault_with(stub);
        let (code, env) = v.run(&["skill", "show"]);
        assert_eq!(code, 1, "{what}: an error diagnostic fails the command: {env:#}");
        let failed = plugin_failed(&env);
        assert_eq!(failed.len(), 1, "{what}: {}", diag_lines(&env));
        assert!(failed[0].contains("stub") && failed[0].contains("skills"), "{what}: {failed:?}");
        let skills = env["data"]["skills"].as_array().unwrap();
        assert!(skills.iter().all(|s| s["plugin"].is_null()), "{what}: plugin skills not dropped");
        assert!(!skills.is_empty(), "{what}: core skills still listed");
    }
}
