//! `loci bench` (m1-m4 §3).

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use loci_core::envelope::{Refusal, Report};
use loci_core::git::Repo;
use loci_core::path::VaultPath;
use serde::Deserialize;
use serde_json::json;

use loci_core::vault;
use wait_timeout::ChildExt;

use super::Ctx;

/// The default cases file, relative to the vault root.
pub const FILE: &str = "bench/cases.jsonl";
const AGENT_TIMEOUT: Duration = Duration::from_secs(600);

/// One case: a question or task for the agent, and a shell command that passes (exit
/// 0) when the agent did it. The check runs in the agent's copy of the vault, with
/// `LOCI_BENCH_OUTPUT` naming a file that holds the agent's reply.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    q: String,
    check: String,
}

/// The median of `xs`, which is not empty.
fn median(xs: &mut [f64]) -> f64 {
    xs.sort_by(f64::total_cmp);
    xs[xs.len() / 2]
}

/// How an agent run ended.
enum Ran {
    /// It ran past the timeout, or left stdout open that long.
    TimedOut,
    /// It exited with `code` (`None` when killed by a signal) and wrote `stdout`.
    Exited { code: Option<i32>, stdout: Vec<u8> },
}

/// Runs `agent` ([`sh`]) with `question` on stdin and loci's stderr as its own.
/// Failing to start or talk to `sh` is an error.
fn run_agent(mut agent: Command, question: &str) -> std::io::Result<Ran> {
    let mut child = agent.stdin(Stdio::piped()).stdout(Stdio::piped()).spawn()?;

    // Stdout is drained on its own thread so that an agent printing more than a pipe
    // buffer does not block before it exits.
    let mut stdout = child.stdout.take().expect("stdout is piped");
    let (sender, output) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut out = vec![];
        // The receiver is gone only after a timeout, when the output is not wanted.
        let _ = sender.send(stdout.read_to_end(&mut out).map(|_| out));
    });

    // An agent may exit without reading the question.
    match child.stdin.take().expect("stdin is piped").write_all(question.as_bytes()) {
        Err(e) if e.kind() != std::io::ErrorKind::BrokenPipe => return Err(e),
        _ => {}
    }

    let start = Instant::now();
    let Some(status) = child.wait_timeout(AGENT_TIMEOUT)? else {
        child.kill()?;
        child.wait()?;
        return Ok(Ran::TimedOut);
    };

    // A process the agent left running may hold stdout open, so the wait is bounded too.
    match output.recv_timeout(AGENT_TIMEOUT.saturating_sub(start.elapsed())) {
        Ok(out) => Ok(Ran::Exited { code: status.code(), stdout: out? }),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Ok(Ran::TimedOut),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => panic!("the stdout reader always sends"),
    }
}

/// `sh -c command` in `cwd`, a copy of the vault, without `git_vars`, the variables
/// that would point git elsewhere: git there finds the copy's repository.
fn sh(command: &str, cwd: &Path, git_vars: &[String]) -> Command {
    let mut c = Command::new("sh");
    c.args(["-c", command]).current_dir(cwd);
    for var in git_vars {
        c.env_remove(var);
    }
    c
}

pub fn run(ctx: &Ctx, agent: &str, cases_file: Option<&Path>) -> Result<Report, Refusal> {
    let vault = ctx.vault()?;
    let path = cases_file.map_or_else(|| vault.abs(&VaultPath::fixed(FILE)), Path::to_path_buf);
    let shown = path.display();
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(Refusal::refused("bench.none", format!("`{shown}` does not exist")));
        }
        Err(e) => return Err(Refusal::io(format!("reading `{shown}`: {e}"))),
    };

    let cases: Vec<Case> = text
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
        .map(|(n, l)| {
            serde_json::from_str(l).map_err(|e| Refusal::refused("bench.case", format!("{shown} line {}: {e}", n + 1)))
        })
        .collect::<Result<_, _>>()?;

    if cases.is_empty() {
        return Err(Refusal::refused("bench.none", format!("`{shown}` has no cases")));
    }

    let git_vars = loci_core::git::location_vars().map_err(Refusal::git)?;
    let (mut passes, mut times, mut results) = (0usize, vec![], vec![]);
    for case in &cases {
        let start = Instant::now();
        let (pass, agent_exit) =
            run_case(vault.root(), agent, case, &git_vars).map_err(|e| Refusal::internal("bench.agent", e))?;
        times.push(start.elapsed().as_secs_f64());
        passes += usize::from(pass);
        results.push(json!({"q": case.q, "pass": pass, "agent_exit": agent_exit}));
    }

    let pass_rate = passes as f64 / cases.len() as f64;
    let median_s = median(&mut times);
    let human = format!("{} cases: pass rate {pass_rate:.3}, median {median_s:.1} s\n", cases.len());
    let data = json!({"cases": cases.len(), "pass_rate": pass_rate, "median_s": median_s, "results": results});
    Ok(Report::ok(data, human))
}

/// Runs one case on a fresh copy of the vault (without `bench/`), so that the agent
/// never sees the cases and no case sees another's edits; the copy is deleted after.
/// Returns whether the case passed and the agent's exit code (`None` when it timed out
/// or was killed by a signal).
fn run_case(root: &Path, agent: &str, case: &Case, git_vars: &[String]) -> Result<(bool, Option<i32>), String> {
    let work =
        tempfile::Builder::new().prefix("loci-bench-").tempdir().map_err(|e| format!("a temporary folder: {e}"))?;
    let copy = work.path().join("vault");
    vault::copy_dir(root, &copy, &["bench", ".git"]).map_err(|e| format!("copying the vault: {e}"))?;
    Repo::new(root).copy_to(&copy).map_err(|e| format!("copying the repository: {e}"))?;
    let ran = run_agent(sh(agent, &copy, git_vars), &case.q)
        .map_err(|e| format!("cannot run the agent through `sh`: {e}"))?;
    let Ran::Exited { code: agent_exit, stdout: reply } = ran else {
        return Ok((false, None));
    };

    let output = work.path().join("output.txt");
    std::fs::write(&output, reply).map_err(|e| format!("writing the agent's reply: {e}"))?;
    let status = sh(&case.check, &copy, git_vars)
        .env("LOCI_BENCH_OUTPUT", &output)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("cannot run the check through `sh`: {e}"))?;
    Ok((status.success(), agent_exit))
}
