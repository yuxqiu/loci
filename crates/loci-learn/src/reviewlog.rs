//! The review log `plugins/learn/reviews.jsonl` (learn-plugin §2.4, §3).

use std::io::Write;
use std::path::Path;

use jiff::Timestamp;
use loci_core::envelope::Refusal;
use loci_core::git;
use serde::{Deserialize, Serialize};

pub const DIR: &str = "plugins/learn";
pub const LOG: &str = "plugins/learn/reviews.jsonl";
pub const ATTRS: &str = "plugins/learn/.gitattributes";
const ATTRS_LINE: &str = "reviews.jsonl merge=union\n";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Grade {
    Again,
    Hard,
    Good,
    Easy,
}

impl Grade {
    /// The FSRS rating: 1–4.
    pub fn rating(self) -> u32 {
        match self {
            Grade::Again => 1,
            Grade::Hard => 2,
            Grade::Good => 3,
            Grade::Easy => 4,
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "again" | "1" => Some(Grade::Again),
            "hard" | "2" => Some(Grade::Hard),
            "good" | "3" => Some(Grade::Good),
            "easy" | "4" => Some(Grade::Easy),
            _ => None,
        }
    }

    /// Reads a grade as fail / partial / pass (§4.2).
    pub fn outcome(self) -> &'static str {
        match self {
            Grade::Again => "fail",
            Grade::Hard => "partial",
            Grade::Good | Grade::Easy => "pass",
        }
    }
}

/// What a line grades: a card, or a question on a concept that has no card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Subject {
    Card(String),
    Concept(String),
}

/// One line of the review log (§3): the `log` input plus `t`. Stored as `card` or
/// `concept`, exactly one of them, which every read checks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawLine", into = "RawLine")]
pub struct Line {
    pub t: Timestamp,
    pub subject: Subject,
    pub grade: Grade,
}

/// A line as stored.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLine {
    t: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    card: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    concept: Option<String>,
    grade: Grade,
}

impl TryFrom<RawLine> for Line {
    type Error = String;

    fn try_from(r: RawLine) -> Result<Self, String> {
        let subject = match (r.card, r.concept) {
            (Some(card), None) => Subject::Card(card),
            (None, Some(concept)) => Subject::Concept(concept),
            _ => return Err("exactly one of `card` and `concept` is required".into()),
        };
        Ok(Line { t: r.t, subject, grade: r.grade })
    }
}

impl From<Line> for RawLine {
    fn from(l: Line) -> Self {
        let (card, concept) = match l.subject {
            Subject::Card(c) => (Some(c), None),
            Subject::Concept(c) => (None, Some(c)),
        };
        RawLine { t: l.t, card, concept, grade: l.grade }
    }
}

/// The `log` input (§2.4): a graded card, or a graded question on a concept.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub card: Option<String>,
    pub concept: Option<String>,
    pub grade: Grade,
}

impl Input {
    /// Validates the input's shape (§2.4) and builds the line. Existence of the card or
    /// concept is checked by the caller.
    pub fn into_line(self, t: Timestamp) -> Result<Line, String> {
        Line::try_from(RawLine { t, card: self.card, concept: self.concept, grade: self.grade })
    }
}

/// Reads every line of the log, sorted by timestamp (stable for equal times). A line
/// that does not parse is `learn.log`.
pub fn read(vault: &Path) -> Result<Vec<Line>, Refusal> {
    let text = match std::fs::read_to_string(vault.join(LOG)) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(Refusal::io(e)),
    };
    let mut lines = vec![];
    for (n, l) in text.lines().enumerate().filter(|(_, l)| !l.trim().is_empty()) {
        let line: Line =
            serde_json::from_str(l).map_err(|e| Refusal::refused("learn.log", format!("{LOG} line {}: {e}", n + 1)))?;
        lines.push(line);
    }
    lines.sort_by_key(|l| l.t);
    Ok(lines)
}

/// Appends one line, creating `plugins/learn/` and its `.gitattributes` first if needed.
/// Returns the file length before the append, for rollback.
pub fn append(vault: &Path, line: &Line) -> std::io::Result<u64> {
    std::fs::create_dir_all(vault.join(DIR))?;
    let attrs = vault.join(ATTRS);
    if !attrs.exists() {
        std::fs::write(&attrs, ATTRS_LINE)?;
    }
    let path = vault.join(LOG);
    let before = match std::fs::metadata(&path) {
        Ok(m) => m.len(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => 0,
        Err(e) => return Err(e),
    };
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&path)?;
    let json = serde_json::to_string(line).expect("a line always serialises");
    // One write, undone on failure, so a partial line never stays in the log.
    if let Err(e) = f.write_all(format!("{json}\n").as_bytes()) {
        f.set_len(before)?;
        return Err(e);
    }
    Ok(before)
}

/// Truncates the log back to `len` bytes (undoing an append).
pub fn truncate(vault: &Path, len: u64) -> std::io::Result<()> {
    std::fs::OpenOptions::new().write(true).open(vault.join(LOG))?.set_len(len)
}

/// Commits the log and its `.gitattributes` alone, as the user (§2.4). Returns the commit.
pub fn commit(vault: &Path, command: &str) -> Result<String, String> {
    git::run(vault, &["add", "--", LOG, ATTRS])?;
    let msg = format!("loci: learn {command}");
    git::run(vault, &["commit", "-q", "-m", &msg, "--", LOG, ATTRS])?;
    let sha = git::run(vault, &["rev-parse", "HEAD"])?;
    Ok(String::from_utf8_lossy(&sha).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(json: &str) -> Result<Line, String> {
        let i: Input = serde_json::from_str(json).map_err(|e| e.to_string())?;
        i.into_line(Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn validation() {
        assert!(input(r#"{"card":"X","grade":"good"}"#).is_ok());
        assert!(input(r#"{"concept":"C","grade":"hard"}"#).is_ok());
        for bad in [
            r#"{"card":"X","grade":"great"}"#,
            r#"{"card":"X","concept":"C","grade":"good"}"#,
            r#"{"grade":"good"}"#,
            r#"{"card":"X"}"#,
            r#"{"card":"X","grade":"good","note":"x"}"#,
        ] {
            assert!(input(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_stored_line_needs_exactly_one_subject() {
        for bad in [
            r#"{"t":"1970-01-01T00:00:00Z","card":"X","concept":"C","grade":"good"}"#,
            r#"{"t":"1970-01-01T00:00:00Z","grade":"good"}"#,
        ] {
            assert!(serde_json::from_str::<Line>(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn line_round_trip() {
        let l = input(r#"{"card":"X","grade":"easy"}"#).unwrap();
        let s = serde_json::to_string(&l).unwrap();
        assert_eq!(s, r#"{"t":"1970-01-01T00:00:00Z","card":"X","grade":"easy"}"#);
        assert_eq!(serde_json::from_str::<Line>(&s).unwrap(), l);
    }
}
