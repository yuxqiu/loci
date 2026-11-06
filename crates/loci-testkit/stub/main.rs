//! The stub plugin the integration tests run as `loci-stub` (std only; built by
//! `build.rs`). It answers from `loci-stub.replies` beside it, five lines: what `lint`
//! prints on stderr, its reply and exit code, and `skills`' reply and exit code.

use std::process::exit;

fn main() {
    let replies = std::env::current_exe().unwrap().with_file_name("loci-stub.replies");
    let replies = std::fs::read_to_string(replies).unwrap();
    let [stderr, lint, lint_exit, skills, skills_exit] = replies.lines().collect::<Vec<_>>()[..] else {
        panic!("loci-stub.replies does not hold five lines")
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("lint") => {
            eprintln!("{stderr}");
            println!("{lint}");
            exit(lint_exit.parse().unwrap())
        }
        Some("skills") => {
            println!("{skills}");
            exit(skills_exit.parse().unwrap())
        }
        // Killed by SIGABRT on Unix; on Windows, an exit code above 255.
        Some("crash") => std::process::abort(),
        _ => {
            let var = |name| std::env::var(name).unwrap_or_default();
            println!("args={}", args.join(" "));
            println!("vault={}", var("LOCI_VAULT"));
            println!("version={}", var("LOCI_VERSION"));
            exit(7)
        }
    }
}
