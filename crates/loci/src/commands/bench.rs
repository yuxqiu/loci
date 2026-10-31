//! `loci bench` (m1-m4 §3).

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use loci_core::envelope::{Outcome, Report};
use loci_core::index::Index;
use loci_core::vault::{self, Kind};
use regex::Regex;
use serde::Deserialize;
use serde_json::json;

use super::{Ctx, find};

pub const FILE: &str = "bench/retrieval.jsonl";
const AGENT_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    q: String,
    expect: Vec<String>,
}

/// True if a hit or a mention names one of the expected items (a vault path, or a
/// concept slug).
fn matches(expect: &[String], path: &str) -> bool {
    let stem = vault::stem(path);
    let is_concept = vault::kind_of(path) == Some(Kind::Concept);
    let without_md = path.strip_suffix(".md").unwrap_or(path);
    expect.iter().any(|e| e.strip_suffix(".md").unwrap_or(e) == without_md || (is_concept && e == stem))
}

/// True if the agent's output mentions an expected item: a vault path (an entry with a
/// `/`, `.md` optional, not part of a longer path), or a concept as `[[slug]]`
/// (also `[[slug|…]]` and `[[slug#…]]`).
fn mentions(expect: &[String], output: &str) -> bool {
    expect.iter().any(|e| {
        let pattern = if e.contains('/') {
            let path = regex::escape(e.strip_suffix(".md").unwrap_or(e));
            format!(r"(?:^|[^\w/.-]){path}(?:\.md)?(?:$|[^\w/-])")
        } else {
            format!(r"(?i)\[\[{}(?:\]\]|\||#)", regex::escape(e))
        };
        Regex::new(&pattern).expect("escaped pattern").is_match(output)
    })
}

fn median(xs: &mut [f64]) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    xs.sort_by(f64::total_cmp);
    xs[xs.len() / 2]
}

/// Runs `command` through `sh -c` with `question` on stdin; returns stdout, or `None`
/// on timeout. Failing to start or talk to `sh` is an error.
fn run_agent(command: &str, question: &str, cwd: &std::path::Path) -> std::io::Result<Option<String>> {
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
    let reader = std::thread::spawn(move || {
        let mut out = vec![];
        stdout.read_to_end(&mut out).map(|_| out)
    });
    // An agent may exit without reading the question.
    match child.stdin.take().expect("stdin is piped").write_all(question.as_bytes()) {
        Err(e) if e.kind() != std::io::ErrorKind::BrokenPipe => return Err(e),
        _ => {}
    }
    let start = Instant::now();
    while child.try_wait()?.is_none() {
        if start.elapsed() > AGENT_TIMEOUT {
            child.kill()?;
            child.wait()?;
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let out = reader.join().expect("the stdout reader does not panic")?;
    Ok(Some(String::from_utf8_lossy(&out).into_owned()))
}

pub fn run(ctx: &Ctx, agent: Option<&str>, k: usize) -> Result<Report, Report> {
    let vault = ctx.vault()?;
    let text = std::fs::read_to_string(vault.abs(FILE))
        .map_err(|_| Report::fail(Outcome::refused("bench.none", format!("`{FILE}` does not exist"))))?;
    let mut cases = vec![];
    for (n, line) in text.lines().enumerate().filter(|(_, l)| !l.trim().is_empty()) {
        match serde_json::from_str::<Case>(line) {
            Ok(c) => cases.push(c),
            Err(e) => return Err(Report::fail(Outcome::refused("bench.case", format!("{FILE} line {}: {e}", n + 1)))),
        }
    }
    let index = Index::open(&vault);
    let docs = find::docs(&index);
    let (mut ranked, mut rr_sum) = (0usize, 0f64);
    let mut latencies = vec![];
    let (mut agent_passes, mut agent_times) = (0usize, vec![]);
    let mut results = vec![];
    for case in &cases {
        let start = Instant::now();
        let hits = match find::query(&index, &docs, &case.q, k, &[]) {
            Ok(h) => h,
            Err(e) => return Err(Report::fail(Outcome::internal("find.index", e))),
        };
        latencies.push(start.elapsed().as_secs_f64() * 1000.0);
        let rank = hits.iter().position(|h| matches(&case.expect, &h.hit.path)).map(|i| i + 1);
        if let Some(r) = rank {
            ranked += 1;
            rr_sum += 1.0 / r as f64;
        }
        let agent_pass = match agent {
            None => None,
            Some(cmd) => {
                let start = Instant::now();
                let out = match run_agent(cmd, &case.q, &vault.root) {
                    Ok(out) => out,
                    Err(e) => {
                        return Err(Report::fail(Outcome::internal(
                            "bench.agent",
                            format!("cannot run the agent through `sh`: {e}"),
                        )));
                    }
                };
                agent_times.push(start.elapsed().as_secs_f64());
                let pass = out.is_some_and(|out| mentions(&case.expect, &out));
                agent_passes += usize::from(pass);
                Some(pass)
            }
        };
        results.push(json!({"q": case.q, "rank": rank, "agent_pass": agent_pass}));
    }
    let n = cases.len().max(1) as f64;
    let recall = ranked as f64 / n;
    let mrr = rr_sum / n;
    let max = latencies.iter().copied().fold(0.0, f64::max);
    let med = median(&mut latencies);
    let agent_json = agent.map(|_| json!({"pass_rate": agent_passes as f64 / n, "median_s": median(&mut agent_times)}));
    let mut human = format!(
        "{} cases: recall@{k} {recall:.3}, MRR {mrr:.3}, find median {med:.1} ms (max {max:.1} ms)\n",
        cases.len()
    );
    if let Some(a) = &agent_json {
        human.push_str(&format!(
            "agent: pass rate {:.3}, median {:.1} s\n",
            a["pass_rate"].as_f64().unwrap_or(0.0),
            a["median_s"].as_f64().unwrap_or(0.0)
        ));
    }
    let data = json!({
        "cases": cases.len(),
        "recall_at_k": recall,
        "mrr": mrr,
        "latency_ms": {"median": med, "max": max},
        "agent": agent_json,
        "results": results,
    });
    Ok(Report::new(Outcome::ok(data), human))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching() {
        let e = vec!["bfs".to_string(), "views/plan".to_string()];
        assert!(matches(&e, "concepts/a/bfs.md"));
        assert!(matches(&e, "views/plan.md"));
        assert!(!matches(&e, "views/bfs.md"));
        assert!(mentions(&e, "see [[bfs]] for details"));
        assert!(mentions(&e, "see [[bfs|breadth-first]] and [[bfs#Proof]]"));
        assert!(mentions(&e, "read views/plan.md."));
        assert!(mentions(&e, "read views/plan"));
        assert!(!mentions(&e, "nothing here"));
        assert!(!mentions(&e, "subfs and bfs-variant, plain bfs"));
        assert!(!mentions(&e, "read views/plan2.md or views/plan/x.md"));
    }
}
