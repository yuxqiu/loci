//! What a command reports (m0 §8): diagnostics, refusals, the `--json` envelope and
//! the exit code.

use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SCHEMA: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

/// A finding. A position is in a file, and is whole or absent: the JSON is checked
/// against this when a plugin's envelope is read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: String,
    pub severity: Severity,
    /// The file it is about; `None` for the whole vault.
    pub location: Option<Location>,
    pub message: String,
    pub hint: Option<String>,
}

/// A file, and optionally a position in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    pub path: String,
    pub span: Option<Span>,
}

/// A byte range of a file, with the 1-based line and column (in Unicode scalar values)
/// of its start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub line: usize,
    pub col: usize,
    pub byte_start: usize,
    pub byte_end: usize,
}

impl Span {
    /// The span of byte range `range` of `text`.
    pub fn new(text: &str, range: std::ops::Range<usize>) -> Self {
        let (line, col) = line_col(text, range.start);
        Span { line, col, byte_start: range.start, byte_end: range.end }
    }
}

/// 1-based line and column (column in Unicode scalar values) of a byte offset.
pub fn line_col(text: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(text.len());
    let before = &text[..offset];
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let col = text[line_start..offset].chars().count() + 1;
    (line, col)
}

impl Diagnostic {
    pub fn error(code: &str, message: impl Into<String>) -> Self {
        Self::new(code, Severity::Error, message)
    }

    pub fn warning(code: &str, message: impl Into<String>) -> Self {
        Self::new(code, Severity::Warning, message)
    }

    fn new(code: &str, severity: Severity, message: impl Into<String>) -> Self {
        Self { code: code.to_string(), severity, location: None, message: message.into(), hint: None }
    }

    pub fn at_path(mut self, path: impl Into<String>) -> Self {
        self.location = Some(Location { path: path.into(), span: None });
        self
    }

    /// Positions the diagnostic at a byte range of `text`, the content of the file
    /// [`Diagnostic::at_path`] named.
    pub fn at(mut self, text: &str, range: std::ops::Range<usize>) -> Self {
        let location = self.location.as_mut().expect("a position is in a file: `at_path` comes first");
        location.span = Some(Span::new(text, range));
        self
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// The path of the file it is about, if any.
    pub fn path(&self) -> Option<&str> {
        self.location.as_ref().map(|l| l.path.as_str())
    }

    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}

/// Sorts diagnostics by path bytes, then byte offset, code and message (m0 §7.1). A
/// diagnostic without a path, or without a position in its file, comes first.
pub fn sort_diagnostics(diags: &mut [Diagnostic]) {
    diags.sort_by_cached_key(|d| {
        let start = d.location.as_ref().and_then(|l| l.span).map(|s| s.byte_start);
        (d.path().map(|p| p.as_bytes().to_vec()), start, d.code.clone(), d.message.clone())
    });
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        })
    }
}

/// The human form: `path:line:col: severity[code]: message`, then the hint.
impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.location {
            Some(Location { path, span: Some(s) }) => write!(f, "{path}:{}:{}: ", s.line, s.col)?,
            Some(Location { path, span: None }) => write!(f, "{path}: ")?,
            None => {}
        }
        write!(f, "{}[{}]: {}", self.severity, self.code, self.message)?;
        if let Some(h) = &self.hint {
            write!(f, "\n  hint: {h}")?;
        }
        Ok(())
    }
}

/// Exit codes (m0 §8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    Ok = 0,
    Refused = 1,
    // 2 (usage) is clap's own exit for bad arguments; loci never returns it.
    Internal = 3,
}

impl From<Exit> for std::process::ExitCode {
    fn from(e: Exit) -> Self {
        Self::from(e as u8)
    }
}

/// The envelope's `error`: why a command could not do its work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CmdError {
    pub code: String,
    pub message: String,
}

/// A command that could not do its work (m0 §8): its exit code and error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub exit: Exit,
    pub error: CmdError,
}

impl Refusal {
    /// A refused precondition (exit 1).
    pub fn refused(code: &str, message: impl Into<String>) -> Self {
        Self::new(Exit::Refused, code, message)
    }

    /// A failure of the environment rather than a refused request (exit 3), with its own
    /// code. [`Refusal::io`] and [`Refusal::git`] are the two common cases.
    pub fn internal(code: &str, message: impl Into<String>) -> Self {
        Self::new(Exit::Internal, code, message)
    }

    /// An I/O failure (`io`, exit 3).
    pub fn io(e: impl fmt::Display) -> Self {
        Self::internal("io", e.to_string())
    }

    /// A git failure (`git`, exit 3).
    pub fn git(e: impl fmt::Display) -> Self {
        Self::internal("git", e.to_string())
    }

    fn new(exit: Exit, code: &str, message: impl Into<String>) -> Self {
        Self { exit, error: CmdError { code: code.to_string(), message: message.into() } }
    }

    pub fn code(&self) -> &str {
        &self.error.code
    }
}

/// The human form: `error[code]: message`.
impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "error[{}]: {}", self.error.code, self.error.message)
    }
}

/// The `--json` document every command prints (m0 §8); plugins print it too.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    pub schema: u32,
    pub loci: String,
    pub command: String,
    pub ok: bool,
    pub data: Value,
    pub diagnostics: Vec<Diagnostic>,
    pub error: Option<CmdError>,
}

/// What a command produced: its data or the refusal that stopped it, the diagnostics
/// that accompany either, and the human rendering of the data.
#[derive(Debug)]
pub struct Report {
    pub result: Result<Value, Refusal>,
    pub diagnostics: Vec<Diagnostic>,
    pub human: String,
}

impl Report {
    pub fn ok(data: Value, human: impl Into<String>) -> Self {
        Self { result: Ok(data), diagnostics: vec![], human: human.into() }
    }

    pub fn with_diagnostics(mut self, diagnostics: Vec<Diagnostic>) -> Self {
        self.diagnostics = diagnostics;
        self
    }

    /// The refusal's exit code; otherwise 1 when any diagnostic is an error, else 0.
    pub fn exit(&self) -> Exit {
        match &self.result {
            Err(r) => r.exit,
            Ok(_) if self.diagnostics.iter().any(Diagnostic::is_error) => Exit::Refused,
            Ok(_) => Exit::Ok,
        }
    }

    pub fn envelope(&self, version: &str, command: &str) -> Envelope {
        let (data, error) = match &self.result {
            Ok(d) => (d.clone(), None),
            Err(r) => (Value::Null, Some(r.error.clone())),
        };
        Envelope {
            schema: SCHEMA,
            loci: version.to_string(),
            command: command.to_string(),
            ok: self.exit() == Exit::Ok,
            data,
            diagnostics: self.diagnostics.clone(),
            error,
        }
    }

    /// Prints the report: the envelope on stdout with `json`; otherwise the human text
    /// on stdout and diagnostics and the error on stderr (m0 §8). Returns the exit code.
    pub fn print(&self, json: bool, version: &str, command: &str) -> Exit {
        if json {
            let env = self.envelope(version, command);
            println!("{}", serde_json::to_string_pretty(&env).expect("an envelope always serialises"));
        } else {
            if !self.human.is_empty() {
                print!("{}", self.human);
                if !self.human.ends_with('\n') {
                    println!();
                }
            }
            for d in &self.diagnostics {
                eprintln!("{d}");
            }
            if let Err(r) = &self.result {
                eprintln!("{r}");
            }
        }
        self.exit()
    }
}

impl From<Refusal> for Report {
    fn from(r: Refusal) -> Self {
        Self { result: Err(r), diagnostics: vec![], human: String::new() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn line_col_counts_chars() {
        let t = "ab\n二分x";
        assert_eq!(line_col(t, 0), (1, 1));
        assert_eq!(line_col(t, 3), (2, 1));
        assert_eq!(line_col(t, 3 + "二分".len()), (2, 3));
    }

    #[test]
    fn a_diagnostic_location_is_nested() {
        let d = Diagnostic::error("x.y", "m").at_path("views/v.md").at("ab\ncd", 3..5);
        let json = serde_json::to_value(&d).unwrap();
        assert_eq!(
            json,
            json!({"code": "x.y", "severity": "error", "location": {"path": "views/v.md",
                "span": {"line": 2, "col": 1, "byte_start": 3, "byte_end": 5}}, "message": "m", "hint": null})
        );
        assert_eq!(serde_json::from_value::<Diagnostic>(json).unwrap(), d);
        assert_eq!(serde_json::to_value(Diagnostic::warning("x.y", "m")).unwrap()["location"], json!(null));
    }

    #[test]
    fn a_partial_position_is_refused_on_read() {
        let partial = json!({"code": "x.y", "severity": "error", "location": {"path": "v.md",
            "span": {"line": 1, "col": 1}}, "message": "m", "hint": null});
        assert!(serde_json::from_value::<Diagnostic>(partial).is_err());
    }

    #[test]
    fn an_error_diagnostic_fails_any_command() {
        let warned = Report::ok(json!({}), "").with_diagnostics(vec![Diagnostic::warning("x.y", "m")]);
        assert_eq!(warned.exit(), Exit::Ok);
        let failed = Report::ok(json!({}), "").with_diagnostics(vec![Diagnostic::error("x.y", "m")]);
        assert_eq!((failed.exit(), failed.envelope("0", "c").ok), (Exit::Refused, false));
    }

    #[test]
    fn human_forms() {
        let d = Diagnostic::warning("x.y", "m").at_path("v.md").at("ab\ncd", 4..5).with_hint("h");
        assert_eq!(d.to_string(), "v.md:2:2: warning[x.y]: m\n  hint: h");
        assert_eq!(Refusal::refused("a.b", "no").to_string(), "error[a.b]: no");
    }
}
