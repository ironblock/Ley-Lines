//! What the tests that run the built `ley` share.
//!
//! A test that needs a program skips itself when the program is missing, and says so.
//! Set LEY_TEST_REQUIRE_TOOLS=1 to make a missing program a failure instead.

// Each test file uses some of these, and none uses all.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub const LEY: &str = env!("CARGO_BIN_EXE_ley");

pub fn repository() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// True when every program is installed. Otherwise the test should return at once.
pub fn have(programs: &[&str]) -> bool {
    for program in programs {
        let found = Command::new(program).arg("--version").output().is_ok();
        if !found {
            if std::env::var_os("LEY_TEST_REQUIRE_TOOLS").is_some() {
                panic!("`{program}` is not installed, and LEY_TEST_REQUIRE_TOOLS is set");
            }
            eprintln!("skipped: `{program}` is not installed");
            return false;
        }
    }
    true
}

/// Take away what this machine and its owner have set for git, so that a test
/// sees the same thing everywhere. That includes the language git answers in.
fn without_personal_git_settings(command: &mut Command) -> &mut Command {
    command
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("LC_ALL", "C")
}

/// The built `ley`, about to run in `dir`.
pub fn ley_command(dir: &Path) -> Command {
    let mut command = Command::new(LEY);
    without_personal_git_settings(command.current_dir(dir));
    command
}

pub fn ley(dir: &Path, args: &[&str]) -> Output {
    ley_command(dir)
        .args(args)
        .output()
        .expect("the ley binary runs")
}

pub fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

pub fn copy(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// A private copy of the example, so a test can change it.
pub fn example() -> (tempfile::TempDir, PathBuf) {
    let holder = tempfile::tempdir().unwrap();
    let dir = holder.path().join("pipeline");
    copy(&repository().join("examples/pipeline"), &dir);
    (holder, dir)
}

/// Where a program is, as the shell finds it.
pub fn path_of(program: &str) -> String {
    let found = Command::new("sh")
        .args(["-c", &format!("command -v {program}")])
        .output()
        .unwrap();
    String::from_utf8_lossy(&found.stdout).trim().to_string()
}

/// A directory to use as PATH: it holds git, and the programs written into it.
/// Each program is a shell script.
#[cfg(unix)]
pub fn path_with_git_and(dir: &Path, programs: &[(&str, &str)]) {
    use std::os::unix::fs::PermissionsExt;
    fs::create_dir_all(dir).unwrap();
    std::os::unix::fs::symlink(path_of("git"), dir.join("git")).unwrap();
    for (name, script) in programs {
        let path = dir.join(name);
        fs::write(&path, script).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

/// A small repository with a known history.
pub struct History {
    _holder: tempfile::TempDir,
    pub dir: PathBuf,
}

impl History {
    pub fn new() -> History {
        let holder = tempfile::tempdir().unwrap();
        let dir = holder.path().join("place");
        fs::create_dir_all(dir.join("src")).unwrap();
        let history = History {
            _holder: holder,
            dir,
        };
        history.git(&["init", "--quiet", "--initial-branch=main"], "");
        history
    }

    /// Run git in this repository, as a committer with a fixed name, at `date`.
    pub fn git(&self, args: &[&str], date: &str) -> String {
        // Git here is about this test's own directory, whatever the environment says.
        let mut command = Command::new("git");
        for name in ley_lines::since::GIT_PLACE {
            command.env_remove(name);
        }
        without_personal_git_settings(&mut command);
        let output = command
            .args([
                "-c",
                "user.name=ley",
                "-c",
                "user.email=ley@example.invalid",
            ])
            .args(["-c", "commit.gpgsign=false"])
            // A test may clone this repository from its own directory.
            .args(["-c", "protocol.file.allow=always"])
            .args(args)
            .current_dir(&self.dir)
            .env("GIT_AUTHOR_DATE", date)
            .env("GIT_COMMITTER_DATE", date)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout)
            .trim_end()
            .to_string()
    }

    /// Write a file, commit everything on the given day, and return the commit.
    pub fn commit(&self, file: &str, text: &str, day: &str) -> String {
        fs::write(self.dir.join(file), text).unwrap();
        self.commit_all(day)
    }

    /// Commit everything as it is on the given day, and return the commit.
    pub fn commit_all(&self, day: &str) -> String {
        self.git(&["add", "--all"], "");
        // `day` is a day, or a day and a time.
        let when = match day.contains('T') {
            true => day.to_string(),
            false => format!("{day}T12:00:00+00:00"),
        };
        self.git(&["commit", "--quiet", "--message", day], &when);
        self.git(&["rev-parse", "HEAD"], "")
    }

    /// A copy of this repository as another person would fetch it: every commit that
    /// a branch leads to, and no others.
    pub fn clone_to(&self, name: &str, more: &[&str]) -> PathBuf {
        let to = self.dir.with_file_name(name);
        let url = format!("file://{}", self.dir.display());
        let mut args = vec!["clone", "--quiet"];
        args.extend(more);
        args.push(&url);
        let to_text = to.display().to_string();
        args.push(&to_text);
        self.git(&args, "");
        to
    }

    /// Everything git keeps for this repository apart from the objects, file by file,
    /// and what `git status` says. A program that only reads leaves both as they were.
    pub fn snapshot(&self) -> (BTreeMap<String, Vec<u8>>, Vec<String>) {
        fn read(dir: &Path, below: &str, files: &mut BTreeMap<String, Vec<u8>>) {
            for entry in fs::read_dir(dir).unwrap() {
                let entry = entry.unwrap();
                let name = format!("{below}{}", entry.file_name().to_string_lossy());
                if entry.file_type().unwrap().is_dir() {
                    if name != "objects" {
                        read(&entry.path(), &format!("{name}/"), files);
                    }
                } else {
                    files.insert(name, fs::read(entry.path()).unwrap());
                }
            }
        }
        let mut files = BTreeMap::new();
        read(&self.dir.join(".git"), "", &mut files);
        // `--no-optional-locks`: asking for the status must not itself write the index.
        let status = self.git(&["--no-optional-locks", "status", "--porcelain"], "");
        (files, status.lines().map(String::from).collect())
    }
}
