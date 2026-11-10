//! The ledger: a TOML file of rows, and the rules a row must follow.
//!
//! On disk a row is one line: a flat table whose keys are all optional but `name`.
//! In memory a row is a sum type, so a row that says nothing cannot exist past
//! `parse`. What kind of row it is follows from the keys it carries. It is never
//! declared.

use std::path::{Path, PathBuf};

use toml::value::{Date, Datetime};
use toml::{Table, Value};

/// The file name `ley` looks for.
pub const FILE_NAME: &str = "ley.toml";

/// Every key a row may carry, in the order they are usually written.
pub const KEYS: [&str; 8] = [
    "name", "is", "said", "run", "shows", "count", "since", "sha",
];

/// Every rule `parse` enforces. Each has at least one file in `fixtures/invalid`.
pub const RULES: [&str; 13] = [
    "toml",
    "top-level",
    "one-line",
    "unknown-key",
    "type",
    "name",
    "text",
    "stored-output",
    "empty-row",
    "shows",
    "count",
    "source",
    "since",
];

#[derive(Debug, Clone)]
pub struct Ledger {
    pub path: PathBuf,
    pub rows: Vec<Row>,
}

impl Ledger {
    /// The directory every row's command runs in.
    pub fn dir(&self) -> &Path {
        match self.path.parent() {
            Some(dir) if !dir.as_os_str().is_empty() => dir,
            _ => Path::new("."),
        }
    }
}

/// One fact about one place.
#[derive(Debug, Clone)]
pub struct Row {
    /// The line of the ledger file the row is on, counted from 1.
    pub line: usize,
    /// The term a reader would look up. Several rows may share a name.
    pub name: String,
    pub body: Body,
}

/// What a row says. It says what the thing is, or how to see it, or both.
#[derive(Debug, Clone)]
pub enum Body {
    Definition(Definition),
    Pointer(Pointer),
    Both(Definition, Pointer),
}

impl Row {
    pub fn definition(&self) -> Option<&Definition> {
        match &self.body {
            Body::Definition(d) | Body::Both(d, _) => Some(d),
            Body::Pointer(_) => None,
        }
    }

    pub fn pointer(&self) -> Option<&Pointer> {
        match &self.body {
            Body::Pointer(p) | Body::Both(_, p) => Some(p),
            Body::Definition(_) => None,
        }
    }
}

/// What a thing is, and who said so. No command can contradict it, so it names its sources.
#[derive(Debug, Clone)]
pub struct Definition {
    pub is: String,
    pub said: Vec<Source>,
}

#[derive(Debug, Clone)]
pub struct Source {
    pub by: String,
    pub on: Date,
}

/// "If you run `run`, it will show you `shows`."
#[derive(Debug, Clone)]
pub struct Pointer {
    pub run: String,
    pub shows: String,
    /// How many results the command shows.
    pub count: u64,
    pub since: Option<Since>,
}

/// The earliest date, and commit, at which the pointer is known to have held.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Since {
    pub date: Date,
    pub sha: Option<String>,
}

/// One way a ledger breaks a rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub line: usize,
    /// The rule broken: one of `RULES`.
    pub rule: &'static str,
    pub message: String,
}

impl Problem {
    fn new(line: usize, rule: &'static str, message: impl Into<String>) -> Self {
        debug_assert!(RULES.contains(&rule));
        Problem {
            line,
            rule,
            message: message.into(),
        }
    }
}

#[derive(Debug)]
pub enum LoadError {
    /// The file could not be read.
    Unreadable(String),
    /// The file was read, and breaks rules.
    Problems(Vec<Problem>),
}

/// Look for `ley.toml` in `start`, then in each directory above it.
pub fn find(start: &Path) -> Option<PathBuf> {
    let mut dir = Some(start);
    while let Some(d) = dir {
        let candidate = d.join(FILE_NAME);
        if candidate.is_file() {
            return Some(candidate);
        }
        dir = d.parent();
    }
    None
}

pub fn load(path: &Path) -> Result<Ledger, LoadError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| LoadError::Unreadable(format!("{}: {e}", path.display())))?;
    let rows = parse(&text).map_err(LoadError::Problems)?;
    Ok(Ledger {
        path: path.to_path_buf(),
        rows,
    })
}

/// Read every row, or report every rule the text breaks.
pub fn parse(text: &str) -> Result<Vec<Row>, Vec<Problem>> {
    let table: Table = match toml::from_str(text) {
        Ok(table) => table,
        Err(e) => {
            let line = e.span().map_or(1, |span| line_at(text, span.start));
            let message = format!("this is not valid TOML: {}", e.message());
            return Err(vec![Problem::new(line, "toml", message)]);
        }
    };

    let mut problems = Vec::new();
    let layout = layout(text, &mut problems);

    for key in table.keys().filter(|key| *key != "row") {
        let line = line_starting_with(text, key).unwrap_or(1);
        problems.push(Problem::new(
            line,
            "top-level",
            format!("`{key}` is not a key a ledger has: the only top-level key is `row`"),
        ));
    }

    let mut rows = Vec::new();
    match table.get("row") {
        None => {}
        Some(Value::Array(values)) => {
            let opener = layout.opener.unwrap_or(1);
            if !layout.broken && layout.rows.len() != values.len() {
                problems.push(Problem::new(
                    opener,
                    "one-line",
                    format!(
                        "{} rows are written on {} lines: write each row on its own line, \
                         so that one search returns the whole row",
                        values.len(),
                        layout.rows.len()
                    ),
                ));
            }
            let lines_match = layout.rows.len() == values.len();
            for (i, value) in values.iter().enumerate() {
                let line = if lines_match { layout.rows[i] } else { opener };
                rows.extend(row(value, line, &mut problems));
            }
        }
        Some(_) => {
            let line = line_starting_with(text, "row").unwrap_or(1);
            problems.push(Problem::new(
                line,
                "top-level",
                "`row` is a list of rows: `row = [` on one line, then one row per line, then `]`",
            ));
        }
    }

    if problems.is_empty() {
        Ok(rows)
    } else {
        problems.sort_by_key(|p| p.line);
        Err(problems)
    }
}

struct Layout {
    /// The line that holds `row = [`.
    opener: Option<usize>,
    /// The line of each row, in order.
    rows: Vec<usize>,
    /// A layout problem was already reported.
    broken: bool,
}

/// Check that every row is one whole line, and note which line each row is on.
///
/// One row per line means one search for a name returns the whole row, and one
/// changed row is one changed line.
fn layout(text: &str, problems: &mut Vec<Problem>) -> Layout {
    let mut found = Layout {
        opener: None,
        rows: Vec::new(),
        broken: false,
    };
    let mut inside = false;
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let squeezed: String = trimmed.split_whitespace().collect();
        if inside {
            if trimmed == "]" {
                inside = false;
            } else if trimmed.starts_with('{')
                && (trimmed.ends_with("},") || trimmed.ends_with('}'))
            {
                found.rows.push(line);
            } else {
                found.broken = true;
                problems.push(Problem::new(
                    line,
                    "one-line",
                    "this line is not a whole row: a row starts with `{` and ends with `},` \
                     on the same line",
                ));
            }
        } else if squeezed == "row=[" {
            found.opener = Some(line);
            inside = true;
        } else if squeezed == "row=[]" {
            found.opener = Some(line);
        } else if squeezed.starts_with("[[row]]") || squeezed.starts_with("[row") {
            found.broken = true;
            problems.push(Problem::new(
                line,
                "one-line",
                "write rows as lines inside `row = [ … ]`, not as `[[row]]` blocks: \
                 then one search returns the whole row",
            ));
        } else if squeezed.starts_with("row=[") {
            found.broken = true;
            found.opener = Some(line);
            problems.push(Problem::new(
                line,
                "one-line",
                "put `row = [` on a line of its own, then one row per line, then `]`",
            ));
        }
    }
    found
}

fn row(value: &Value, line: usize, problems: &mut Vec<Problem>) -> Option<Row> {
    let before = problems.len();
    let Value::Table(table) = value else {
        problems.push(Problem::new(
            line,
            "type",
            "a row is a table on one line: `{ name = \"…\", … },`",
        ));
        return None;
    };

    let name = match table.get("name") {
        None => {
            problems.push(Problem::new(
                line,
                "name",
                "a row needs a `name`: the term a reader would look up",
            ));
            None
        }
        Some(Value::String(name)) if name.trim().is_empty() => {
            problems.push(Problem::new(line, "name", "`name` is empty"));
            None
        }
        Some(_) => text(table, "name", line, problems),
    };
    let label = name.clone().unwrap_or_default();
    let mut problem = |rule: &'static str, message: String| {
        let message = match label.as_str() {
            "" => message,
            label => format!("\"{label}\": {message}"),
        };
        problems.push(Problem::new(line, rule, message));
    };

    for key in table.keys().filter(|key| !KEYS.contains(&key.as_str())) {
        problem(
            "unknown-key",
            format!(
                "`{key}` is not a key a row has (a row has: {})",
                KEYS.join(", ")
            ),
        );
    }

    let mut local = Vec::new();
    let is = text(table, "is", line, &mut local);
    let run = text(table, "run", line, &mut local);
    let shows = text(table, "shows", line, &mut local);
    for p in local {
        problem(p.rule, p.message);
    }

    for (key, value) in [("is", &is), ("shows", &shows)] {
        let Some(value) = value else { continue };
        if value.ends_with('.') {
            problem(
                "text",
                format!("`{key}` ends with a full stop: `ley` adds it when it prints the sentence"),
            );
        }
        if let Some(found) = stored_output(value) {
            problem(
                "stored-output",
                format!(
                    "`{key}` holds `{found}`: a row does not store where a match was. \
                     The command finds it"
                ),
            );
        }
    }

    if let Some(shows) = &shows {
        let lower = shows.to_lowercase();
        if let Some(opening) = ["no ", "nothing", "that ", "whether "]
            .into_iter()
            .find(|opening| lower.starts_with(opening))
        {
            problem(
                "shows",
                format!(
                    "`shows` starts with \"{}\": it must name what the command looks for, \
                     so that \"it will show you …\" reads as a sentence. \
                     When the command finds none, `count = 0` says so",
                    opening.trim()
                ),
            );
        }
    }

    let count = match table.get("count") {
        None => None,
        Some(Value::Integer(n)) if *n >= 0 => Some(*n as u64),
        Some(Value::Integer(_)) => {
            problem("count", "`count` is below zero".into());
            None
        }
        Some(_) => {
            problem(
                "type",
                "`count` is a whole number, written without quotes".into(),
            );
            None
        }
    };

    let mut said = Vec::new();
    match table.get("said") {
        None => {}
        Some(Value::Array(sources)) if !sources.is_empty() => {
            for source in sources {
                match self::source(source) {
                    Ok(source) => said.push(source),
                    Err(message) => problem("source", message),
                }
            }
        }
        Some(_) => problem(
            "source",
            "`said` is a list of sources: `said = [{ by = \"…\", on = 2026-11-10 }]`".into(),
        ),
    }

    let since = match table.get("since") {
        None => None,
        Some(Value::Datetime(when)) => match date(when) {
            Some(date) => Some(date),
            None => {
                problem("since", "`since` is a date with no time: 2026-11-10".into());
                None
            }
        },
        Some(_) => {
            problem(
                "since",
                "`since` is a date, written without quotes: 2026-11-10".into(),
            );
            None
        }
    };
    let sha = match table.get("sha") {
        None => None,
        Some(Value::String(sha))
            if (7..=40).contains(&sha.len())
                && sha.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')) =>
        {
            Some(sha.clone())
        }
        Some(_) => {
            problem(
                "since",
                "`sha` is a commit: 7 to 40 characters, each one of 0-9 or a-f".into(),
            );
            None
        }
    };

    // What goes with what.
    let has = |key: &str| table.contains_key(key);
    if !has("is") && !has("run") {
        problem(
            "empty-row",
            "a row says what the thing is (`is`), or gives a command that shows it (`run`)".into(),
        );
    }
    if has("run") && !has("shows") {
        problem(
            "shows",
            "`run` needs `shows`: what the command will show the reader".into(),
        );
    }
    if has("shows") && !has("run") {
        problem(
            "shows",
            "`shows` needs `run`: the command that shows it".into(),
        );
    }
    if has("count") && !has("run") {
        problem(
            "count",
            "`count` needs `run`: it is how many results the command shows".into(),
        );
    }
    if has("is") && !has("said") {
        problem(
            "source",
            "`is` needs `said`: no command can contradict this sentence, \
             so the row names who said it, and when"
                .into(),
        );
    }
    if has("said") && !has("is") {
        problem(
            "source",
            "`said` needs `is`: the sentence the sources said".into(),
        );
    }
    if (has("since") || has("sha")) && !has("run") {
        problem(
            "since",
            "`since` and `sha` need `run`: they say how long the command has shown this".into(),
        );
    }
    if has("sha") && !has("since") {
        problem(
            "since",
            "`sha` needs `since`: the date of that commit".into(),
        );
    }

    if problems.len() > before {
        return None;
    }

    let definition = is.map(|is| Definition { is, said });
    let pointer = run.zip(shows).map(|(run, shows)| Pointer {
        run,
        shows,
        count: count.unwrap_or(1),
        since: since.map(|date| Since { date, sha }),
    });
    let body = match (definition, pointer) {
        (Some(d), Some(p)) => Body::Both(d, p),
        (Some(d), None) => Body::Definition(d),
        (None, Some(p)) => Body::Pointer(p),
        (None, None) => return None,
    };
    Some(Row {
        line,
        name: name?,
        body,
    })
}

/// Read a key that holds one line of text.
fn text(table: &Table, key: &str, line: usize, problems: &mut Vec<Problem>) -> Option<String> {
    let value = match table.get(key)? {
        Value::String(value) => value,
        _ => {
            problems.push(Problem::new(
                line,
                "type",
                format!("`{key}` is text, written in quotes"),
            ));
            return None;
        }
    };
    let fault = if value.is_empty() {
        Some("is empty")
    } else if value.trim() != value {
        Some("starts or ends with a space")
    } else if value.contains(['\n', '\r', '\t']) {
        Some("is more than one line")
    } else {
        None
    };
    match fault {
        Some(fault) => {
            problems.push(Problem::new(line, "text", format!("`{key}` {fault}")));
            None
        }
        None => Some(value.clone()),
    }
}

fn source(value: &Value) -> Result<Source, String> {
    const FORM: &str = "a source is `{ by = \"…\", on = 2026-11-10 }`: who said it, and when";
    let Value::Table(table) = value else {
        return Err(FORM.into());
    };
    if let Some(key) = table.keys().find(|key| *key != "by" && *key != "on") {
        return Err(format!("`{key}` is not a key a source has: {FORM}"));
    }
    let by = match table.get("by") {
        Some(Value::String(by)) if !by.trim().is_empty() && by.trim() == by => by.clone(),
        _ => return Err(FORM.into()),
    };
    let on = match table.get("on") {
        Some(Value::Datetime(when)) => date(when).ok_or(FORM)?,
        _ => return Err(FORM.into()),
    };
    Ok(Source { by, on })
}

/// A TOML date with no time and no offset.
fn date(when: &Datetime) -> Option<Date> {
    match (when.date, when.time, when.offset) {
        (Some(date), None, None) => Some(date),
        _ => None,
    }
}

/// Find text that records where a match was: `src/a.rs:12`, or `line 12`.
fn stored_output(text: &str) -> Option<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    for (i, word) in words.iter().enumerate() {
        let starts_with_digit = |s: &str| s.chars().next().is_some_and(|c| c.is_ascii_digit());
        if matches!(word.to_ascii_lowercase().as_str(), "line" | "lines")
            && words.get(i + 1).is_some_and(|next| starts_with_digit(next))
        {
            return Some(format!("{word} {}", words[i + 1]));
        }
        if word.contains("://") {
            continue;
        }
        for (at, _) in word.match_indices(':') {
            let (file, rest) = (&word[..at], &word[at + 1..]);
            if file.contains(['.', '/']) && starts_with_digit(rest) {
                return Some((*word).to_string());
            }
        }
    }
    None
}

fn line_at(text: &str, offset: usize) -> usize {
    let end = offset.min(text.len());
    text.as_bytes()[..end]
        .iter()
        .filter(|b| **b == b'\n')
        .count()
        + 1
}

fn line_starting_with(text: &str, key: &str) -> Option<usize> {
    text.lines()
        .position(|line| {
            let line = line.trim_start();
            line.strip_prefix(key)
                .is_some_and(|rest| rest.trim_start().starts_with(['=', '.', ']']))
                || line
                    .strip_prefix('[')
                    .is_some_and(|rest| rest.trim_start_matches('[').trim_start().starts_with(key))
        })
        .map(|index| index + 1)
}

/// Record `since` and `sha` on rows, changing nothing else in the file.
///
/// `updates` holds the position of each row in the ledger, and what to record.
pub fn record_since(text: &str, updates: &[(usize, Since)]) -> Result<String, String> {
    let mut doc: toml_edit::DocumentMut = text
        .parse()
        .map_err(|e| format!("the ledger is not valid TOML: {e}"))?;
    let rows = doc
        .get_mut("row")
        .and_then(|item| item.as_array_mut())
        .ok_or("the ledger has no `row` list")?;
    for (index, since) in updates {
        let row = rows
            .get_mut(*index)
            .and_then(|value| value.as_inline_table_mut())
            .ok_or_else(|| format!("row {} is not a table on one line", index + 1))?;
        let date: toml_edit::Datetime = since
            .date
            .to_string()
            .parse()
            .map_err(|e| format!("{}: {e}", since.date))?;
        row.insert("since", toml_edit::Value::from(date));
        match &since.sha {
            Some(sha) => {
                row.insert("sha", toml_edit::Value::from(sha.as_str()));
            }
            None => {
                row.remove("sha");
            }
        }
        row.fmt();
    }
    Ok(doc.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_output_is_found() {
        assert_eq!(
            stored_output("defined at src/a.rs:12").as_deref(),
            Some("src/a.rs:12")
        );
        assert_eq!(
            stored_output("see a.rs:12:3 for it").as_deref(),
            Some("a.rs:12:3")
        );
        assert_eq!(
            stored_output("it is on line 40").as_deref(),
            Some("line 40")
        );
        assert_eq!(stored_output("the run at 10:30 each night"), None);
        assert_eq!(
            stored_output("the page at https://example.org:8080/x"),
            None
        );
        assert_eq!(stored_output("the line each stage is declared on"), None);
        assert_eq!(stored_output("the type Foo::Bar"), None);
    }

    #[test]
    fn recording_since_changes_only_that_row() {
        let text = "# a ledger\nrow = [\n  { name = \"a\", run = \"rg x src\", shows = \"x\" },\n\n  # kept\n  { name = \"b\",   is = \"spaced\",  said = [{ by = \"m\", on = 2026-11-10 }] },\n]\n";
        let since = Since {
            date: "2026-11-09".parse::<Datetime>().unwrap().date.unwrap(),
            sha: Some("0123456789ab".into()),
        };
        let out = record_since(text, &[(0, since.clone())]).unwrap();
        let expected = text.replace(
            "shows = \"x\" }",
            "shows = \"x\", since = 2026-11-09, sha = \"0123456789ab\" }",
        );
        assert_eq!(out, expected);
        // Recording the same thing again changes nothing.
        assert_eq!(record_since(&out, &[(0, since)]).unwrap(), out);
        assert!(parse(&out).is_ok());
    }
}
