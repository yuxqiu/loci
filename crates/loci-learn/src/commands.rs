//! The plugin's commands (learn-plugin §2).

use std::collections::BTreeMap;
use std::io::{BufRead, Read, Write};

use jiff::Timestamp;
use loci_core::envelope::{Diagnostic, Refusal, Report};
use loci_core::id::Ulid;
use loci_core::index::Index;
use loci_core::parse::{Note, Spanned};
use loci_core::path::Kind;
use loci_core::plugin::Skill;
use loci_core::vault::Vault;
use serde_json::json;

use crate::cards::{self, Card, CardId};
use crate::config::Config;
use crate::reviewlog::{Grade, Input, Line, ReviewLog, Subject};
use crate::schedule::{Estimate, Scheduler};

const SKILL_FILES: &[&str] = &[include_str!("../skills/learn-session/SKILL.md")];

pub fn skills() -> Result<Report, Refusal> {
    let skills = Skill::compiled(SKILL_FILES);
    let human: String = skills.iter().map(|s| format!("{s}\n")).collect();
    Ok(Report::ok(json!({ "skills": skills }), human))
}

pub fn lint(vault: &Vault) -> Result<Report, Refusal> {
    let index = Index::open(vault);
    let mut diags = vec![];
    let mut count = 0;
    let mut ids: Vec<(Ulid, (&Note, std::ops::Range<usize>))> = vec![];
    for note in index.concepts() {
        let (cards, issues) = cards::of_note(note);
        for span in issues {
            let msg = "card is not `- question :: answer`, optionally followed by a block id";
            diags.push(Diagnostic::error("learn.card-format", msg).in_note(note, span));
        }

        count += cards.len();
        let assign_ids = format!("loci block-ids {} --section {}", note.path, cards::SECTION);
        for c in cards {
            match &c.id {
                CardId::Missing => diags.push(
                    Diagnostic::error("learn.no-id", "card has no block id")
                        .in_note(note, c.span.clone())
                        .with_hint(assign_ids.clone()),
                ),
                CardId::Malformed(id) => diags.push(
                    Diagnostic::error("learn.id-form", format!("card id `^{id}` is not a ULID"))
                        .in_note(note, c.span.clone())
                        .with_hint(format!("delete the id and run `{assign_ids}`")),
                ),
                CardId::Valid(id) => ids.push((*id, (note, c.span.clone()))),
            }
        }
    }

    for (id, cs) in shared_ids(ids) {
        let others: Vec<&str> = cs[1..].iter().map(|(n, _)| n.path.as_str()).collect();
        let msg = format!("card id `^{id}` is used by {}", others.join(", "));
        diags.push(Diagnostic::error("learn.id-duplicate", msg).in_note(cs[0].0, cs[0].1.clone()));
    }

    let errors = diags.len();
    Ok(Report::ok(serde_json::Value::Null, format!("{count} cards, {errors} errors\n")).with_diagnostics(diags))
}

/// The ids given to more than one card, each with what `items` holds for those cards
/// (card ids are vault-unique, §1.2).
fn shared_ids<T>(items: impl IntoIterator<Item = (Ulid, T)>) -> impl Iterator<Item = (Ulid, Vec<T>)> {
    let mut by_id: BTreeMap<Ulid, Vec<T>> = BTreeMap::new();
    for (id, item) in items {
        by_id.entry(id).or_default().push(item);
    }
    by_id.into_iter().filter(|(_, items)| items.len() > 1)
}

/// Refuses when two cards share an id (`learn.id-duplicate`): a grade could not say
/// which card it is for.
fn refuse_shared_ids(all: &[Spanned<Card>]) -> Result<(), Refusal> {
    match shared_ids(all.iter().filter_map(|c| Some((c.id.valid()?, c.path.as_str())))).next() {
        Some((id, paths)) => Err(Refusal::refused(
            "learn.id-duplicate",
            format!("card id `^{id}` is used by {}; `loci learn lint` lists every such id", paths.join(", ")),
        )),
        None => Ok(()),
    }
}

/// Validates a line against the working tree: its card or concept must exist.
fn check_exists(line: &Line, all: &[Spanned<Card>], index: &Index) -> Result<(), Refusal> {
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
    let log = ReviewLog::open(vault)?;
    let mut input = String::new();
    stdin.read_to_string(&mut input).map_err(|e| Refusal::io(format!("reading stdin: {e}")))?;
    let bad = |e: String| Refusal::refused("learn.bad-input", e);
    let parsed: Input = serde_json::from_str(&input).map_err(|e| bad(e.to_string()))?;
    let line = parsed.into_line(Timestamp::now()).map_err(bad)?;
    let index = Index::open(vault);
    let all = cards::all(&index);
    refuse_shared_ids(&all)?;
    check_exists(&line, &all, &index)?;
    let before = log.append(&line)?;
    match log.commit("log") {
        Ok(sha) => Ok(Report::ok(json!({"line": line, "commit": sha}), format!("logged {sha}\n"))),
        Err(e) => Err(match log.truncate(before) {
            Ok(()) => Refusal::git(e),
            Err(t) => Refusal::git(format!("{e}; removing the uncommitted line also failed: {}", t.error.message)),
        }),
    }
}

pub fn state(vault: &Vault, arg: &str) -> Result<Report, Refusal> {
    let log = ReviewLog::open(vault)?;
    let index = Index::open(vault);
    let note = index.note_named(arg)?.of_kind(|k| k == Kind::Concept, "a concept")?;
    let Some(concept) = note.id() else {
        return Err(Refusal::refused("learn.unknown-concept", format!("`{}` has no valid id", note.path)));
    };
    let config = Config::load(vault)?;
    let lines = log.read()?;
    let (note_cards, _) = cards::of_note(note);
    let (estimate, used) =
        Scheduler::new(config.desired_retention).estimate(&note_cards, concept, &lines, Timestamp::now());
    let human = match &estimate {
        Estimate::Cards { r } => format!("{arg}: recall {:.0}% (from {} line(s))\n", r * 100.0, used.len()),
        Estimate::Item { grade, age_days } => format!("{arg}: last quiz {} {age_days:.1} days ago\n", grade.outcome()),
        Estimate::Unknown => format!("{arg}: unknown\n"),
    };
    Ok(Report::ok(json!({"concept": concept, "estimate": estimate, "lines": used}), human))
}

pub fn review(vault: &Vault) -> Result<Report, Refusal> {
    let log = ReviewLog::open(vault)?;
    let config = Config::load(vault)?;
    let lines = log.read()?;
    let index = Index::open(vault);
    let all = cards::all(&index);
    refuse_shared_ids(&all)?;
    let scheduler = Scheduler::new(config.desired_retention);
    let (session, new_count) = scheduler.select(&all, &lines, config.new_per_day, Timestamp::now());

    // Each grade is appended at once; the lines are committed at the end (§2.6 step 6).
    let (reviewed, stopped) = grade_cards(&session, log);

    // Lines already appended are committed even when the session stopped on an error.
    let uncommitted = |e| Refusal::git(format!("{e}; the {reviewed} grade(s) stay in the log, uncommitted"));
    let commit = if reviewed > 0 { Some(log.commit("review").map_err(uncommitted)?) } else { None };
    if let Err(e) = stopped {
        let committed = commit.map_or(String::new(), |sha| format!(" ({reviewed} grades committed in {sha})"));
        return Err(Refusal::io(format!("the session stopped: {}{committed}", e.error.message)));
    }

    let left = session.len() - reviewed;
    let data = json!({"reviewed": reviewed, "new": new_count, "due_left": left, "commit": commit});
    Ok(Report::ok(data, format!("reviewed {reviewed}, {left} left\n")))
}

/// Asks for a grade on each card of `session` and appends it to the log. Returns how
/// many it appended, and how it stopped: at the end, on `q` or end of input (`Ok`), or
/// on an I/O error.
fn grade_cards(session: &[(Ulid, &Spanned<Card>)], log: ReviewLog) -> (usize, Result<(), Refusal>) {
    let (mut input, mut err) = (std::io::stdin().lock(), std::io::stderr());
    let mut appended = 0;
    for (i, &(id, card)) in session.iter().enumerate() {
        let progress = format!("{}/{}", i + 1, session.len());
        let grade = match ask(card, &progress, &mut input, &mut err) {
            Ok(Some(grade)) => grade,
            Ok(None) => break,
            Err(e) => return (appended, Err(Refusal::io(format!("the terminal: {e}")))),
        };
        let line = Line { t: Timestamp::now(), subject: Subject::Card(id), grade };
        if let Err(e) = log.append(&line) {
            return (appended, Err(e));
        }
        appended += 1;
    }
    (appended, Ok(()))
}

/// Shows one card and reads the user's grade. `None` when the user stops (`q` or end
/// of input).
fn ask(
    card: &Spanned<Card>,
    progress: &str,
    input: &mut impl BufRead,
    err: &mut impl Write,
) -> std::io::Result<Option<Grade>> {
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
        match buf.parse::<Grade>() {
            Ok(g) => return Ok(Some(g)),
            Err(_) => writeln!(err, "type again, hard, good, easy, 1-4 or q")?,
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
