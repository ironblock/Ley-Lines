//! Two small helpers for the plain text `ley` prints.

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
