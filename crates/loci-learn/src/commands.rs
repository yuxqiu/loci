//! The plugin's commands (learn-plugin §2).

use std::collections::BTreeMap;
use std::io::{BufRead, Read, Write};
use std::sync::{Arc, Mutex};

use jiff::Timestamp;
use loci_core::envelope::{Diagnostic, Outcome, Refusal, Report};
use loci_core::index::Index;
use loci_core::plugin::Skill;
use loci_core::vault::Vault;
use serde_json::json;

use crate::cards::{self, Card};
use crate::config::Config;
use crate::reviewlog::{self, Grade, Input, Line, Subject};
use crate::schedule::{self, Estimate, Scheduler};

const SKILL_FILES: &[(&str, &str)] = &[("learn-session", include_str!("../skills/learn-session/SKILL.md"))];

pub fn skills() -> Result<Report, Refusal> {
    let skills: Vec<Skill> = SKILL_FILES.iter().map(|(n, b)| Skill::from_markdown(n, b)).collect();
    let human: String = skills.iter().map(|s| format!("{}: {}\n", s.name, s.description)).collect();
    Ok(Report::new(Outcome::ok(json!({ "skills": skills })), human))
}

pub fn lint(vault: &Vault) -> Result<Report, Refusal> {
    let index = Index::open(vault);
    let (all, issues) = cards::all(&index);
    let text_of = |path: &str| index.markdown.get(path).map(|n| n.text.as_str()).expect("cards come from parsed notes");
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
    Ok(Report::new(Outcome::lint(serde_json::Value::Null, diags), format!("{} cards, {errors} errors\n", all.len())))
}

/// Validates a line against the working tree: its card or concept must exist.
fn check_exists(line: &Line, all: &[Card], index: &Index) -> Result<(), Refusal> {
    match &line.subject {
        Subject::Card(card) if !all.iter().any(|c| c.id.as_deref() == Some(card)) => {
            Err(Refusal::refused("learn.unknown-card", format!("no card has id `{card}`")))
        }
        Subject::Concept(concept) if !index.concepts().any(|n| n.fm_str("id") == Some(concept)) => {
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
        Ok(sha) => Ok(Report::new(Outcome::ok(json!({"line": line, "commit": sha})), format!("logged {sha}\n"))),
        Err(e) => Err(match reviewlog::truncate(&vault.root, before) {
            Ok(()) => Refusal::git(e),
            Err(t) => Refusal::git(format!("{e}; removing the uncommitted line also failed: {t}")),
        }),
    }
}

pub fn state(vault: &Vault, slug: &str) -> Result<Report, Refusal> {
    let index = Index::open(vault);
    let slug = loci_core::vault::path_arg(slug)?;
    let note = index.concept(&slug).map_err(|e| e.refusal("learn.unknown-concept", &slug))?;
    let Some(concept) = note.fm_str("id").map(str::to_string) else {
        return Err(Refusal::refused("learn.unknown-concept", format!("`{slug}` has no valid id")));
    };
    let config = Config::load(&vault.root)?;
    let lines = reviewlog::read(&vault.root)?;
    let (note_cards, _) = cards::of_note(note);
    let (estimate, used) =
        schedule::estimate(&note_cards, &concept, &lines, &Scheduler::new(config.desired_retention), Timestamp::now());
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
    Ok(Report::new(Outcome::ok(json!({"concept": concept, "estimate": est_json, "lines": used})), human))
}

pub fn review(vault: &Vault) -> Result<Report, Refusal> {
    let config = Config::load(&vault.root)?;
    let lines = reviewlog::read(&vault.root)?;
    let index = Index::open(vault);
    let (all, _) = cards::all(&index);
    let scheduler = Scheduler::new(config.desired_retention);
    let (session, new_count) = schedule::select(&all, &lines, &scheduler, config.new_per_day, Timestamp::now());

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
    handler.map_err(|e| Refusal::io(format!("cannot install the interrupt handler: {e}")))?;

    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let mut err = std::io::stderr();
    // The session stops at the end, on `q` or end of input (`Ok`), or on an I/O error.
    let stopped: std::io::Result<()> = (|| {
        for (i, card) in session.iter().enumerate() {
            let progress = format!("{}/{}", i + 1, session.len());
            let Some(grade) = ask(card, &progress, &mut input, &mut err)? else { return Ok(()) };
            let id = card.id.clone().expect("only cards with ids are scheduled");
            let line = Line { t: Timestamp::now(), subject: Subject::Card(id), grade };
            // The interrupt handler takes this lock, so it waits for a line being written
            // and always sees whole lines.
            let mut n = appended.lock().expect("the counter lock is never poisoned");
            reviewlog::append(&vault.root, &line)?;
            *n += 1;
        }
        Ok(())
    })();
    let mut pending = appended.lock().expect("the counter lock is never poisoned");
    let reviewed = *pending;
    // Lines already appended are committed even when the session stopped on an error.
    let commit =
        if reviewed > 0 { Some(reviewlog::commit(&vault.root, "review").map_err(Refusal::git)?) } else { None };
    // Committed: a later interrupt has nothing to commit.
    *pending = 0;
    drop(pending);
    if let Err(e) = stopped {
        let committed = commit.map_or(String::new(), |sha| format!(" ({reviewed} grades committed in {sha})"));
        return Err(Refusal::io(format!("the session stopped: {e}{committed}")));
    }
    let due_left = session.len() - reviewed;
    let data = json!({"reviewed": reviewed, "new": new_count, "due_left": due_left, "commit": commit});
    Ok(Report::new(Outcome::ok(data), format!("reviewed {reviewed}, {due_left} left\n")))
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
