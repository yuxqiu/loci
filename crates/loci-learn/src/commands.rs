//! The plugin's commands (learn-plugin §2).

use std::collections::BTreeMap;
use std::io::{BufRead, Read, Write};
use std::sync::{Arc, Mutex};

use jiff::Timestamp;
use loci_core::envelope::{Diagnostic, Outcome, Report};
use loci_core::index::Index;
use loci_core::plugin::Skill;
use loci_core::vault::Vault;
use serde_json::json;

use crate::cards::{self, Card};
use crate::config::Config;
use crate::reviewlog::{self, Grade, Grader, Input, Line, Purpose};
use crate::schedule::{self, Estimate, Scheduler};

const SKILL_FILES: &[(&str, &str)] = &[("learn-session", include_str!("../skills/learn-session/SKILL.md"))];

pub fn skills() -> Report {
    let skills: Vec<Skill> = SKILL_FILES.iter().map(|(n, b)| Skill::from_markdown(n, b)).collect();
    let human: String = skills.iter().map(|s| format!("{}: {}\n", s.name, s.description)).collect();
    Report::new(Outcome::ok(json!({ "skills": skills })), human)
}

pub fn lint(vault: &Vault) -> Report {
    let index = Index::open(vault);
    let (all, issues) = cards::all(&index);
    let text_of = |path: &str| index.notes.get(path).map(|n| n.text.as_str()).unwrap_or("");
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
    let mut by_id: BTreeMap<&str, Vec<&Card>> = BTreeMap::new();
    for c in &all {
        let at = |code: &str, msg: String| {
            Diagnostic::error(code, msg).at_path(c.path.as_str()).at(text_of(&c.path), c.range.clone())
        };
        match &c.id {
            None => diags.push(
                at("learn.no-id", "card has no block id".into())
                    .with_hint(format!("loci block-ids {} --section Cards", c.path)),
            ),
            Some(id) if !loci_core::vault::is_ulid(id) => diags.push(
                at("learn.id-form", format!("card id `^{id}` is not a ULID"))
                    .with_hint(format!("delete the id and run `loci block-ids {} --section Cards`", c.path)),
            ),
            Some(id) => by_id.entry(id.as_str()).or_default().push(c),
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
    Report::new(Outcome::lint(serde_json::Value::Null, diags), format!("{} cards, {errors} errors\n", all.len()))
}

/// Validates a line against the working tree: its card or concept must exist.
fn check_exists(line: &Line, all: &[Card], index: &Index) -> Result<(), Outcome> {
    if let Some(card) = &line.card
        && !all.iter().any(|c| c.id.as_deref() == Some(card))
    {
        return Err(Outcome::refused("learn.unknown-card", format!("no card has id `{card}`")));
    }
    if let Some(concept) = &line.concept {
        let known = index.concepts().any(|n| n.fm_str("id") == Some(concept));
        if !known {
            return Err(Outcome::refused("learn.unknown-concept", format!("no concept has id `{concept}`")));
        }
    }
    Ok(())
}

pub fn log(vault: &Vault, mut stdin: impl Read) -> Report {
    let mut input = String::new();
    if let Err(e) = stdin.read_to_string(&mut input) {
        return Report::fail(Outcome::internal("io", e.to_string()));
    }
    let parsed: Input = match serde_json::from_str(&input) {
        Ok(i) => i,
        Err(e) => return Report::fail(Outcome::refused("learn.bad-input", e.to_string())),
    };
    let line = match parsed.into_line(Timestamp::now()) {
        Ok(l) => l,
        Err(e) => return Report::fail(Outcome::refused("learn.bad-input", e)),
    };
    let index = Index::open(vault);
    let (all, _) = cards::all(&index);
    if let Err(o) = check_exists(&line, &all, &index) {
        return Report::fail(o);
    }
    let before = match reviewlog::append(&vault.root, &line) {
        Ok(b) => b,
        Err(e) => return Report::fail(Outcome::internal("io", e.to_string())),
    };
    match reviewlog::commit(&vault.root, "log") {
        Ok(sha) => Report::new(Outcome::ok(json!({"line": line, "commit": sha})), format!("logged {sha}\n")),
        Err(e) => match reviewlog::truncate(&vault.root, before) {
            Ok(()) => Report::fail(Outcome::internal("git", e)),
            Err(t) => {
                Report::fail(Outcome::internal("git", format!("{e}; removing the uncommitted line also failed: {t}")))
            }
        },
    }
}

pub fn state(vault: &Vault, slug: &str) -> Report {
    let index = Index::open(vault);
    let note = match index.concept(slug) {
        Ok(n) => n,
        Err(e) => {
            let (code, message) = e.refusal("learn.unknown-concept", slug);
            return Report::fail(Outcome::refused(code, message));
        }
    };
    let Some(concept) = note.fm_str("id").map(str::to_string) else {
        return Report::fail(Outcome::refused("learn.unknown-concept", format!("`{slug}` has no valid id")));
    };
    let config = match Config::load(&vault.root) {
        Ok(c) => c,
        Err(e) => return Report::fail(Outcome::refused("learn.config", e)),
    };
    let lines = match reviewlog::read(&vault.root) {
        Ok(l) => l,
        Err(e) => return Report::fail(Outcome::internal("learn.log", e)),
    };
    let (note_cards, _) = cards::of_note(note);
    let (estimate, used) =
        estimate(&note_cards, &concept, &lines, &Scheduler::new(config.desired_retention), Timestamp::now());
    let est_json = match &estimate {
        Estimate::Cards { r } => json!({"kind": "cards", "r": r}),
        Estimate::Item { grade, age_days } => json!({"kind": "item", "grade": grade, "age_days": age_days}),
        Estimate::Unknown => json!({"kind": "unknown"}),
    };
    let human = match &estimate {
        Estimate::Cards { r } => format!("{slug}: recall {:.0}% (from {} line(s))\n", r * 100.0, used.len()),
        Estimate::Item { grade, age_days } => format!("{slug}: last quiz {} {age_days:.1} days ago\n", grade.outcome()),
        Estimate::Unknown => format!("{slug}: unknown\n"),
    };
    Report::new(Outcome::ok(json!({"concept": concept, "estimate": est_json, "lines": used})), human)
}

/// A concept's estimate and the lines it was computed from (§4.2).
fn estimate<'a>(
    note_cards: &[Card],
    concept: &str,
    lines: &'a [Line],
    scheduler: &Scheduler,
    now: Timestamp,
) -> (Estimate, Vec<&'a Line>) {
    let by_card = schedule::lines_by_card(lines);
    let mut rs = vec![];
    let mut used: Vec<&Line> = vec![];
    for c in note_cards {
        let Some(card_lines) = c.id.as_deref().and_then(|id| by_card.get(id)) else { continue };
        if let Some(state) = scheduler.state(card_lines) {
            rs.push(schedule::retrievability(&state, now));
            used.extend(card_lines.iter().copied());
        }
    }
    if !rs.is_empty() {
        used.sort_by_key(|l| l.t);
        let r = rs.iter().sum::<f32>() / rs.len() as f32;
        return (Estimate::Cards { r }, used);
    }
    let items: Vec<&Line> = lines.iter().filter(|l| l.concept.as_deref() == Some(concept)).collect();
    match items.last() {
        Some(last) => {
            let age_days = (now.as_second() - last.t.as_second()).max(0) as f64 / 86_400.0;
            (Estimate::Item { grade: last.grade, age_days }, items)
        }
        None => (Estimate::Unknown, vec![]),
    }
}

/// Cards for a session (§2.6 step 3): due cards by due time, then new cards.
fn select<'a>(
    all: &'a [Card],
    lines: &[Line],
    scheduler: &Scheduler,
    config: &Config,
    now: Timestamp,
) -> (Vec<&'a Card>, usize) {
    let by_card = schedule::lines_by_card(lines);
    let mut due: Vec<(Timestamp, &Card)> = vec![];
    let mut fresh: Vec<&Card> = vec![];
    for c in all {
        let Some(id) = c.id.as_deref() else { continue };
        match by_card.get(id).and_then(|ls| scheduler.state(ls)) {
            Some(state) if state.due <= now => due.push((state.due, c)),
            Some(_) => {}
            None => fresh.push(c),
        }
    }
    due.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.id.cmp(&b.1.id)));
    fresh.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()).then(a.position.cmp(&b.position)));
    let today = now.to_zoned(jiff::tz::TimeZone::UTC).date();
    let introduced_today = by_card
        .values()
        .filter(|ls| ls.first().is_some_and(|l| l.t.to_zoned(jiff::tz::TimeZone::UTC).date() == today))
        .count();
    let allowance = (config.new_per_day as usize).saturating_sub(introduced_today);
    let new_count = fresh.len().min(allowance);
    let mut out: Vec<&Card> = due.into_iter().map(|(_, c)| c).collect();
    out.extend(fresh.into_iter().take(new_count));
    (out, new_count)
}

pub fn review(vault: &Vault) -> Report {
    let config = match Config::load(&vault.root) {
        Ok(c) => c,
        Err(e) => return Report::fail(Outcome::refused("learn.config", e)),
    };
    let lines = match reviewlog::read(&vault.root) {
        Ok(l) => l,
        Err(e) => return Report::fail(Outcome::internal("learn.log", e)),
    };
    let index = Index::open(vault);
    let (all, _) = cards::all(&index);
    let scheduler = Scheduler::new(config.desired_retention);
    let (session, new_count) = select(&all, &lines, &scheduler, &config, Timestamp::now());

    // Lines appended so far; committed at the end or on interrupt (§2.6 step 6).
    let appended = Arc::new(Mutex::new(0usize));
    let root = vault.root.clone();
    let on_interrupt = Arc::clone(&appended);
    let handler = ctrlc::set_handler(move || {
        let n = *on_interrupt.lock().expect("the counter lock is never poisoned");
        if n == 0 {
            std::process::exit(0);
        }
        match reviewlog::commit(&root, "review") {
            Ok(_) => std::process::exit(0),
            Err(e) => {
                eprintln!("loci learn review: committing the review log failed: {e}");
                std::process::exit(3);
            }
        }
    });
    if let Err(e) = handler {
        return Report::fail(Outcome::internal("io", format!("cannot install the interrupt handler: {e}")));
    }

    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let mut err = std::io::stderr();
    let mut reviewed = 0;
    // The session stops at the end, on `q` or end of input (`Ok`), or on an I/O error.
    let stopped: std::io::Result<()> = (|| {
        for (i, card) in session.iter().enumerate() {
            writeln!(
                err,
                "\n[{}/{}] {}\n\n{}\n\n(press Enter to show the answer)",
                i + 1,
                session.len(),
                card.path,
                card.question
            )?;
            let mut buf = String::new();
            if input.read_line(&mut buf)? == 0 {
                return Ok(());
            }
            writeln!(err, "{}\n\ngrade: again/hard/good/easy (1-4), q to stop", card.answer)?;
            let grade = loop {
                buf.clear();
                if input.read_line(&mut buf)? == 0 || buf.trim() == "q" {
                    return Ok(());
                }
                match Grade::parse(&buf) {
                    Some(g) => break g,
                    None => writeln!(err, "type again, hard, good, easy, 1-4 or q")?,
                }
            };
            let line = Line {
                t: Timestamp::now(),
                card: card.id.clone(),
                concept: None,
                item: None,
                purpose: Purpose::Review,
                grader: Grader::Myself,
                grade,
                verdicts: None,
            };
            reviewlog::append(&vault.root, &line)?;
            *appended.lock().expect("the counter lock is never poisoned") += 1;
            reviewed += 1;
        }
        Ok(())
    })();
    // Lines already appended are committed even when the session stopped on an error.
    let commit = if reviewed > 0 {
        match reviewlog::commit(&vault.root, "review") {
            Ok(sha) => Some(sha),
            Err(e) => return Report::fail(Outcome::internal("git", e)),
        }
    } else {
        None
    };
    if let Err(e) = stopped {
        let committed = commit.map_or(String::new(), |sha| format!(" ({reviewed} grades committed in {sha})"));
        return Report::fail(Outcome::internal("io", format!("the session stopped: {e}{committed}")));
    }
    let due_left = session.len() - reviewed;
    let data = json!({"reviewed": reviewed, "new": new_count, "due_left": due_left, "commit": commit});
    Report::new(Outcome::ok(data), format!("reviewed {reviewed}, {due_left} left\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(path: &str, pos: usize, id: &str) -> Card {
        Card {
            path: path.into(),
            range: 0..0,
            position: pos,
            id: Some(id.into()),
            question: "q".into(),
            answer: "a".into(),
        }
    }

    fn line(card: &str, t: &str, grade: Grade) -> Line {
        Line {
            t: t.parse().unwrap(),
            card: Some(card.into()),
            concept: None,
            item: None,
            purpose: Purpose::Review,
            grader: Grader::Myself,
            grade,
            verdicts: None,
        }
    }

    #[test]
    fn selection_order_and_new_limit() {
        let all = vec![
            card("concepts/a/z.md", 0, "N1"),
            card("concepts/a/b.md", 1, "N2"),
            card("concepts/a/b.md", 0, "N3"),
            card("concepts/a/x.md", 0, "D1"),
        ];
        let lines =
            vec![line("D1", "2026-01-01T00:00:00Z", Grade::Again), line("GONE", "2026-01-01T00:00:00Z", Grade::Good)];
        let s = Scheduler::new(0.9);
        let config = Config { new_per_day: 2, desired_retention: 0.9 };
        let (sel, n) = select(&all, &lines, &s, &config, "2026-10-30T00:00:00Z".parse().unwrap());
        let ids: Vec<&str> = sel.iter().map(|c| c.id.as_deref().unwrap()).collect();
        assert_eq!(ids, vec!["D1", "N3", "N2"]);
        assert_eq!(n, 2);
    }

    #[test]
    fn estimates() {
        let s = Scheduler::new(0.9);
        let now: Timestamp = "2026-10-30T00:00:00Z".parse().unwrap();
        let cs = vec![card("concepts/a/x.md", 0, "C1"), card("concepts/a/x.md", 1, "C2")];
        let lines = vec![line("C1", "2026-10-29T00:00:00Z", Grade::Good)];
        let (e, used) = estimate(&cs, "K", &lines, &s, now);
        assert!(matches!(e, Estimate::Cards { r } if r > 0.5 && r <= 1.0));
        assert_eq!(used.len(), 1);
        let quiz = vec![Line {
            card: None,
            concept: Some("K".into()),
            item: Some("q".into()),
            purpose: Purpose::Diagnostic,
            ..line("x", "2026-10-28T00:00:00Z", Grade::Hard)
        }];
        let (e, _) = estimate(&[], "K", &quiz, &s, now);
        assert!(matches!(e, Estimate::Item { grade: Grade::Hard, age_days } if (age_days - 2.0).abs() < 1e-9));
        assert_eq!(estimate(&[], "K", &[], &s, now).0, Estimate::Unknown);
    }
}
