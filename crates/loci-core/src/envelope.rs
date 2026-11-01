//! The `--json` envelope, diagnostics and exit codes (m0 §8).

use serde::{Deserialize, Serialize};

pub const SCHEMA: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: String,
    pub severity: Severity,
    pub path: Option<String>,
    pub line: Option<usize>,
    pub col: Option<usize>,
    pub byte_start: Option<usize>,
    pub byte_end: Option<usize>,
    pub message: String,
    pub hint: Option<String>,
}

impl Diagnostic {
    pub fn new(code: &str, severity: Severity, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            severity,
            path: None,
            line: None,
            col: None,
            byte_start: None,
            byte_end: None,
            message: message.into(),
            hint: None,
        }
    }

    pub fn error(code: &str, message: impl Into<String>) -> Self {
        Self::new(code, Severity::Error, message)
    }

    pub fn warning(code: &str, message: impl Into<String>) -> Self {
        Self::new(code, Severity::Warning, message)
    }

    pub fn at_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// Positions the diagnostic at a byte range of `text` (the file's content).
    pub fn at(mut self, text: &str, range: std::ops::Range<usize>) -> Self {
        let (line, col) = line_col(text, range.start);
        self.line = Some(line);
        self.col = Some(col);
        self.byte_start = Some(range.start);
        self.byte_end = Some(range.end);
        self
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }

    /// The human form: `path:line:col: severity[code]: message`, then the hint.
    pub fn human(&self) -> String {
        let sev = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        let loc = match (&self.path, self.line, self.col) {
            (Some(p), Some(l), Some(c)) => format!("{p}:{l}:{c}: "),
            (Some(p), _, _) => format!("{p}: "),
            _ => String::new(),
        };
        let mut s = format!("{loc}{sev}[{}]: {}", self.code, self.message);
        if let Some(h) = &self.hint {
            s.push_str(&format!("\n  hint: {h}"));
        }
        s
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

/// Sorts diagnostics by (path bytes, byte offset, code) (m0 §7.1). Pathless diagnostics
/// come first; positionless ones come first within a path.
pub fn sort_diagnostics(diags: &mut [Diagnostic]) {
    diags.sort_by(|a, b| {
        let pa = a.path.as_deref().map(str::as_bytes);
        let pb = b.path.as_deref().map(str::as_bytes);
        pa.cmp(&pb).then(a.byte_start.cmp(&b.byte_start)).then(a.code.cmp(&b.code)).then(a.message.cmp(&b.message))
    });
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CmdError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    pub schema: u32,
    pub loci: String,
    pub command: String,
    pub ok: bool,
    pub data: serde_json::Value,
    pub diagnostics: Vec<Diagnostic>,
    pub error: Option<CmdError>,
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

/// Why a command could not do its work (m0 §8): the exit code and the envelope's
/// `error`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub exit: Exit,
    pub error: CmdError,
}

impl Refusal {
    /// A refused precondition (exit 1).
    pub fn refused(code: &str, message: impl Into<String>) -> Self {
        Self { exit: Exit::Refused, error: CmdError { code: code.to_string(), message: message.into() } }
    }

    /// An I/O, git or internal failure (exit 3).
    pub fn internal(code: &str, message: impl Into<String>) -> Self {
        Self { exit: Exit::Internal, error: CmdError { code: code.to_string(), message: message.into() } }
    }

    /// An I/O failure (`io`, exit 3).
    pub fn io(e: impl std::fmt::Display) -> Self {
        Self::internal("io", e.to_string())
    }

    /// A git failure (`git`, exit 3).
    pub fn git(e: impl std::fmt::Display) -> Self {
        Self::internal("git", e.to_string())
    }

    pub fn code(&self) -> &str {
        &self.error.code
    }
}

/// The outcome of a command: data on success, or a refusal. Diagnostics travel
/// separately so that warnings accompany both.
#[derive(Debug)]
pub struct Outcome {
    pub result: Result<serde_json::Value, Refusal>,
    pub diagnostics: Vec<Diagnostic>,
    /// Exit code when `result` is `Ok` (1 for a lint with errors).
    exit_ok: Exit,
}

impl Outcome {
    pub fn ok(data: serde_json::Value) -> Self {
        Self { result: Ok(data), diagnostics: vec![], exit_ok: Exit::Ok }
    }

    /// A lint result (m0 §8): exit 1 when any diagnostic is an error.
    pub fn lint(data: serde_json::Value, diagnostics: Vec<Diagnostic>) -> Self {
        let exit_ok = if diagnostics.iter().any(Diagnostic::is_error) { Exit::Refused } else { Exit::Ok };
        Self { result: Ok(data), diagnostics, exit_ok }
    }

    pub fn with_diagnostics(mut self, diags: Vec<Diagnostic>) -> Self {
        self.diagnostics = diags;
        self
    }

    pub fn exit(&self) -> Exit {
        match &self.result {
            Ok(_) => self.exit_ok,
            Err(r) => r.exit,
        }
    }

    pub fn envelope(&self, version: &str, command: &str) -> Envelope {
        let exit = self.exit();
        let (data, error) = match &self.result {
            Ok(d) => (d.clone(), None),
            Err(r) => (serde_json::Value::Null, Some(r.error.clone())),
        };
        Envelope {
            schema: SCHEMA,
            loci: version.to_string(),
            command: command.to_string(),
            ok: exit == Exit::Ok,
            data,
            diagnostics: self.diagnostics.clone(),
            error,
        }
    }
}

impl From<Refusal> for Outcome {
    fn from(r: Refusal) -> Self {
        Self { result: Err(r), diagnostics: vec![], exit_ok: Exit::Ok }
    }
}

impl From<Refusal> for Report {
    fn from(r: Refusal) -> Self {
        Self { outcome: r.into(), human: String::new() }
    }
}

/// What a command produced: the envelope content and its human rendering.
pub struct Report {
    pub outcome: Outcome,
    pub human: String,
}

impl Report {
    pub fn new(outcome: Outcome, human: impl Into<String>) -> Self {
        Self { outcome, human: human.into() }
    }

    /// Prints the report: the envelope on stdout with `json`; otherwise the human text
    /// on stdout and diagnostics and the error on stderr (m0 §8). Returns the exit code.
    pub fn print(&self, json: bool, version: &str, command: &str) -> Exit {
        let outcome = &self.outcome;
        if json {
            let env = outcome.envelope(version, command);
            println!("{}", serde_json::to_string_pretty(&env).expect("an envelope always serialises"));
        } else {
            if !self.human.is_empty() {
                print!("{}", self.human);
                if !self.human.ends_with('\n') {
                    println!();
                }
            }
            for d in &outcome.diagnostics {
                eprintln!("{}", d.human());
            }
            if let Err(r) = &outcome.result {
                eprintln!("error[{}]: {}", r.error.code, r.error.message);
            }
        }
        outcome.exit()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_col_counts_chars() {
        let t = "ab\n二分x";
        assert_eq!(line_col(t, 0), (1, 1));
        assert_eq!(line_col(t, 3), (2, 1));
        assert_eq!(line_col(t, 3 + "二分".len()), (2, 3));
    }
}
