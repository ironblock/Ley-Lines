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
ley.toml:20: differs: \"stage\": the row says 3 results, the command showed 2
    rg -n -F '[stage.' pipeline.toml
ley.toml:31: differs: \"platform\": the row says 3 results, the command showed 2
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
    let added = format!(
        "{text}\n[[row]]\nname = \"Larkspur\"\nrun = \"rg -n -F Larkspur docs/glossary.md\"\nshows = \"the glossary line that defines Larkspur as the streaming platform\"\n\n[[row]]\nname = \"extensions\"\nrun = \"rg -n -F packageManager extensions/package.json\"\nshows = \"the package manager the extensions directory declares: npm@10.8.0\"\n"
    );
    fs::write(&ledger, added).unwrap();

    let output = ley(&dir, &["check"]);
    assert_eq!(output.status.code(), Some(1));
    let printed = stdout(&output);
    assert!(
        printed.contains(
            "restates: \"Larkspur\": `shows` repeats `streaming platform`, which the command printed."
        ),
        "{printed}"
    );
    assert!(
        printed.contains(
            "restates: \"extensions\": `shows` repeats `npm@10.8.0`, which the command printed."
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
        rules.starts_with("show, do not state\n    A row gives a command that shows the thing"),
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
        assert!(printed.starts_with("source-missing.toml:3: \"Quillon\": `is` needs `said`"));
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
        // Git here is about this test's own directory, whatever the environment says.
        let mut command = Command::new("git");
        for name in ley_lines::since::GIT_PLACE {
            command.env_remove(name);
        }
        let output = command
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
        // `day` is a day, or a day and a time.
        let when = match day.contains('T') {
            true => day.to_string(),
            false => format!("{day}T12:00:00+00:00"),
        };
        self.git(&["commit", "--quiet", "--message", day], &when);
        self.git(&["rev-parse", "HEAD"], "")
    }
}

const HISTORY_LEDGER: &str = "\
[[row]]
name = \"alpha\"
run = \"rg -n -F alpha src\"
shows = \"the line with alpha\"

[[row]]
name = \"beta\"
run = \"rg -n -F beta src\"
shows = \"the line with beta\"

[[row]]
name = \"gamma\"
run = \"rg -n -F gamma src\"
shows = \"the line with gamma\"

[[row]]
name = \"delta\"
is = \"a word with no command\"
said = [{ by = \"maintainer\", on = 2026-11-10 }]
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
ley.toml:1: \"alpha\": true since at least 2026-01-01 (commit {first}, the oldest commit here): new
ley.toml:6: \"beta\": true since at least 2026-04-01 (commit {again}, held at 2 commits): new
ley.toml:11: \"gamma\": does not hold at the current commit (the command showed 0): nothing recorded
Recorded 2 rows in ley.toml.
"
    );
    assert_eq!(printed, expected);

    // The ledger now carries the dates, as two new lines in each block. Nothing else changed.
    let text = fs::read_to_string(history.dir.join("ley.toml")).unwrap();
    let expected = HISTORY_LEDGER
        .replace(
            "shows = \"the line with alpha\"\n",
            &format!("shows = \"the line with alpha\"\nsince = 2026-01-01\nsha = \"{first}\"\n"),
        )
        .replace(
            "shows = \"the line with beta\"\n",
            &format!("shows = \"the line with beta\"\nsince = 2026-04-01\nsha = \"{again}\"\n"),
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

/// A ledger with one row, for the tests below. `fields` is the row's lines.
fn one_row(dir: &Path, fields: &str) {
    fs::write(dir.join("ley.toml"), format!("[[row]]\n{fields}\n")).unwrap();
}

// The next tests each come from a review of this program. Every case was reproduced
// before it was fixed.

#[test]
fn a_rule_whose_severity_is_error_is_counted_and_not_failed() {
    if !have(&["ast-grep"]) {
        return;
    }
    let (_holder, dir) = example();
    // ast-grep then ends with "Error: 1 error(s) found in code." That is a count of matches.
    one_row(
        &dir,
        r#"name = "interface"
run = "ast-grep scan --inline-rules '{id: r, language: ts, severity: error, rule: {kind: interface_declaration}}' services"
shows = "each interface under services""#,
    );
    let output = ley(&dir, &["check"]);
    assert_eq!(stdout(&output), "1 row: 1 holds.\n");
    assert!(output.status.success());
}

const QUILLON: &str = r#"name = "Quillon"
run = "rg -n -F Quillon docs/glossary.md"
shows = "the glossary line that defines Quillon""#;

#[test]
fn a_personal_ripgrep_configuration_changes_nothing() {
    if !have(&["rg"]) {
        return;
    }
    let (holder, dir) = example();
    let config = holder.path().join("ripgreprc");
    fs::write(&config, "--context=1\n--smart-case\n").unwrap();
    one_row(&dir, QUILLON);
    let output = Command::new(LEY)
        .arg("check")
        .current_dir(&dir)
        .env("RIPGREP_CONFIG_PATH", &config)
        .output()
        .unwrap();
    assert_eq!(stdout(&output), "1 row: 1 holds.\n");
}

#[test]
fn an_empty_line_that_rg_prints_is_a_result() {
    if !have(&["rg"]) {
        return;
    }
    let (_holder, dir) = example();
    fs::write(dir.join("blanks.txt"), "\n\n\n").unwrap();
    one_row(
        &dir,
        "name = \"blank\"\nrun = \"rg -x '' blanks.txt\"\nshows = \"each empty line of the file\"\ncount = 3",
    );
    assert_eq!(stdout(&ley(&dir, &["check"])), "1 row: 1 holds.\n");
}

#[test]
fn show_lists_a_name_once_whatever_its_capitals() {
    let holder = tempfile::tempdir().unwrap();
    fs::write(
        holder.path().join("ley.toml"),
        "[[row]]\nname = \"Stage\"\nrun = \"rg -n a src\"\nshows = \"a\"\n\n[[row]]\nname = \"stage\"\nrun = \"rg -n b src\"\nshows = \"b\"\n",
    )
    .unwrap();
    assert_eq!(
        stdout(&ley(holder.path(), &["show"])),
        "Stage  how to see it\n"
    );
}

/// A ledger with one row about `alpha`, and any further lines the row carries.
fn alpha_ledger(more: &str) -> String {
    format!(
        "[[row]]\nname = \"alpha\"\nrun = \"rg -n -F alpha src\"\nshows = \"the line with alpha\"\n{more}"
    )
}

/// The two lines `ley since` writes.
fn recorded(date: &str, commit: &str) -> String {
    format!("since = {date}\nsha = \"{}\"\n", &commit[..12])
}

#[test]
fn since_keeps_a_recorded_date_in_a_shallow_clone() {
    if !have(&["rg", "git"]) {
        return;
    }
    let history = History::new();
    let first = history.commit("src/words.txt", "alpha\n", "2026-01-01");
    history.commit("ley.toml", &alpha_ledger(""), "2026-02-01");
    assert!(ley(&history.dir, &["since"]).status.success());
    let text = fs::read_to_string(history.dir.join("ley.toml")).unwrap();
    assert_eq!(text, alpha_ledger(&recorded("2026-01-01", &first)));
    history.commit("ley.toml", &text, "2026-03-01");

    // A clone with one commit sees the row hold from 2026-03-01. That is no reason
    // to say the fact is newer than it is.
    let shallow = history.dir.with_file_name("shallow");
    let url = format!("file://{}", history.dir.display());
    let cloned = Command::new("git")
        .args(["clone", "--quiet", "--depth", "1", &url])
        .arg(&shallow)
        .output()
        .unwrap();
    assert!(
        cloned.status.success(),
        "{}",
        String::from_utf8_lossy(&cloned.stderr)
    );
    let output = ley(&shallow, &["since"]);
    assert_eq!(
        stdout(&output),
        "ley.toml:1: \"alpha\": the recorded date is kept: this clone holds only part of the history\nNothing to record.\n"
    );
    assert_eq!(fs::read_to_string(shallow.join("ley.toml")).unwrap(), text);
}

#[test]
fn since_moves_a_date_later_only_past_a_commit_where_the_row_broke() {
    if !have(&["rg", "git"]) {
        return;
    }
    let history = History::new();
    history.commit("src/words.txt", "beta\n", "2026-01-01");
    let was = history.commit("src/words.txt", "alpha\nbeta\n", "2026-02-01");
    let broke = history.commit("src/words.txt", "beta\n", "2026-03-01");
    let again = history.commit("src/words.txt", "alpha\nbeta\n", "2026-04-01");
    let ledger = alpha_ledger(&recorded("2026-02-01", &was));
    history.commit("ley.toml", &ledger, "2026-05-01");

    let printed = stdout(&ley(&history.dir, &["since"]));
    assert_eq!(
        printed,
        format!(
            "ley.toml:1: \"alpha\": true since at least 2026-04-01 (commit {}, held at 2 commits): moved later, because the row did not hold at commit {}\nRecorded 1 row in ley.toml.\n",
            &again[..12],
            &broke[..12]
        )
    );
    let text = fs::read_to_string(history.dir.join("ley.toml")).unwrap();
    assert_eq!(text, alpha_ledger(&recorded("2026-04-01", &again)));
}

#[test]
fn since_keeps_the_date_when_the_recorded_commit_is_gone() {
    if !have(&["rg", "git"]) {
        return;
    }
    // As after a squash: the recorded commit is not in this history, and the commit
    // before the row starts to hold is older than the recorded date.
    let history = History::new();
    history.commit("src/words.txt", "other\n", "2026-01-01");
    fs::write(history.dir.join("src/words.txt"), "alpha\n").unwrap();
    let ledger = alpha_ledger("since = 2026-01-15\nsha = \"0123456789ab\"\n");
    let squashed = history.commit("ley.toml", &ledger, "2026-05-01");

    let printed = stdout(&ley(&history.dir, &["since"]));
    assert_eq!(
        printed,
        format!(
            "ley.toml:1: \"alpha\": true since at least 2026-01-15: the date is kept, and the commit is now {}, the earliest one in this history\nRecorded 1 row in ley.toml.\n",
            &squashed[..12]
        )
    );
    let text = fs::read_to_string(history.dir.join("ley.toml")).unwrap();
    assert_eq!(text, alpha_ledger(&recorded("2026-01-15", &squashed)));
}

#[test]
fn since_removes_the_date_of_a_row_that_no_longer_holds() {
    if !have(&["rg", "git"]) {
        return;
    }
    let history = History::new();
    history.commit("src/words.txt", "other\n", "2026-01-01");
    let ledger = alpha_ledger("since = 2025-06-01\nsha = \"0123456789ab\"\n");
    history.commit("ley.toml", &ledger, "2026-02-01");

    let output = ley(&history.dir, &["since"]);
    assert!(output.status.success());
    assert_eq!(
        stdout(&output),
        "ley.toml:1: \"alpha\": does not hold at the current commit (the command showed 0): the recorded date is removed\nRecorded 1 row in ley.toml.\n"
    );
    let text = fs::read_to_string(history.dir.join("ley.toml")).unwrap();
    assert_eq!(text, alpha_ledger(""));
}

#[test]
fn since_says_so_when_a_program_could_not_run() {
    if !have(&["rg", "git"]) {
        return;
    }
    let history = History::new();
    history.commit("src/words.txt", "alpha\n", "2026-01-01");
    let ledger = alpha_ledger("since = 2026-01-01\nsha = \"0123456789ab\"\n");
    history.commit("ley.toml", &ledger, "2026-02-01");

    // A PATH that has git and nothing else.
    let bin = history.dir.with_file_name("only-git");
    fs::create_dir(&bin).unwrap();
    let git = Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .unwrap();
    let git = String::from_utf8_lossy(&git.stdout).trim().to_string();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&git, bin.join("git")).unwrap();
    let output = Command::new(LEY)
        .arg("since")
        .current_dir(&history.dir)
        .env("PATH", &bin)
        .output()
        .unwrap();
    assert_eq!(
        stdout(&output),
        "ley.toml:1: \"alpha\": could not run: `rg` was not found on PATH\nNothing to record.\n"
    );
    assert_eq!(output.status.code(), Some(1));
    // The date it had is left alone: nothing showed it to be wrong.
    assert_eq!(
        fs::read_to_string(history.dir.join("ley.toml")).unwrap(),
        ledger
    );
}

// The next tests come from a second review.

#[test]
fn since_run_from_a_git_hook_leaves_the_users_checkout_alone() {
    if !have(&["rg", "git"]) {
        return;
    }
    let history = History::new();
    history.commit("src/words.txt", "alpha\n", "2026-01-01");
    history.commit("src/words.txt", "alpha\nmore\n", "2026-02-01");
    history.commit("ley.toml", &alpha_ledger(""), "2026-03-01");
    let head = history.git(&["rev-parse", "HEAD"], "");

    // Git sets these when it runs a hook. They name the user's own repository.
    let output = Command::new(LEY)
        .args(["since", "--dry-run"])
        .current_dir(&history.dir)
        .env("GIT_DIR", history.dir.join(".git"))
        .env("GIT_INDEX_FILE", ".git/index")
        .env("GIT_PREFIX", "")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout(&output).contains("true since at least 2026-01-01"),
        "{}",
        stdout(&output)
    );

    assert_eq!(
        history.git(&["symbolic-ref", "--short", "HEAD"], ""),
        "main"
    );
    assert_eq!(history.git(&["rev-parse", "HEAD"], ""), head);
    assert_eq!(history.git(&["status", "--porcelain"], ""), "");
}

#[test]
fn since_sees_a_break_on_the_very_day_it_has_recorded() {
    if !have(&["rg", "git"]) {
        return;
    }
    let history = History::new();
    history.commit("src/words.txt", "other\n", "2026-01-09");
    let was = history.commit("src/words.txt", "alpha\n", "2026-01-10T08:00:00+00:00");
    let broke = history.commit("src/words.txt", "other\n", "2026-01-10T16:00:00+00:00");
    fs::write(history.dir.join("src/words.txt"), "alpha\n").unwrap();
    let ledger = alpha_ledger(&recorded("2026-01-10", &was));
    let again = history.commit("ley.toml", &ledger, "2026-01-12");

    let printed = stdout(&ley(&history.dir, &["since"]));
    assert_eq!(
        printed,
        format!(
            "ley.toml:1: \"alpha\": true since at least 2026-01-12 (commit {}, held at 1 commit): moved later, because the row did not hold at commit {}\nRecorded 1 row in ley.toml.\n",
            &again[..12],
            &broke[..12]
        )
    );
}

#[test]
fn a_personal_list_of_ignored_files_changes_nothing() {
    if !have(&["rg", "git"]) {
        return;
    }
    let history = History::new();
    history.commit("src/words.txt", "alpha\n", "2026-01-01");
    one_row(
        &history.dir,
        "name = \"alpha\"\nrun = \"rg -l -F alpha src\"\nshows = \"the files with alpha\"",
    );
    // A reader who ignores every .txt file, on their own machine only.
    let home = history.dir.with_file_name("home");
    fs::create_dir_all(home.join("config/git")).unwrap();
    fs::write(home.join("config/git/ignore"), "*.txt\n").unwrap();
    let personal = |command: &mut Command| {
        command
            .current_dir(&history.dir)
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", home.join("config"));
    };
    // With that list, the row's command shows that reader nothing...
    let mut alone = Command::new("rg");
    alone.args(["-l", "-F", "alpha", "src"]);
    personal(&mut alone);
    assert_eq!(String::from_utf8_lossy(&alone.output().unwrap().stdout), "");
    // ...and `ley check` still counts what the repository holds.
    let mut check = Command::new(LEY);
    check.arg("check");
    personal(&mut check);
    assert_eq!(stdout(&check.output().unwrap()), "1 row: 1 holds.\n");
}

#[test]
fn a_source_cannot_forge_a_second_row_when_it_is_printed() {
    let dir = repository().join("fixtures/invalid");
    let output = ley(
        &dir,
        &["--ledger", "source-on-two-lines.toml", "show", "Quillon"],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout(&output).contains("`by` is a name on one line"),
        "{}",
        stdout(&output)
    );
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
