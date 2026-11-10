//! `ley check`: run each row's command and compare what it shows with what the row says.

use std::path::Path;
use std::time::Duration;

use crate::command;
use crate::ledger::{Ledger, Pointer, Row};
use crate::run::{self, Outcome};
use crate::shape::{self, Plan};

/// What became of one row's command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// The command showed as many results as the row says.
    Holds,
    /// The command ran and showed a different number of results.
    Differs { got: u64, note: Option<String> },
    /// The row holds, and its `shows` repeats this text, which the command printed.
    Restates(String),
    /// The command fits a shape, and could not run or said it failed.
    CouldNotRun(String),
    /// The command fits no shape, so it was not run.
    NotChecked(String),
}

/// Fit a row's command to a shape, or say why it fits none.
pub fn fit(pointer: &Pointer) -> Result<Plan, String> {
    command::split(&pointer.run).and_then(|words| shape::plan(&words))
}

/// Run one pointer's command in `dir` and compare the count. Never returns `Restates`.
pub fn status(pointer: &Pointer, dir: &Path, limit: Duration) -> Status {
    match fit(pointer) {
        Ok(plan) => counted(pointer, &plan, dir, limit),
        Err(why) => Status::NotChecked(why),
    }
}

fn counted(pointer: &Pointer, plan: &Plan, dir: &Path, limit: Duration) -> Status {
    match run::run(plan, dir, limit) {
        Outcome::Count { results, .. } if results == pointer.count => Status::Holds,
        Outcome::Count { results, note } => Status::Differs { got: results, note },
        Outcome::Failed(why) => Status::CouldNotRun(why),
    }
}

/// Text in `shows` that the command printed, and that is not in the command or the name.
///
/// A row says what kind of thing its command shows. It does not say the thing.
/// "the package manager the directory declares" stays true when the version
/// changes. "the package manager the directory declares: npm@10.8.0" does not,
/// and the row would go on holding. Two kinds of repeat are found:
///
/// - a word that looks like a value or a name: it has a digit, a capital letter,
///   or one of `@ = / _` in it;
/// - two plain words in a row, each of four letters or more.
pub fn restated(pointer: &Pointer, name: &str, printed: &str) -> Option<String> {
    let allowed = format!("{} {name}", pointer.run).to_lowercase();
    let printed_lower = printed
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    let words: Vec<&str> = pointer
        .shows
        .split_whitespace()
        .map(|word| word.trim_matches(|c: char| !c.is_alphanumeric()))
        .collect();

    for word in &words {
        let looks_like_a_value = word.chars().count() >= 4
            && word.chars().any(|c| {
                c.is_ascii_digit() || c.is_uppercase() || matches!(c, '@' | '=' | '/' | '_')
            });
        if looks_like_a_value && !allowed.contains(&word.to_lowercase()) && has_word(printed, word)
        {
            return Some((*word).to_string());
        }
    }
    for pair in words.windows(2) {
        let plain = |word: &str| word.chars().count() >= 4 && word.chars().all(char::is_alphabetic);
        if !plain(pair[0]) || !plain(pair[1]) {
            continue;
        }
        // Words that are on the command line, or in the name, may be used freely.
        let on_the_command_line = pair
            .iter()
            .all(|word| allowed.contains(&word.to_lowercase()));
        let phrase = format!("{} {}", pair[0], pair[1]).to_lowercase();
        if !on_the_command_line && has_word(&printed_lower, &phrase) {
            return Some(format!("{} {}", pair[0], pair[1]));
        }
    }
    None
}

/// True when `needle` is in `text` with no letter or digit touching either end.
fn has_word(text: &str, needle: &str) -> bool {
    text.match_indices(needle).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + needle.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Tally {
    pub rows: usize,
    pub hold: usize,
    pub differ: usize,
    pub restate: usize,
    pub could_not_run: usize,
    pub not_checked: usize,
    pub no_command: usize,
}

impl Tally {
    /// True when every command that ran holds. With `strict`, also when none went unchecked.
    pub fn ok(&self, strict: bool) -> bool {
        self.differ == 0
            && self.restate == 0
            && self.could_not_run == 0
            && (!strict || self.not_checked == 0)
    }

    pub fn sentence(&self) -> String {
        let mut parts = Vec::new();
        let mut part = |n: usize, one: &str, many: &str| {
            if n > 0 {
                parts.push(format!("{n} {}", if n == 1 { one } else { many }));
            }
        };
        part(self.hold, "holds", "hold");
        part(self.differ, "differs", "differ");
        part(self.restate, "restates", "restate");
        part(self.could_not_run, "could not run", "could not run");
        part(self.not_checked, "not checked", "not checked");
        part(self.no_command, "has no command", "have no command");
        let rows = if self.rows == 1 { "row" } else { "rows" };
        if parts.is_empty() {
            format!("{} {rows}.", self.rows)
        } else {
            format!("{} {rows}: {}.", self.rows, parts.join(", "))
        }
    }
}

fn results(n: u64) -> String {
    format!("{n} {}", if n == 1 { "result" } else { "results" })
}

/// Check every row. Returns the lines to print, and the tally.
pub fn check(ledger: &Ledger, verbose: bool, limit: Duration) -> (Vec<String>, Tally) {
    let mut lines = Vec::new();
    let mut tally = Tally {
        rows: ledger.rows.len(),
        ..Tally::default()
    };
    let file = ledger.path.display();
    let dir = ledger.dir();
    for row in &ledger.rows {
        let Some(pointer) = row.pointer() else {
            tally.no_command += 1;
            continue;
        };
        let at = |word: &str| format!("{file}:{}: {word}: \"{}\"", row.line, row.name);
        let mut detail = vec![format!("    {}", pointer.run)];

        let status = match fit(pointer) {
            Err(why) => Status::NotChecked(why),
            Ok(plan) => match counted(pointer, &plan, dir, limit) {
                Status::Holds => {
                    // Compare `shows` with what a reader would see: the command as written.
                    let printed = run::printed(&plan, dir, limit);
                    match printed.and_then(|text| restated(pointer, &row.name, &text)) {
                        Some(text) => Status::Restates(text),
                        None => Status::Holds,
                    }
                }
                other => other,
            },
        };

        match status {
            Status::Holds => {
                tally.hold += 1;
                if verbose {
                    lines.push(format!("{} ({})", at("holds"), results(pointer.count)));
                }
                continue;
            }
            Status::Differs { got, note } => {
                tally.differ += 1;
                lines.push(format!(
                    "{}: the row says {}, the command showed {got}",
                    at("differs"),
                    results(pointer.count)
                ));
                detail.extend(note.map(|note| format!("    {note}")));
            }
            Status::Restates(text) => {
                tally.restate += 1;
                lines.push(format!(
                    "{}: `shows` repeats `{text}`, which the command printed. \
                     Say what kind of thing the command shows, and let the command show it",
                    at("restates")
                ));
            }
            Status::CouldNotRun(why) => {
                tally.could_not_run += 1;
                lines.push(format!("{}: {why}", at("could not run")));
            }
            Status::NotChecked(why) => {
                tally.not_checked += 1;
                lines.push(format!("{}: {why}", at("not checked")));
            }
        }
        lines.extend(detail);
    }
    (lines, tally)
}

/// The rows that have a command, with their position in the ledger.
pub fn pointers(ledger: &Ledger) -> impl Iterator<Item = (usize, &Row, &Pointer)> {
    ledger
        .rows
        .iter()
        .enumerate()
        .filter_map(|(index, row)| Some((index, row, row.pointer()?)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pointer(run: &str, shows: &str) -> Pointer {
        Pointer {
            run: run.into(),
            shows: shows.into(),
            count: 1,
            since: None,
        }
    }

    const MANIFEST: &str = "1:{ \"name\": \"extensions\", \"packageManager\": \"npm@10.8.0\" }\n";
    const GLOSSARY: &str = "5:- **Larkspur**: the streaming platform.\n";

    #[test]
    fn a_value_copied_into_shows_is_found() {
        let row = pointer(
            "rg -n -F packageManager extensions/package.json",
            "the package manager the extensions directory declares: npm@10.8.0",
        );
        assert_eq!(
            restated(&row, "extensions", MANIFEST).as_deref(),
            Some("npm@10.8.0")
        );
    }

    #[test]
    fn a_phrase_copied_into_shows_is_found() {
        let row = pointer(
            "rg -n -F Larkspur docs/glossary.md",
            "the glossary line that defines Larkspur as the streaming platform",
        );
        assert_eq!(
            restated(&row, "Larkspur", GLOSSARY).as_deref(),
            Some("streaming platform")
        );
    }

    #[test]
    fn saying_what_kind_of_thing_is_shown_is_fine() {
        for (run, shows, printed) in [
            (
                "rg -n -F packageManager extensions/package.json",
                "the package manager the extensions directory declares",
                MANIFEST,
            ),
            (
                "rg -n -F Larkspur docs/glossary.md",
                "the glossary line that defines Larkspur",
                GLOSSARY,
            ),
            // Words that are on the command line may be used freely.
            (
                "rg -n -F QUILLON-409 services",
                "the line that raises error QUILLON-409",
                "services/quillon/errors.ts:3:    super(`QUILLON-409: ledger out of balance`);\n",
            ),
            // One plain word in common is not a repeat.
            (
                "rg -n -F '[stage.' pipeline.toml",
                "the name of each stage of the billing run, in the order they run",
                "3:[stage.ingest]\n6:[stage.reconcile]\n9:[stage.publish]\n",
            ),
            // A count of nothing printed cannot be repeated.
            (
                "rg -l -F legacy services",
                "the files that mention legacy",
                "",
            ),
        ] {
            assert_eq!(
                restated(&pointer(run, shows), "x", printed),
                None,
                "{shows}"
            );
        }
    }

    #[test]
    fn two_words_that_are_both_on_the_command_line_are_not_a_repeat() {
        let row = pointer(
            "ast-grep outline --type interface --match '^Stage$' --view expanded services",
            "the file, line and fields of interface Stage",
        );
        let printed = "services/quillon/stage.ts\n3: export interface Stage {\n4:   name: string\n";
        assert_eq!(restated(&row, "stage", printed), None);
    }

    #[test]
    fn a_word_inside_a_longer_word_is_not_a_repeat() {
        let row = pointer("rg -n retries src", "the lines that set Retry");
        assert_eq!(restated(&row, "x", "4: RetryPolicy::default()\n"), None);
        assert_eq!(
            restated(&row, "x", "4: Retry::default()\n").as_deref(),
            Some("Retry")
        );
    }
}
