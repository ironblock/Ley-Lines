//! `ley since`: find the earliest commit at which each row held.
//!
//! "Held" means the row's command, as written now, showed the number of results
//! the row says. `ley` walks back from the current commit, one commit at a time,
//! and stops at the first commit where the row does not hold. It does not bisect:
//! a fact can hold, break and hold again, and a bisect would then give the wrong
//! start.
//!
//! A recorded date is "the earliest known". It moves earlier when the walk finds an
//! earlier commit. It moves later in one case only: the recorded commit is in this
//! history, and the row did not hold at a commit that comes after it. A history
//! that is cut short, or squashed, or that does not have the recorded commit shows
//! no such thing, so the date stays.
//!
//! The walk registers nothing in the repository and runs none of its hooks. See `Tree`.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::Duration;

use toml::value::Date;

use crate::check::{self, Status};
use crate::ledger::{Ledger, Since};
use crate::run::Fault;
use crate::text::shown;

/// What the walk found for one row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Found {
    /// The row held at every commit from this one to the current one.
    Since {
        since: Since,
        /// How many commits the row held at, counting the current one.
        commits: usize,
        /// Why the walk went no further back.
        stopped: Stopped,
        /// Where the commit the row records sits, next to the commit the walk found.
        recorded: Recorded,
    },
    /// The row's command ran at the current commit and did not show what the row says.
    NotNow(String),
    /// The row's command could not run at the current commit.
    CouldNotRun(String),
    /// The row's command fits no shape, so it was not run.
    NotChecked(String),
}

/// Why the walk went no further back than the commit it found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stopped {
    /// The row did not hold at this commit, the one before.
    Broke(Commit),
    /// The row's command could not run at this commit. Nothing is known about this
    /// commit or any before it: this is not a commit where the row did not hold.
    CouldNotRun { at: Commit, why: String },
    /// This history has no commit before the one the walk found.
    NoOlderCommit,
}

/// Where the commit a row records sits in this history, next to the commit the
/// walk found. Git answers this from the shape of the history. Dates are not asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recorded {
    /// The row records no commit, or one this history does not have, or one that
    /// is on another line of it.
    Unplaced,
    /// The commit the walk found.
    Same,
    /// A commit that comes after the one the walk found.
    After,
    /// A commit that comes before the one the walk found. `broke_since` says whether
    /// the commit at which the row did not hold comes after it too. After a merge
    /// it may not: the recorded commit can sit on the line that was merged in.
    Before { broke_since: bool },
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
    /// The row did not hold at a commit after the recorded one. Record the later start.
    Later { since: Since, broke: Commit },
    /// The recorded commit cannot be placed in this history. Keep the date, and
    /// record the earliest commit here at which the row is known to hold.
    Commit(Since),
    /// Nothing shows the recorded date to be wrong, and nothing better was found.
    /// Keep what is recorded.
    Kept,
    /// The row records the commit the walk found.
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
    let (since, stopped, placed) = match found {
        Found::Since {
            since,
            stopped,
            recorded,
            ..
        } => (since, stopped, *recorded),
        Found::NotNow(_) if recorded.is_some() => return Change::Removed,
        Found::NotNow(_) | Found::CouldNotRun(_) | Found::NotChecked(_) => return Change::Nothing,
    };
    let Some(recorded) = recorded else {
        return Change::New(since.clone());
    };
    // After a squash a row keeps its date and takes the new commit. So a row may
    // carry a date that is older than its commit, and that is not a difference.
    if placed == Recorded::Same || recorded == since {
        return Change::Unchanged;
    }
    let keep_the_date = || Since {
        date: recorded.date,
        sha: since.sha.clone(),
    };
    match (placed, stopped) {
        // The one way a date moves later: the history has the recorded commit, and
        // shows a commit after it at which the row did not hold.
        (Recorded::Before { broke_since: true }, Stopped::Broke(broke)) => Change::Later {
            since: since.clone(),
            broke: broke.clone(),
        },
        // The recorded commit is in this history, and nothing shows a break after it.
        (Recorded::Before { .. }, _) => Change::Kept,
        // The walk reached further back than the recorded commit.
        (Recorded::After, _) if since.date <= recorded.date => Change::Earlier(since.clone()),
        (Recorded::After, _) => Change::Commit(keep_the_date()),
        // The recorded commit cannot be placed, so only the dates are left to compare.
        // They can show that the row held earlier. They cannot show that it broke.
        (_, _) if since.date < recorded.date => Change::Earlier(since.clone()),
        (_, Stopped::NoOlderCommit) if shallow => Change::Kept,
        (_, _) => Change::Commit(keep_the_date()),
    }
}

/// Walk the history and say, for each row with a command, since when it has held.
pub fn since(ledger: &Ledger, limit: Duration) -> Result<Walk, String> {
    let dir = ledger
        .dir()
        .canonicalize()
        .map_err(|e| format!("{}: {e}", ledger.dir().display()))?;
    let root = git(&dir, &["rev-parse", "--show-toplevel"]).map_err(|said| {
        format!(
            "`ley since` reads git history, and git could not read any in {}\n{said}",
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
    let commits = history(&root)?;
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
        let tree = Tree::new(&root)?;
        // Each row that stopped: the oldest step it held at, the step it stopped at, and why.
        let mut stopped: Vec<(usize, Option<usize>, usize, Status)> = Vec::new();
        for (step, (commit, _)) in commits.iter().enumerate() {
            if holding.is_empty() {
                break;
            }
            tree.show(commit)?;
            let place = tree.path.join(&below);
            let mut still = Vec::new();
            for (index, oldest) in holding {
                let pointer = ledger.rows[index]
                    .pointer()
                    .expect("only rows with a command are walked");
                let status = if place.is_dir() {
                    check::status(pointer, &place, limit)
                } else {
                    Status::CouldNotRun {
                        why: "the ledger's directory is not in this commit".into(),
                        fault: Fault::Path,
                    }
                };
                match status {
                    Status::Holds | Status::Restates(_) => still.push((index, Some(step))),
                    other => stopped.push((index, oldest, step, other)),
                }
            }
            holding = still;
        }

        let at = |step: usize| Commit {
            sha: commits[step].0.chars().take(12).collect(),
            date: commits[step].1,
        };
        let held_since = |step: usize| Since {
            date: commits[step].1,
            sha: Some(at(step).sha),
        };
        let mut graph = Graph::new(&root);
        // Where the commit that row `index` records sits, next to the commit at
        // `found`, given the step at which the row did not hold, if there is one.
        let mut placed = |index: usize, found: usize, broke: Option<usize>| -> Recorded {
            let recorded = ledger.rows[index]
                .pointer()
                .and_then(|pointer| pointer.since.as_ref()?.sha.as_deref());
            let Some(recorded) = recorded.and_then(|sha| graph.commit(sha)) else {
                return Recorded::Unplaced;
            };
            let found = commits[found].0.as_str();
            if recorded == found {
                Recorded::Same
            } else if graph.is_ancestor(found, &recorded) {
                Recorded::After
            } else if graph.is_ancestor(&recorded, found) {
                Recorded::Before {
                    broke_since: broke
                        .is_some_and(|step| graph.is_ancestor(&recorded, &commits[step].0)),
                }
            } else {
                Recorded::Unplaced
            }
        };

        for (index, oldest) in holding {
            let step = oldest.expect("a row that is still holding held at some step");
            entries.push(Entry {
                index,
                line: ledger.rows[index].line,
                name: ledger.rows[index].name.clone(),
                found: Found::Since {
                    since: held_since(step),
                    commits: step + 1,
                    stopped: Stopped::NoOlderCommit,
                    recorded: placed(index, step, None),
                },
            });
        }
        for (index, oldest, stopped_at, status) in stopped {
            let found = match (oldest, status) {
                // The program did not run at that commit. That says nothing about
                // the place, so it is not a commit where the row did not hold.
                (
                    Some(step),
                    Status::CouldNotRun {
                        why,
                        fault: Fault::DidNotRun,
                    },
                ) => Found::Since {
                    since: held_since(step),
                    commits: step + 1,
                    stopped: Stopped::CouldNotRun {
                        at: at(stopped_at),
                        why,
                    },
                    recorded: placed(index, step, None),
                },
                // The command showed something else, or a path it names was not there
                // yet, or the program said it failed.
                (Some(step), _) => Found::Since {
                    since: held_since(step),
                    commits: step + 1,
                    stopped: Stopped::Broke(at(stopped_at)),
                    recorded: placed(index, step, Some(stopped_at)),
                },
                (None, Status::Differs { got, .. }) => {
                    Found::NotNow(format!("the command showed {got}"))
                }
                // A path the row names is not in the current commit: the row cannot
                // hold there. Any other failure may be this machine's, an old
                // program for one, so it shows nothing about the row.
                (
                    None,
                    Status::CouldNotRun {
                        why,
                        fault: Fault::Path,
                    },
                ) => Found::NotNow(why),
                (None, Status::CouldNotRun { why, .. } | Status::NotChecked(why)) => {
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

/// The commits from the current one back along the first parent, each with the
/// day it was committed. Git is asked once, with a command whose output a user's
/// settings do not change.
fn history(root: &Path) -> Result<Vec<(String, Date)>, String> {
    let listed = match git(root, &["rev-list", "--first-parent", "--timestamp", "HEAD"]) {
        Ok(listed) => listed,
        Err(said) => {
            // With no commit on any branch, there is nothing wrong: nothing is there yet.
            let none_at_all =
                git(root, &["rev-list", "--max-count=1", "--all"]).is_ok_and(|any| any.is_empty());
            return Err(match none_at_all {
                true => "the repository has no commits yet".into(),
                false => format!("git could not list the commits of this repository\n{said}"),
            });
        }
    };
    let mut commits = Vec::new();
    for line in listed.lines() {
        let read = line.split_once(' ').and_then(|(seconds, commit)| {
            Some((commit.trim().to_string(), day(seconds.parse().ok()?)?))
        });
        commits.push(read.ok_or_else(|| format!("git listed a commit `ley` cannot read: {line}"))?);
    }
    Ok(commits)
}

/// The day, in UTC, of a time given in seconds since 1970.
///
/// Every commit's day is taken in UTC, whoever made it and wherever. Then two
/// days can be compared, and a later commit never has an earlier day because its
/// author lives further west.
fn day(seconds: i64) -> Option<Date> {
    // Days to a date in the calendar, after Howard Hinnant's `civil_from_days`.
    let days = seconds.div_euclid(86_400) + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    Some(Date {
        year: u16::try_from(year).ok()?,
        month: u8::try_from(month).ok()?,
        day: u8::try_from(day).ok()?,
    })
}

/// The files of one commit at a time, in a temporary directory.
///
/// Git puts them there through an index that belongs to the walk. So the walk
/// registers no working tree. It does not touch the user's files, index or HEAD.
/// It runs none of the user's hooks. A walk that is stopped half way leaves only
/// its temporary directory behind.
///
/// Git checks the files out the way it would for the user. So the filters that
/// the user's settings name for that do run, and git fetches what a commit needs
/// and the clone lacks: large files kept by git-lfs, files a partial clone left
/// out. Switching the filters off would give the walk other files than the user
/// has, and a row could then hold for the user and not for the walk.
struct Tree {
    root: PathBuf,
    /// Where the files are.
    path: PathBuf,
    index: PathBuf,
    /// An empty directory, given to git as the place to look for hooks.
    no_hooks: PathBuf,
    _holder: tempfile::TempDir,
}

impl Tree {
    fn new(root: &Path) -> Result<Tree, String> {
        let holder = tempfile::Builder::new()
            .prefix("ley-since-")
            .tempdir()
            .map_err(|e| format!("no temporary directory: {e}"))?;
        let path = holder.path().join("tree");
        let no_hooks = holder.path().join("no-hooks");
        // `rg` and `ast-grep` follow `.gitignore` files only inside a repository, and
        // they know a repository by its `.git`. This empty one makes them read the
        // files here the way they read a checkout.
        for dir in [&path.join(".git"), &no_hooks] {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        Ok(Tree {
            root: root.to_path_buf(),
            index: holder.path().join("index"),
            path,
            no_hooks,
            _holder: holder,
        })
    }

    /// Make the directory hold the files of `commit`, and no others.
    fn show(&self, commit: &str) -> Result<(), String> {
        let joined = |name: &str, path: &Path| {
            let mut arg = OsString::from(name);
            arg.push(path);
            arg
        };
        let mut command = git_command(&self.root);
        command
            // Writing an index runs the `post-index-change` hook. The walk runs no hooks.
            .arg("-c")
            .arg(joined("core.hooksPath=", &self.no_hooks))
            // These two would start a file watcher for the directory, or write a
            // shared index into the repository.
            .args(["-c", "core.fsmonitor=false", "-c", "core.splitIndex=false"])
            .arg(joined("--work-tree=", &self.path))
            .args(["read-tree", "--reset", "-u", commit])
            .env("GIT_INDEX_FILE", &self.index);
        finish(command, "read-tree").map(|_| ())
    }
}

/// The variables git sets to say which repository a command is about. Inside a git
/// hook they name the user's own repository and index. `ley` clears them, so that
/// every git command it runs is about the directory it names and nothing else.
///
/// This is the list as git 2.43 gives it. `git_place` also asks the git that is
/// installed, which may know of more.
pub const GIT_PLACE: [&str; 15] = [
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CONFIG",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT",
    "GIT_OBJECT_DIRECTORY",
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_IMPLICIT_WORK_TREE",
    "GIT_GRAFT_FILE",
    "GIT_INDEX_FILE",
    "GIT_NO_REPLACE_OBJECTS",
    "GIT_REPLACE_REF_BASE",
    "GIT_PREFIX",
    "GIT_SHALLOW_FILE",
    "GIT_COMMON_DIR",
];

/// Every variable to clear: the list above, and the list the installed git gives.
fn git_place() -> &'static [String] {
    static NAMES: OnceLock<Vec<String>> = OnceLock::new();
    NAMES.get_or_init(|| {
        let mut names: Vec<String> = GIT_PLACE.iter().map(|name| name.to_string()).collect();
        let mut command = Command::new("git");
        command
            .args(["rev-parse", "--local-env-vars"])
            .stdin(Stdio::null());
        for name in GIT_PLACE {
            command.env_remove(name);
        }
        if let Ok(output) = command.output() {
            for name in String::from_utf8_lossy(&output.stdout).lines() {
                let name = name.trim();
                if name.starts_with("GIT_") && !names.iter().any(|known| known == name) {
                    names.push(name.to_string());
                }
            }
        }
        names
    })
}

/// A git command about the repository in `dir`, with nothing on standard input.
fn git_command(dir: &Path) -> Command {
    let mut command = Command::new("git");
    command.arg("-C").arg(dir).stdin(Stdio::null());
    for name in git_place() {
        command.env_remove(name);
    }
    command
}

/// Run a git command and return what it printed, or what it said went wrong.
fn finish(mut command: Command, name: &str) -> Result<String, String> {
    let output = command
        .output()
        .map_err(|e| format!("git could not start: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        // What git says may quote a name from the repository. None of it is printed raw.
        let said = String::from_utf8_lossy(&output.stderr);
        let said: Vec<String> = said.trim().lines().map(shown).collect();
        Err(format!("git {name}: {}", said.join(" ")))
    }
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let mut command = git_command(dir);
    command.args(args);
    finish(command, args.first().copied().unwrap_or(""))
}

/// Questions about the shape of the history. Git is asked each one once.
struct Graph<'a> {
    root: &'a Path,
    commits: HashMap<String, Option<String>>,
    ancestors: HashMap<(String, String), bool>,
}

impl<'a> Graph<'a> {
    fn new(root: &'a Path) -> Graph<'a> {
        Graph {
            root,
            commits: HashMap::new(),
            ancestors: HashMap::new(),
        }
    }

    /// The full name of the commit a row records. `None` when this clone does not
    /// have it, or when the short name fits more than one commit.
    fn commit(&mut self, sha: &str) -> Option<String> {
        if let Some(known) = self.commits.get(sha) {
            return known.clone();
        }
        let name = format!("{sha}^{{commit}}");
        let full = git(self.root, &["rev-parse", "--verify", "--quiet", &name]).ok();
        self.commits.insert(sha.to_string(), full.clone());
        full
    }

    /// True when `older` is `newer` or one of its ancestors. False when it is not,
    /// and when git cannot say.
    fn is_ancestor(&mut self, older: &str, newer: &str) -> bool {
        let key = (older.to_string(), newer.to_string());
        if let Some(known) = self.ancestors.get(&key) {
            return *known;
        }
        let mut command = git_command(self.root);
        command
            .args(["merge-base", "--is-ancestor", older, newer])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let answer = command.status().ok().and_then(|status| status.code()) == Some(0);
        self.ancestors.insert(key, answer);
        answer
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toml::value::Datetime;

    fn date(text: &str) -> Date {
        text.parse::<Datetime>().unwrap().date.unwrap()
    }

    fn at(day: &str, sha: &str) -> Since {
        Since {
            date: date(day),
            sha: Some(sha.into()),
        }
    }

    fn broke(day: &str, sha: &str) -> Stopped {
        Stopped::Broke(Commit {
            sha: sha.into(),
            date: date(day),
        })
    }

    fn found(day: &str, sha: &str, stopped: Stopped, recorded: Recorded) -> Found {
        Found::Since {
            since: at(day, sha),
            commits: 1,
            stopped,
            recorded,
        }
    }

    const BROKE_SINCE: Recorded = Recorded::Before { broke_since: true };
    const MERGED_IN: Recorded = Recorded::Before { broke_since: false };

    #[test]
    fn a_day_is_read_in_utc() {
        for (seconds, day) in [
            (0, "1970-01-01"),
            (86_399, "1970-01-01"),
            (86_400, "1970-01-02"),
            (951_782_400, "2000-02-29"),
            (951_868_800, "2000-03-01"),
            (1_078_099_199, "2004-02-29"),
            (1_735_689_599, "2024-12-31"),
            (1_735_689_600, "2025-01-01"),
            // 2026-11-09 at 17:30 on the west coast of America is the 10th in UTC.
            (1_794_274_200, "2026-11-10"),
            (4_102_444_800, "2100-01-01"),
            (4_107_542_400, "2100-03-01"),
        ] {
            assert_eq!(super::day(seconds), Some(date(day)), "{seconds}");
        }
        assert_eq!(super::day(-1), Some(date("1969-12-31")));
    }

    #[test]
    fn a_row_with_no_date_takes_what_the_walk_found() {
        let walk = found(
            "2026-03-01",
            "ccc",
            Stopped::NoOlderCommit,
            Recorded::Unplaced,
        );
        assert_eq!(
            decide(None, &walk, false),
            Change::New(at("2026-03-01", "ccc"))
        );
    }

    #[test]
    fn a_date_moves_earlier_when_more_history_is_read() {
        let recorded = at("2026-03-01", "ccc");
        // The walk reached a commit that comes before the recorded one.
        let walk = found("2026-01-01", "aaa", Stopped::NoOlderCommit, Recorded::After);
        assert_eq!(
            decide(Some(&recorded), &walk, false),
            Change::Earlier(at("2026-01-01", "aaa"))
        );
        // An earlier commit on the recorded day is earlier too.
        let walk = found("2026-03-01", "bbb", Stopped::NoOlderCommit, Recorded::After);
        assert_eq!(
            decide(Some(&recorded), &walk, false),
            Change::Earlier(at("2026-03-01", "bbb"))
        );
        // With no recorded commit to place, an earlier day is enough.
        let walk = found(
            "2026-01-01",
            "aaa",
            Stopped::NoOlderCommit,
            Recorded::Unplaced,
        );
        assert_eq!(
            decide(Some(&recorded), &walk, false),
            Change::Earlier(at("2026-01-01", "aaa"))
        );
    }

    #[test]
    fn a_date_moves_later_only_past_a_commit_where_the_row_broke() {
        let recorded = at("2026-02-01", "bbb");
        let walk = found("2026-04-01", "ddd", broke("2026-03-01", "ccc"), BROKE_SINCE);
        match decide(Some(&recorded), &walk, false) {
            Change::Later { since, broke } => {
                assert_eq!(since, at("2026-04-01", "ddd"));
                assert_eq!(broke.sha, "ccc");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_break_on_the_recorded_day_still_moves_the_date_later() {
        // Recorded at commit aaa on the 10th. It broke later that same day, and held
        // again from the 12th. The history says the break came after aaa.
        let recorded = at("2026-01-10", "aaa");
        let walk = found("2026-01-12", "ccc", broke("2026-01-10", "bbb"), BROKE_SINCE);
        match decide(Some(&recorded), &walk, false) {
            Change::Later { since, .. } => assert_eq!(since, at("2026-01-12", "ccc")),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_commit_that_was_merged_in_is_kept() {
        // The row was recorded on a line that was later merged. On the line the walk
        // follows, the row does not hold before the merge. That commit does not come
        // after the recorded one, so nothing shows a break.
        let recorded = at("2026-01-10", "aaa");
        let walk = found("2026-01-20", "merge", broke("2026-01-15", "bbb"), MERGED_IN);
        assert_eq!(decide(Some(&recorded), &walk, false), Change::Kept);
    }

    #[test]
    fn a_date_does_not_move_later_because_a_clone_is_shallow() {
        let recorded = at("2026-01-01", "aaa");
        let walk = found(
            "2026-11-10",
            "zzz",
            Stopped::NoOlderCommit,
            Recorded::Unplaced,
        );
        assert_eq!(decide(Some(&recorded), &walk, true), Change::Kept);
    }

    #[test]
    fn a_squashed_history_keeps_the_date_and_takes_the_new_commit() {
        // The recorded commit is not in this history. The row holds from the squashed
        // commit. The commit before that is later than the recorded date, as it is
        // when the main line moved on before the squash. No break after the recorded
        // commit is shown, whatever the dates say.
        let recorded = at("2026-01-02", "gone");
        let walk = found(
            "2026-01-05",
            "squash",
            broke("2026-01-03", "main"),
            Recorded::Unplaced,
        );
        assert_eq!(
            decide(Some(&recorded), &walk, false),
            Change::Commit(at("2026-01-02", "squash"))
        );
        // The next walk finds the commit the row now records. Nothing changes again.
        let recorded = at("2026-01-02", "squash");
        let walk = found(
            "2026-01-05",
            "squash",
            broke("2026-01-03", "main"),
            Recorded::Same,
        );
        assert_eq!(decide(Some(&recorded), &walk, false), Change::Unchanged);
    }

    #[test]
    fn a_command_that_could_not_run_at_an_older_commit_is_not_a_break() {
        let could_not_run = || Stopped::CouldNotRun {
            at: Commit {
                sha: "bbb".into(),
                date: date("2026-02-01"),
            },
            why: "`rg` did not finish within 60 seconds".into(),
        };
        // The recorded commit is older than the commit the walk stopped at.
        let recorded = at("2026-01-01", "aaa");
        let walk = found("2026-03-01", "ccc", could_not_run(), MERGED_IN);
        assert_eq!(decide(Some(&recorded), &walk, false), Change::Kept);
        // What the walk did find is still worth recording on a row with no date.
        let walk = found("2026-03-01", "ccc", could_not_run(), Recorded::Unplaced);
        assert_eq!(
            decide(None, &walk, false),
            Change::New(at("2026-03-01", "ccc"))
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
