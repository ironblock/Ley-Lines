//! Shapes: the commands `ley check` is willing to run.
//!
//! A shape names a program and the flags a row may pass to it. The shapes are data,
//! in `shapes.toml`, compiled into the binary. A command that fits no shape is
//! shown to readers but never run.

use std::sync::OnceLock;

use serde::Deserialize;

use crate::text::wrap;

const SHAPES: &str = include_str!("shapes.toml");

/// How the results of a command are counted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Counts {
    /// Run the command as written. Each line it prints is one result.
    Lines,
    /// Add `--json=stream`. Each line it prints is one result.
    JsonLines,
    /// Add `--json=stream`. Each line is one file; each entry of its `items` is one result.
    JsonItems,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shape {
    pub program: String,
    #[serde(default)]
    pub sub: Option<String>,
    pub about: String,
    pub counts: Counts,
    #[serde(default)]
    pub needs_path: bool,
    /// Flags `ley` adds each time it runs the program. A row never carries them.
    #[serde(default)]
    pub adds: Vec<String>,
    #[serde(default)]
    pub needs_one_of: Vec<String>,
    #[serde(default)]
    pub no_pattern_with: Vec<String>,
    #[serde(default)]
    pub flags: Vec<String>,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub path_options: Vec<String>,
}

impl Shape {
    /// The shape's name as a reader would type it: `rg`, or `ast-grep outline`.
    pub fn title(&self) -> String {
        match &self.sub {
            Some(sub) => format!("{} {sub}", self.program),
            None => self.program.clone(),
        }
    }
}

#[derive(Deserialize)]
struct File {
    shape: Vec<Shape>,
}

/// Every shape, in the order of `shapes.toml`.
pub fn shapes() -> &'static [Shape] {
    static PARSED: OnceLock<Vec<Shape>> = OnceLock::new();
    PARSED.get_or_init(|| {
        toml::from_str::<File>(SHAPES)
            .expect("shapes.toml is compiled in and must parse")
            .shape
    })
}

/// A command that fits a shape, ready to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// What `ley` runs to count results: the row's command, and the flags `ley` adds.
    pub argv: Vec<String>,
    /// What `ley` runs to see the text a reader would see, when `argv` prints
    /// something else. `None` when `argv` already prints that text.
    pub plain: Option<Vec<String>>,
    pub counts: Counts,
}

/// Fit `words` to a shape, or say why they fit none.
pub fn plan(words: &[String]) -> Result<Plan, String> {
    let program = words.first().ok_or("the command is empty")?;
    let candidates: Vec<&Shape> = shapes().iter().filter(|s| &s.program == program).collect();
    if candidates.is_empty() {
        return Err(format!("no shape for `{program}`"));
    }
    let shape = match candidates.iter().find(|s| s.sub.is_none()) {
        Some(shape) => shape,
        None => {
            let known: Vec<&str> = candidates.iter().filter_map(|s| s.sub.as_deref()).collect();
            let known = known.join(", ");
            let Some(sub) = words.get(1) else {
                return Err(format!("`{program}` needs a subcommand: {known}"));
            };
            candidates
                .iter()
                .find(|s| s.sub.as_deref() == Some(sub.as_str()))
                .ok_or_else(|| {
                    format!("no shape for `{program} {sub}` (there is one for: {known})")
                })?
        }
    };
    let title = shape.title();
    let rest = &words[if shape.sub.is_some() { 2 } else { 1 }..];

    let mut seen: Vec<&str> = Vec::new();
    let mut positional: Vec<&str> = Vec::new();
    let mut only_positional = false;
    let mut i = 0;
    while i < rest.len() {
        let word = rest[i].as_str();
        i += 1;
        if only_positional || !word.starts_with('-') || word == "-" {
            positional.push(word);
            continue;
        }
        if word == "--" {
            only_positional = true;
            continue;
        }
        let (name, inline) = match word.split_once('=') {
            Some((name, value)) if word.starts_with("--") => (name, Some(value)),
            _ => (word, None),
        };
        if shape.flags.iter().any(|f| f == name) {
            if inline.is_some() {
                return Err(format!("`{name}` takes no value"));
            }
        } else if shape.options.iter().any(|o| o == name) {
            let value = match inline {
                Some(value) => value,
                None => {
                    let value = rest
                        .get(i)
                        .ok_or_else(|| format!("`{name}` needs a value"))?;
                    i += 1;
                    value.as_str()
                }
            };
            if shape.path_options.iter().any(|o| o == name) {
                inside(value)?;
            }
        } else {
            return Err(format!("`{name}` is not in the shape for `{title}`"));
        }
        seen.push(name);
    }

    let has = |names: &[String]| names.iter().any(|n| seen.contains(&n.as_str()));
    if !shape.needs_one_of.is_empty() && !has(&shape.needs_one_of) {
        return Err(format!(
            "`{title}` needs one of: {}",
            shape.needs_one_of.join(", ")
        ));
    }
    let takes_pattern = !shape.no_pattern_with.is_empty() && !has(&shape.no_pattern_with);
    let paths = if takes_pattern {
        if positional.is_empty() {
            return Err(format!("`{title}` needs a pattern"));
        }
        &positional[1..]
    } else {
        &positional[..]
    };
    for path in paths {
        inside(path)?;
    }
    if shape.needs_path && paths.is_empty() {
        return Err(format!(
            "`{title}` needs a path: with none, it reads standard input when that is a pipe"
        ));
    }

    // Added flags go right after the program and its subcommand, before any `--`.
    let mut plain = words.to_vec();
    let at = if shape.sub.is_some() { 2 } else { 1 };
    plain.splice(at..at, shape.adds.iter().cloned());
    let (argv, plain) = match shape.counts {
        Counts::Lines => (plain, None),
        Counts::JsonLines | Counts::JsonItems => {
            let mut argv = plain.clone();
            argv.insert(at, "--json=stream".into());
            (argv, Some(plain))
        }
    };
    Ok(Plan {
        argv,
        plain,
        counts: shape.counts,
    })
}

/// A path in a row is relative and stays inside the ledger's directory.
fn inside(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("a path is empty".into());
    }
    if path == "-" {
        return Err("`-` means standard input, which is not the same for every reader".into());
    }
    if path.starts_with('/') || path.starts_with('\\') || path.contains(':') {
        return Err(format!(
            "`{path}` is not relative: a row's paths start at the ledger's directory"
        ));
    }
    if path.split(['/', '\\']).any(|part| part == "..") {
        return Err(format!("`{path}` leaves the ledger's directory"));
    }
    Ok(())
}

/// A plain-text list of the shapes, for `ley shapes`.
pub fn describe() -> String {
    let mut out = String::new();
    for shape in shapes() {
        out.push_str(&format!("{}\n    {}\n", shape.title(), shape.about));
        if shape.needs_path {
            out.push_str("    needs a path\n");
        }
        let mut adds = shape.adds.clone();
        if matches!(shape.counts, Counts::JsonLines | Counts::JsonItems) {
            adds.push("--json=stream".into());
        }
        if !adds.is_empty() {
            out.push_str(&listed("`ley` adds:", &adds));
        }
        if !shape.needs_one_of.is_empty() {
            out.push_str(&listed("needs one of:", &shape.needs_one_of));
        }
        if !shape.flags.is_empty() {
            out.push_str(&listed("flags:", &shape.flags));
        }
        if !shape.options.is_empty() {
            out.push_str(&listed("options, each with a value:", &shape.options));
        }
        out.push('\n');
    }
    out.push_str(
        "Paths are relative and stay inside the ledger's directory.\n\
         A command that fits no shape is shown to readers but never run.\n",
    );
    out
}

fn listed(label: &str, words: &[String]) -> String {
    wrap(
        &format!("    {label}"),
        "       ",
        words.iter().map(String::as_str),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fit(command: &str) -> Result<Plan, String> {
        plan(&crate::command::split(command).unwrap())
    }

    #[test]
    fn the_compiled_shapes_parse() {
        assert!(shapes().len() >= 4);
    }

    #[test]
    fn a_flag_is_either_a_flag_or_an_option_never_both() {
        for shape in shapes() {
            for flag in &shape.flags {
                assert!(!shape.options.contains(flag), "{}: {flag}", shape.title());
            }
            for option in &shape.path_options {
                assert!(
                    shape.options.contains(option),
                    "{}: {option}",
                    shape.title()
                );
            }
        }
    }

    #[test]
    fn rg_runs_as_written() {
        let plan = fit("rg -n 'interface Stage' src").unwrap();
        assert_eq!(
            plan.argv,
            ["rg", "--no-config", "-n", "interface Stage", "src"]
        );
        assert_eq!(plan.plain, None);
        assert_eq!(plan.counts, Counts::Lines);
    }

    #[test]
    fn ast_grep_gets_a_json_flag() {
        let plan = fit("ast-grep outline --type interface --match '^Stage$' src").unwrap();
        assert_eq!(plan.argv[..3], ["ast-grep", "outline", "--json=stream"]);
        assert_eq!(
            plan.plain.as_ref().unwrap()[..3],
            ["ast-grep", "outline", "--type"]
        );
        assert_eq!(plan.counts, Counts::JsonItems);
        let plan = fit("ast-grep run -p 'foo($A)' -- src").unwrap();
        assert_eq!(
            plan.argv,
            [
                "ast-grep",
                "run",
                "--json=stream",
                "-p",
                "foo($A)",
                "--",
                "src"
            ]
        );
    }

    #[test]
    fn refusals_say_why() {
        for (command, why) in [
            ("cargo test", "no shape for `cargo`"),
            ("ast-grep new rule", "no shape for `ast-grep new`"),
            (
                "ast-grep",
                "`ast-grep` needs a subcommand: run, scan, outline",
            ),
            ("rg --pre cat x src", "`--pre` is not in the shape"),
            ("rg Stage", "needs a path"),
            ("rg --files", "needs a path"),
            ("rg -e Stage", "needs a path"),
            ("rg", "needs a pattern"),
            ("rg Stage /etc", "is not relative"),
            ("rg Stage -", "`-` means standard input"),
            ("rg Stage ../other", "leaves the ledger's directory"),
            ("rg Stage src/../../x", "leaves the ledger's directory"),
            ("rg -g", "`-g` needs a value"),
            ("rg --files=yes src", "takes no value"),
            ("ast-grep run src", "needs one of"),
            ("ast-grep run -p x -U src", "`-U` is not in the shape"),
            ("ast-grep scan src", "needs one of"),
            ("ast-grep scan --rule /tmp/r.yml src", "is not relative"),
        ] {
            let refusal = fit(command).unwrap_err();
            assert!(refusal.contains(why), "{command:?} gave {refusal:?}");
        }
    }
}
