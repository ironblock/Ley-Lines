//! `ley show`: print what the ledger says about a name.
//!
//! The author of a row supplies the parts. `ley` supplies the sentence, and the
//! sentence is always a statement of fact: "If you run X, it will show you Y."
//! It never tells the reader to run anything.

use crate::ledger::{Ledger, Row};

/// The rows a query selects: every row with that name, or, if there is none,
/// every row whose name contains it. Letter case is ignored.
pub fn select<'a>(ledger: &'a Ledger, query: &str) -> Vec<&'a Row> {
    let query = query.to_lowercase();
    let exact: Vec<&Row> = ledger
        .rows
        .iter()
        .filter(|row| row.name.to_lowercase() == query)
        .collect();
    if !exact.is_empty() {
        return exact;
    }
    ledger
        .rows
        .iter()
        .filter(|row| row.name.to_lowercase().contains(&query))
        .collect()
}

/// One row, as sentences.
pub fn sentences(row: &Row) -> String {
    let mut out = String::new();
    if let Some(definition) = row.definition() {
        out.push_str(&format!("{} is {}.\n", row.name, definition.is));
        let said: Vec<String> = definition
            .said
            .iter()
            .map(|source| format!("by {} on {}", source.by, source.on))
            .collect();
        out.push_str(&format!("  Said {}.\n", said.join(", ")));
    }
    if let Some(pointer) = row.pointer() {
        if !out.is_empty() {
            out.push('\n');
        }
        let results = match pointer.count {
            1 => "1 result".to_string(),
            n => format!("{n} results"),
        };
        out.push_str(&format!(
            "If you run\n    {}\nit will show you {}: {results}.\n",
            pointer.run, pointer.shows
        ));
        if let Some(since) = &pointer.since {
            match &since.sha {
                Some(sha) => out.push_str(&format!(
                    "  True since at least {} (commit {sha}).\n",
                    since.date
                )),
                None => out.push_str(&format!("  True since at least {}.\n", since.date)),
            }
        }
    }
    out
}

/// Several rows, a blank line between them.
pub fn render(rows: &[&Row]) -> String {
    rows.iter()
        .map(|row| sentences(row))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every name in the ledger, once each, with what the ledger holds for it.
pub fn index(ledger: &Ledger) -> String {
    let mut names: Vec<(&str, bool, bool)> = Vec::new();
    for row in &ledger.rows {
        let what = row.definition().is_some();
        let how = row.pointer().is_some();
        match names.iter_mut().find(|(name, _, _)| *name == row.name) {
            Some(entry) => {
                entry.1 |= what;
                entry.2 |= how;
            }
            None => names.push((&row.name, what, how)),
        }
    }
    let width = names
        .iter()
        .map(|(name, _, _)| name.chars().count())
        .max()
        .unwrap_or(0);
    let mut out = String::new();
    for (name, what, how) in names {
        let holds = match (what, how) {
            (true, true) => "what it is, how to see it",
            (true, false) => "what it is",
            (false, _) => "how to see it",
        };
        let pad = " ".repeat(width - name.chars().count());
        out.push_str(&format!("{name}{pad}  {holds}\n"));
    }
    out
}
