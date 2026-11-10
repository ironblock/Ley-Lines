//! `ley since`: find the earliest commit at which each row held.
//!
//! "Held" means the row's command, as written now, showed the number of results
//! the row says. `ley` walks back from the current commit, one commit at a time,
//! and stops at the first commit where the row does not hold. It does not bisect:
//! a fact can hold, break and hold again, and a bisect would then give the wrong
//! start.
//!
//! The date is "the earliest known". If the history here is cut short, the fact
//! may be older. So a recorded date moves earlier as more history is read. It
//! moves later only if the row stopped holding at some commit in between.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use toml::value::Datetime;

use crate::check::{self, Status};
use crate::ledger::{Ledger, Since};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Found {
    /// The row held at every commit from this one to the current one.
    Since {
        since: Since,
        /// How many commits the row held at, counting the current one.
        commits: usize,
        /// The row still held at the oldest commit in this history.
        oldest: bool,
    },
    /// The row does not hold at the current commit, so there is nothing to record.
    NotNow(String),
    /// The row's command fits no shape, so it was not run.
    NotChecked(String),
}

#[derive(Debug, Clone)]
pub struct Entry {
    /// The row's position in the ledger.
    pub index: usize,
    pub line: usize,
    pub name: String,
    pub found: Found,
}

/// Walk the history and say, for each row with a command, since when it has held.
pub fn since(ledger: &Ledger, limit: Duration) -> Result<Vec<Entry>, String> {
    let dir = ledger
        .dir()
        .canonicalize()
        .map_err(|e| format!("{}: {e}", ledger.dir().display()))?;
    let root = git(&dir, &["rev-parse", "--show-toplevel"]).map_err(|_| {
        format!(
            "`ley since` reads git history, and {} is not in a git repository",
            dir.display()
        )
    })?;
    let root = PathBuf::from(root)
        .canonicalize()
        .map_err(|e| format!("{e}"))?;
    let below = dir
        .strip_prefix(&root)
        .map_err(|_| "the ledger is outside its repository".to_string())?
        .to_path_buf();
    let history = git(&root, &["rev-list", "--first-parent", "HEAD"])
        .map_err(|_| "the repository has no commits yet".to_string())?;
    let commits: Vec<&str> = history.lines().collect();

    let mut entries = Vec::new();
    // The rows still holding, with the oldest commit each has held at so far.
    let mut holding: Vec<(usize, Option<usize>)> = Vec::new();
    for (index, row, pointer) in check::pointers(ledger) {
        match check::fit(pointer) {
            Ok(_) => holding.push((index, None)),
            Err(why) => entries.push(Entry {
                index,
                line: row.line,
                name: row.name.clone(),
                found: Found::NotChecked(why),
            }),
        }
    }

    if !holding.is_empty() {
        let tree = Tree::add(&root, commits[0])?;
        let mut done: Vec<(usize, Option<usize>, String)> = Vec::new();
        for (step, commit) in commits.iter().enumerate() {
            if holding.is_empty() {
                break;
            }
            if step > 0 {
                tree.move_to(commit)?;
            }
            let place = tree.path.join(&below);
            let mut still = Vec::new();
            for (index, oldest) in holding {
                let pointer = ledger.rows[index]
                    .pointer()
                    .expect("only pointers are walked");
                let status = if place.is_dir() {
                    check::status(pointer, &place, limit)
                } else {
                    Status::CouldNotRun("the ledger's directory did not exist yet".into())
                };
                match status {
                    Status::Holds | Status::Restates(_) => still.push((index, Some(step))),
                    Status::Differs { got, .. } => {
                        done.push((index, oldest, format!("the command showed {got}")));
                    }
                    Status::CouldNotRun(why) | Status::NotChecked(why) => {
                        done.push((index, oldest, why));
                    }
                }
            }
            holding = still;
        }
        let reached_oldest = holding.clone();
        done.extend(
            holding
                .into_iter()
                .map(|(i, oldest)| (i, oldest, String::new())),
        );

        for (index, oldest, why) in done {
            let row = &ledger.rows[index];
            let found = match oldest {
                None => Found::NotNow(why),
                Some(step) => {
                    let commit = commits[step];
                    let date = git(&root, &["show", "-s", "--format=%cs", commit])?;
                    let date = date
                        .parse::<Datetime>()
                        .ok()
                        .and_then(|d| d.date)
                        .ok_or_else(|| format!("git gave a date `ley` cannot read: {date}"))?;
                    Found::Since {
                        since: Since {
                            date,
                            sha: Some(commit.chars().take(12).collect()),
                        },
                        commits: step + 1,
                        oldest: reached_oldest.iter().any(|(i, _)| *i == index),
                    }
                }
            };
            entries.push(Entry {
                index,
                line: row.line,
                name: row.name.clone(),
                found,
            });
        }
    }

    entries.sort_by_key(|entry| entry.index);
    Ok(entries)
}

/// A second working tree of the repository, in a temporary directory.
/// The user's own working tree is never touched.
struct Tree {
    root: PathBuf,
    path: PathBuf,
    _holder: tempfile::TempDir,
}

impl Tree {
    fn add(root: &Path, commit: &str) -> Result<Tree, String> {
        let holder = tempfile::tempdir().map_err(|e| format!("no temporary directory: {e}"))?;
        let path = holder.path().join("tree");
        let at = path.to_string_lossy().into_owned();
        git(
            root,
            &["worktree", "add", "--detach", "--quiet", &at, commit],
        )?;
        Ok(Tree {
            root: root.to_path_buf(),
            path,
            _holder: holder,
        })
    }

    fn move_to(&self, commit: &str) -> Result<(), String> {
        git(&self.path, &["checkout", "--detach", "--quiet", commit]).map(|_| ())
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let at = self.path.to_string_lossy().into_owned();
        let _ = git(&self.root, &["worktree", "remove", "--force", &at]);
        let _ = git(&self.root, &["worktree", "prune"]);
    }
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("git could not start: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(format!(
            "git {}: {}",
            args.first().copied().unwrap_or(""),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}
