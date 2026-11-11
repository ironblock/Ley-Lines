//! Small helpers for the plain text `ley` reads and prints.

/// True for a character that does not show as itself on one line of text. It
/// breaks the line, moves the cursor, changes the direction of what follows, looks
/// like a space and is not one, or cannot be seen at all. `ley` prints none raw.
///
/// The list of what cannot be seen is the Unicode property
/// Default_Ignorable_Code_Point, as of Unicode 18. It includes the tag characters,
/// which a person does not see and a model reads as letters.
///
/// Characters that look alike across scripts, such as a Cyrillic "а" for a Latin
/// "a", are not found here. Nothing in `ley` finds them.
pub fn hidden(c: char) -> bool {
    c.is_control()
        // A line or paragraph separator, or any space but the plain one.
        || (c.is_whitespace() && c != ' ')
        || not_shown(c)
        || matches!(
            c,
            // A blank that is not a space by Unicode's own list.
            '\u{2800}'
            // Private use: no two machines need show these alike.
            | '\u{E000}'..='\u{F8FF}' | '\u{F0000}'..='\u{FFFFD}' | '\u{100000}'..='\u{10FFFD}'
            // Code points that are not characters.
            | '\u{FDD0}'..='\u{FDEF}'
        )
        || (c as u32) & 0xFFFE == 0xFFFE
}

/// Default_Ignorable_Code_Point: a character with no shape of its own.
fn not_shown(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}' | '\u{034F}' | '\u{061C}' | '\u{115F}'..='\u{1160}' | '\u{17B4}'..='\u{17B5}'
        | '\u{180B}'..='\u{180F}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}'
        | '\u{2060}'..='\u{206F}' | '\u{3164}' | '\u{FE00}'..='\u{FE0F}' | '\u{FEFF}'
        | '\u{FFA0}' | '\u{FFF0}'..='\u{FFF8}' | '\u{1BCA0}'..='\u{1BCA3}'
        | '\u{1D173}'..='\u{1D17A}' | '\u{E0000}'..='\u{E0FFF}'
    )
}

/// A mark with no shape of its own that some scripts cannot be written without: a
/// joiner, or a mark that picks one form of the letter before it.
fn joins(c: char) -> bool {
    matches!(
        c,
        '\u{200C}' | '\u{200D}' | '\u{034F}' | '\u{17B4}'..='\u{17B5}' | '\u{180B}'..='\u{180F}'
        | '\u{FE00}'..='\u{FE0F}' | '\u{E0100}'..='\u{E01EF}'
    )
}

/// The first character of `text` that a reader cannot see as text, if there is one.
///
/// A hidden character is one. The exception is a mark that a script needs, where
/// that script would put it: after a letter that is not ASCII, and no more than
/// two in a row. So a word of Persian keeps its joiner, and a line of English
/// cannot carry marks that nobody sees.
pub fn unseen(text: &str) -> Option<char> {
    // How many marks have come since the last character with a shape, and whether
    // that character was one a script's mark can follow.
    let mut marks = 0;
    let mut after_a_letter_of_a_script = false;
    for c in text.chars() {
        if joins(c) {
            marks += 1;
            if !after_a_letter_of_a_script || marks > 2 {
                return Some(c);
            }
        } else if hidden(c) {
            return Some(c);
        } else {
            marks = 0;
            after_a_letter_of_a_script = !c.is_ascii();
        }
    }
    None
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
        // A space that is not the plain space, and a tag letter that only a model reads.
        assert_eq!(shown("a\u{a0}b\u{e0049}"), "a\\u{a0}b\\u{e0049}");
    }

    #[test]
    fn every_kind_of_hidden_character_is_found() {
        for c in [
            '\t',
            '\n',
            '\u{85}',
            '\u{1b}',
            '\u{a0}',
            '\u{2003}',
            '\u{202f}',
            '\u{3000}',
            '\u{2028}',
            '\u{2029}',
            '\u{ad}',
            '\u{200b}',
            '\u{200e}',
            '\u{200f}',
            '\u{61c}',
            '\u{202e}',
            '\u{2066}',
            '\u{2060}',
            '\u{2064}',
            '\u{feff}',
            '\u{3164}',
            '\u{2800}',
            '\u{e0001}',
            '\u{e0041}',
            '\u{e007f}',
            '\u{e000}',
            '\u{f0000}',
            '\u{fdd0}',
            '\u{ffff}',
            '\u{1fffe}',
            '\u{fe0f}',
            '\u{200d}',
        ] {
            assert!(hidden(c), "U+{:04X}", c as u32);
        }
        for c in [
            ' ', 'a', 'é', 'я', '中', '—', '…', '“', '€', '✔', '\u{fffd}',
        ] {
            assert!(!hidden(c), "U+{:04X}", c as u32);
        }
    }

    #[test]
    fn a_mark_a_script_needs_is_seen_where_the_script_puts_it() {
        // Persian, with a joiner between two letters.
        assert_eq!(unseen("می\u{200c}خواهم"), None);
        // Devanagari, with a joiner after a mark that is itself not a letter.
        assert_eq!(unseen("क\u{94d}\u{200d}ष"), None);
        // A sign with the mark that asks for its coloured form, and a family joined from three.
        assert_eq!(unseen("done ✔\u{fe0f}"), None);
        assert_eq!(unseen("👩\u{200d}❤\u{fe0f}\u{200d}👨"), None);
        // The same marks in English are not text a reader can see.
        assert_eq!(unseen("bill\u{200d}ing"), Some('\u{200d}'));
        assert_eq!(unseen("a\u{fe0f}"), Some('\u{fe0f}'));
        assert_eq!(unseen("\u{200c}start"), Some('\u{200c}'));
        // Nor is a run of them, which could carry a message of its own.
        assert_eq!(unseen("✔\u{fe0f}\u{200d}\u{200c}"), Some('\u{200c}'));
        // Everything else that is hidden is refused wherever it is.
        assert_eq!(unseen("می\u{200b}خواهم"), Some('\u{200b}'));
        assert_eq!(
            unseen("the line\u{e0049}\u{e0047} with it"),
            Some('\u{e0049}')
        );
        assert_eq!(unseen("rg -n -F alpha\u{a0}src"), Some('\u{a0}'));
        assert_eq!(unseen("plain text"), None);
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
