//! The review log `plugins/learn/reviews.jsonl` (learn-plugin §2.4, §3).

use std::io::{Read, Seek, SeekFrom, Write};

use jiff::Timestamp;
use loci_core::envelope::Refusal;
use loci_core::id::Ulid;
use loci_core::path::VaultPath;
use loci_core::vault::Vault;
use serde::{Deserialize, Serialize};

const DIR: &str = "plugins/learn";
const LOG: &str = "plugins/learn/reviews.jsonl";
const ATTRS: &str = "plugins/learn/.gitattributes";
const ATTRS_LINE: &str = "reviews.jsonl merge=union\n";

/// A grade; its discriminant is the FSRS rating, 1–4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Grade {
    Again = 1,
    Hard,
    Good,
    Easy,
}

impl Grade {
    /// The FSRS rating: 1–4.
    pub fn rating(self) -> u32 {
        self as u32
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

/// A grade as typed in a review: its name or its rating.
impl std::str::FromStr for Grade {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        match s.trim() {
            "again" | "1" => Ok(Grade::Again),
            "hard" | "2" => Ok(Grade::Hard),
            "good" | "3" => Ok(Grade::Good),
            "easy" | "4" => Ok(Grade::Easy),
            other => Err(format!("`{other}` is no grade")),
        }
    }
}

/// What a line grades: a card, or a question on a concept that has no card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Subject {
    Card(Ulid),
    Concept(Ulid),
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
    card: Option<Ulid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    concept: Option<Ulid>,
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
    pub card: Option<Ulid>,
    pub concept: Option<Ulid>,
    pub grade: Grade,
}

impl Input {
    /// Validates the input's shape (§2.4) and builds the line. Existence of the card or
    /// concept is checked by the caller.
    pub fn into_line(self, t: Timestamp) -> Result<Line, String> {
        Line::try_from(RawLine { t, card: self.card, concept: self.concept, grade: self.grade })
    }
}

/// The review log of a vault.
#[derive(Debug, Clone, Copy)]
pub struct ReviewLog<'a>(&'a Vault);

impl<'a> ReviewLog<'a> {
    /// The log of `vault`. Refuses a symlink on the way to the log, its `.gitattributes`
    /// or the config (`file.symlink`): loci never reads or writes through one (m0 §3),
    /// and an ignored one escaped the vault's walk.
    pub fn open(vault: &'a Vault) -> Result<Self, Refusal> {
        [LOG, ATTRS, crate::config::PATH].into_iter().try_for_each(|p| vault.refuse_symlink(&VaultPath::fixed(p)))?;
        Ok(Self(vault))
    }

    /// Reads every line of the log, sorted by timestamp (stable for equal times). A line
    /// that does not parse is `learn.log`.
    pub fn read(&self) -> Result<Vec<Line>, Refusal> {
        let path = self.0.abs(&VaultPath::fixed(LOG));
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(Refusal::io_at(&path)(e)),
        };
        let mut lines: Vec<Line> = text
            .lines()
            .enumerate()
            .filter(|(_, l)| !l.trim().is_empty())
            .map(|(n, l)| {
                serde_json::from_str(l).map_err(|e| Refusal::refused("learn.log", format!("{LOG} line {}: {e}", n + 1)))
            })
            .collect::<Result<_, _>>()?;
        lines.sort_by_key(|l| l.t);
        Ok(lines)
    }

    /// Appends one line, creating `plugins/learn/` and its `.gitattributes` first if needed.
    /// Returns the file length before the append, for rollback. An error names the file.
    pub fn append(&self, line: &Line) -> Result<u64, String> {
        let dir = self.0.abs(&VaultPath::fixed(DIR));
        std::fs::create_dir_all(&dir).map_err(at(&dir))?;
        let attrs = self.0.abs(&VaultPath::fixed(ATTRS));
        if !attrs.exists() {
            std::fs::write(&attrs, ATTRS_LINE).map_err(at(&attrs))?;
        }
        let log = self.0.abs(&VaultPath::fixed(LOG));
        let mut f = std::fs::OpenOptions::new().create(true).read(true).append(true).open(&log).map_err(at(&log))?;
        let before = f.metadata().map_err(at(&log))?.len();
        // A last line without its line break (an edit by hand) gets one, so the new line
        // does not join it.
        let mut last = *b"\n";
        if before > 0 {
            f.seek(SeekFrom::End(-1)).and_then(|_| f.read_exact(&mut last)).map_err(at(&log))?;
        }
        let lead = if last[0] == b'\n' { "" } else { "\n" };
        let json = serde_json::to_string(line).expect("a line always serialises");
        // One write, undone on failure, so a partial line never stays in the log.
        if let Err(e) = f.write_all(format!("{lead}{json}\n").as_bytes()) {
            f.set_len(before).map_err(at(&log))?;
            return Err(at(&log)(e));
        }
        Ok(before)
    }

    /// Truncates the log back to `len` bytes (undoing an append). An error names the file.
    pub fn truncate(&self, len: u64) -> Result<(), String> {
        let log = self.0.abs(&VaultPath::fixed(LOG));
        std::fs::OpenOptions::new().write(true).open(&log).and_then(|f| f.set_len(len)).map_err(at(&log))
    }

    /// Commits the log and its `.gitattributes` alone, as the user (§2.4). Returns the commit.
    pub fn commit(&self, command: &str) -> Result<String, String> {
        self.0.repo().commit_paths(&format!("loci: learn {command}"), &[VaultPath::fixed(LOG), VaultPath::fixed(ATTRS)])
    }
}

/// An I/O error at `path`, as a message naming it.
fn at(path: &std::path::Path) -> impl Fn(std::io::Error) -> String + '_ {
    move |e| format!("{}: {e}", path.display())
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
        assert!(input(r#"{"card":"01JB9XQ7Z3K4M5N6P7Q8R9S0TV","grade":"good"}"#).is_ok());
        assert!(input(r#"{"concept":"01JBA2K7N4P8R3T6W9X2Z5C8DF","grade":"hard"}"#).is_ok());
        for bad in [
            r#"{"card":"01JB9XQ7Z3K4M5N6P7Q8R9S0TV","grade":"great"}"#,
            r#"{"card":"01JB9XQ7Z3K4M5N6P7Q8R9S0TV","concept":"01JBA2K7N4P8R3T6W9X2Z5C8DF","grade":"good"}"#,
            r#"{"grade":"good"}"#,
            r#"{"card":"01JB9XQ7Z3K4M5N6P7Q8R9S0TV"}"#,
            r#"{"card":"01JB9XQ7Z3K4M5N6P7Q8R9S0TV","grade":"good","note":"x"}"#,
            r#"{"card":"X","grade":"good"}"#,
        ] {
            assert!(input(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_stored_line_needs_exactly_one_subject() {
        for bad in [
            r#"{"t":"1970-01-01T00:00:00Z","card":"01JB9XQ7Z3K4M5N6P7Q8R9S0TV","concept":"01JBA2K7N4P8R3T6W9X2Z5C8DF","grade":"good"}"#,
            r#"{"t":"1970-01-01T00:00:00Z","grade":"good"}"#,
            r#"{"t":"1970-01-01T00:00:00Z","card":"X","grade":"good"}"#,
        ] {
            assert!(serde_json::from_str::<Line>(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn line_round_trip() {
        let l = input(r#"{"card":"01JB9XQ7Z3K4M5N6P7Q8R9S0TV","grade":"easy"}"#).unwrap();
        let s = serde_json::to_string(&l).unwrap();
        assert_eq!(s, r#"{"t":"1970-01-01T00:00:00Z","card":"01JB9XQ7Z3K4M5N6P7Q8R9S0TV","grade":"easy"}"#);
        assert_eq!(serde_json::from_str::<Line>(&s).unwrap(), l);
    }
}
