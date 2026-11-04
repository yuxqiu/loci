//! `loci updates` (m0 §9.11, docs/specs/updates.md): the vault updates compiled into
//! the binary, offered by the agent and applied on the user's yes.

use loci_core::config::{self, Config, UpdateId};
use loci_core::envelope::{Refusal, Report};
use loci_core::path::VaultPath;
use serde::Serialize;
use serde_json::json;

use super::Ctx;
use crate::assets;

/// What an agent does with the waiting updates; printed above them.
const PROCEDURE: &str = "\
1. Show the user the Offer of every waiting update, together, noting any that look
   done already, or that do not apply here, and why; ask which to apply.
2. Apply the accepted ones oldest first: do the How in the working tree (stop and
   ask if it goes beyond the Offer), then go through its Done when, reporting each
   criterion as met or not, with evidence. Show the diff.
3. Record them all, accepted or not: `loci updates --handled <newest id>`. Never
   commit.
";

/// One entry of `share/loci/updates.md`.
#[derive(Serialize)]
pub struct Update {
    pub id: UpdateId,
    pub title: String,
    /// The whole entry, heading included.
    pub text: String,
}

/// The updates compiled into the binary, oldest first.
pub fn all() -> Vec<Update> {
    parse(assets::UPDATES)
}

/// The entries of `list`: each is a level-2 heading `<id> · <date> · <title>` and its
/// section, as loci's markdown parser reads them (a `## ` line in a code block is text).
fn parse(list: &str) -> Vec<Update> {
    let note = loci_core::parse::markdown(list.to_string());
    note.sections()
        .filter(|(h, _)| h.level == 2)
        .map(|(h, section)| {
            let mut parts = h.content.value(list).splitn(3, " · ");
            let id = parts.next().expect("an id").parse().expect("ids are `u` and four digits");
            let title = parts.nth(1).expect("a date and a title").to_string();
            Update { id, title, text: format!("{}\n", list[section].trim_end()) }
        })
        .collect()
}

/// The updates after mark `mark` (`None`: none handled), oldest first; refused
/// (`updates.newer`) when the vault has handled updates this binary does not know.
pub fn waiting(mark: Option<UpdateId>, list: &[Update]) -> Result<Vec<&Update>, Refusal> {
    if let Some(mark) = mark.filter(|m| *m > list.last().expect("updates ship").id) {
        return Err(Refusal::refused(
            "updates.newer",
            format!("this vault has handled updates through {mark}, newer than this loci knows; use a newer loci"),
        ));
    }
    Ok(list.iter().filter(|u| Some(u.id) > mark).collect())
}

pub fn run(ctx: &Ctx, every: bool, handled: Option<&str>) -> Result<Report, Refusal> {
    let vault = ctx.vault()?;
    let list = all();
    match (every, handled) {
        (true, _) => {
            let text: String = list.iter().map(|u| format!("{}\n", u.text)).collect();
            Ok(Report::ok(json!({"updates": list}), text))
        }
        (false, None) => show_waiting(waiting(vault.config.updates, &list)?),
        (false, Some(id)) => handle(&vault, &list, id),
    }
}

/// The waiting updates in full, after the procedure for applying them.
fn show_waiting(waiting: Vec<&Update>) -> Result<Report, Refusal> {
    let human = if waiting.is_empty() {
        "No updates waiting.\n".to_string()
    } else {
        let entries: String = waiting.iter().map(|u| format!("\n{}", u.text)).collect();
        format!("{} update(s) waiting.\n\n{PROCEDURE}{entries}", waiting.len())
    };
    Ok(Report::ok(json!({"waiting": waiting}), human))
}

/// `--handled <id>`: records every update through `id` as handled, by setting the
/// `updates` key in place. Refuses an unknown id and one already handled.
fn handle(vault: &loci_core::vault::Vault, list: &[Update], id: &str) -> Result<Report, Refusal> {
    let mark = vault.config.updates;
    let Some(target) = list.iter().find(|u| id.parse() == Ok(u.id)) else {
        let ids: Vec<String> = list.iter().map(|u| u.id.to_string()).collect();
        return Err(Refusal::refused("updates.unknown", format!("no update `{id}`; known: {}", ids.join(", "))));
    };
    // Only for its refusal: a vault through a newer update than this loci knows.
    waiting(mark, list)?;
    if let Some(mark) = mark.filter(|m| target.id <= *m) {
        return Err(Refusal::refused(
            "updates.order",
            format!("`{id}` is already handled (this vault is through {mark})"),
        ));
    }

    let path = VaultPath::fixed(config::FILE);
    let text = std::fs::read_to_string(vault.abs(&path)).map_err(Refusal::io)?;
    let updated = Config::set_updates(&text, target.id).map_err(|e| Refusal::refused("config.invalid", e))?;
    vault.write(&path, updated.as_bytes())?;
    Ok(Report::ok(json!({"handled": target.id}), format!("handled updates through {}\n", target.id)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_updates_are_well_formed() {
        let list = all();
        assert!(!list.is_empty());
        for (i, u) in list.iter().enumerate() {
            assert_eq!(u.id.to_string(), format!("u{:04}", i + 1), "ids are u0001, u0002, … in order");
            for field in ["- **Offer:**", "- **How:**", "- **Done when:**"] {
                assert!(u.text.contains(field), "{} has {field}", u.id);
            }
            let date = u.text.lines().next().unwrap().split(" · ").nth(1).unwrap();
            assert!(date.len() == 10 && date.as_bytes()[4] == b'-', "{} is dated: {date}", u.id);
        }
    }

    #[test]
    fn every_update_the_lint_tables_name_has_an_entry_that_applies_and_checks_it() {
        let list = all();
        for id in loci_core::lint::update_ids() {
            let entry = list.iter().find(|u| u.id == id).unwrap_or_else(|| panic!("{id} is in updates.md"));
            let code = loci_core::lint::Code::Update(id);
            assert!(entry.text.contains(&format!("--only {code}")), "{id}'s How runs `loci lint --fix --only {code}`");
            let done = entry.text.split("**Done when:**").nth(1).expect("an entry has Done when");
            assert!(done.contains(&format!("`{code}`")), "{id}'s Done when names `{code}`");
            assert!(entry.text.contains("Declining"), "{id}'s Offer says what declining means");
        }
    }

    #[test]
    fn an_entry_runs_to_the_next_heading_not_to_a_code_line() {
        let list =
            parse("# x\n\n## u0001 · 2026-01-01 · A\n\n```\n## not a heading\n```\n\n## u0002 · 2026-01-02 · B\n\nb\n");
        assert_eq!(list.iter().map(|u| u.id.to_string()).collect::<Vec<_>>(), ["u0001", "u0002"]);
        assert!(list[0].text.contains("## not a heading") && list[0].text.starts_with("## u0001"));
    }

    #[test]
    fn waiting_against_the_list() {
        let list = parse("# x\n\n## u0001 · 2026-01-01 · A\n\na\n\n## u0002 · 2026-01-02 · B\n\nb\n");
        let ids = |m: Option<&str>| match waiting(m.map(|m| m.parse().unwrap()), &list) {
            Ok(w) => Some(w.iter().map(|u| u.id.to_string()).collect::<Vec<_>>()),
            Err(r) => {
                assert_eq!(r.code(), "updates.newer");
                None
            }
        };
        assert_eq!(ids(None), Some(vec!["u0001".into(), "u0002".into()]), "no key: none handled");
        assert_eq!(ids(Some("u0001")), Some(vec!["u0002".into()]));
        assert_eq!(ids(Some("u0002")), Some(vec![]));
        assert_eq!(ids(Some("u0003")), None, "newer than the list");
    }
}
