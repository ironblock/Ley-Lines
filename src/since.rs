//! `ley since`: find the earliest commit at which each row held.
//!
//! "Held" means the row's command, as written now, showed the number of results
//! the row says. `ley` walks back from the current commit, one commit at a time,
//! and stops at the first commit where the row does not hold. It does not bisect:
//! a fact can hold, break and hold again, and a bisect would then give the wrong
//! start.
//!
//! A recorded date is "the earliest known". It moves earlier when the walk finds an
//! earlier commit. It moves later only when the walk finds a commit, after the
//! recorded date, at which the row did not hold. A history that is cut short, or
//! squashed, does not show that, so the date stays.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use toml::value::{Date, Datetime};

use crate::check::{self, Status};
use crate::ledger::{Ledger, Since};

/// What the walk found for one row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Found {
    /// The row held at every commit from this one to the current one.
    Since {
        since: Since,
        /// How many commits the row held at, counting the current one.
        commits: usize,
        /// The newest commit at which the row did not hold. `None` when the row
        /// still held at the oldest commit in this history.
        broke: Option<Commit>,
    },
    /// The row's command ran at the current commit and did not show what the row says.
    NotNow(String),
    /// The row's command could not run at the current commit.
    CouldNotRun(String),
    /// The row's command fits no shape, so it was not run.
    NotChecked(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub sha: String,
    pub date: Date,
}

#[derive(Debug, Clone)]
pub struct Entry {
    /// The row's position in the ledger.
    pub index: usize,
    pub line: usize,
    pub name: String,
    pub found: Found,
}

#[derive(Debug, Clone)]
pub struct Walk {
    pub entries: Vec<Entry>,
    /// This clone holds only part of the history.
    pub shallow: bool,
}

/// What to do with the date a row carries, given what the walk found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// The row carried no date. Record this one.
    New(Since),
    /// The walk found an earlier commit. Record it.
    Earlier(Since),
    /// The row did not hold at a commit after the recorded date. Record the later start.
    Later { since: Since, broke: Commit },
    /// The recorded commit is not where this history starts to hold. Keep the date,
    /// and record the earliest commit here.
    Commit(Since),
    /// This clone holds only part of the history. Keep what is recorded.
    Kept,
    /// The row carries exactly what the walk found.
    Unchanged,
    /// The row does not hold now. Remove the date it carries.
    Removed,
    /// Nothing is recorded and nothing can be.
    Nothing,
}

impl Change {
    /// What to write into the ledger: `Some(None)` removes the date.
    pub fn update(&self) -> Option<Option<Since>> {
        match self {
            Change::New(since) | Change::Earlier(since) | Change::Commit(since) => {
                Some(Some(since.clone()))
            }
            Change::Later { since, .. } => Some(Some(since.clone())),
            Change::Removed => Some(None),
            Change::Kept | Change::Unchanged | Change::Nothing => None,
        }
    }
}

/// Decide what becomes of a row's recorded date.
pub fn decide(recorded: Option<&Since>, found: &Found, shallow: bool) -> Change {
    let (since, broke) = match found {
        Found::Since { since, broke, .. } => (since, broke),
        Found::NotNow(_) if recorded.is_some() => return Change::Removed,
        Found::NotNow(_) | Found::CouldNotRun(_) | Found::NotChecked(_) => return Change::Nothing,
    };
    let Some(recorded) = recorded else {
        return Change::New(since.clone());
    };
    if recorded == since {
        return Change::Unchanged;
    }
    if since.date < recorded.date {
        return Change::Earlier(since.clone());
    }
    // The walk starts later than the recorded date, or on the same day at another commit.
    match broke {
        Some(broke) if broke.date > recorded.date => Change::Later {
            since: since.clone(),
            broke: broke.clone(),
        },
        None if shallow => Change::Kept,
        _ => Change::Commit(Since {
            date: recorded.date,
            sha: since.sha.clone(),
        }),
    }
}

/// Walk the history and say, for each row with a command, since when it has held.
pub fn since(ledger: &Ledger, limit: Duration) -> Result<Walk, String> {
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
    let shallow = git(&root, &["rev-parse", "--is-shallow-repository"]).as_deref() == Ok("true");

    let mut entries = Vec::new();
    // The rows still holding, with the oldest step each has held at so far.
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
        // Each row that stopped: the oldest step it held at, the step it broke at, and why.
        let mut stopped: Vec<(usize, Option<usize>, usize, Status)> = Vec::new();
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
                    .expect("only rows with a command are walked");
                let status = if place.is_dir() {
                    check::status(pointer, &place, limit)
                } else {
                    Status::CouldNotRun("the ledger's directory did not exist yet".into())
                };
                match status {
                    Status::Holds | Status::Restates(_) => still.push((index, Some(step))),
                    other => stopped.push((index, oldest, step, other)),
                }
            }
            holding = still;
        }

        let date_of = |commit: &str| -> Result<Date, String> {
            let date = git(&root, &["show", "-s", "--format=%cs", commit])?;
            date.parse::<Datetime>()
                .ok()
                .and_then(|d| d.date)
                .ok_or_else(|| format!("git gave a date `ley` cannot read: {date}"))
        };
        let short = |commit: &str| -> String { commit.chars().take(12).collect() };
        let held_since = |step: usize| -> Result<Since, String> {
            Ok(Since {
                date: date_of(commits[step])?,
                sha: Some(short(commits[step])),
            })
        };

        for (index, oldest) in holding {
            let step = oldest.expect("a row that is still holding held at some step");
            entries.push(Entry {
                index,
                line: ledger.rows[index].line,
                name: ledger.rows[index].name.clone(),
                found: Found::Since {
                    since: held_since(step)?,
                    commits: step + 1,
                    broke: None,
                },
            });
        }
        for (index, oldest, broke_at, status) in stopped {
            let found = match (oldest, status) {
                (Some(step), _) => Found::Since {
                    since: held_since(step)?,
                    commits: step + 1,
                    broke: Some(Commit {
                        sha: short(commits[broke_at]),
                        date: date_of(commits[broke_at])?,
                    }),
                },
                (None, Status::Differs { got, .. }) => {
                    Found::NotNow(format!("the command showed {got}"))
                }
                (None, Status::CouldNotRun(why) | Status::NotChecked(why)) => {
                    Found::CouldNotRun(why)
                }
                (None, Status::Holds | Status::Restates(_)) => {
                    unreachable!("a row that stopped does not hold")
                }
            };
            entries.push(Entry {
                index,
                line: ledger.rows[index].line,
                name: ledger.rows[index].name.clone(),
                found,
            });
        }
    }

    entries.sort_by_key(|entry| entry.index);
    Ok(Walk { entries, shallow })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn day(text: &str) -> Date {
        text.parse::<Datetime>().unwrap().date.unwrap()
    }

    fn at(date: &str, sha: &str) -> Since {
        Since {
            date: day(date),
            sha: Some(sha.into()),
        }
    }

    fn found(date: &str, sha: &str, broke: Option<(&str, &str)>) -> Found {
        Found::Since {
            since: at(date, sha),
            commits: 1,
            broke: broke.map(|(date, sha)| Commit {
                sha: sha.into(),
                date: day(date),
            }),
        }
    }

    #[test]
    fn a_row_with_no_date_takes_what_the_walk_found() {
        let change = decide(None, &found("2026-03-01", "ccc", None), false);
        assert_eq!(change, Change::New(at("2026-03-01", "ccc")));
    }

    #[test]
    fn a_date_moves_earlier_when_more_history_is_read() {
        let recorded = at("2026-03-01", "ccc");
        let change = decide(Some(&recorded), &found("2026-01-01", "aaa", None), false);
        assert_eq!(change, Change::Earlier(at("2026-01-01", "aaa")));
    }

    #[test]
    fn a_date_does_not_move_later_because_a_clone_is_shallow() {
        let recorded = at("2026-01-01", "aaa");
        let change = decide(Some(&recorded), &found("2026-11-10", "zzz", None), true);
        assert_eq!(change, Change::Kept);
    }

    #[test]
    fn a_date_moves_later_only_past_a_commit_where_the_row_broke() {
        let recorded = at("2026-02-01", "bbb");
        let walk = found("2026-04-01", "ddd", Some(("2026-03-01", "ccc")));
        match decide(Some(&recorded), &walk, false) {
            Change::Later { since, broke } => {
                assert_eq!(since, at("2026-04-01", "ddd"));
                assert_eq!(broke.sha, "ccc");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_squashed_history_keeps_the_date_and_takes_the_new_commit() {
        // The recorded commit is gone. The row holds from the squashed commit, and
        // the commit before that is older than the recorded date: no break is shown.
        let recorded = at("2026-01-15", "gone");
        let walk = found("2026-05-01", "squash", Some(("2026-01-01", "aaa")));
        assert_eq!(
            decide(Some(&recorded), &walk, false),
            Change::Commit(at("2026-01-15", "squash"))
        );
    }

    #[test]
    fn a_row_that_does_not_hold_now_loses_its_date() {
        let recorded = at("2026-01-01", "aaa");
        let now = Found::NotNow("the command showed 0".into());
        assert_eq!(decide(Some(&recorded), &now, false), Change::Removed);
        assert_eq!(decide(None, &now, false), Change::Nothing);
    }

    #[test]
    fn a_row_that_could_not_run_keeps_what_it_has() {
        let recorded = at("2026-01-01", "aaa");
        let failed = Found::CouldNotRun("`rg` was not found on PATH".into());
        assert_eq!(decide(Some(&recorded), &failed, false), Change::Nothing);
    }
}
