//! Run the built `ley` against real files, with the real `rg`, `ast-grep` and `git`.
//!
//! A test that needs a program skips itself when the program is missing, and says so.
//! Set LEY_TEST_REQUIRE_TOOLS=1 to make a missing program a failure instead.

use std::fs;
use std::path::Path;
use std::process::Command;

use ley_lines::ledger;

mod common;
use common::{History, example, have, ley, ley_command, repository, stdout};

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
    let output = ley_command(&dir)
        .arg("check")
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

    // The walk registered no working tree, and no other file changed.
    assert_eq!(history.git(&["worktree", "list"], "").lines().count(), 1);
    assert_eq!(history.git(&["status", "--porcelain"], ""), " M ley.toml");
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
    let output = ley_command(&dir)
        .arg("check")
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
    let shallow = history.clone_to("shallow", &["--depth", "1"]);
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
            "ley.toml:1: \"alpha\": true since at least 2026-01-15: the date is kept, and the commit is now {}, the earliest one here at which the row is known to hold\nRecorded 1 row in ley.toml.\n",
            &squashed[..12]
        )
    );
    let text = fs::read_to_string(history.dir.join("ley.toml")).unwrap();
    assert_eq!(text, alpha_ledger(&recorded("2026-01-15", &squashed)));

    // From a third review: the next run must find nothing left to do. The row keeps
    // a date that is older than its commit, and that is not a difference.
    let printed = stdout(&ley(&history.dir, &["since"]));
    assert_eq!(
        printed,
        format!(
            "ley.toml:1: \"alpha\": true since at least 2026-01-15 (commit {}, held at 1 commit): unchanged\nNothing to record.\n",
            &squashed[..12]
        )
    );
    assert_eq!(
        fs::read_to_string(history.dir.join("ley.toml")).unwrap(),
        text
    );
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

#[cfg(unix)]
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
    common::path_with_git_and(&bin, &[]);
    let output = ley_command(&history.dir)
        .arg("since")
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
    let output = ley_command(&history.dir)
        .args(["since", "--dry-run"])
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
    let mut check = ley_command(&history.dir);
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
    // The reason is given in git's own words, whatever it is.
    let said = String::from_utf8_lossy(&output.stderr);
    assert!(
        said.starts_with("ley: `ley since` reads git history, and git could not read any in "),
        "{said}"
    );
    assert!(said.contains("not a git repository"), "{said}");
}

// The next tests come from a third review.

/// A ledger of several rows, for the tests below. Each entry is one row's lines.
fn rows(dir: &Path, rows: &[&str]) {
    let blocks: Vec<String> = rows.iter().map(|row| format!("[[row]]\n{row}\n")).collect();
    fs::write(dir.join("ley.toml"), blocks.join("\n")).unwrap();
}

#[test]
fn a_path_that_is_not_there_is_reported_and_never_counted_as_nothing() {
    if !have(&["rg", "ast-grep"]) {
        return;
    }
    // Given a path that does not exist, ast-grep prints nothing and exits the way it
    // does when it finds no match. A row that says `count = 0` must not hold for it.
    let (_holder, dir) = example();
    rows(
        &dir,
        &[
            "name = \"a\"\nrun = \"rg -n Stage nowhere\"\nshows = \"the lines that mention Stage\"\ncount = 0",
            "name = \"b\"\nrun = \"ast-grep run -p 'interface $A { $$$ }' nowhere\"\nshows = \"each interface\"\ncount = 0",
            "name = \"c\"\nrun = \"ast-grep scan --inline-rules '{id: r, language: ts, rule: {kind: interface_declaration}}' nowhere\"\nshows = \"each interface\"\ncount = 0",
            "name = \"d\"\nrun = \"ast-grep outline --type interface nowhere\"\nshows = \"each interface\"\ncount = 0",
        ],
    );
    let output = ley(&dir, &["check"]);
    let printed = stdout(&output);
    assert_eq!(output.status.code(), Some(1), "{printed}");
    assert!(printed.ends_with("4 rows: 4 could not run.\n"), "{printed}");
    for name in ["a", "b", "c", "d"] {
        assert!(
            printed.contains(&format!("could not run: \"{name}\": ")),
            "{printed}"
        );
    }
}

#[test]
fn a_rule_that_is_switched_off_matches_nothing() {
    if !have(&["ast-grep"]) {
        return;
    }
    // `ley` passes no flag that changes a rule's severity. One such flag would make
    // the "error(s) found" line go away, and would also switch this rule back on.
    let (_holder, dir) = example();
    one_row(
        &dir,
        r#"name = "interface"
run = "ast-grep scan --inline-rules '{id: r, language: ts, severity: off, rule: {kind: interface_declaration}}' services"
shows = "each interface under services"
count = 0"#,
    );
    assert_eq!(stdout(&ley(&dir, &["check"])), "1 row: 1 holds.\n");
}

#[cfg(unix)]
#[test]
fn a_second_run_that_fails_is_reported_and_not_passed_over() {
    if !have(&["ast-grep", "git"]) {
        return;
    }
    // For an ast-grep row `ley` counts JSON, then runs the command once more to see
    // the text a reader would see. This ast-grep fails on that second run.
    let (holder, dir) = example();
    let bin = holder.path().join("bin");
    let script = format!(
        "#!/bin/sh\ncase \" $* \" in\n*\" --json=stream \"*) exec '{}' \"$@\" ;;\n*) echo 'Error: no plain text today' >&2; exit 2 ;;\nesac\n",
        common::path_of("ast-grep")
    );
    common::path_with_git_and(&bin, &[("ast-grep", &script)]);
    one_row(
        &dir,
        "name = \"stage\"\nrun = \"ast-grep outline --type interface --match '^Stage$' services\"\nshows = \"the interface that every stage implements\"",
    );
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    let output = ley_command(&dir)
        .arg("check")
        .env("PATH", path)
        .output()
        .unwrap();
    assert_eq!(
        stdout(&output),
        "ley.toml:1: could not run: \"stage\": ast-grep said: Error: no plain text today\n    ast-grep outline --type interface --match '^Stage$' services\n1 row: 1 could not run.\n"
    );
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn a_key_cannot_forge_a_line_of_output() {
    let dir = repository().join("fixtures/invalid");
    let output = ley(
        &dir,
        &["--ledger", "unknown-key-that-forges-a-line.toml", "check"],
    );
    let printed = stdout(&output);
    assert_eq!(output.status.code(), Some(1));
    // One line for the problem and one for the count. The key added none, and no
    // character that a terminal would act on came through.
    assert_eq!(printed.lines().count(), 2, "{printed}");
    assert!(!printed.contains('\u{1b}'), "{printed:?}");
    assert!(
        printed.contains("`evil\\u{a}ley.toml:1: forged\\u{1b}[31m` is not a key a row has"),
        "{printed}"
    );
}

#[test]
fn since_gives_one_answer_whether_or_not_the_old_commit_is_still_there() {
    if !have(&["rg", "git"]) {
        return;
    }
    let history = History::new();
    history.commit("src/other.txt", "other\n", "2026-01-01");
    // A branch on which the row first holds, and where its date is recorded.
    history.git(&["switch", "--quiet", "--create", "work"], "");
    let on_branch = history.commit("src/words.txt", "alpha\n", "2026-01-02");
    // The main line moves on, then takes the branch as one squashed commit.
    history.git(&["switch", "--quiet", "main"], "");
    history.commit("src/more.txt", "more\n", "2026-01-03");
    history.git(&["merge", "--squash", "--quiet", "work"], "");
    let ledger = alpha_ledger(&recorded("2026-01-02", &on_branch));
    let squashed = history.commit("ley.toml", &ledger, "2026-01-05");
    history.git(&["branch", "--quiet", "-D", "work"], "");

    // This repository still has the branch's commit, though no branch leads to it.
    // A fresh clone does not have it. Both must say the same thing: the commit
    // before the squash is no sign that the row broke after it was recorded.
    let expected = format!(
        "ley.toml:1: \"alpha\": true since at least 2026-01-02: the date is kept, and the commit is now {}, the earliest one here at which the row is known to hold\n1 row would change.\n",
        &squashed[..12]
    );
    assert_eq!(
        stdout(&ley(&history.dir, &["since", "--dry-run"])),
        expected
    );
    let fresh = history.clone_to("fresh", &[]);
    assert!(
        Command::new("git")
            .args(["cat-file", "-e", &on_branch])
            .current_dir(&fresh)
            .env_remove("GIT_DIR")
            .output()
            .is_ok_and(|output| !output.status.success()),
        "the fresh clone should not have the branch's commit"
    );
    assert_eq!(stdout(&ley(&fresh, &["since", "--dry-run"])), expected);
}

#[test]
fn since_keeps_a_commit_that_was_merged_in() {
    if !have(&["rg", "git"]) {
        return;
    }
    let history = History::new();
    history.commit("src/other.txt", "other\n", "2026-01-01");
    history.git(&["switch", "--quiet", "--create", "work"], "");
    let on_branch = history.commit("src/words.txt", "alpha\n", "2026-01-02");
    history.git(&["switch", "--quiet", "main"], "");
    history.commit("src/more.txt", "more\n", "2026-01-03");
    history.git(
        &["merge", "--quiet", "--no-ff", "--message", "merge", "work"],
        "2026-01-04T12:00:00+00:00",
    );
    let ledger = alpha_ledger(&recorded("2026-01-02", &on_branch));
    history.commit("ley.toml", &ledger, "2026-01-05");

    // The walk follows the main line, where the row does not hold before the merge.
    // The recorded commit is on the line that was merged in. It is in this history,
    // and the commit where the row does not hold does not come after it.
    let output = ley(&history.dir, &["since"]);
    assert_eq!(
        stdout(&output),
        "ley.toml:1: \"alpha\": the recorded date is kept: the row is not seen to break after the commit it records\nNothing to record.\n"
    );
    assert!(output.status.success());
    assert_eq!(
        fs::read_to_string(history.dir.join("ley.toml")).unwrap(),
        ledger
    );
}

#[cfg(unix)]
#[test]
fn since_does_not_take_a_command_that_could_not_run_for_a_break() {
    if !have(&["rg", "git"]) {
        return;
    }
    let history = History::new();
    let first = history.commit("src/words.txt", "alpha\n", "2026-01-01");
    let second = history.commit("src/stop-here", "", "2026-02-01");
    fs::remove_file(history.dir.join("src/stop-here")).unwrap();
    history.commit("src/words.txt", "alpha\nmore\n", "2026-03-01");
    let ledger = alpha_ledger(&recorded("2026-01-01", &first));
    history.commit("ley.toml", &ledger, "2026-04-01");

    // An rg that is stopped before it can answer, at the one commit that has the marker.
    let bin = history.dir.with_file_name("bin");
    let script = format!(
        "#!/bin/sh\nif [ -e src/stop-here ]; then kill -9 $$; fi\nexec '{}' \"$@\"\n",
        common::path_of("rg")
    );
    common::path_with_git_and(&bin, &[("rg", &script)]);
    let output = ley_command(&history.dir)
        .arg("since")
        .env("PATH", &bin)
        .output()
        .unwrap();
    // The row held at every commit, so its date is right. Nothing showed otherwise:
    // the walk only could not look.
    assert_eq!(
        stdout(&output),
        format!(
            "ley.toml:1: \"alpha\": the recorded date is kept: the walk could not go further back\n    the walk stopped at commit {}, where the command could not run: `rg` was stopped by a signal\nNothing to record.\n",
            &second[..12]
        )
    );
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        fs::read_to_string(history.dir.join("ley.toml")).unwrap(),
        ledger
    );
}

#[test]
fn since_takes_a_path_that_was_not_there_yet_for_a_break() {
    if !have(&["rg", "git"]) {
        return;
    }
    // Before `src/deep` existed, rg says the path is not there. The program ran and
    // said so: at that commit the row did not hold.
    let history = History::new();
    history.commit("src/words.txt", "other\n", "2026-01-01");
    fs::create_dir_all(history.dir.join("src/deep")).unwrap();
    let made = history.commit("src/deep/words.txt", "alpha\n", "2026-02-01");
    one_row(
        &history.dir,
        "name = \"alpha\"\nrun = \"rg -n -F alpha src/deep\"\nshows = \"the line with alpha\"",
    );
    history.commit_all("2026-03-01");
    let output = ley(&history.dir, &["since"]);
    assert_eq!(
        stdout(&output),
        format!(
            "ley.toml:1: \"alpha\": true since at least 2026-02-01 (commit {}, held at 2 commits): new\nRecorded 1 row in ley.toml.\n",
            &made[..12]
        )
    );
    assert!(output.status.success());
}

/// A repository with a ledger, and with work in progress that the walk must not
/// disturb: a staged file, a changed file and a new file.
fn history_with_work_in_progress(commits: usize) -> History {
    let history = History::new();
    for n in 0..commits {
        let day = format!("2026-01-01T00:{:02}:{:02}+00:00", n / 60, n % 60);
        history.commit("src/words.txt", &format!("alpha\n{n}\n"), &day);
    }
    history.commit("ley.toml", &alpha_ledger(""), "2026-03-01");
    fs::write(history.dir.join("src/staged.txt"), "staged\n").unwrap();
    history.git(&["add", "src/staged.txt"], "");
    fs::write(history.dir.join("src/words.txt"), "alpha\nnot saved\n").unwrap();
    fs::write(history.dir.join("notes.txt"), "not tracked\n").unwrap();
    history
}

#[cfg(unix)]
#[test]
fn since_writes_nothing_into_the_repository_and_runs_none_of_its_hooks() {
    use std::os::unix::fs::PermissionsExt;
    if !have(&["rg", "git"]) {
        return;
    }
    let history = history_with_work_in_progress(3);
    // Hooks that leave a mark when git runs them.
    let log = history.dir.with_file_name("hooks-ran");
    for hook in ["post-checkout", "post-index-change"] {
        let path = history.dir.join(".git/hooks").join(hook);
        let script = format!("#!/bin/sh\necho {hook} >> '{}'\n", log.display());
        fs::write(&path, script).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let (files, status) = history.snapshot();
    assert_eq!(
        status,
        ["A  src/staged.txt", " M src/words.txt", "?? notes.txt"]
    );
    assert!(!log.exists(), "taking the snapshot ran a hook");

    let output = ley(&history.dir, &["since"]);
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

    assert!(
        !log.exists(),
        "`ley since` ran: {}",
        fs::read_to_string(&log).unwrap_or_default()
    );
    let (files_after, status_after) = history.snapshot();
    // The index, HEAD, the branches and every other file git keeps are the same,
    // byte for byte. No working tree was registered.
    assert_eq!(
        files.keys().collect::<Vec<_>>(),
        files_after.keys().collect::<Vec<_>>()
    );
    for (name, bytes) in &files {
        assert!(files_after[name] == *bytes, ".git/{name} changed");
    }
    // The user's work in progress is as it was, and the ledger is the one file that changed.
    assert_eq!(
        status_after,
        [
            " M ley.toml",
            "A  src/staged.txt",
            " M src/words.txt",
            "?? notes.txt"
        ]
    );
    assert_eq!(
        fs::read_to_string(history.dir.join("src/words.txt")).unwrap(),
        "alpha\nnot saved\n"
    );
}

#[cfg(unix)]
#[test]
fn a_since_that_is_stopped_half_way_leaves_nothing_in_the_repository() {
    if !have(&["rg", "git"]) {
        return;
    }
    let history = history_with_work_in_progress(3);
    let (files, status) = history.snapshot();
    // The walk's own temporary directory goes here, so the test can look for it.
    let temporary = history.dir.with_file_name("temporary");
    fs::create_dir(&temporary).unwrap();
    // An rg that never answers: it says which process it is, and waits. So the walk
    // is sure to be half way when it is stopped, however fast or slow the machine.
    let waiting = history.dir.with_file_name("rg-is-waiting");
    let bin = history.dir.with_file_name("bin");
    let script = format!(
        "#!/bin/sh\necho $$ > '{}'\nexec '{}' 600\n",
        waiting.display(),
        common::path_of("sleep")
    );
    common::path_with_git_and(&bin, &[("rg", &script)]);

    let mut child = ley_command(&history.dir)
        .arg("since")
        .env("PATH", &bin)
        .env("TMPDIR", &temporary)
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let rg = loop {
        let said = fs::read_to_string(&waiting).unwrap_or_default();
        if let Ok(process) = said.trim().parse::<u32>() {
            break process;
        }
        assert!(
            child.try_wait().unwrap().is_none(),
            "`ley since` ended before it started rg"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    // Stop `ley` the way a closed terminal or a full machine would: with no chance
    // to tidy up. Then stop the rg it left waiting.
    child.kill().unwrap();
    child.wait().unwrap();
    let _ = Command::new("kill").args(["-9", &rg.to_string()]).status();

    // What is left behind is in the temporary directory, and only there.
    let left_behind = fs::read_dir(&temporary)
        .unwrap()
        .flatten()
        .any(|entry| entry.path().join("tree/src/words.txt").is_file());
    assert!(left_behind, "the walk had put no files into its directory");
    assert_eq!(history.git(&["worktree", "list"], "").lines().count(), 1);
    assert!(!history.dir.join(".git/worktrees").exists());
    let (files_after, status_after) = history.snapshot();
    assert_eq!(
        files.keys().collect::<Vec<_>>(),
        files_after.keys().collect::<Vec<_>>()
    );
    for (name, bytes) in &files {
        assert!(files_after[name] == *bytes, ".git/{name} changed");
    }
    assert_eq!(status, status_after);
}

// The next tests come from a fourth review.

#[test]
fn show_reads_several_words_as_one_name() {
    // Most names are several words, and `ley show` lists them without quotes.
    let (_holder, dir) = example();
    let quoted = stdout(&ley(&dir, &["show", "package manager"]));
    assert_eq!(quoted.matches("If you run").count(), 1, "{quoted}");
    assert_eq!(stdout(&ley(&dir, &["show", "package", "manager"])), quoted);

    // "no" is part of "known" and "not". The rule's own rows are all that is printed.
    let rule = stdout(&ley(repository(), &["show", "no", "stored", "output"]));
    assert_eq!(
        rule,
        stdout(&ley(repository(), &["show", "no stored output"]))
    );
    assert!(
        rule.starts_with("no stored output is the rule that"),
        "{rule}"
    );
    assert!(!rule.contains("earliest known"), "{rule}");

    // When no name holds all the words, each word is a name of its own, and a row
    // that two of them select is printed once.
    let both = ley(&dir, &["show", "stage", "platform", "Stage"]);
    assert!(both.status.success());
    let both = stdout(&both);
    assert_eq!(
        both,
        format!(
            "{}\n{}",
            stdout(&ley(&dir, &["show", "stage"])),
            stdout(&ley(&dir, &["show", "platform"]))
        )
    );
}

#[test]
fn show_prints_each_source_on_a_line_of_its_own() {
    let shown = stdout(&ley(repository(), &["show", "ledger"]));
    assert!(
        shown.contains(
            "\n  Said by Claude Fable 5.2 on 2026-11-10.\n  Said by Corey Vixie on 2026-11-11.\n"
        ),
        "{shown}"
    );
}

#[cfg(unix)]
#[test]
fn a_link_that_leads_out_of_the_ledgers_directory_is_not_followed() {
    if !have(&["rg"]) {
        return;
    }
    let (holder, dir) = example();
    // A file outside the ledger's directory, and a link to its directory inside.
    let elsewhere = holder.path().join("elsewhere");
    fs::create_dir(&elsewhere).unwrap();
    fs::write(elsewhere.join("notes.txt"), "Stage\n").unwrap();
    std::os::unix::fs::symlink(&elsewhere, dir.join("outside")).unwrap();
    // A link that stays inside is an ordinary path.
    std::os::unix::fs::symlink(dir.join("docs"), dir.join("papers")).unwrap();
    rows(
        &dir,
        &[
            "name = \"out\"\nrun = \"rg -l -F Stage outside/notes.txt\"\nshows = \"the notes that mention a stage\"",
            "name = \"in\"\nrun = \"rg -l -F Quillon papers/glossary.md\"\nshows = \"the glossary\"",
        ],
    );
    let output = ley(&dir, &["check"]);
    assert_eq!(
        stdout(&output),
        "ley.toml:1: could not run: \"out\": `outside/notes.txt` leads out of the ledger's directory\n    rg -l -F Stage outside/notes.txt\n2 rows: 1 holds, 1 could not run.\n"
    );
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn since_removes_the_date_of_a_row_whose_path_is_gone() {
    if !have(&["rg", "git"]) {
        return;
    }
    // The row searches `src`, and a commit takes `src` away. That is not a program
    // that could not run: the row cannot hold, and its date goes, as it does for
    // a row whose match is gone.
    let history = History::new();
    let first = history.commit("src/words.txt", "alpha\n", "2026-01-01");
    fs::remove_dir_all(history.dir.join("src")).unwrap();
    history.commit(
        "ley.toml",
        &alpha_ledger(&recorded("2026-01-01", &first)),
        "2026-02-01",
    );

    let output = ley(&history.dir, &["since"]);
    assert_eq!(
        stdout(&output),
        "ley.toml:1: \"alpha\": does not hold at the current commit (`src` is not there): the recorded date is removed\nRecorded 1 row in ley.toml.\n"
    );
    assert!(output.status.success());
    assert_eq!(
        fs::read_to_string(history.dir.join("ley.toml")).unwrap(),
        alpha_ledger("")
    );
}

#[test]
fn since_says_when_it_did_not_check_a_date() {
    if !have(&["git"]) {
        return;
    }
    let history = History::new();
    history.commit("src/words.txt", "alpha\n", "2026-01-01");
    let ledger = "[[row]]\nname = \"alpha\"\nrun = \"rg -c -F alpha src | sort\"\nshows = \"how many lines hold alpha\"\nsince = 2020-01-01\nsha = \"0123456789ab\"\n";
    history.commit("ley.toml", ledger, "2026-02-01");
    let output = ley(&history.dir, &["since"]);
    assert_eq!(
        stdout(&output),
        "ley.toml:1: \"alpha\": not checked: `|` needs a shell, and `ley` starts none. The date this row carries was not checked either\nNothing to record.\n"
    );
    // The date stays: another version of `ley` may have been able to check it.
    assert_eq!(
        fs::read_to_string(history.dir.join("ley.toml")).unwrap(),
        ledger
    );
}
