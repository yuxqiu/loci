//! The plugin's commands (learn-plugin §2).

use std::collections::BTreeMap;
use std::io::{BufRead, Read, Write};

use jiff::Timestamp;
use loci_core::envelope::{Diagnostic, Refusal, Report};
use loci_core::index::Index;
use loci_core::plugin::Skill;
use loci_core::vault::Vault;
use serde_json::json;

use crate::cards::{self, Card, CardId};
use crate::config::Config;
use crate::reviewlog::{self, Grade, Input, Line, Subject};
use crate::schedule::{self, Estimate, Scheduler};

const SKILL_FILES: &[(&str, &str)] = &[("learn-session", include_str!("../skills/learn-session/SKILL.md"))];

pub fn skills() -> Result<Report, Refusal> {
    let skills: Vec<Skill> = SKILL_FILES.iter().map(|(n, b)| Skill::from_markdown(n, b)).collect();
    let human: String = skills.iter().map(|s| format!("{}: {}\n", s.name, s.description)).collect();
    Ok(Report::ok(json!({ "skills": skills }), human))
}

pub fn lint(vault: &Vault) -> Result<Report, Refusal> {
    let index = Index::open(vault);
    let (all, issues) = cards::all(&index);
    let text_of =
        |path: &loci_core::path::VaultPath| index.parsed(path).map(|n| n.text.as_str()).expect("cards come from notes");
    let mut diags = vec![];
    for i in &issues {
        diags.push(
            Diagnostic::error(
                "learn.card-format",
                "card is not `- question :: answer`, optionally followed by a block id",
            )
            .at_path(i.path.as_str())
            .at(text_of(&i.path), i.range.clone()),
        );
    }

    let mut by_id: BTreeMap<loci_core::id::Ulid, Vec<&Card>> = BTreeMap::new();
    for c in &all {
        let at = |code: &str, msg: String| {
            Diagnostic::error(code, msg).at_path(c.path.as_str()).at(text_of(&c.path), c.range.clone())
        };
        match &c.id {
            CardId::Missing => diags.push(
                at("learn.no-id", "card has no block id".into())
                    .with_hint(format!("loci block-ids {} --section Cards", c.path)),
            ),
            CardId::Malformed(id) => diags.push(
                at("learn.id-form", format!("card id `^{id}` is not a ULID"))
                    .with_hint(format!("delete the id and run `loci block-ids {} --section Cards`", c.path)),
            ),
            CardId::Valid(id) => by_id.entry(*id).or_default().push(c),
        }
    }

    for (id, cs) in by_id.iter().filter(|(_, cs)| cs.len() > 1) {
        let others: Vec<&str> = cs[1..].iter().map(|c| c.path.as_str()).collect();
        diags.push(
            Diagnostic::error("learn.id-duplicate", format!("card id `^{id}` is used by {}", others.join(", ")))
                .at_path(cs[0].path.as_str())
                .at(text_of(&cs[0].path), cs[0].range.clone()),
        );
    }

    let errors = diags.len();
    Ok(Report::ok(serde_json::Value::Null, format!("{} cards, {errors} errors\n", all.len())).with_diagnostics(diags))
}

/// Validates a line against the working tree: its card or concept must exist.
fn check_exists(line: &Line, all: &[Card], index: &Index) -> Result<(), Refusal> {
    match &line.subject {
        Subject::Card(card) if !all.iter().any(|c| c.id.valid() == Some(*card)) => {
            Err(Refusal::refused("learn.unknown-card", format!("no card has id `{card}`")))
        }
        Subject::Concept(concept) if !index.concepts().any(|n| n.id() == Some(*concept)) => {
            Err(Refusal::refused("learn.unknown-concept", format!("no concept has id `{concept}`")))
        }
        _ => Ok(()),
    }
}

pub fn log(vault: &Vault, mut stdin: impl Read) -> Result<Report, Refusal> {
    let mut input = String::new();
    stdin.read_to_string(&mut input).map_err(Refusal::io)?;
    let bad = |e: String| Refusal::refused("learn.bad-input", e);
    let parsed: Input = serde_json::from_str(&input).map_err(|e| bad(e.to_string()))?;
    let line = parsed.into_line(Timestamp::now()).map_err(bad)?;
    let index = Index::open(vault);
    let (all, _) = cards::all(&index);
    check_exists(&line, &all, &index)?;
    let before = reviewlog::append(&vault.root, &line).map_err(Refusal::io)?;
    match reviewlog::commit(&vault.root, "log") {
        Ok(sha) => Ok(Report::ok(json!({"line": line, "commit": sha}), format!("logged {sha}\n"))),
        Err(e) => Err(match reviewlog::truncate(&vault.root, before) {
            Ok(()) => Refusal::git(e),
            Err(t) => Refusal::git(format!("{e}; removing the uncommitted line also failed: {t}")),
        }),
    }
}

pub fn state(vault: &Vault, slug: &str) -> Result<Report, Refusal> {
    let index = Index::open(vault);
    let slug = loci_core::path::VaultPath::from_arg(slug)?;
    let note = index.concept(&slug, "learn.unknown-concept")?;
    let Some(concept) = note.id() else {
        return Err(Refusal::refused("learn.unknown-concept", format!("`{slug}` has no valid id")));
    };
    let config = Config::load(&vault.root)?;
    let lines = reviewlog::read(&vault.root)?;
    let (note_cards, _) = cards::of_note(note);
    let (estimate, used) =
        schedule::estimate(&note_cards, concept, &lines, &Scheduler::new(config.desired_retention), Timestamp::now());
    let human = match &estimate {
        Estimate::Cards { r } => format!("{slug}: recall {:.0}% (from {} line(s))\n", r * 100.0, used.len()),
        Estimate::Item { grade, age_days } => format!("{slug}: last quiz {} {age_days:.1} days ago\n", grade.outcome()),
        Estimate::Unknown => format!("{slug}: unknown\n"),
    };
    Ok(Report::ok(json!({"concept": concept, "estimate": estimate, "lines": used}), human))
}

pub fn review(vault: &Vault) -> Result<Report, Refusal> {
    let config = Config::load(&vault.root)?;
    let lines = reviewlog::read(&vault.root)?;
    let index = Index::open(vault);
    let (all, _) = cards::all(&index);
    let scheduler = Scheduler::new(config.desired_retention);
    let (session, new_count) = schedule::select(&all, &lines, &scheduler, config.new_per_day, Timestamp::now());

    // Each grade is appended at once; the lines are committed at the end (§2.6 step 6).
    let mut reviewed = 0;
    let stopped = grade_cards(&session, &vault.root, &mut reviewed);

    // Lines already appended are committed even when the session stopped on an error.
    let commit =
        if reviewed > 0 { Some(reviewlog::commit(&vault.root, "review").map_err(Refusal::git)?) } else { None };
    if let Err(e) = stopped {
        let committed = commit.map_or(String::new(), |sha| format!(" ({reviewed} grades committed in {sha})"));
        return Err(Refusal::io(format!("the session stopped: {e}{committed}")));
    }

    let due_left = session.len() - reviewed;
    let data = json!({"reviewed": reviewed, "new": new_count, "due_left": due_left, "commit": commit});
    Ok(Report::ok(data, format!("reviewed {reviewed}, {due_left} left\n")))
}

/// Asks for a grade on each card of `session` and appends it to the log, counting in
/// `appended`. Stops at the end, on `q` or end of input (`Ok`), or on an I/O error.
fn grade_cards(session: &[&Card], root: &std::path::Path, appended: &mut usize) -> std::io::Result<()> {
    let (mut input, mut err) = (std::io::stdin().lock(), std::io::stderr());
    for (i, card) in session.iter().enumerate() {
        let progress = format!("{}/{}", i + 1, session.len());
        let Some(grade) = ask(card, &progress, &mut input, &mut err)? else { return Ok(()) };
        let id = card.id.valid().expect("only cards with valid ids are scheduled");
        let line = Line { t: Timestamp::now(), subject: Subject::Card(id), grade };
        reviewlog::append(root, &line)?;
        *appended += 1;
    }
    Ok(())
}

/// Shows one card and reads the user's grade. `None` when the user stops (`q` or end
/// of input).
fn ask(card: &Card, progress: &str, input: &mut impl BufRead, err: &mut impl Write) -> std::io::Result<Option<Grade>> {
    writeln!(err, "\n[{progress}] {}\n\n{}\n\n(press Enter to show the answer)", card.path, card.question)?;
    let mut buf = String::new();
    if input.read_line(&mut buf)? == 0 {
        return Ok(None);
    }
    writeln!(err, "{}\n\ngrade: again/hard/good/easy (1-4), q to stop", card.answer)?;
    loop {
        buf.clear();
        if input.read_line(&mut buf)? == 0 || buf.trim() == "q" {
            return Ok(None);
        }
        match Grade::parse(&buf) {
            Some(g) => return Ok(Some(g)),
            None => writeln!(err, "type again, hard, good, easy, 1-4 or q")?,
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_compiled_skills_have_descriptions() {
        let report = super::skills().unwrap();
        assert!(report.human.lines().all(|l| l.split_once(": ").is_some_and(|(_, d)| !d.is_empty())));
    }
}
