//! `loci updates` (m0 §9.11, docs/specs/updates.md): the vault updates compiled into
//! the binary, offered by the agent and applied on the user's yes.

use loci_core::config::{self, Config};
use loci_core::envelope::{Refusal, Report};
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
pub struct Update {
    pub id: String,
    pub number: u32,
    pub title: String,
    /// The whole entry, heading included.
    pub text: String,
}

/// The updates compiled into the binary, oldest first.
pub fn all() -> Vec<Update> {
    parse(assets::UPDATES)
}

/// The entries of `list`: each starts at a level-2 heading (as markdown sees it, so a
/// `## ` line in a code block is text) and runs to the next one.
fn parse(list: &str) -> Vec<Update> {
    use pulldown_cmark::{Event, HeadingLevel, Parser, Tag};
    let starts: Vec<usize> = Parser::new(list)
        .into_offset_iter()
        .filter_map(|(e, r)| matches!(e, Event::Start(Tag::Heading { level: HeadingLevel::H2, .. })).then_some(r.start))
        .collect();
    let ends = starts.iter().skip(1).copied().chain([list.len()]);
    starts
        .iter()
        .zip(ends)
        .map(|(&start, end)| {
            let entry = &list[start..end];
            let heading = entry.lines().next().expect("an entry has a heading").trim_start_matches("## ");
            let mut parts = heading.splitn(3, " · ");
            let id = parts.next().expect("an id").to_string();
            let number = config::update_number(&id).expect("ids are `u` and four digits");
            let title = parts.nth(1).expect("a date and a title").to_string();
            Update { id, number, title, text: format!("{}\n", entry.trim_end()) }
        })
        .collect()
}

/// Where a vault stands against the updates the binary knows.
pub enum Standing<'a> {
    /// Updates after the vault's mark (none when it is current).
    Waiting(Vec<&'a Update>),
    /// The vault has handled updates this binary does not know: it is older.
    Newer,
}

/// The standing of a vault whose `updates` key is `mark` (`None`: none handled).
pub fn standing<'a>(mark: Option<&str>, list: &'a [Update]) -> Standing<'a> {
    let m = mark.map_or(0, |id| config::update_number(id).expect("validated when loci.toml was read"));
    if m > list.last().expect("updates ship").number {
        Standing::Newer
    } else {
        Standing::Waiting(list.iter().filter(|u| u.number > m).collect())
    }
}

/// The refusal for a vault that cannot take the updates this binary has.
fn refusal(standing: &Standing, mark: Option<&str>) -> Option<Refusal> {
    let mark = mark.unwrap_or("none");
    match standing {
        Standing::Waiting(_) => None,
        Standing::Newer => Some(Refusal::refused(
            "updates.newer",
            format!("this vault has handled updates through {mark}, newer than this loci knows; use a newer loci"),
        )),
    }
}

pub fn run(ctx: &Ctx, every: bool, handled: Option<&str>) -> Result<Report, Refusal> {
    let vault = ctx.vault()?;
    let list = all();
    let mark = vault.config.updates.as_deref();
    if every {
        let text: String = list.iter().map(|u| format!("{}\n", u.text)).collect();
        let entries: Vec<_> = list.iter().map(|u| json!({"id": u.id, "title": u.title, "text": u.text})).collect();
        return Ok(Report::ok(json!({"updates": entries}), text));
    }
    let standing = standing(mark, &list);
    if let Some(r) = refusal(&standing, mark) {
        return Err(r);
    }
    let Standing::Waiting(waiting) = standing else { unreachable!("refused above") };
    match handled {
        None => {
            let human = if waiting.is_empty() {
                "No updates waiting.\n".to_string()
            } else {
                let entries: String = waiting.iter().map(|u| format!("\n{}", u.text)).collect();
                format!("{} update(s) waiting.\n\n{PROCEDURE}{entries}", waiting.len())
            };
            let entries: Vec<_> =
                waiting.iter().map(|u| json!({"id": u.id, "title": u.title, "text": u.text})).collect();
            Ok(Report::ok(json!({"waiting": entries}), human))
        }
        Some(id) => {
            let Some(target) = list.iter().find(|u| u.id == id) else {
                let ids: Vec<&str> = list.iter().map(|u| u.id.as_str()).collect();
                return Err(Refusal::refused(
                    "updates.unknown",
                    format!("no update `{id}`; known: {}", ids.join(", ")),
                ));
            };
            if !waiting.iter().any(|u| u.id == target.id) {
                return Err(Refusal::refused(
                    "updates.order",
                    format!("`{id}` is already handled (this vault is through {})", mark.unwrap_or("none")),
                ));
            }
            let path = vault.abs(config::FILE);
            let text = std::fs::read_to_string(&path).map_err(Refusal::io)?;
            let updated = Config::set_updates(&text, id).map_err(|e| Refusal::refused("config.invalid", e))?;
            Config::parse(&updated).map_err(|e| Refusal::internal("updates.write", e))?;
            loci_core::vault::write_atomic(&path, updated.as_bytes()).map_err(Refusal::io)?;
            Ok(Report::ok(json!({"handled": id}), format!("handled updates through {id}\n")))
        }
    }
}

/// The `loci status` part: a line when updates are waiting, a warning when the vault
/// cannot take them.
pub fn status(mark: Option<&str>) -> (String, Vec<String>, Option<loci_core::envelope::Diagnostic>) {
    let list = all();
    let standing = standing(mark, &list);
    let diag =
        refusal(&standing, mark).map(|r| loci_core::envelope::Diagnostic::warning(&r.error.code, r.error.message));
    match standing {
        Standing::Waiting(w) if !w.is_empty() => {
            let ids: Vec<String> = w.iter().map(|u| u.id.clone()).collect();
            let line = format!("{} waiting ({}); read them with `loci updates`\n", ids.len(), ids.join(", "));
            (line, ids, diag)
        }
        _ => (String::new(), vec![], diag),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_updates_are_well_formed() {
        let list = all();
        assert!(!list.is_empty());
        for (i, u) in list.iter().enumerate() {
            assert_eq!(u.number, list[0].number + i as u32, "ids only increase, one at a time: {}", u.id);
            for field in ["- **Offer:**", "- **How:**", "- **Done when:**"] {
                assert!(u.text.contains(field), "{} has {field}", u.id);
            }
            let date = u.text.lines().next().unwrap().split(" · ").nth(1).unwrap();
            assert!(date.len() == 10 && date.as_bytes()[4] == b'-', "{} is dated: {date}", u.id);
        }
    }

    #[test]
    fn an_entry_runs_to_the_next_heading_not_to_a_code_line() {
        let list =
            parse("# x\n\n## u0001 · 2026-01-01 · A\n\n```\n## not a heading\n```\n\n## u0002 · 2026-01-02 · B\n\nb\n");
        assert_eq!(list.iter().map(|u| u.id.as_str()).collect::<Vec<_>>(), ["u0001", "u0002"]);
        assert!(list[0].text.contains("## not a heading") && list[0].text.starts_with("## u0001"));
    }

    #[test]
    fn standing_against_the_list() {
        let list = parse("# x\n\n## u0001 · 2026-01-01 · A\n\na\n\n## u0002 · 2026-01-02 · B\n\nb\n");
        let waiting = |m: Option<&str>| match standing(m, &list) {
            Standing::Waiting(w) => Some(w.iter().map(|u| u.id.as_str()).collect::<Vec<_>>()),
            _ => None,
        };
        assert_eq!(waiting(None), Some(vec!["u0001", "u0002"]), "no key: none handled");
        assert_eq!(waiting(Some("u0001")), Some(vec!["u0002"]));
        assert_eq!(waiting(Some("u0002")), Some(vec![]));
        assert!(matches!(standing(Some("u0003"), &list), Standing::Newer));
    }
}
