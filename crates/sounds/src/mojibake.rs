const CP1252_REVERSE: &[(char, u8)] = &[
    ('\u{20ac}', 0x80),
    ('\u{201a}', 0x82),
    ('\u{0192}', 0x83),
    ('\u{201e}', 0x84),
    ('\u{2026}', 0x85),
    ('\u{2020}', 0x86),
    ('\u{2021}', 0x87),
    ('\u{02c6}', 0x88),
    ('\u{2030}', 0x89),
    ('\u{0160}', 0x8a),
    ('\u{2039}', 0x8b),
    ('\u{0152}', 0x8c),
    ('\u{017d}', 0x8e),
    ('\u{2018}', 0x91),
    ('\u{2019}', 0x92),
    ('\u{201c}', 0x93),
    ('\u{201d}', 0x94),
    ('\u{2022}', 0x95),
    ('\u{2013}', 0x96),
    ('\u{2014}', 0x97),
    ('\u{02dc}', 0x98),
    ('\u{2122}', 0x99),
    ('\u{0161}', 0x9a),
    ('\u{203a}', 0x9b),
    ('\u{0153}', 0x9c),
    ('\u{017e}', 0x9e),
    ('\u{0178}', 0x9f),
];

fn cp1252_byte(value: char) -> Option<u8> {
    CP1252_REVERSE
        .iter()
        .find(|(mapped, _)| *mapped == value)
        .map(|(_, byte)| *byte)
}

fn has_candidate_lead(value: &str) -> bool {
    value.chars().any(
        |character| matches!(mangled_byte(character), Some(byte) if (0xc2..=0xf4).contains(&byte)),
    )
}

fn mangled_byte(character: char) -> Option<u8> {
    let code = character as u32;
    if code <= 0xff {
        Some(code as u8)
    } else {
        cp1252_byte(character)
    }
}

fn is_improvement(before: &str, after: &str) -> bool {
    after != before && after.chars().any(|character| character as u32 > 0x7f)
}

pub fn repair_mojibake(value: &str) -> String {
    let mut current = value.to_string();
    for _ in 0..3 {
        let next = repair_once(&current);
        if next == current {
            break;
        }
        current = next;
    }
    current
}

fn repair_once(value: &str) -> String {
    if !has_candidate_lead(value) {
        return value.to_string();
    }

    let mut bytes = Vec::with_capacity(value.len());
    for character in value.chars() {
        match mangled_byte(character) {
            Some(byte) => bytes.push(byte),
            None => return value.to_string(),
        }
    }

    match String::from_utf8(bytes) {
        Ok(decoded) if is_improvement(value, &decoded) => decoded,
        _ => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn double_encoded_cyrillic_is_recovered() {
        let mangled = "\u{d0}\u{9f}\u{d1}\u{80}\u{d0}\u{b8}\u{d0}\u{b2}\u{d0}\u{b5}\u{d1}\u{82}";
        assert_eq!(repair_mojibake(mangled), "Привет");
    }

    #[test]
    fn double_encoded_latin_is_recovered() {
        assert_eq!(repair_mojibake("CafÃ© del Mar"), "Café del Mar");
    }

    #[test]
    fn genuine_accents_are_left_alone() {
        for value in ["Extraño", "Café del Mar", "Привет", "普通の日本語", ""] {
            assert_eq!(repair_mojibake(value), value, "{value}");
        }
    }

    #[test]
    fn punctuation_and_non_cyrillic_lead_bytes_are_recovered() {
        for (mangled, expected) in [
            ("Donâ€™t Stop", "Don’t Stop"),
            ("Aâ€”B", "A—B"),
            ("â€œQuotedâ€\u{9d}", "“Quoted”"),
            ("Åšwiat", "Świat"),
            ("KrakÃ³w", "Kraków"),
            ("Ã\u{9c}bermorgen", "Übermorgen"),
            ("æ\u{97}¥æ\u{9c}¬èª\u{9e}", "日本語"),
            ("Ã‰tude", "Étude"),
        ] {
            assert_eq!(repair_mojibake(mangled), expected, "{mangled}");
        }
    }

    #[test]
    fn a_repaired_title_is_stable_under_a_second_pass() {
        let once = repair_mojibake("Donâ€™t Stop");
        assert_eq!(repair_mojibake(&once), once);
    }

    #[test]
    fn more_genuine_text_is_left_alone() {
        for value in [
            "Åland",
            "Ünsal",
            "Ão",
            "Señor",
            "Tiếng Việt",
            "Ελληνικά",
            "80's Mix",
        ] {
            assert_eq!(repair_mojibake(value), value, "{value}");
        }
    }

    #[test]
    fn ascii_is_untouched() {
        assert_eq!(
            repair_mojibake("Plain ASCII title 42"),
            "Plain ASCII title 42"
        );
    }
}
