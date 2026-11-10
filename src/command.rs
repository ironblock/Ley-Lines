//! Split a row's command into words.
//!
//! The words are split the way a POSIX shell splits them. Anything a shell would
//! interpret is refused: a pipe, a variable, a glob, a redirect. So a command that
//! passes means the same thing to `ley` and to the shell of whoever reads the row.
//! `ley` never starts a shell.

/// Characters a shell interprets when they are not quoted.
const SPECIAL: &str = "|&;<>()$`*?[]{}~#!";

/// Split `command` into words, or say why a shell would not read it literally.
pub fn split(command: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut chars = command.chars();

    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            '\n' | '\r' => return Err("the command is more than one line".into()),
            '\'' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some('\n' | '\r') => {
                            return Err("the command is more than one line".into());
                        }
                        Some(c) => word.push(c),
                        None => return Err("a single quote is never closed".into()),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some(c @ ('\\' | '"' | '$' | '`')) => word.push(c),
                            Some(c) => {
                                word.push('\\');
                                word.push(c);
                            }
                            None => return Err("a double quote is never closed".into()),
                        },
                        Some(c @ ('$' | '`' | '!')) => {
                            return Err(format!(
                                "a shell expands `{c}` inside double quotes: use single quotes"
                            ));
                        }
                        Some('\n' | '\r') => {
                            return Err("the command is more than one line".into());
                        }
                        Some(c) => word.push(c),
                        None => return Err("a double quote is never closed".into()),
                    }
                }
            }
            '\\' => match chars.next() {
                Some('\n' | '\r') | None => {
                    return Err("a backslash has nothing after it".into());
                }
                Some(c) => {
                    in_word = true;
                    word.push(c);
                }
            },
            '=' if !in_word => {
                return Err("a word starts with `=`, which zsh expands: quote it".into());
            }
            '|' | '&' | ';' | '<' | '>' => {
                return Err(format!("`{c}` needs a shell, and `ley` starts none"));
            }
            c if SPECIAL.contains(c) => {
                return Err(format!(
                    "a shell interprets `{c}`: put the word in single quotes"
                ));
            }
            c => {
                in_word = true;
                word.push(c);
            }
        }
    }
    if in_word {
        words.push(word);
    }
    if words.is_empty() {
        return Err("the command is empty".into());
    }
    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::split;

    fn words(command: &str) -> Vec<String> {
        split(command).unwrap()
    }

    #[test]
    fn plain_words() {
        assert_eq!(words("rg -n Stage src"), ["rg", "-n", "Stage", "src"]);
        assert_eq!(words("  rg\t-n   Stage  "), ["rg", "-n", "Stage"]);
    }

    #[test]
    fn single_quotes_keep_everything() {
        assert_eq!(
            words("ast-grep run -p 'foo($$$A, $B)' src"),
            ["ast-grep", "run", "-p", "foo($$$A, $B)", "src"]
        );
        assert_eq!(
            words(r#"rg '"packageManager"' a"#),
            ["rg", "\"packageManager\"", "a"]
        );
    }

    #[test]
    fn double_quotes_and_escapes() {
        assert_eq!(words(r#"rg "a b" src"#), ["rg", "a b", "src"]);
        assert_eq!(words(r#"rg "say \"hi\"" src"#), ["rg", "say \"hi\"", "src"]);
        assert_eq!(words(r#"rg "a\.b" src"#), ["rg", r"a\.b", "src"]);
        assert_eq!(words(r"rg it\'s src"), ["rg", "it's", "src"]);
        assert_eq!(words(r"rg 'it'\''s' src"), ["rg", "it's", "src"]);
    }

    #[test]
    fn empty_quotes_make_an_empty_word() {
        assert_eq!(words("rg '' src"), ["rg", "", "src"]);
    }

    #[test]
    fn option_values_with_equals() {
        assert_eq!(
            words("ast-grep run --lang=ts -p x"),
            ["ast-grep", "run", "--lang=ts", "-p", "x"]
        );
    }

    #[test]
    fn refuses_what_a_shell_would_interpret() {
        for command in [
            "rg Stage src | wc -l",
            "rg Stage src > out",
            "rg $HOME src",
            "rg \"$HOME\" src",
            "rg `id` src",
            "rg Stage src; id",
            "rg Stage src && id",
            "rg Stage *.ts",
            "rg Stage ~/src",
            "rg Stage src # note",
            "rg (a) src",
            "rg {a,b} src",
            "rg \"wow!\" src",
            "=rg Stage src",
            "rg 'unclosed src",
            "rg \"unclosed src",
            "rg Stage src\\",
            "rg Stage\nsrc",
            "",
            "   ",
        ] {
            assert!(split(command).is_err(), "should refuse: {command:?}");
        }
    }
}
