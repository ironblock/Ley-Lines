//! Every rule has fixtures: ledgers that follow it, and ledgers that break it.
//! Each broken ledger says, in its first lines, which rule it breaks and why.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use ley_lines::ledger::{self, RULES};
use ley_lines::{command, shape};

fn files(dir: &str) -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(dir);
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == "toml"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no fixtures in {}", dir.display());
    files
}

#[test]
fn valid_ledgers_break_no_rule() {
    for file in files("fixtures/valid") {
        let text = fs::read_to_string(&file).unwrap();
        if let Err(problems) = ledger::parse(&text) {
            panic!("{} should be valid, but: {problems:#?}", file.display());
        }
    }
}

/// The rule an invalid fixture names in its first line: `# breaks: <rule>`.
fn rule_named(file: &Path, text: &str) -> String {
    let first = text.lines().next().unwrap_or("");
    let rule = first.strip_prefix("# breaks: ").unwrap_or_else(|| {
        panic!(
            "{}: the first line must be `# breaks: <rule>`",
            file.display()
        )
    });
    let reason = text.lines().nth(1).unwrap_or("");
    assert!(
        reason.starts_with("# ") && reason.len() > 12,
        "{}: the second line must say why, in plain words",
        file.display()
    );
    rule.trim().to_string()
}

#[test]
fn invalid_ledgers_break_exactly_the_rule_they_name() {
    for file in files("fixtures/invalid") {
        let text = fs::read_to_string(&file).unwrap();
        let rule = rule_named(&file, &text);
        assert!(
            RULES.contains(&rule.as_str()),
            "{}: `{rule}` is not a rule",
            file.display()
        );
        let problems = ledger::parse(&text)
            .err()
            .unwrap_or_else(|| panic!("{} should break `{rule}`, but it is valid", file.display()));
        let broken: BTreeSet<&str> = problems.iter().map(|p| p.rule).collect();
        assert_eq!(
            broken,
            BTreeSet::from([rule.as_str()]),
            "{}: {problems:#?}",
            file.display()
        );
        for problem in &problems {
            assert!(problem.line >= 1 && problem.line <= text.lines().count());
        }
    }
}

#[test]
fn every_rule_has_a_fixture_that_breaks_it() {
    let covered: BTreeSet<String> = files("fixtures/invalid")
        .iter()
        .map(|file| rule_named(file, &fs::read_to_string(file).unwrap()))
        .collect();
    for rule in RULES {
        assert!(
            covered.contains(rule),
            "no fixture in fixtures/invalid breaks `{rule}`"
        );
    }
}

#[test]
fn a_problem_names_the_line_of_its_row() {
    let text = "row = [\n  { name = \"a\", run = \"rg a src\", shows = \"a\" },\n\n  # a comment\n  { name = \"b\" },\n]\n";
    let problems = ledger::parse(text).unwrap_err();
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].line, 5);
    assert_eq!(problems[0].rule, "empty-row");
}

#[test]
fn every_problem_in_a_ledger_is_reported_at_once() {
    let text = "row = [\n  { name = \"a\" },\n  { name = \"b\", run = \"rg b src\" },\n  { nam = \"c\" },\n]\n";
    let problems = ledger::parse(text).unwrap_err();
    let lines: Vec<usize> = problems.iter().map(|p| p.line).collect();
    assert_eq!(lines, [2, 3, 4, 4, 4]);
}

#[derive(serde::Deserialize)]
struct Commands {
    command: Vec<Case>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    run: String,
    argv: Option<Vec<String>>,
    refused: Option<String>,
    why: Option<String>,
}

fn commands() -> Vec<Case> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/commands.toml");
    toml::from_str::<Commands>(&fs::read_to_string(path).unwrap())
        .unwrap()
        .command
}

#[test]
fn commands_run_or_are_refused_as_the_fixture_says() {
    for case in commands() {
        let made = command::split(&case.run).and_then(|words| shape::plan(&words));
        match (&case.argv, &case.refused) {
            (Some(argv), None) => {
                let plan = made.unwrap_or_else(|why| panic!("{:?} was refused: {why}", case.run));
                assert_eq!(&plan.argv, argv, "{:?}", case.run);
            }
            (None, Some(refused)) => {
                let why = made
                    .err()
                    .unwrap_or_else(|| panic!("{:?} should be refused", case.run));
                assert!(
                    why.contains(refused),
                    "{:?} was refused with: {why}",
                    case.run
                );
                assert!(
                    case.why.as_ref().is_some_and(|w| w.len() > 12),
                    "{:?}: a refused command says why, in plain words",
                    case.run
                );
            }
            _ => panic!("{:?}: give `argv` or `refused`, not both", case.run),
        }
    }
}

/// `ley` and a real shell must split every command `ley` accepts into the same words.
/// Otherwise a row would mean one thing to `ley check` and another to its reader.
#[cfg(unix)]
#[test]
fn ley_splits_words_the_way_sh_does() {
    let mut cases: Vec<String> = commands().into_iter().map(|case| case.run).collect();
    cases.extend(
        [
            r#"rg "say \"hi\"" src"#,
            r#"rg "a\.b" src"#,
            r"rg it\'s src",
            r"rg 'it'\''s' src",
            "rg '' src",
            r#"rg "a b"'c d'e src"#,
            r#"rg "back\\slash" src"#,
            "rg 'tab\there' src",
        ]
        .map(String::from),
    );
    let mut compared = 0;
    for run in cases {
        let Ok(words) = command::split(&run) else {
            continue;
        };
        let script = format!("printf '%s\\0' {run}");
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(&script)
            .output()
            .expect("sh is needed for this test");
        assert!(output.status.success(), "sh refused {run:?}");
        let text = String::from_utf8(output.stdout).unwrap();
        let mut from_sh: Vec<&str> = text.split('\0').collect();
        assert_eq!(from_sh.pop(), Some(""), "{run:?}");
        assert_eq!(words, from_sh, "{run:?}");
        compared += 1;
    }
    assert!(compared >= 15, "only {compared} commands were compared");
}
