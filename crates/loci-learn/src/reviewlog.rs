//! The review log `plugins/learn/reviews.jsonl` (learn-plugin §2.4, §3).

use std::io::Write;
use std::path::Path;

use jiff::Timestamp;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Purpose {
    Diagnostic,
    Practice,
    Review,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Grader {
    #[serde(rename = "self")]
    Myself,
    Llm,
}

/// One line of the review log (§3): the `log` input plus `t` and, for LLM grading, the
/// derived `grade`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Line {
    pub t: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concept: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
    pub purpose: Purpose,
    pub grader: Grader,
    pub grade: Grade,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verdicts: Option<Vec<Vec<bool>>>,
}

/// The `log` input (§2.4).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub card: Option<String>,
    pub concept: Option<String>,
    pub item: Option<String>,
    pub purpose: Purpose,
    pub grader: Grader,
    pub grade: Option<Grade>,
    pub verdicts: Option<Vec<Vec<bool>>>,
}

/// Grade of one LLM grader call (§3).
fn call_grade(verdicts: &[bool]) -> Grade {
    if verdicts.iter().all(|v| *v) {
        Grade::Good
    } else if verdicts.iter().any(|v| *v) {
        Grade::Hard
    } else {
        Grade::Again
    }
}

/// The grade from one or two grader calls; disagreement gives `hard` (§3).
fn derive_grade(calls: &[Vec<bool>]) -> Grade {
    let grades: Vec<Grade> = calls.iter().map(|c| call_grade(c)).collect();
    if grades.windows(2).any(|w| w[0] != w[1]) { Grade::Hard } else { grades[0] }
}

impl Input {
    /// Validates the input's shape (§2.4) and builds the line. Existence of the card or
    /// concept is checked by the caller.
    pub fn into_line(self, t: Timestamp) -> Result<Line, String> {
        match (&self.card, &self.concept) {
            (Some(_), None) => {
                if self.item.is_some() {
                    return Err("`item` is only for quiz items (`concept`)".into());
                }
            }
            (None, Some(_)) => {
                if self.item.as_deref().is_none_or(|i| i.trim().is_empty()) {
                    return Err("a `concept` line needs a non-empty `item`".into());
                }
                if self.purpose == Purpose::Review {
                    return Err("purpose `review` needs a `card`".into());
                }
            }
            _ => return Err("exactly one of `card` and `concept` is required".into()),
        }
        let grade = match (self.grader, self.grade, &self.verdicts) {
            (Grader::Myself, Some(g), None) => g,
            (Grader::Llm, None, Some(calls)) => {
                if calls.is_empty() || calls.len() > 2 || calls.iter().any(|c| c.is_empty()) {
                    return Err("`verdicts` is one or two non-empty arrays of booleans".into());
                }
                derive_grade(calls)
            }
            (Grader::Myself, _, _) => return Err("`grader: self` needs `grade` and no `verdicts`".into()),
            (Grader::Llm, _, _) => return Err("`grader: llm` needs `verdicts` and no `grade`".into()),
        };
        Ok(Line {
            t,
            card: self.card,
            concept: self.concept,
            item: self.item,
            purpose: self.purpose,
            grader: self.grader,
            grade,
            verdicts: self.verdicts,
        })
    }
}

/// Reads every line of the log, sorted by timestamp (stable for equal times).
pub fn read(vault: &Path) -> Result<Vec<Line>, String> {
    let text = match std::fs::read_to_string(vault.join(LOG)) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e.to_string()),
    };
    let mut lines = vec![];
    for (n, l) in text.lines().enumerate().filter(|(_, l)| !l.trim().is_empty()) {
        let line: Line = serde_json::from_str(l).map_err(|e| format!("{LOG} line {}: {e}", n + 1))?;
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
    writeln!(f, "{json}")?;
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
    fn grades_from_verdicts() {
        assert_eq!(derive_grade(&[vec![true, true]]), Grade::Good);
        assert_eq!(derive_grade(&[vec![true, false]]), Grade::Hard);
        assert_eq!(derive_grade(&[vec![false, false]]), Grade::Again);
        assert_eq!(derive_grade(&[vec![true, true], vec![true, true]]), Grade::Good);
        assert_eq!(derive_grade(&[vec![true, true], vec![false, false]]), Grade::Hard);
    }

    #[test]
    fn validation() {
        assert!(input(r#"{"card":"X","purpose":"review","grader":"self","grade":"good"}"#).is_ok());
        let l = input(r#"{"concept":"C","item":"q?","purpose":"diagnostic","grader":"llm","verdicts":[[true,false]]}"#)
            .unwrap();
        assert_eq!(l.grade, Grade::Hard);
        for bad in [
            r#"{"card":"X","purpose":"pretest","grader":"self","grade":"good"}"#,
            r#"{"concept":"C","item":"q","purpose":"review","grader":"self","grade":"good"}"#,
            r#"{"card":"X","purpose":"review","grader":"llm","grade":"good","verdicts":[[true]]}"#,
            r#"{"card":"X","concept":"C","purpose":"review","grader":"self","grade":"good"}"#,
            r#"{"concept":"C","purpose":"practice","grader":"self","grade":"good"}"#,
            r#"{"card":"X","purpose":"review","grader":"self","grade":"good","extra":1}"#,
        ] {
            assert!(input(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn line_round_trip() {
        let l = input(r#"{"card":"X","purpose":"review","grader":"self","grade":"easy"}"#).unwrap();
        let s = serde_json::to_string(&l).unwrap();
        assert_eq!(s, r#"{"t":"1970-01-01T00:00:00Z","card":"X","purpose":"review","grader":"self","grade":"easy"}"#);
        assert_eq!(serde_json::from_str::<Line>(&s).unwrap(), l);
    }
}
