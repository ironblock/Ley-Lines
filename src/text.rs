//! Small helpers for the plain text `ley` reads and prints.

/// True for a character that does not show as itself on one line of text. It
/// breaks the line, moves the cursor, changes the direction of what follows, looks
/// like a space and is not one, or cannot be seen at all. A ledger's text may hold
/// none, and `ley` prints none raw.
///
/// The list of what cannot be seen is the Unicode property
/// Default_Ignorable_Code_Point, as of Unicode 18. It includes the tag characters,
/// which a person does not see and a model reads as letters.
///
/// There is no exception. A few scripts need a joiner that has no shape of its
/// own, and a ledger cannot hold one yet. An exception wide enough for those
/// scripts was tried, and it was wide enough to carry text that nobody sees.
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
    fn a_mark_with_no_shape_is_hidden_wherever_it_is() {
        // A joiner in a word of Persian, a mark that asks for the coloured form of a
        // sign, a mark that picks one form of an ideograph. Each is honest text, and
        // each is refused for now: nobody can see it.
        for c in [
            '\u{200c}',
            '\u{200d}',
            '\u{fe0f}',
            '\u{fe00}',
            '\u{e0100}',
            '\u{34f}',
        ] {
            assert!(hidden(c), "U+{:04X}", c as u32);
        }
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
