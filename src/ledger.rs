//! The ledger: a TOML file of rows, and the rules a row must follow.
//!
//! On disk a row is a `[[row]]` block, one field on each line. Every key is optional but `name`.
//! In memory a row is a sum type, so a row that says nothing cannot exist past
//! `parse`. What kind of row it is follows from the keys it carries. It is never
//! declared.

use std::path::{Path, PathBuf};

use toml::value::{Date, Datetime};
use toml::{Table, Value};

use crate::text::hidden;

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
        // A message may quote a key of the ledger. None of it is printed raw.
        Problem {
            line,
            rule,
            message: crate::text::shown(&message.into()),
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
        let line = layout.keys.get(key).copied().unwrap_or(1);
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
            // Written as blocks, each row has a line of its own from the other parser.
            if !layout.rows.is_empty() && layout.rows.len() != values.len() {
                problems.push(Problem::new(
                    layout.opener,
                    "toml",
                    "`ley` reads this file twice, and the two readings do not count the same \
                     number of rows. This is a fault in `ley`",
                ));
            }
            for (i, value) in values.iter().enumerate() {
                let line = layout.rows.get(i).copied().unwrap_or(layout.opener);
                rows.extend(row(value, line, &mut problems));
            }
        }
        Some(_) => {
            let line = layout.keys.get("row").copied().unwrap_or(1);
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
    /// The line each top-level key is on.
    keys: std::collections::BTreeMap<String, usize>,
}

/// Check that rows are written as blocks, and note which line each row starts on.
///
/// A row is a `[[row]]` block with one field on each line. Then a change to one
/// field is one changed line, a review comment can sit on one field, and two
/// people can change two fields of a row without a conflict. TOML itself puts each
/// field of a block on its own line. Two other ways to write the same rows are
/// refused: all of a row's fields on one line, and a field in a block of its own,
/// as `[[row.said]]`. Then a row is written one way only, and everything that
/// belongs to it sits between its `[[row]]` line and the next one. The positions
/// come from the parser.
fn layout(text: &str, problems: &mut Vec<Problem>) -> Layout {
    let mut found = Layout {
        opener: 1,
        rows: Vec::new(),
        keys: std::collections::BTreeMap::new(),
    };
    // The values were read by one parser, and the positions come from another.
    // Both read the same TOML. If they ever disagree, say so: do not guess a line.
    const DISAGREE: &str = "`ley` reads this file twice, once for values and once for \
                            positions, and the two readings disagree. This is a fault in `ley`";
    let Ok(document) = toml_edit::Document::parse(text) else {
        problems.push(Problem::new(1, "toml", DISAGREE));
        return found;
    };
    let line_of = |span: Option<std::ops::Range<usize>>| span.map(|span| line_at(text, span.start));
    for (name, _) in document.as_table().iter() {
        let key = document.as_table().get_key_value(name).map(|(key, _)| key);
        if let Some(line) = line_of(key.and_then(|key| key.span())) {
            found.keys.insert(name.to_string(), line);
        }
    }
    let Some(item) = document.as_table().get("row") else {
        return found;
    };
    if let Some(blocks) = item.as_array_of_tables() {
        for block in blocks.iter() {
            let line = line_of(block.span()).unwrap_or(1);
            found.rows.push(line);
            for (name, field) in block.iter() {
                if field.is_value() {
                    continue;
                }
                let key = block.get_key_value(name).map(|(key, _)| key);
                problems.push(Problem::new(
                    line_of(key.and_then(|key| key.span())).unwrap_or(line),
                    "layout",
                    format!(
                        "`{name}` is not written as one field of its row. Write every field \
                         inside the `[[row]]` block, as `{name} = …`: then the whole row is in \
                         one place"
                    ),
                ));
            }
        }
        found.opener = found.rows.first().copied().unwrap_or(1);
    } else if let Some(array) = item.as_array() {
        found.opener = line_of(array.span()).unwrap_or(1);
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
        Some("is empty".to_string())
    } else if value.trim() != value {
        Some("starts or ends with a space".to_string())
    } else {
        // A new line would let a row forge a second row when it is printed. A control
        // character can rewrite what a terminal shows. A change of direction can make
        // a command read as another command.
        value.chars().find(|c| hidden(*c)).map(|c| {
            format!(
                "holds U+{:04X}: text is one line, with no control character, no space \
                 but the plain one, and no character that a reader cannot see",
                c as u32
            )
        })
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
            if !by.trim().is_empty() && by.trim() == by && !by.chars().any(hidden) =>
        {
            by.clone()
        }
        Some(Value::String(_)) => {
            return Err(
                "`by` is a name on one line, with no space at either end, no control character \
                 and no character that a reader cannot see"
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
/// cannot be complete, and a missed ending lets a position through.
///
/// Some endings here also end host names: `sh`, `rs`, `md` and others. A host of
/// that kind, written with its port, is refused as a position. The list keeps
/// them because the files are far more common in a ledger than the hosts. It
/// leaves out `pl` and `ml`, where that is less clear.
const FILE_ENDINGS: &[&str] = &[
    "adoc",
    "bat",
    "c",
    "cc",
    "cfg",
    "cjs",
    "clj",
    "cmake",
    "conf",
    "cpp",
    "cs",
    "css",
    "csv",
    "cts",
    "cxx",
    "dart",
    "el",
    "erl",
    "ex",
    "exs",
    "go",
    "gradle",
    "graphql",
    "groovy",
    "h",
    "hh",
    "hpp",
    "hs",
    "html",
    "hxx",
    "ini",
    "java",
    "jl",
    "js",
    "json",
    "jsx",
    "kt",
    "kts",
    "lock",
    "lua",
    "md",
    "mjs",
    "mts",
    "nix",
    "php",
    "properties",
    "proto",
    "ps1",
    "py",
    "rb",
    "rs",
    "rst",
    "sbt",
    "scala",
    "scss",
    "sh",
    "sql",
    "svelte",
    "swift",
    "tex",
    "tf",
    "toml",
    "ts",
    "tsv",
    "tsx",
    "txt",
    "vim",
    "vue",
    "xml",
    "yaml",
    "yml",
    "zig",
    "zsh",
];

/// Files that are known by their whole name, in small letters.
const FILE_NAMES: &[&str] = &[
    "brewfile",
    "caddyfile",
    "containerfile",
    "dockerfile",
    "gemfile",
    "jenkinsfile",
    "justfile",
    "makefile",
    "procfile",
    "rakefile",
    "vagrantfile",
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
            // The file's name, without a quote mark or bracket after it.
            let (file, rest) = (&word[..at], &word[at + 1..]);
            // A full stop stays: `D.C.:5` names no file.
            let file = file.trim_end_matches(|c: char| !c.is_alphanumeric() && c != '.');
            let ending = file
                .rsplit_once('.')
                .map(|(_, end)| end.to_ascii_lowercase());
            // The file's own name, without a quote mark or bracket in front of it.
            let name = file.rsplit(['/', '\\']).next().unwrap_or(file);
            let name = name
                .trim_start_matches(|c: char| !c.is_alphanumeric())
                .to_ascii_lowercase();
            // `Dockerfile`, and `Dockerfile.prod` too.
            let is_a_known_name = FILE_NAMES.iter().any(|known| {
                name.strip_prefix(known)
                    .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
            });
            let is_a_file =
                is_a_known_name || ending.is_some_and(|end| FILE_ENDINGS.contains(&end.as_str()));
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

/// Record `since` and `sha` on rows, or remove them, changing nothing else in the file.
///
/// `updates` holds the position of each row in the ledger, and what to record.
/// `None` removes both keys. Only those two lines of a block change: every other
/// line, the comments and a byte-order mark stay as they were. A new line is
/// written the way the block's other fields are: lined up or not, and with the
/// line break the file uses.
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
    // Where the line that holds `at` ends: at its line break, or at the end of the text.
    let line_end = |at: usize| text[at..].find('\n').map_or(text.len(), |i| at + i);
    // The line break after the line that holds `at`. The last line may have none:
    // then it is the one the rest of the file uses.
    let line_break = |at: usize| {
        let end = line_end(at);
        let carriage_return = match end < text.len() {
            true => text[..end].ends_with('\r'),
            false => text.contains("\r\n"),
        };
        if carriage_return { "\r\n" } else { "\n" }
    };

    // Each edit replaces one span of the text.
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    for (index, since) in updates {
        let row = rows
            .get(*index)
            .ok_or_else(|| format!("the ledger has no row {}", index + 1))?;
        // The fields of the block in the order they are written, with where each key
        // and each value sits in the text.
        let mut entries: Vec<(&str, Range<usize>, Range<usize>)> = Vec::new();
        for (name, field) in row.iter() {
            if !field.is_value() {
                return Err(format!(
                    "`{name}` in row {} is not written as one field",
                    index + 1
                ));
            }
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
                    let last = last.ok_or("a row has no fields")?;
                    // The new lines go after the block's last field, before its line break.
                    let end = line_end(last);
                    let at = end - usize::from(text[..end].ends_with('\r'));
                    let line_break = line_break(last);
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
                        lines.push_str(line_break);
                        match width > name.len() {
                            true => lines.push_str(&format!("{indent}{name:<width$}= {value}")),
                            false => lines.push_str(&format!("{indent}{name} = {value}")),
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

    // Lines that are taken out one after another become one span. No other two
    // edits may touch the same text.
    edits.sort_by_key(|(span, _)| (span.start, span.end));
    let mut joined: Vec<(Range<usize>, String)> = Vec::new();
    for (span, with) in edits {
        match joined.last_mut() {
            Some((last, last_with))
                if with.is_empty() && last_with.is_empty() && span.start <= last.end =>
            {
                last.end = last.end.max(span.end);
            }
            Some((last, _)) if span.start < last.end => {
                return Err("two changes to the ledger touch the same text".into());
            }
            _ => joined.push((span, with)),
        }
    }
    // The last line of a file may have no line break after it. When that line is
    // taken out, the line break before the lines that go is taken out with them.
    for (span, with) in &mut joined {
        if with.is_empty() && span.end == text.len() && !text.ends_with('\n') {
            let before = &text[..span.start];
            span.start -= match before.ends_with("\r\n") {
                true => 2,
                false => usize::from(before.ends_with('\n')),
            };
        }
    }
    // Work from the end of the text back, so earlier spans stay where they were.
    let mut out = text.to_string();
    for (span, with) in joined.into_iter().rev() {
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
            ("the target defined at Makefile:12", Some("Makefile:12")),
            ("built by docker/Dockerfile:3", Some("docker/Dockerfile:3")),
            ("the clause at lib/app.ex:40", Some("lib/app.ex:40")),
            ("the type in Main.hs:7", Some("Main.hs:7")),
            ("the target at `Makefile:12`", Some("`Makefile:12`")),
            ("the target at `Makefile`:12", Some("`Makefile`:12")),
            ("see \"stage.ts\":14 for it", Some("\"stage.ts\":14")),
            (
                "the stage (Dockerfile.prod:3) that builds it",
                Some("(Dockerfile.prod:3)"),
            ),
            ("line 12 of the glossary", Some("line 12")),
            ("the entry at [stage.ts]:14", Some("[stage.ts]:14")),
            ("see **stage.ts**:14 for it", Some("**stage.ts**:14")),
            ("see _stage.ts_:14 for it", Some("_stage.ts_:14")),
            ("the entry at stage.ts,:14", Some("stage.ts,:14")),
            // Not positions:
            // An abbreviation that ends in a full stop names no file.
            ("the office in D.C.:5 on the plan", None),
            ("the depot in N.H.:3", None),
            ("the run at 10:30 each night", None),
            ("the page at https://example.org:8080/x", None),
            ("the proxy that listens on billing.internal:8443", None),
            ("the image published as billing/api:2", None),
            // A host whose name ends in a way this list leaves out.
            ("the mirror at example.pl:8080", None),
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
    fn recording_since_keeps_the_line_breaks_the_file_uses() {
        // A file written on Windows: every line ends with a carriage return and a line feed.
        let windows = TWO_ROWS.replace('\n', "\r\n");
        let update = [(0, Some(since("2026-11-09", "0123456789ab")))];
        let out = record_since(&windows, &update).unwrap();
        let expected = windows.replace(
            "shows = \"x\"\r\n",
            "shows = \"x\"\r\nsince = 2026-11-09\r\nsha = \"0123456789ab\"\r\n",
        );
        assert_eq!(out, expected);
        assert_eq!(out.matches('\n').count(), out.matches("\r\n").count());
        assert!(parse(&out).is_ok());
        assert_eq!(record_since(&out, &[(0, None)]).unwrap(), windows);
    }

    #[test]
    fn recording_since_on_a_last_line_with_no_line_break_adds_none() {
        for line_break in ["\n", "\r\n"] {
            let text = format!(
                "[[row]]{line_break}name = \"a\"{line_break}run = \"rg x src\"{line_break}shows = \"x\""
            );
            let update = [(0, Some(since("2026-11-09", "0123456789ab")))];
            let out = record_since(&text, &update).unwrap();
            assert_eq!(
                out,
                format!("{text}{line_break}since = 2026-11-09{line_break}sha = \"0123456789ab\"")
            );
            // Taking both lines out again gives back exactly the text there was.
            assert_eq!(record_since(&out, &[(0, None)]).unwrap(), text);
        }
    }

    #[test]
    fn recording_since_goes_after_sources_that_sit_one_to_a_line() {
        let text = "[[row]]\nname = \"a\"\nrun = \"rg x src\"\nshows = \"x\"\nis = \"a thing\"\nsaid = [\n  { by = \"m\", on = 2026-11-10 },\n  { by = \"n\", on = 2026-11-11 },\n] # two sources\n\n[[row]]\nname = \"b\"\nrun = \"rg y src\"\nshows = \"y\"\n";
        let update = [(0, Some(since("2026-11-09", "0123456789ab")))];
        let out = record_since(text, &update).unwrap();
        let expected = text.replace(
            "] # two sources\n",
            "] # two sources\nsince = 2026-11-09\nsha = \"0123456789ab\"\n",
        );
        assert_eq!(out, expected);
        let rows = parse(&out).unwrap();
        assert!(rows[0].pointer().unwrap().since.is_some());
        assert!(rows[1].pointer().unwrap().since.is_none());
    }

    #[test]
    fn a_field_in_a_block_of_its_own_is_refused() {
        // Valid TOML, and the same rows. But `since` and `sha` written after it would
        // land inside the source.
        let text = "[[row]]\nname = \"a\"\nis = \"a thing\"\n\n[[row.said]]\nby = \"m\"\non = 2026-11-10\n";
        let problems = parse(text).unwrap_err();
        assert_eq!(problems.len(), 1, "{problems:#?}");
        assert_eq!((problems[0].line, problems[0].rule), (5, "layout"));
        // So is a dotted key.
        let text = "[[row]]\nname = \"a\"\nis = \"a thing\"\nsaid.by = \"m\"\n";
        let problems = parse(text).unwrap_err();
        assert!(problems.iter().any(|p| (p.line, p.rule) == (4, "layout")));
    }

    #[test]
    fn a_top_level_key_takes_its_line_from_the_parser() {
        // `[name]` is not the row's `name = ` line, and `[ro]` is not `[[row]]`.
        let text = "[[row]]\nname = \"a\"\nrun = \"rg a src\"\nshows = \"a\"\n\n[name]\nx = 1\n\n[ro]\ny = 2\n";
        let problems = parse(text).unwrap_err();
        let found: Vec<(usize, &str)> = problems.iter().map(|p| (p.line, p.rule)).collect();
        assert_eq!(found, [(6, "top-level"), (9, "top-level")]);
    }

    #[test]
    fn a_key_cannot_forge_a_line_or_reach_the_terminal() {
        let text = "[[row]]\nname = \"a\"\nrun = \"rg a src\"\nshows = \"a\"\n\"evil\\nley.toml:1: forged\\u001b[31m\" = 1\n";
        let problems = parse(text).unwrap_err();
        assert_eq!(problems.len(), 1);
        let message = &problems[0].message;
        assert!(!message.chars().any(crate::text::hidden), "{message:?}");
        assert!(
            message.contains("evil\\u{a}ley.toml:1: forged\\u{1b}[31m"),
            "{message}"
        );
        // The same for a key at the top of the file, and for a key of a source.
        for text in [
            "\"top\\u001b[2K\" = 1\n",
            "[[row]]\nname = \"a\"\nis = \"b\"\nsaid = [{ by = \"m\", on = 2026-11-10, \"x\\ny\" = 1 }]\n",
        ] {
            for problem in parse(text).unwrap_err() {
                assert!(
                    !problem.message.chars().any(crate::text::hidden),
                    "{problem:?}"
                );
            }
        }
    }

    #[test]
    fn text_that_hides_or_reorders_what_a_reader_sees_is_refused() {
        for hidden in [
            "\\u2028", "\\u202E", "\\u2066", "\\u200B", "\\uFEFF", "\\u001b",
        ] {
            for field in ["name", "is", "run", "shows"] {
                let mut fields = vec![
                    ("name", "a".to_string()),
                    ("is", "a thing".to_string()),
                    ("run", "rg a src".to_string()),
                    ("shows", "the thing".to_string()),
                ];
                for (key, value) in &mut fields {
                    if key == &field {
                        value.insert_str(1, hidden);
                    }
                }
                let lines: Vec<String> = fields
                    .iter()
                    .map(|(key, value)| format!("{key} = \"{value}\""))
                    .collect();
                let text = format!(
                    "[[row]]\n{}\nsaid = [{{ by = \"m\", on = 2026-11-10 }}]\n",
                    lines.join("\n")
                );
                let problems = parse(&text).unwrap_err();
                assert!(
                    problems
                        .iter()
                        .all(|p| p.rule == "text" || p.rule == "name"),
                    "{field} with {hidden}: {problems:#?}"
                );
            }
            // A source's name, too.
            let text = format!(
                "[[row]]\nname = \"a\"\nis = \"b\"\nsaid = [{{ by = \"m{hidden}\", on = 2026-11-10 }}]\n"
            );
            let problems = parse(&text).unwrap_err();
            assert_eq!(problems[0].rule, "source", "{hidden}");
        }
        // A joiner that a script needs is refused too, for now. Nobody can see it.
        let text = "[[row]]\nname = \"a\"\nis = \"می\\u200Cخواهم\"\nsaid = [{ by = \"m\", on = 2026-11-10 }]\n";
        let problems = parse(text).unwrap_err();
        assert_eq!(problems.len(), 1);
        assert!(problems[0].message.contains("`is` holds U+200C"));
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
