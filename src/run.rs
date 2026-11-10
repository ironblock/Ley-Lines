//! Run a command that fits a shape, and count what it shows.
//!
//! The program is started directly, with no shell, in the ledger's directory, with
//! nothing on standard input. A row is judged by what its command shows, never by
//! its exit status alone: several tools exit the same way for "no match" and for
//! "something went wrong". So an error is read from what the tool says.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::shape::{Counts, Plan};

/// How long one command may take.
pub const LIMIT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The command ran. It showed this many results.
    Count {
        results: u64,
        /// The first thing the program said on standard error, if anything.
        note: Option<String>,
    },
    /// The command could not run, or said it failed.
    Failed(String),
}

/// What a program did.
struct Ran {
    out: String,
    /// The first thing it said on standard error, if anything.
    note: Option<String>,
}

/// Start a program, wait for it, and say what it printed, or why that failed.
fn start(argv: &[String], dir: &Path, limit: Duration) -> Result<Ran, String> {
    let program = &argv[0];
    if !dir.is_dir() {
        return Err(format!("the directory {} does not exist", dir.display()));
    }
    let mut child = match Command::new(program)
        .args(&argv[1..])
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(format!("`{program}` was not found on PATH"));
        }
        Err(e) => return Err(format!("`{program}` could not start: {e}")),
    };

    // Read both streams while the program runs, so a full pipe cannot stall it.
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() > limit => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "`{program}` did not finish within {} seconds",
                    limit.as_secs()
                ));
            }
            Ok(None) => thread::sleep(Duration::from_millis(2)),
            Err(e) => return Err(format!("waiting for `{program}` failed: {e}")),
        }
    };
    let out = stdout.join().unwrap_or_default();
    let err = stderr.join().unwrap_or_default();

    let said = |line: &str| format!("{program} said: {}", line.trim());
    let note = err.lines().find(|line| !line.trim().is_empty());
    let error = err
        .lines()
        .find(|line| line.trim_start().to_ascii_lowercase().starts_with("error"));
    if let Some(line) = error {
        return Err(said(line));
    }
    match status.code() {
        Some(0 | 1) => Ok(Ran {
            out,
            note: note.map(said),
        }),
        Some(code) => Err(match note {
            Some(line) => said(line),
            None => format!("`{program}` exited with status {code}"),
        }),
        None => Err(format!("`{program}` was stopped by a signal")),
    }
}

/// Run a command and count what it shows.
pub fn run(plan: &Plan, dir: &Path, limit: Duration) -> Outcome {
    let ran = match start(&plan.argv, dir, limit) {
        Ok(ran) => ran,
        Err(why) => return Outcome::Failed(why),
    };
    let lines = ran.out.lines().filter(|line| !line.trim().is_empty());
    let results = match plan.counts {
        Counts::Lines | Counts::JsonLines => lines.count() as u64,
        Counts::JsonItems => {
            let mut items = 0;
            for line in lines {
                let listed = serde_json::from_str::<serde_json::Value>(line)
                    .ok()
                    .and_then(|file| file.get("items")?.as_array().map(Vec::len));
                match listed {
                    Some(n) => items += n as u64,
                    None => {
                        return Outcome::Failed(format!(
                            "`{}` printed something other than a list of items",
                            plan.argv[0]
                        ));
                    }
                }
            }
            items
        }
    };
    Outcome::Count {
        results,
        note: ran.note,
    }
}

/// What a reader would see: the output of the command exactly as the row has it.
pub fn printed(plan: &Plan, dir: &Path, limit: Duration) -> Option<String> {
    start(&plan.written, dir, limit).ok().map(|ran| ran.out)
}

fn drain(stream: Option<impl Read + Send + 'static>) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut stream) = stream {
            let _ = stream.read_to_end(&mut bytes);
        }
        String::from_utf8_lossy(&bytes).into_owned()
    })
}
