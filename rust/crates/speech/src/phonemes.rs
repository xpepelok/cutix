use crate::SpeechError;

pub const PHONEME_VOCAB: &[(char, i64)] = &[
    ('$', 0),
    (';', 1),
    (':', 2),
    (',', 3),
    ('.', 4),
    ('!', 5),
    ('?', 6),
    ('—', 9),
    ('…', 10),
    ('"', 11),
    ('(', 12),
    (')', 13),
    ('“', 14),
    ('”', 15),
    (' ', 16),
    ('\u{0303}', 17),
    ('ʣ', 18),
    ('ʥ', 19),
    ('ʦ', 20),
    ('ʨ', 21),
    ('ᵝ', 22),
    ('ꭧ', 23),
    ('A', 24),
    ('I', 25),
    ('O', 31),
    ('Q', 33),
    ('S', 35),
    ('T', 36),
    ('W', 39),
    ('Y', 41),
    ('ᵊ', 42),
    ('a', 43),
    ('b', 44),
    ('c', 45),
    ('d', 46),
    ('e', 47),
    ('f', 48),
    ('h', 50),
    ('i', 51),
    ('j', 52),
    ('k', 53),
    ('l', 54),
    ('m', 55),
    ('n', 56),
    ('o', 57),
    ('p', 58),
    ('q', 59),
    ('r', 60),
    ('s', 61),
    ('t', 62),
    ('u', 63),
    ('v', 64),
    ('w', 65),
    ('x', 66),
    ('y', 67),
    ('z', 68),
    ('ɑ', 69),
    ('ɐ', 70),
    ('ɒ', 71),
    ('æ', 72),
    ('β', 75),
    ('ɔ', 76),
    ('ɕ', 77),
    ('ç', 78),
    ('ɖ', 80),
    ('ð', 81),
    ('ʤ', 82),
    ('ə', 83),
    ('ɚ', 85),
    ('ɛ', 86),
    ('ɜ', 87),
    ('ɟ', 90),
    ('ɡ', 92),
    ('ɥ', 99),
    ('ɨ', 101),
    ('ɪ', 102),
    ('ʝ', 103),
    ('ɯ', 110),
    ('ɰ', 111),
    ('ŋ', 112),
    ('ɳ', 113),
    ('ɲ', 114),
    ('ɴ', 115),
    ('ø', 116),
    ('ɸ', 118),
    ('θ', 119),
    ('œ', 120),
    ('ɹ', 123),
    ('ɾ', 125),
    ('ɻ', 126),
    ('ʁ', 128),
    ('ɽ', 129),
    ('ʂ', 130),
    ('ʃ', 131),
    ('ʈ', 132),
    ('ʧ', 133),
    ('ʊ', 135),
    ('ʋ', 136),
    ('ʌ', 138),
    ('ɣ', 139),
    ('ɤ', 140),
    ('χ', 142),
    ('ʎ', 143),
    ('ʒ', 147),
    ('ʔ', 148),
    ('ˈ', 156),
    ('ˌ', 157),
    ('ː', 158),
    ('ʰ', 162),
    ('ʲ', 164),
    ('↓', 169),
    ('→', 171),
    ('↗', 172),
    ('↘', 173),
    ('ᵻ', 177),
];

pub const IGNORED_SYMBOLS: &[char] = &['\u{200d}', '\u{200c}'];

pub fn is_ignored(symbol: char) -> bool {
    IGNORED_SYMBOLS.contains(&symbol)
}

pub const PAD_TOKEN: i64 = 0;

pub fn token_for(symbol: char) -> Option<i64> {
    PHONEME_VOCAB
        .iter()
        .find(|(candidate, _)| *candidate == symbol)
        .map(|(_, id)| *id)
}

pub fn encode(phonemes: &str) -> Vec<i64> {
    phonemes
        .chars()
        .filter(|symbol| !is_ignored(*symbol))
        .filter_map(token_for)
        .collect()
}

pub fn unsupported_symbols(phonemes: &str) -> Vec<char> {
    let mut missing: Vec<char> = Vec::new();
    for symbol in phonemes.chars() {
        if token_for(symbol).is_none() && !is_ignored(symbol) && !missing.contains(&symbol) {
            missing.push(symbol);
        }
    }
    missing
}

pub fn pad(tokens: &[i64], maximum: usize) -> Result<Vec<i64>, SpeechError> {
    if tokens.is_empty() {
        return Err(SpeechError::EmptyText);
    }
    if tokens.len() > maximum {
        return Err(SpeechError::TooManyTokens {
            count: tokens.len(),
            maximum,
        });
    }
    let mut padded = Vec::with_capacity(tokens.len() + 2);
    padded.push(PAD_TOKEN);
    padded.extend_from_slice(tokens);
    padded.push(PAD_TOKEN);
    Ok(padded)
}

pub fn split_tokens(tokens: &[i64], maximum: usize) -> Vec<Vec<i64>> {
    if maximum == 0 {
        return Vec::new();
    }
    let sentence_breaks: Vec<i64> = ".!?;:".chars().filter_map(token_for).collect();
    let space = token_for(' ');

    let mut chunks = Vec::new();
    let mut rest = tokens;
    while rest.len() > maximum {
        let window = &rest[..maximum];
        let cut = window
            .iter()
            .rposition(|token| sentence_breaks.contains(token))
            .map(|index| index + 1)
            .or_else(|| space.and_then(|space| window.iter().rposition(|token| *token == space)))
            .unwrap_or(maximum)
            .max(1);
        chunks.push(rest[..cut].to_vec());
        rest = &rest[cut..];
    }
    if !rest.is_empty() {
        chunks.push(rest.to_vec());
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vocabulary_has_the_expected_size() {
        assert_eq!(PHONEME_VOCAB.len(), 115);
    }

    #[test]
    fn zero_width_joiners_are_dropped_quietly() {
        assert_eq!(encode("a\u{200d}b"), encode("ab"));
        assert!(unsupported_symbols("a\u{200d}b").is_empty());
        assert_eq!(unsupported_symbols("a\u{00e9}b"), vec!['\u{00e9}']);
    }

    #[test]
    fn vocabulary_has_no_duplicate_symbols_or_ids() {
        for (index, (symbol, id)) in PHONEME_VOCAB.iter().enumerate() {
            for (other_symbol, other_id) in &PHONEME_VOCAB[..index] {
                assert_ne!(symbol, other_symbol, "duplicate symbol {symbol}");
                assert_ne!(id, other_id, "duplicate id {id}");
            }
        }
    }

    #[test]
    fn known_symbols_map_to_their_ids() {
        assert_eq!(token_for('$'), Some(0));
        assert_eq!(token_for(' '), Some(16));
        assert_eq!(token_for('h'), Some(50));
        assert_eq!(token_for('\u{02c8}'), Some(156));
        assert_eq!(token_for('\u{00e9}'), None);
    }

    #[test]
    fn encoding_skips_symbols_outside_the_vocabulary() {
        let encoded = encode("h\u{00e9}l");
        assert_eq!(
            encoded,
            vec![token_for('h').unwrap(), token_for('l').unwrap()]
        );
        assert_eq!(unsupported_symbols("h\u{00e9}l\u{00e9}"), vec!['\u{00e9}']);
    }

    #[test]
    fn padding_brackets_the_sequence() {
        let padded = pad(&[5, 6], 10).expect("padded");
        assert_eq!(padded, vec![PAD_TOKEN, 5, 6, PAD_TOKEN]);
    }

    #[test]
    fn padding_rejects_empty_and_oversized_input() {
        assert!(matches!(pad(&[], 10), Err(SpeechError::EmptyText)));
        assert!(matches!(
            pad(&[1, 2, 3], 2),
            Err(SpeechError::TooManyTokens {
                count: 3,
                maximum: 2
            })
        ));
    }

    #[test]
    fn short_runs_are_not_split() {
        let tokens = encode("hello");
        let chunks = split_tokens(&tokens, 510);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], tokens);
    }

    #[test]
    fn long_runs_split_at_sentence_boundaries() {
        let tokens = encode("hello world. hello again");
        let chunks = split_tokens(&tokens, 14);
        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|chunk| chunk.len() <= 14));
        assert_eq!(chunks.concat(), tokens);
        assert_eq!(chunks[0].last().copied(), token_for('.'));
    }

    #[test]
    fn splitting_falls_back_to_a_hard_cut_without_separators() {
        let tokens = vec![50i64; 25];
        let chunks = split_tokens(&tokens, 10);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks.concat(), tokens);
    }

    #[test]
    fn splitting_an_empty_run_yields_nothing() {
        assert!(split_tokens(&[], 10).is_empty());
        assert!(split_tokens(&[1, 2], 0).is_empty());
    }
}
