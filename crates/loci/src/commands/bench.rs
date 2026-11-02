//! `loci bench` (m1-m4 §3).

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use loci_core::envelope::{Refusal, Report};
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

/// Runs `command` through `sh -c` with `question` on stdin; returns stdout, or `None`
/// on timeout. Failing to start or talk to `sh` is an error.
fn run_agent(command: &str, question: &str, cwd: &std::path::Path) -> std::io::Result<Option<Vec<u8>>> {
    let mut child = Command::new("sh")
        .args(["-c", command])
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
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
    if child.wait_timeout(AGENT_TIMEOUT)?.is_none() {
        child.kill()?;
        child.wait()?;
        return Ok(None);
    }
    // A process the agent left running may hold stdout open, so the wait is bounded too.
    match output.recv_timeout(AGENT_TIMEOUT.saturating_sub(start.elapsed())) {
        Ok(out) => Ok(Some(out?)),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Ok(None),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => panic!("the stdout reader always sends"),
    }
}

pub fn run(ctx: &Ctx, agent: &str, cases_file: Option<&Path>) -> Result<Report, Refusal> {
    let vault = ctx.vault()?;
    let path = cases_file.map_or_else(|| vault.root.join(FILE), Path::to_path_buf);
    let shown = path.display();
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(Refusal::refused("bench.none", format!("`{shown}` does not exist")));
        }
        Err(e) => return Err(Refusal::io(format!("reading `{shown}`: {e}"))),
    };
    let mut cases = vec![];
    for (n, line) in text.lines().enumerate().filter(|(_, l)| !l.trim().is_empty()) {
        let case = serde_json::from_str::<Case>(line)
            .map_err(|e| Refusal::refused("bench.case", format!("{shown} line {}: {e}", n + 1)))?;
        cases.push(case);
    }
    if cases.is_empty() {
        return Err(Refusal::refused("bench.none", format!("`{shown}` has no cases")));
    }
    let (mut passes, mut times, mut results) = (0usize, vec![], vec![]);
    for case in &cases {
        let start = Instant::now();
        let pass = run_case(&vault.root, agent, case).map_err(|e| Refusal::internal("bench.agent", e))?;
        times.push(start.elapsed().as_secs_f64());
        passes += usize::from(pass);
        results.push(json!({"q": case.q, "pass": pass}));
    }
    let pass_rate = passes as f64 / cases.len() as f64;
    let median_s = median(&mut times);
    let human = format!("{} cases: pass rate {pass_rate:.3}, median {median_s:.1} s\n", cases.len());
    let data = json!({"cases": cases.len(), "pass_rate": pass_rate, "median_s": median_s, "results": results});
    Ok(Report::ok(data, human))
}

/// Runs one case on a fresh copy of the vault (without `bench/`), so that the agent
/// never sees the cases and no case sees another's edits; the copy is deleted after.
fn run_case(root: &Path, agent: &str, case: &Case) -> Result<bool, String> {
    let work =
        tempfile::Builder::new().prefix("loci-bench-").tempdir().map_err(|e| format!("a temporary folder: {e}"))?;
    let copy = work.path().join("vault");
    vault::copy_dir(root, &copy, &["bench"]).map_err(|e| format!("copying the vault: {e}"))?;
    let Some(reply) =
        run_agent(agent, &case.q, &copy).map_err(|e| format!("cannot run the agent through `sh`: {e}"))?
    else {
        return Ok(false);
    };
    let output = work.path().join("output.txt");
    std::fs::write(&output, reply).map_err(|e| format!("writing the agent's reply: {e}"))?;
    let status = Command::new("sh")
        .args(["-c", &case.check])
        .current_dir(&copy)
        .env("LOCI_BENCH_OUTPUT", &output)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("cannot run the check through `sh`: {e}"))?;
    Ok(status.success())
}
