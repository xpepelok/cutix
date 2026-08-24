use std::collections::HashMap;

use crate::MlError;

pub fn bytes_to_unicode() -> Vec<char> {
    let mut table = vec!['\u{0}'; 256];
    let mut used = Vec::new();
    for byte in (b'!'..=b'~').chain(0xa1u8..=0xac).chain(0xaeu8..=0xff) {
        used.push(byte);
    }
    for &byte in &used {
        table[byte as usize] = char::from_u32(byte as u32).expect("latin1 char");
    }
    let mut next = 256u32;
    for (byte, slot) in table.iter_mut().enumerate() {
        if !used.contains(&(byte as u8)) {
            *slot = char::from_u32(next).expect("bmp char");
            next += 1;
        }
    }
    table
}

pub fn unicode_to_bytes() -> HashMap<char, u8> {
    bytes_to_unicode()
        .into_iter()
        .enumerate()
        .map(|(byte, character)| (character, byte as u8))
        .collect()
}

pub struct Tokenizer {
    pieces: HashMap<u32, String>,
    specials: HashMap<u32, String>,
    special_ids: HashMap<String, u32>,
    decoder: HashMap<char, u8>,
}

impl Tokenizer {
    /// Whether this id is a control token rather than text.
    ///
    /// Only the tests ask; compiled for them alone.
    #[cfg(test)]
    pub fn is_special(&self, id: u32) -> bool {
        self.specials.contains_key(&id)
    }

    pub fn from_json(vocab: &str, added: &str) -> Result<Self, MlError> {
        let vocab: serde_json::Value = serde_json::from_str(vocab)
            .map_err(|error| MlError::Tokenizer(format!("vocab.json: {error}")))?;
        let added: serde_json::Value = serde_json::from_str(added)
            .map_err(|error| MlError::Tokenizer(format!("added_tokens.json: {error}")))?;

        let vocab = vocab
            .as_object()
            .ok_or_else(|| MlError::Tokenizer("vocab.json is not an object".to_string()))?;
        let added = added
            .as_object()
            .ok_or_else(|| MlError::Tokenizer("added_tokens.json is not an object".to_string()))?;

        let mut pieces = HashMap::new();
        for (token, id) in vocab {
            let id = id
                .as_u64()
                .ok_or_else(|| MlError::Tokenizer(format!("vocab entry {token} is not an id")))?;
            pieces.insert(id as u32, token.clone());
        }

        let mut specials = HashMap::new();
        let mut special_ids = HashMap::new();
        for (token, id) in added {
            let id = id.as_u64().ok_or_else(|| {
                MlError::Tokenizer(format!("added_tokens entry {token} is not an id"))
            })? as u32;
            specials.insert(id, token.clone());
            special_ids.insert(token.clone(), id);
            pieces.remove(&id);
        }

        if pieces.is_empty() {
            return Err(MlError::Tokenizer("vocabulary is empty".to_string()));
        }

        Ok(Self {
            pieces,
            specials,
            special_ids,
            decoder: unicode_to_bytes(),
        })
    }

    pub fn special_id(&self, token: &str) -> Option<u32> {
        self.special_ids.get(token).copied()
    }

    pub fn special_name(&self, id: u32) -> Option<&str> {
        self.specials.get(&id).map(String::as_str)
    }

    pub fn language_code(&self, id: u32) -> Option<String> {
        let name = self.special_name(id)?;
        let inner = name.strip_prefix("<|")?.strip_suffix("|>")?;
        if inner.len() <= 3 && inner.chars().all(|c| c.is_ascii_lowercase()) {
            Some(inner.to_string())
        } else {
            None
        }
    }

    pub fn decode(&self, ids: &[u32]) -> String {
        let mut bytes = Vec::new();
        for id in ids {
            let Some(piece) = self.pieces.get(id) else {
                continue;
            };
            for character in piece.chars() {
                match self.decoder.get(&character) {
                    Some(byte) => bytes.push(*byte),
                    None => {
                        let mut buffer = [0u8; 4];
                        bytes.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
                    }
                }
            }
        }
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Tokenizer {
        let table = bytes_to_unicode();
        let mut vocab = serde_json::Map::new();
        let mut next_id = 0u64;
        let mut add = |text: &str, vocab: &mut serde_json::Map<String, serde_json::Value>| {
            let encoded: String = text.bytes().map(|byte| table[byte as usize]).collect();
            vocab.insert(encoded, serde_json::Value::from(next_id));
            next_id += 1;
        };
        add("Hello", &mut vocab);
        add(" world", &mut vocab);
        add("Привет", &mut vocab);
        add(" мир", &mut vocab);
        add("!", &mut vocab);
        let added = serde_json::json!({
            "<|endoftext|>": 500,
            "<|startoftranscript|>": 501,
            "<|ru|>": 502,
            "<|transcribe|>": 503,
        });
        Tokenizer::from_json(
            &serde_json::Value::Object(vocab).to_string(),
            &added.to_string(),
        )
        .expect("tokenizer")
    }

    #[test]
    fn byte_table_covers_every_byte_uniquely() {
        let table = bytes_to_unicode();
        assert_eq!(table.len(), 256);
        let reverse = unicode_to_bytes();
        assert_eq!(reverse.len(), 256);
        for byte in 0..256usize {
            assert_eq!(reverse[&table[byte]], byte as u8);
        }
    }

    #[test]
    fn ascii_round_trips() {
        let tokenizer = fixture();
        assert_eq!(tokenizer.decode(&[0, 1, 4]), "Hello world!");
    }

    #[test]
    fn cyrillic_round_trips() {
        let tokenizer = fixture();
        assert_eq!(tokenizer.decode(&[2, 3, 4]), "Привет мир!");
    }

    #[test]
    fn special_tokens_are_not_decoded_as_text() {
        let tokenizer = fixture();
        assert_eq!(tokenizer.decode(&[0, 500, 1]), "Hello world");
        assert!(tokenizer.is_special(500));
        assert!(!tokenizer.is_special(0));
    }

    #[test]
    fn special_tokens_resolve_both_ways() {
        let tokenizer = fixture();
        assert_eq!(tokenizer.special_id("<|startoftranscript|>"), Some(501));
        assert_eq!(tokenizer.special_name(503), Some("<|transcribe|>"));
        assert_eq!(tokenizer.language_code(502).as_deref(), Some("ru"));
        assert_eq!(tokenizer.language_code(501), None);
    }
}
