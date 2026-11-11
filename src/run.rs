//! Run a command that fits a shape, and count what it shows.
//!
//! The program is started directly, with no shell, in the ledger's directory, with
//! nothing on standard input.
//!
//! A row is judged by what its command shows, never by its exit status alone. The
//! status does not say enough: given a path that does not exist, `ast-grep` shows
//! no result and exits with 0 or 1, as it does when it finds no match. A row that
//! says `count = 0` would then hold for a path nobody can search. So a failure is
//! read from what the program says on standard error. If a program words a count
//! of matches in a way `ley` does not know, the row is reported as "could not
//! run". It is never passed.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::shape::{Counts, Plan};
use crate::text::shown;

/// How long one command may take.
pub const LIMIT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The command ran. It showed this many results.
    Count {
        results: u64,
        /// The first thing the program said on standard error, if anything.
        note: Option<String>,
        /// What the counting run printed.
        printed: String,
    },
    /// The command showed nothing that can be counted.
    Failed {
        why: String,
        /// True when the program ran to its end and said it failed, as it does for a
        /// path that is not there. False when it did not start, did not finish, or
        /// printed something `ley` cannot count: then nothing was learned about the
        /// place.
        ran: bool,
    },
}

/// Why a program gave nothing to count. `ran` is as in `Outcome::Failed`.
struct Failure {
    why: String,
    ran: bool,
}

impl Failure {
    /// The program did not start, or did not run to its end.
    fn did_not_run(why: String) -> Failure {
        Failure { why, ran: false }
    }

    /// The program ran to its end and said it failed.
    fn said_so(why: String) -> Failure {
        Failure { why, ran: true }
    }
}

/// What a program did.
struct Ran {
    out: String,
    /// The first thing it said on standard error, if anything.
    note: Option<String>,
}

/// Start a program, wait for it, and say what it printed, or why that failed.
fn start(argv: &[String], dir: &Path, limit: Duration) -> Result<Ran, Failure> {
    let program = &argv[0];
    if !dir.is_dir() {
        return Err(Failure::did_not_run(format!(
            "the directory {} does not exist",
            shown(&dir.display().to_string())
        )));
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
            return Err(Failure::did_not_run(format!(
                "`{program}` was not found on PATH"
            )));
        }
        Err(e) => {
            return Err(Failure::did_not_run(format!(
                "`{program}` could not start: {e}"
            )));
        }
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
                return Err(Failure::did_not_run(format!(
                    "`{program}` did not finish within {} seconds",
                    limit.as_secs()
                )));
            }
            Ok(None) => thread::sleep(Duration::from_millis(2)),
            Err(e) => {
                return Err(Failure::did_not_run(format!(
                    "waiting for `{program}` failed: {e}"
                )));
            }
        }
    };
    let out = stdout.join().unwrap_or_default();
    let err = stderr.join().unwrap_or_default();

    // What the program said may quote a file name. None of it is printed raw.
    let said = |line: &str| format!("{program} said: {}", shown(line.trim()));
    let note = err
        .lines()
        .find(|line| !line.trim().is_empty() && !is_a_tally_of_findings(line));
    let Some(code) = status.code() else {
        return Err(Failure::did_not_run(format!(
            "`{program}` was stopped by a signal"
        )));
    };
    if let Some(line) = err.lines().find(|line| reports_an_error(line)) {
        return Err(Failure::said_so(said(line)));
    }
    match code {
        0 | 1 => Ok(Ran {
            out,
            note: note.map(said),
        }),
        code => Err(Failure::said_so(match note {
            Some(line) => said(line),
            None => format!("`{program}` exited with status {code}"),
        })),
    }
}

/// True for a line on standard error that says the program failed.
fn reports_an_error(line: &str) -> bool {
    line.trim_start().to_ascii_lowercase().starts_with("error") && !is_a_tally_of_findings(line)
}

/// `ast-grep scan` ends with "Error: 1 error(s) found in code." when a rule whose
/// severity is `error` matched. That is a count of matches, not a failed run.
///
/// `ley` could make the line go away by passing `--hint`, which gives every rule
/// that severity. It does not: `--hint` also switches on a rule that is switched
/// off, and the row would then count matches its reader never sees.
fn is_a_tally_of_findings(line: &str) -> bool {
    line.trim()
        .strip_prefix("Error: ")
        .and_then(|rest| rest.strip_suffix(" error(s) found in code."))
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

/// Run a command and count what it shows.
pub fn run(plan: &Plan, dir: &Path, limit: Duration) -> Outcome {
    let ran = match start(&plan.argv, dir, limit) {
        Ok(ran) => ran,
        Err(Failure { why, ran }) => return Outcome::Failed { why, ran },
    };
    let results = match plan.counts {
        // Every line is a result, an empty one too: `rg -x ''` prints empty lines.
        Counts::Lines => ran.out.lines().count() as u64,
        Counts::JsonLines => ran.out.lines().filter(|l| !l.trim().is_empty()).count() as u64,
        Counts::JsonItems => {
            let mut items = 0;
            for line in ran.out.lines().filter(|l| !l.trim().is_empty()) {
                let listed = serde_json::from_str::<serde_json::Value>(line)
                    .ok()
                    .and_then(|file| file.get("items")?.as_array().map(Vec::len));
                match listed {
                    Some(n) => items += n as u64,
                    None => {
                        return Outcome::Failed {
                            why: format!(
                                "`{}` printed something other than a list of items",
                                plan.argv[0]
                            ),
                            ran: false,
                        };
                    }
                }
            }
            items
        }
    };
    Outcome::Count {
        results,
        note: ran.note,
        printed: ran.out,
    }
}

/// The text a reader of the row would see. For `rg` that is `printed`, what the
/// counting run printed. For `ast-grep`, `ley` counted JSON, so the command runs
/// once more without it. `Err` says why that second run gave nothing.
pub fn seen(plan: &Plan, printed: String, dir: &Path, limit: Duration) -> Result<String, String> {
    match &plan.plain {
        None => Ok(printed),
        Some(argv) => match start(argv, dir, limit) {
            Ok(ran) => Ok(ran.out),
            Err(failure) => Err(failure.why),
        },
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tally_of_findings_is_not_an_error() {
        assert!(is_a_tally_of_findings("Error: 1 error(s) found in code."));
        assert!(is_a_tally_of_findings("Error: 12 error(s) found in code."));
        assert!(!reports_an_error("Error: 1 error(s) found in code."));
        assert!(reports_an_error("Error: Cannot parse rule INLINE_RULES"));
        assert!(reports_an_error(
            "ERROR: nowhere: No such file or directory (os error 2)"
        ));
        assert!(reports_an_error("error: unrecognized subcommand 'outline'"));
        assert!(!reports_an_error("Warning: Pattern contains an ERROR node"));
    }
}
