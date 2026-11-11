//! The ledger: a TOML file of rows, and the rules a row must follow.
//!
//! On disk a row is a `[[row]]` block, one field on each line. Every key is optional but `name`.
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
    "layout",
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
    /// The text the rows were read from.
    pub text: String,
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
        text,
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
            for (i, value) in values.iter().enumerate() {
                let line = layout.rows.get(i).copied().unwrap_or(layout.opener);
                rows.extend(row(value, line, &mut problems));
            }
        }
        Some(_) => {
            let line = line_starting_with(text, "row").unwrap_or(1);
            problems.push(Problem::new(
                line,
                "top-level",
                "rows are `[[row]]` blocks: a line `[[row]]`, then one field on each line",
            ));
        }
    }

    if problems.is_empty() {
        Ok(rows)
    } else {
        problems.sort_by_key(|p| p.line);
        problems.dedup();
        Err(problems)
    }
}

struct Layout {
    /// The line the first row starts on.
    opener: usize,
    /// The line each row starts on, in order.
    rows: Vec<usize>,
}

/// Check that rows are written as blocks, and note which line each row starts on.
///
/// A row is a `[[row]]` block with one field on each line. Then a change to one
/// field is one changed line, a review comment can sit on one field, and two
/// people can change two fields of a row without a conflict. TOML itself puts each
/// field of a block on its own line. The other way to write a list of tables, all
/// of a row's fields on one line, is refused. The positions come from the parser.
fn layout(text: &str, problems: &mut Vec<Problem>) -> Layout {
    let mut found = Layout {
        opener: 1,
        rows: Vec::new(),
    };
    let Ok(document) = toml_edit::Document::parse(text) else {
        return found;
    };
    let Some(item) = document.as_table().get("row") else {
        return found;
    };
    if let Some(blocks) = item.as_array_of_tables() {
        for block in blocks.iter() {
            let line = block.span().map_or(1, |span| line_at(text, span.start));
            found.rows.push(line);
        }
        found.opener = found.rows.first().copied().unwrap_or(1);
    } else if let Some(array) = item.as_array() {
        found.opener = array.span().map_or(1, |span| line_at(text, span.start));
        if !array.is_empty() {
            problems.push(Problem::new(
                found.opener,
                "layout",
                "write each row as a `[[row]]` block, with one field on each line: \
                 then a change to one field is one changed line",
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
            "a row is a `[[row]]` block, with one field on each line",
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
    } else if value.chars().any(char::is_control) {
        // A new line would let a row forge a second row when it is printed. Other
        // control characters can rewrite what a terminal shows.
        Some("is more than one line, or holds a control character")
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
        Some(Value::String(by))
            if !by.trim().is_empty() && by.trim() == by && !by.chars().any(char::is_control) =>
        {
            by.clone()
        }
        Some(Value::String(_)) => {
            return Err(
                "`by` is a name on one line, with no space at either end and no control character"
                    .into(),
            );
        }
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

/// File endings that mark a word as a file, so that `stage.ts:12` is a position
/// and `billing.internal:8443` or the image tag `billing/api:2` is not. The list
/// cannot be complete. A missed ending lets a position through; it never refuses
/// honest text.
const FILE_ENDINGS: [&str; 46] = [
    "c", "cc", "cfg", "cjs", "conf", "cpp", "cs", "css", "cts", "dart", "go", "h", "hpp", "html",
    "ini", "java", "js", "json", "jsx", "kt", "lock", "lua", "md", "mjs", "mts", "php", "proto",
    "py", "rb", "rs", "scala", "sh", "sql", "svelte", "swift", "tf", "toml", "ts", "tsx", "txt",
    "vue", "xml", "yaml", "yml", "zig", "zsh",
];

/// Find text that records where a match was: `src/a.rs:12`, or `on line 12`.
fn stored_output(text: &str) -> Option<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let starts_with_digit = |s: &str| s.chars().next().is_some_and(|c| c.is_ascii_digit());
    let lower = |i: usize| words.get(i).map(|word| word.to_ascii_lowercase());
    for (i, word) in words.iter().enumerate() {
        // "on line 12" and "line 12 of stage.ts" are positions. "the line 2 team" is not.
        if matches!(lower(i).as_deref(), Some("line" | "lines"))
            && words.get(i + 1).is_some_and(|next| starts_with_digit(next))
        {
            let before = i.checked_sub(1).and_then(lower);
            if matches!(before.as_deref(), Some("on" | "at" | "from"))
                || matches!(lower(i + 2).as_deref(), Some("of" | "in"))
            {
                return Some(format!("{word} {}", words[i + 1]));
            }
        }
        if word.contains("://") {
            continue;
        }
        for (at, _) in word.match_indices(':') {
            let (file, rest) = (&word[..at], &word[at + 1..]);
            let ending = file
                .rsplit_once('.')
                .map(|(_, end)| end.to_ascii_lowercase());
            let is_a_file = ending.is_some_and(|end| FILE_ENDINGS.contains(&end.as_str()));
            if is_a_file && starts_with_digit(rest) {
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

/// Record `since` and `sha` on rows, or remove them, changing nothing else in the file.
///
/// `updates` holds the position of each row in the ledger, and what to record.
/// `None` removes both keys. Only those two lines of a block change: every other
/// line, the comments and a byte-order mark stay as they were. A new line is
/// written the way the block's other fields are, lined up or not.
pub fn record_since(text: &str, updates: &[(usize, Option<Since>)]) -> Result<String, String> {
    use std::ops::Range;

    let document = toml_edit::Document::parse(text)
        .map_err(|e| format!("the ledger is not valid TOML: {e}"))?;
    let rows = document
        .as_table()
        .get("row")
        .and_then(|item| item.as_array_of_tables())
        .ok_or("the ledger has no `[[row]]` blocks")?;
    let line_start = |at: usize| text[..at].rfind('\n').map_or(0, |i| i + 1);
    let line_end = |at: usize| text[at..].find('\n').map_or(text.len(), |i| at + i);

    // Each edit replaces one span of the text.
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    for (index, since) in updates {
        let row = rows
            .get(*index)
            .ok_or_else(|| format!("the ledger has no row {}", index + 1))?;
        // The fields of the block in the order they are written, with where each key
        // and each value sits in the text.
        let mut entries: Vec<(&str, Range<usize>, Range<usize>)> = Vec::new();
        for (name, _) in row.iter() {
            let spans = row
                .get_key_value(name)
                .and_then(|(key, value)| Some((key.span()?, value.span()?)));
            let (key, value) = spans.ok_or("the parser did not say where a key sits")?;
            entries.push((name, key, value));
        }
        let place = |name: &str| entries.iter().position(|(key, _, _)| *key == name);
        // A field's whole line, with the line break after it.
        let whole_line = |i: usize| {
            let end = line_end(entries[i].2.end);
            line_start(entries[i].1.start)..(end + 1).min(text.len())
        };
        match since {
            Some(since) => {
                let mut added = Vec::new();
                let date = since.date.to_string();
                match place("since") {
                    Some(i) => edits.push((entries[i].2.clone(), date)),
                    None => added.push(("since", date)),
                }
                match (&since.sha, place("sha")) {
                    (Some(sha), Some(i)) => {
                        edits.push((entries[i].2.clone(), format!("\"{sha}\"")));
                    }
                    (Some(sha), None) => added.push(("sha", format!("\"{sha}\""))),
                    (None, Some(i)) => edits.push((whole_line(i), String::new())),
                    (None, None) => {}
                }
                if !added.is_empty() {
                    let last = entries.iter().map(|(_, _, value)| value.end).max();
                    let at = line_end(last.ok_or("a row has no fields")?);
                    // Write the new lines the way the block writes its fields: the same
                    // indent, and the same column for `=` when the block lines them up.
                    let first = &entries[0];
                    let indent = &text[line_start(first.1.start)..first.1.start];
                    let column = |(_, key, value): &(&str, Range<usize>, Range<usize>)| {
                        let equals = text[key.end..value.start].find('=')? + key.end;
                        Some(equals - line_start(key.start))
                    };
                    let lined_up = match column(first) {
                        Some(c)
                            if entries.len() > 1
                                && entries.iter().all(|e| column(e) == Some(c)) =>
                        {
                            Some(c)
                        }
                        _ => None,
                    };
                    let mut lines = String::new();
                    for (name, value) in added {
                        let width = lined_up.map_or(0, |c| c.saturating_sub(indent.len()));
                        match width > name.len() {
                            true => lines.push_str(&format!("\n{indent}{name:<width$}= {value}")),
                            false => lines.push_str(&format!("\n{indent}{name} = {value}")),
                        }
                    }
                    edits.push((at..at, lines));
                }
            }
            None => {
                for name in ["since", "sha"] {
                    if let Some(i) = place(name) {
                        edits.push((whole_line(i), String::new()));
                    }
                }
            }
        }
    }

    // Work from the end of the text back, so earlier spans stay where they were.
    edits.sort_by_key(|(span, _)| std::cmp::Reverse((span.start, span.end)));
    let mut out = text.to_string();
    for (span, with) in edits {
        out.replace_range(span, &with);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_output_is_found() {
        for (text, found) in [
            ("defined at src/a.rs:12", Some("src/a.rs:12")),
            ("see a.rs:12:3 for it", Some("a.rs:12:3")),
            ("it is on line 40", Some("line 40")),
            (
                "the interface declared on line 3 of stage.ts",
                Some("line 3"),
            ),
            ("line 12 of the glossary", Some("line 12")),
            // Not positions:
            ("the run at 10:30 each night", None),
            ("the page at https://example.org:8080/x", None),
            ("the proxy that listens on billing.internal:8443", None),
            ("the image published as billing/api:2", None),
            ("the host 10.0.0.5:443", None),
            ("the ratio v1.2:3", None),
            ("the line each stage is declared on", None),
            ("the line 2 escalation team", None),
            ("the type Foo::Bar", None),
        ] {
            assert_eq!(stored_output(text).as_deref(), found, "{text}");
        }
    }

    #[test]
    fn a_row_takes_its_line_from_the_parser() {
        // A byte-order mark, a comment, and a blank line do not shift the count.
        let text = "\u{feff}# first\n\n[[row]]\nname = \"a\"\n";
        let problems = parse(text).unwrap_err();
        assert_eq!((problems[0].line, problems[0].rule), (3, "empty-row"));
    }

    const TWO_ROWS: &str = "# a ledger\n[[row]]\nname = \"a\"\nrun = \"rg x src\"\nshows = \"x\"\n\n# kept\n[[row]]\nname  = \"b\"\nis    = \"lined up\"\nsaid  = [{ by = \"m\", on = 2026-11-10 }]\n";

    fn since(date: &str, sha: &str) -> Since {
        Since {
            date: date.parse::<Datetime>().unwrap().date.unwrap(),
            sha: Some(sha.into()),
        }
    }

    #[test]
    fn recording_since_adds_two_lines_and_changes_nothing_else() {
        let first = [(0, Some(since("2026-11-09", "0123456789ab")))];
        let out = record_since(TWO_ROWS, &first).unwrap();
        let expected = TWO_ROWS.replace(
            "shows = \"x\"\n",
            "shows = \"x\"\nsince = 2026-11-09\nsha = \"0123456789ab\"\n",
        );
        assert_eq!(out, expected);
        // Recording the same thing again changes nothing.
        assert_eq!(record_since(&out, &first).unwrap(), out);
        assert!(parse(&out).is_ok());
        // A later date and another commit replace only those two values.
        let later = [(0, Some(since("2026-12-01", "ba9876543210")))];
        let moved = record_since(&out, &later).unwrap();
        let expected = out
            .replace("2026-11-09", "2026-12-01")
            .replace("0123456789ab", "ba9876543210");
        assert_eq!(moved, expected);
        // Removing it gives the first file back.
        assert_eq!(record_since(&moved, &[(0, None)]).unwrap(), TWO_ROWS);
    }

    #[test]
    fn recording_since_follows_the_way_a_block_is_written() {
        // The second block lines up its `=` signs. A byte-order mark stays.
        let lined_up = TWO_ROWS.replace(
            "is    = \"lined up\"\nsaid  = [{ by = \"m\", on = 2026-11-10 }]",
            "run   = \"rg y src\"\nshows = \"y\"",
        );
        let text = format!("\u{feff}{lined_up}");
        let update = [(1, Some(since("2026-11-09", "0123456789ab")))];
        let out = record_since(&text, &update).unwrap();
        assert!(out.starts_with('\u{feff}'));
        assert!(
            out.ends_with("shows = \"y\"\nsince = 2026-11-09\nsha   = \"0123456789ab\"\n"),
            "{out}"
        );
        assert_eq!(record_since(&out, &[(1, None)]).unwrap(), text);
    }

    #[test]
    fn rows_written_on_one_line_are_refused() {
        let text = "row = [\n  { name = \"a\", run = \"rg x src\", shows = \"x\" },\n]\n";
        let problems = parse(text).unwrap_err();
        assert_eq!((problems[0].line, problems[0].rule), (1, "layout"));
        // A list with nothing in it is a ledger with no rows.
        assert!(parse("row = []\n").unwrap().is_empty());
    }
}
