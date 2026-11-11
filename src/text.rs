//! Small helpers for the plain text `ley` reads and prints.

/// True for a character that does not show as itself on one line of text. It
/// breaks the line, moves the cursor, changes the direction of what follows, or
/// cannot be seen at all. A ledger's text may not hold one, and `ley` prints none.
///
/// The joiners U+200C and U+200D are let through: some scripts cannot be written
/// without them.
pub fn hidden(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            // The soft hyphen and the zero width space.
            '\u{00AD}' | '\u{200B}'
            // The line separator and the paragraph separator.
            | '\u{2028}' | '\u{2029}'
            // Embeddings, overrides and isolates of direction.
            | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
            // The word joiner and the zero width no-break space.
            | '\u{2060}' | '\u{FEFF}'
        )
}

/// `text` with every hidden character written out, as `\u{1b}`. Text that `ley` did
/// not write itself goes through this before it is printed: a key of the ledger, a
/// file name, what another program said.
pub fn shown(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if hidden(c) {
            out.extend(c.escape_unicode());
        } else {
            out.push(c);
        }
    }
    out
}

/// "1 row" or "3 rows".
pub fn count(n: u64, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Lay words out after `lead`, wrapped to 80 columns. Lines after the first start with `hang`.
pub fn wrap<'a>(lead: &str, hang: &str, words: impl IntoIterator<Item = &'a str>) -> String {
    let mut out = String::from(lead);
    let mut width = lead.chars().count();
    for word in words {
        let length = word.chars().count();
        if width + 1 + length > 80 {
            out.push('\n');
            out.push_str(hang);
            width = hang.chars().count();
        }
        out.push(' ');
        out.push_str(word);
        width += 1 + length;
    }
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_characters_are_written_out() {
        assert_eq!(shown("plain text, naïve café"), "plain text, naïve café");
        assert_eq!(
            shown("a\u{1b}[31mb\nc\u{202e}d\u{2028}e\u{200b}f"),
            "a\\u{1b}[31mb\\u{a}c\\u{202e}d\\u{2028}e\\u{200b}f"
        );
        // A script that needs a joiner keeps it.
        assert!(!hidden('\u{200c}') && !hidden('\u{200d}'));
        assert!(hidden('\u{feff}') && hidden('\u{ad}') && hidden('\t') && hidden('\u{85}'));
    }

    #[test]
    fn counts_agree_with_their_noun() {
        assert_eq!(count(0, "result", "results"), "0 results");
        assert_eq!(count(1, "result", "results"), "1 result");
        assert_eq!(count(2, "result", "results"), "2 results");
    }

    #[test]
    fn no_wrapped_line_is_longer_than_80_columns() {
        let words = ["twelve-chars"; 40];
        let text = wrap("    label:", "       ", words);
        assert!(text.lines().count() > 1);
        for line in text.lines() {
            assert!(line.chars().count() <= 80, "{line}");
        }
        assert!(
            text.lines()
                .nth(1)
                .unwrap()
                .starts_with("        twelve-chars")
        );
    }
}
