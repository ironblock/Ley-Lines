//! `ley rules`: the rules a ledger follows.
//!
//! The rules are rows of this project's own ledger, compiled in. So the program,
//! the ledger and the README cannot drift apart: a test compares all three.

const OWN_LEDGER: &str = include_str!("../ley.toml");

/// A row whose definition opens with these words is a rule.
pub const OPENING: &str = "the rule that ";

/// Each rule's name, and what it says.
pub fn rules() -> Vec<(String, String)> {
    crate::ledger::parse(OWN_LEDGER)
        .expect("this project's own ledger is compiled in and must parse")
        .into_iter()
        .filter_map(|row| {
            let text = row.definition()?.is.strip_prefix(OPENING)?;
            Some((row.name.clone(), text.to_string()))
        })
        .collect()
}

/// The rules as plain text: each name, then its sentence, indented and wrapped.
pub fn describe() -> String {
    let mut out = String::new();
    for (name, text) in rules() {
        let mut sentence: String = text.chars().take(1).flat_map(char::to_uppercase).collect();
        sentence.extend(text.chars().skip(1));
        sentence.push('.');
        out.push_str(&name);
        out.push('\n');
        let mut width = 0;
        for word in sentence.split_whitespace() {
            if width == 0 {
                out.push_str("   ");
                width = 3;
            } else if width + 1 + word.chars().count() > 80 {
                out.push_str("\n   ");
                width = 3;
            }
            out.push(' ');
            out.push_str(word);
            width += 1 + word.chars().count();
        }
        out.push_str("\n\n");
    }
    out.push_str("`ley check` enforces the rules a program can check.\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_compiled_ledger_has_rules() {
        let rules = rules();
        assert!(rules.len() >= 5);
        assert!(rules.iter().any(|(name, _)| name == "no stored output"));
        for line in describe().lines() {
            assert!(line.chars().count() <= 80, "{line}");
        }
    }
}
