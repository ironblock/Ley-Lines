//! The README shows real output. These tests run the commands it shows and compare.
//! If you change what `ley` prints, the README changes in the same commit.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ley_lines::{command, rules};

fn repository() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn readme() -> String {
    fs::read_to_string(repository().join("README.md")).unwrap()
}

/// The fenced blocks of one language, each as its lines.
fn blocks(text: &str, language: &str) -> Vec<Vec<String>> {
    let mut found = Vec::new();
    let mut open: Option<Vec<String>> = None;
    for line in text.lines() {
        match &mut open {
            Some(lines) if line.trim_end() == "```" => {
                found.push(std::mem::take(lines));
                open = None;
            }
            Some(lines) => lines.push(line.to_string()),
            None if line.trim_end() == format!("```{language}") => open = Some(Vec::new()),
            None => {}
        }
    }
    found
}

fn installed(program: &str) -> bool {
    let found = Command::new(program).arg("--version").output().is_ok();
    if !found && std::env::var_os("LEY_TEST_REQUIRE_TOOLS").is_some() {
        panic!("`{program}` is not installed, and LEY_TEST_REQUIRE_TOOLS is set");
    }
    found
}

#[test]
fn console_blocks_show_what_ley_really_prints() {
    let have_tools = installed("rg") && installed("ast-grep");
    let mut compared = 0;
    for block in blocks(&readme(), "console") {
        let mut dir: PathBuf = repository().to_path_buf();
        let mut i = 0;
        while i < block.len() {
            let line = &block[i];
            i += 1;
            let Some(typed) = line.strip_prefix("$ ") else {
                assert!(
                    line.trim().is_empty(),
                    "output with no command above it: {line:?}"
                );
                continue;
            };
            let mut expected = Vec::new();
            while i < block.len() && !block[i].starts_with("$ ") {
                expected.push(block[i].as_str());
                i += 1;
            }
            let expected = expected.join("\n");
            let words = command::split(typed).unwrap();
            match words[0].as_str() {
                "cd" => {
                    dir = repository().join(&words[1]);
                    assert!(dir.is_dir(), "{typed}");
                    assert!(expected.trim().is_empty(), "`cd` prints nothing");
                }
                "ley" => {
                    let runs_programs = words.iter().any(|w| w == "check" || w == "since");
                    if runs_programs && !have_tools {
                        eprintln!("skipped: `{typed}` needs rg and ast-grep");
                        continue;
                    }
                    let output = Command::new(env!("CARGO_BIN_EXE_ley"))
                        .args(&words[1..])
                        .current_dir(&dir)
                        .output()
                        .unwrap();
                    let printed = String::from_utf8_lossy(&output.stdout);
                    assert_eq!(
                        printed.trim_end(),
                        expected.trim_end(),
                        "README.md: `{typed}`"
                    );
                    compared += 1;
                }
                other => panic!("README.md shows `{other}`, which this test cannot run"),
            }
        }
    }
    assert!(
        compared >= 3,
        "only {compared} commands in README.md were compared"
    );
}

#[test]
fn rows_quoted_in_the_readme_are_rows_of_a_real_ledger() {
    let ledgers = ["ley.toml", "examples/pipeline/ley.toml"]
        .map(|path| fs::read_to_string(repository().join(path)).unwrap())
        .join("\n");
    let real: BTreeSet<&str> = ledgers.lines().map(str::trim).collect();
    let mut quoted = 0;
    for block in blocks(&readme(), "toml") {
        for line in &block {
            let line = line.trim();
            // A line with an ellipsis is a sketch of the format, not a row.
            if !line.starts_with("{ name = ") || line.contains('…') {
                continue;
            }
            assert!(
                real.contains(line),
                "README.md quotes a row no ledger has:\n{line}"
            );
            quoted += 1;
        }
    }
    assert!(quoted >= 3);
}

#[test]
fn the_readme_and_the_ledger_name_the_same_rules() {
    let text = readme();
    let section = text
        .split("\n## The rules\n")
        .nth(1)
        .and_then(|rest| rest.split("\n## ").next())
        .expect("README.md has a section named `The rules`");
    let in_readme: BTreeSet<String> = section
        .lines()
        .filter_map(|line| {
            let (number, rest) = line.split_once(". **")?;
            number.parse::<u32>().ok()?;
            Some(rest.split_once(".**")?.0.to_string())
        })
        .collect();

    // `ley rules` prints these. They are read from ley.toml when `ley` is built.
    let in_ledger: BTreeSet<String> = rules::rules().into_iter().map(|(name, _)| name).collect();

    assert!(in_readme.len() >= 5);
    assert_eq!(in_readme, in_ledger);
    assert!(
        section.contains(&format!("follows {} rules", spell(in_ledger.len()))),
        "README.md should say how many rules there are: {}",
        in_ledger.len()
    );
}

fn spell(n: usize) -> String {
    const WORDS: [&str; 13] = [
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
        "eleven", "twelve",
    ];
    WORDS
        .get(n)
        .map_or_else(|| n.to_string(), |word| word.to_string())
}
