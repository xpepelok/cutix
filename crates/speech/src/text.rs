pub fn normalise(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut pending_space = false;

    for symbol in text.chars() {
        let replacement = match symbol {
            '\u{2018}' | '\u{2019}' | '\u{02bc}' => Some('\''),
            '\u{201c}' | '\u{201d}' | '\u{00ab}' | '\u{00bb}' => Some('"'),
            '\u{2013}' | '\u{2014}' | '\u{2212}' => Some('-'),
            '\u{00a0}' | '\u{2009}' | '\u{200a}' => Some(' '),
            _ => None,
        };
        let symbol = replacement.unwrap_or(symbol);

        if symbol.is_whitespace() {
            pending_space = !output.is_empty();
            continue;
        }
        if symbol == '\u{200b}' || symbol == '\u{feff}' {
            continue;
        }
        if pending_space {
            output.push(' ');
            pending_space = false;
        }
        output.push(symbol);
    }

    output
}

pub fn is_blank(text: &str) -> bool {
    !text.chars().any(|symbol| symbol.is_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitespace_is_collapsed_and_trimmed() {
        assert_eq!(normalise("  hello \n\t world  "), "hello world");
        assert_eq!(normalise(""), "");
        assert_eq!(normalise("   "), "");
    }

    #[test]
    fn smart_punctuation_is_folded() {
        assert_eq!(normalise("\u{201c}it\u{2019}s\u{201d}"), "\"it's\"");
        assert_eq!(normalise("a\u{2014}b"), "a-b");
    }

    #[test]
    fn zero_width_characters_are_dropped() {
        assert_eq!(normalise("a\u{200b}b\u{feff}c"), "abc");
    }

    #[test]
    fn non_breaking_space_becomes_a_normal_space() {
        assert_eq!(normalise("a\u{00a0}b"), "a b");
    }

    #[test]
    fn blank_detection_ignores_punctuation() {
        assert!(is_blank(""));
        assert!(is_blank("   "));
        assert!(is_blank("... !!!"));
        assert!(!is_blank("hi"));
        assert!(!is_blank("42"));
    }
}
