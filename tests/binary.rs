//! Run the built `ley` against real files, with the real `rg`, `ast-grep` and `git`.
//!
//! A test that needs a program skips itself when the program is missing, and says so.
//! Set LEY_TEST_REQUIRE_TOOLS=1 to make a missing program a failure instead.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use ley_lines::ledger;

const LEY: &str = env!("CARGO_BIN_EXE_ley");

fn repository() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// True when every program is installed. Otherwise the test should return at once.
fn have(programs: &[&str]) -> bool {
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

fn ley(dir: &Path, args: &[&str]) -> Output {
    Command::new(LEY)
        .args(args)
        .current_dir(dir)
        .output()
        .expect("the ley binary runs")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn copy(from: &Path, to: &Path) {
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
fn example() -> (tempfile::TempDir, PathBuf) {
    let holder = tempfile::tempdir().unwrap();
    let dir = holder.path().join("pipeline");
    copy(&repository().join("examples/pipeline"), &dir);
    (holder, dir)
}

#[test]
fn this_repositorys_own_ledger_holds() {
    if !have(&["rg", "ast-grep"]) {
        return;
    }
    let output = ley(repository(), &["check", "--strict"]);
    assert!(output.status.success(), "{}", stdout(&output));
}

#[test]
fn the_example_holds() {
    if !have(&["rg", "ast-grep"]) {
        return;
    }
    let (_holder, dir) = example();
    let output = ley(&dir, &["check", "--strict"]);
    assert!(output.status.success(), "{}", stdout(&output));
    assert_eq!(stdout(&output), "8 rows: 6 hold, 2 have no command.\n");
}

#[test]
fn a_moved_file_changes_nothing() {
    if !have(&["rg", "ast-grep"]) {
        return;
    }
    let (_holder, dir) = example();
    fs::create_dir_all(dir.join("services/billing/types")).unwrap();
    fs::rename(
        dir.join("services/quillon/stage.ts"),
        dir.join("services/billing/types/stage.ts"),
    )
    .unwrap();
    let output = ley(&dir, &["check", "--strict"]);
    assert!(output.status.success(), "{}", stdout(&output));
}

#[test]
fn a_changed_place_is_reported() {
    if !have(&["rg", "ast-grep"]) {
        return;
    }
    let (_holder, dir) = example();
    let pipeline = dir.join("pipeline.toml");
    let text = fs::read_to_string(&pipeline).unwrap();
    let without_publish = text.replace("[stage.publish]\nplatform = \"larkspur\"\n", "");
    assert_ne!(text, without_publish);
    fs::write(&pipeline, without_publish).unwrap();

    let output = ley(&dir, &["check"]);
    assert_eq!(output.status.code(), Some(1));
    let printed = stdout(&output);
    let expected = "\
ley.toml:6: differs: \"stage\": the row says 3 results, the command showed 2
    rg -n -F '[stage.' pipeline.toml
ley.toml:8: differs: \"platform\": the row says 3 results, the command showed 2
    rg -n '^platform' pipeline.toml
8 rows: 4 hold, 2 differ, 2 have no command.
";
    assert_eq!(printed, expected);

    // The README shows this output. Keep the two the same.
    let readme = fs::read_to_string(repository().join("README.md")).unwrap();
    assert!(
        readme.contains(expected),
        "README.md should show exactly what `ley check` prints here:\n{expected}"
    );
}

#[test]
fn a_renamed_thing_is_reported() {
    if !have(&["rg", "ast-grep"]) {
        return;
    }
    let (_holder, dir) = example();
    let file = dir.join("services/quillon/stage.ts");
    let text = fs::read_to_string(&file).unwrap();
    fs::write(&file, text.replace("interface Stage", "interface Step")).unwrap();
    let output = ley(&dir, &["check"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout(&output).contains("differs: \"stage\": the row says 1 result, the command showed 0"),
        "{}",
        stdout(&output)
    );
}

/// Two rows a small model wrote on its first try, with only `ley --help` to go on.
/// Both show the right number of results. Both also put the answer into `shows`,
/// where it would go out of date while the row went on holding.
#[test]
fn a_row_that_repeats_what_its_command_printed_is_reported() {
    if !have(&["rg", "ast-grep"]) {
        return;
    }
    let (_holder, dir) = example();
    let ledger = dir.join("ley.toml");
    let text = fs::read_to_string(&ledger).unwrap();
    let added = text.replace(
        "\n]\n",
        "\n  { name = \"Larkspur\", run = \"rg -n -F Larkspur docs/glossary.md\", shows = \"the glossary line that defines Larkspur as the streaming platform\", count = 1 },\n  { name = \"extensions\", run = \"rg -n -F packageManager extensions/package.json\", shows = \"the package manager the extensions directory declares: npm@10.8.0\", count = 1 },\n]\n",
    );
    assert_ne!(text, added);
    fs::write(&ledger, added).unwrap();

    let output = ley(&dir, &["check"]);
    assert_eq!(output.status.code(), Some(1));
    let printed = stdout(&output);
    assert!(
        printed.contains(
            "ley.toml:11: restates: \"Larkspur\": `shows` repeats `streaming platform`, which the command printed."
        ),
        "{printed}"
    );
    assert!(
        printed.contains(
            "ley.toml:12: restates: \"extensions\": `shows` repeats `npm@10.8.0`, which the command printed."
        ),
        "{printed}"
    );
    assert!(
        printed.ends_with("10 rows: 6 hold, 2 restate, 2 have no command.\n"),
        "{printed}"
    );
}

#[test]
fn the_program_describes_the_file_and_the_rules_itself() {
    let dir = repository();
    let help = stdout(&ley(dir, &["--help"]));
    for key in ledger::KEYS {
        assert!(
            help.contains(&format!("\n  {key}")) || help.contains(&format!(", {key} ")),
            "`ley --help` does not describe `{key}`"
        );
    }
    let rules = stdout(&ley(dir, &["rules"]));
    assert!(
        rules.starts_with("show, do not state\n    A row gives the command"),
        "{rules}"
    );
    assert!(rules.contains("\nno stored output\n"));
}

#[test]
fn a_missing_program_is_reported_and_never_passed_over() {
    let (holder, dir) = example();
    let empty = holder.path().join("no-programs");
    fs::create_dir(&empty).unwrap();
    let output = Command::new(LEY)
        .arg("check")
        .current_dir(&dir)
        .env("PATH", &empty)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let printed = stdout(&output);
    assert!(printed.contains("could not run: \"Quillon\": `rg` was not found on PATH"));
    assert!(printed.contains("could not run: \"stage\": `ast-grep` was not found on PATH"));
    assert!(printed.ends_with("8 rows: 6 could not run, 2 have no command.\n"));
}

#[test]
fn a_command_with_no_shape_is_shown_and_not_run() {
    let dir = repository().join("fixtures/valid");
    let loose = ley(&dir, &["--ledger", "command-with-no-shape.toml", "check"]);
    assert!(loose.status.success());
    assert!(stdout(&loose).contains("not checked: \"stage count\": `|` needs a shell"));
    let strict = ley(
        &dir,
        &[
            "--ledger",
            "command-with-no-shape.toml",
            "check",
            "--strict",
        ],
    );
    assert_eq!(strict.status.code(), Some(1));
    let shown = ley(
        &dir,
        &[
            "--ledger",
            "command-with-no-shape.toml",
            "show",
            "stage count",
        ],
    );
    assert!(stdout(&shown).contains("    rg -c -F '[stage.' pipeline.toml | sort\n"));
}

#[test]
fn a_ledger_that_breaks_a_rule_runs_nothing() {
    let dir = repository().join("fixtures/invalid");
    for command in ["check", "show"] {
        let output = ley(&dir, &["--ledger", "source-missing.toml", command]);
        assert_eq!(output.status.code(), Some(1));
        let printed = stdout(&output);
        assert!(printed.starts_with("source-missing.toml:4: \"Quillon\": `is` needs `said`"));
        assert!(printed.ends_with("1 problem in the ledger. Nothing was run.\n"));
    }
}

#[test]
fn with_no_ledger_ley_says_so() {
    let holder = tempfile::tempdir().unwrap();
    let output = ley(holder.path(), &["show"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("no ley.toml in"));
}

/// A small repository with a known history.
struct History {
    _holder: tempfile::TempDir,
    dir: PathBuf,
}

impl History {
    fn new() -> History {
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

    fn git(&self, args: &[&str], date: &str) -> String {
        let output = Command::new("git")
            .args([
                "-c",
                "user.name=ley",
                "-c",
                "user.email=ley@example.invalid",
            ])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .current_dir(&self.dir)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_DATE", date)
            .env("GIT_COMMITTER_DATE", date)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    /// Write a file, commit everything on the given day, and return the commit.
    fn commit(&self, file: &str, text: &str, day: &str) -> String {
        fs::write(self.dir.join(file), text).unwrap();
        self.git(&["add", "--all"], "");
        self.git(
            &["commit", "--quiet", "--message", day],
            &format!("{day}T12:00:00+00:00"),
        );
        self.git(&["rev-parse", "HEAD"], "")
    }
}

const HISTORY_LEDGER: &str = "\
row = [
  { name = \"alpha\", run = \"rg -n -F alpha src\", shows = \"the line with alpha\" },
  { name = \"beta\", run = \"rg -n -F beta src\", shows = \"the line with beta\" },
  { name = \"gamma\", run = \"rg -n -F gamma src\", shows = \"the line with gamma\" },
  { name = \"delta\", is = \"a word with no command\", said = [{ by = \"maintainer\", on = 2026-11-10 }] },
]
";

#[test]
fn since_walks_back_through_history_and_does_not_bisect() {
    if !have(&["rg", "git"]) {
        return;
    }
    let history = History::new();
    let first = history.commit("src/words.txt", "alpha\n", "2026-01-01");
    history.commit("src/words.txt", "alpha\nbeta\n", "2026-02-01");
    // beta holds, breaks, and holds again. Its run starts at the last of these.
    history.commit("src/words.txt", "alpha\n", "2026-03-01");
    let again = history.commit("src/words.txt", "alpha\nbeta\n", "2026-04-01");
    history.commit("ley.toml", HISTORY_LEDGER, "2026-05-01");

    let output = ley(&history.dir, &["since"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let printed = stdout(&output);
    let (first, again) = (&first[..12], &again[..12]);
    let expected = format!(
        "\
ley.toml:2: \"alpha\": true since at least 2026-01-01 (commit {first}, the oldest commit here): new
ley.toml:3: \"beta\": true since at least 2026-04-01 (commit {again}, held at 2 commits): new
ley.toml:4: \"gamma\": does not hold at the current commit (the command showed 0): nothing recorded
Recorded 2 rows in ley.toml.
"
    );
    assert_eq!(printed, expected);

    // The ledger now carries the dates, and nothing else in it changed.
    let text = fs::read_to_string(history.dir.join("ley.toml")).unwrap();
    let expected = HISTORY_LEDGER
        .replace(
            "shows = \"the line with alpha\" }",
            &format!("shows = \"the line with alpha\", since = 2026-01-01, sha = \"{first}\" }}"),
        )
        .replace(
            "shows = \"the line with beta\" }",
            &format!("shows = \"the line with beta\", since = 2026-04-01, sha = \"{again}\" }}"),
        );
    assert_eq!(text, expected);
    let rows = ledger::parse(&text).unwrap();
    let alpha = rows[0].pointer().unwrap().since.as_ref().unwrap();
    assert_eq!(alpha.date.to_string(), "2026-01-01");

    // `ley show` prints the date.
    let shown = stdout(&ley(&history.dir, &["show", "beta"]));
    assert!(shown.contains(&format!(
        "  True since at least 2026-04-01 (commit {again}).\n"
    )));

    // A second run finds the same thing and writes nothing.
    let output = ley(&history.dir, &["since"]);
    assert!(
        stdout(&output).ends_with("Nothing to record.\n"),
        "{}",
        stdout(&output)
    );
    assert_eq!(
        fs::read_to_string(history.dir.join("ley.toml")).unwrap(),
        text
    );

    // The walk used a temporary working tree. It is gone, and no other file changed.
    assert_eq!(history.git(&["worktree", "list"], "").lines().count(), 1);
    assert_eq!(history.git(&["status", "--porcelain"], ""), "M ley.toml");
}

#[test]
fn since_needs_a_git_repository() {
    if !have(&["git"]) {
        return;
    }
    let (_holder, dir) = example();
    let output = ley(&dir, &["since"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("is not in a git repository"));
}
