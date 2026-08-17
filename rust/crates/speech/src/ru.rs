pub const PRIMARY_STRESS: char = '\u{02c8}';

const COMBINING_ACUTE: char = '\u{0301}';

fn is_vowel(letter: char) -> bool {
    matches!(
        letter,
        'а' | 'е' | 'ё' | 'и' | 'о' | 'у' | 'ы' | 'э' | 'ю' | 'я'
    )
}

fn is_softening_vowel(letter: char) -> bool {
    matches!(letter, 'е' | 'ё' | 'и' | 'ю' | 'я')
}

fn is_always_hard(letter: char) -> bool {
    matches!(letter, 'ж' | 'ш' | 'ц')
}

fn is_always_soft(letter: char) -> bool {
    matches!(letter, 'ч' | 'щ' | 'й')
}

fn is_consonant(letter: char) -> bool {
    matches!(
        letter,
        'б' | 'в'
            | 'г'
            | 'д'
            | 'ж'
            | 'з'
            | 'й'
            | 'к'
            | 'л'
            | 'м'
            | 'н'
            | 'п'
            | 'р'
            | 'с'
            | 'т'
            | 'ф'
            | 'х'
            | 'ц'
            | 'ч'
            | 'ш'
            | 'щ'
    )
}

fn voiced_pair(letter: char) -> Option<char> {
    Some(match letter {
        'б' => 'п',
        'в' => 'ф',
        'г' => 'к',
        'д' => 'т',
        'ж' => 'ш',
        'з' => 'с',
        _ => return None,
    })
}

fn voiceless_pair(letter: char) -> Option<char> {
    Some(match letter {
        'п' => 'б',
        'ф' => 'в',
        'к' => 'г',
        'т' => 'д',
        'ш' => 'ж',
        'с' => 'з',
        'ц' => 'ʣ',
        'ч' => 'ʥ',
        'х' => 'ɣ',
        _ => return None,
    })
}

fn is_voiced_obstruent(letter: char) -> bool {
    voiced_pair(letter).is_some()
}

fn is_voiceless_obstruent(letter: char) -> bool {
    matches!(
        letter,
        'п' | 'ф' | 'к' | 'т' | 'ш' | 'с' | 'х' | 'ц' | 'ч' | 'щ'
    )
}

fn punctuation_token(symbol: char) -> Option<char> {
    Some(match symbol {
        '.' | '\u{2026}' => '.',
        ',' => ',',
        '!' => '!',
        '?' => '?',
        ';' => ';',
        ':' => ':',
        '-' | '\u{2013}' | '\u{2014}' => '-',
        _ => return None,
    })
}

fn transliterate_latin(letter: char) -> &'static str {
    match letter {
        'a' => "а",
        'b' => "б",
        'c' => "к",
        'd' => "д",
        'e' => "е",
        'f' => "ф",
        'g' => "г",
        'h' => "х",
        'i' => "и",
        'j' => "дж",
        'k' => "к",
        'l' => "л",
        'm' => "м",
        'n' => "н",
        'o' => "о",
        'p' => "п",
        'q' => "к",
        'r' => "р",
        's' => "с",
        't' => "т",
        'u' => "у",
        'v' => "в",
        'w' => "в",
        'x' => "кс",
        'y' => "й",
        'z' => "з",
        _ => "",
    }
}

pub fn normalise(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut digits = String::new();

    let flush = |digits: &mut String, output: &mut String| {
        if digits.is_empty() {
            return;
        }
        let words = number_to_words(digits.parse::<u64>().unwrap_or(0));
        if !output.is_empty() && !output.ends_with(' ') {
            output.push(' ');
        }
        output.push_str(&words);
        output.push(' ');
        digits.clear();
    };

    for symbol in text.chars() {
        let symbol = match symbol {
            '\u{0451}' => 'ё',
            '\u{0401}' => 'ё',
            other => other,
        };
        if symbol.is_ascii_digit() {
            if digits.len() < 18 {
                digits.push(symbol);
            }
            continue;
        }
        flush(&mut digits, &mut output);

        if symbol == COMBINING_ACUTE {
            output.push(symbol);
            continue;
        }

        let lowered = symbol.to_lowercase().next().unwrap_or(symbol);
        if ('а'..='я').contains(&lowered) || lowered == 'ё' || lowered == 'ъ' || lowered == 'ь'
        {
            output.push(lowered);
        } else if lowered.is_ascii_alphabetic() {
            output.push_str(transliterate_latin(lowered));
        } else if let Some(mark) = punctuation_token(lowered) {
            output.push(mark);
        } else if lowered.is_whitespace() && !output.ends_with(' ') && !output.is_empty() {
            output.push(' ');
        }
    }
    flush(&mut digits, &mut output);

    output.trim().to_string()
}

const ONES: [&str; 20] = [
    "ноль",
    "один",
    "два",
    "три",
    "четыре",
    "пять",
    "шесть",
    "семь",
    "восемь",
    "девять",
    "десять",
    "одиннадцать",
    "двенадцать",
    "тринадцать",
    "четырнадцать",
    "пятнадцать",
    "шестнадцать",
    "семнадцать",
    "восемнадцать",
    "девятнадцать",
];

const TENS: [&str; 10] = [
    "",
    "",
    "двадцать",
    "тридцать",
    "сорок",
    "пятьдесят",
    "шестьдесят",
    "семьдесят",
    "восемьдесят",
    "девяносто",
];

const HUNDREDS: [&str; 10] = [
    "",
    "сто",
    "двести",
    "триста",
    "четыреста",
    "пятьсот",
    "шестьсот",
    "семьсот",
    "восемьсот",
    "девятьсот",
];

fn plural_form(count: u64) -> usize {
    let last_two = count % 100;
    let last = count % 10;
    if (11..=14).contains(&last_two) || last == 0 || last >= 5 {
        2
    } else if last == 1 {
        0
    } else {
        1
    }
}

fn group_to_words(value: u64, feminine: bool, output: &mut Vec<String>) {
    let hundreds = (value / 100) as usize;
    let remainder = value % 100;
    if hundreds > 0 {
        output.push(HUNDREDS[hundreds].to_string());
    }
    if remainder >= 20 {
        output.push(TENS[(remainder / 10) as usize].to_string());
        let unit = (remainder % 10) as usize;
        if unit > 0 {
            output.push(feminine_unit(unit, feminine));
        }
    } else if remainder > 0 {
        output.push(feminine_unit(remainder as usize, feminine));
    }
}

fn feminine_unit(unit: usize, feminine: bool) -> String {
    match (unit, feminine) {
        (1, true) => "одна".to_string(),
        (2, true) => "две".to_string(),
        _ => ONES[unit].to_string(),
    }
}

pub fn number_to_words(value: u64) -> String {
    if value == 0 {
        return ONES[0].to_string();
    }

    const SCALES: [([&str; 3], bool); 4] = [
        (["", "", ""], false),
        (["тысяча", "тысячи", "тысяч"], true),
        (["миллион", "миллиона", "миллионов"], false),
        (["миллиард", "миллиарда", "миллиардов"], false),
    ];

    let mut groups = Vec::new();
    let mut rest = value;
    while rest > 0 {
        groups.push(rest % 1000);
        rest /= 1000;
    }

    let mut words = Vec::new();
    for index in (0..groups.len()).rev() {
        let group = groups[index];
        if group == 0 {
            continue;
        }
        let (names, feminine) = SCALES[index.min(SCALES.len() - 1)];
        group_to_words(group, feminine, &mut words);
        if index > 0 {
            words.push(names[plural_form(group)].to_string());
        }
    }
    words.join(" ")
}

pub const STRESS_DICTIONARY: &[(&str, usize)] = &[
    ("привет", 1),
    ("здравствуйте", 0),
    ("спасибо", 1),
    ("пожалуйста", 1),
    ("хорошо", 2),
    ("плохо", 0),
    ("сейчас", 1),
    ("сегодня", 1),
    ("завтра", 0),
    ("вчера", 1),
    ("который", 1),
    ("которая", 1),
    ("которые", 1),
    ("человек", 2),
    ("люди", 0),
    ("время", 0),
    ("работа", 1),
    ("работать", 1),
    ("говорить", 2),
    ("сказать", 1),
    ("делать", 0),
    ("сделать", 0),
    ("видео", 0),
    ("аудио", 0),
    ("звук", 0),
    ("речь", 0),
    ("синтез", 0),
    ("синтеза", 0),
    ("тест", 0),
    ("теста", 0),
    ("проект", 1),
    ("проекта", 1),
    ("файл", 0),
    ("файла", 0),
    ("файлы", 0),
    ("экран", 1),
    ("окно", 1),
    ("кнопка", 0),
    ("текст", 0),
    ("текста", 0),
    ("русский", 0),
    ("язык", 1),
    ("языка", 2),
    ("слово", 0),
    ("слова", 1),
    ("буква", 0),
    ("число", 1),
    ("молоко", 2),
    ("хорошая", 1),
    ("большой", 1),
    ("маленький", 0),
    ("новый", 0),
    ("старый", 0),
    ("первый", 0),
    ("второй", 1),
    ("много", 0),
    ("мало", 0),
    ("очень", 0),
    ("почему", 2),
    ("потому", 2),
    ("что", 0),
    ("чтобы", 0),
    ("когда", 1),
    ("сюда", 1),
    ("туда", 1),
    ("здесь", 0),
    ("там", 0),
    ("это", 0),
    ("этот", 0),
    ("эта", 0),
    ("эти", 0),
    ("требует", 0),
    ("требуется", 0),
    ("обеспечение", 2),
    ("обеспечения", 2),
    ("подход", 1),
    ("подхода", 1),
    ("моя", 1),
    ("мой", 0),
    ("моё", 1),
    ("твоя", 1),
    ("свой", 0),
    ("часы", 1),
    ("часа", 1),
    ("вода", 1),
    ("голова", 2),
    ("сторона", 2),
    ("город", 0),
    ("города", 2),
    ("дорога", 1),
    ("машина", 1),
    ("хотеть", 1),
    ("может", 0),
    ("нужно", 0),
    ("надо", 0),
    ("можно", 0),
    ("сколько", 0),
    ("столько", 0),
    ("всегда", 1),
    ("никогда", 2),
    ("иногда", 2),
    ("собака", 1),
    ("кошка", 0),
    ("хлеб", 0),
    ("деньги", 0),
    ("минута", 1),
    ("секунда", 1),
    ("неделя", 1),
    ("месяц", 0),
    ("год", 0),
    ("сюрприз", 1),
    ("компьютер", 1),
    ("программа", 1),
    ("система", 1),
    ("настройка", 1),
    ("настройки", 1),
    ("дорожка", 1),
    ("монтаж", 1),
    ("камера", 0),
    ("готово", 1),
    ("ошибка", 1),
    ("отмена", 1),
    ("сохранить", 2),
    ("открыть", 1),
    ("закрыть", 1),
    ("удалить", 2),
    ("добавить", 1),
];

const STRESS_SUFFIXES: &[(&str, usize)] = &[
    ("ция", 1),
    ("ции", 1),
    ("цию", 1),
    ("циями", 2),
    ("ание", 2),
    ("ания", 2),
    ("аний", 2),
    ("ение", 2),
    ("ения", 2),
    ("ений", 2),
    ("ениями", 3),
    ("ость", 1),
    ("ости", 2),
    ("ически", 1),
    ("ческий", 1),
    ("овать", 0),
    ("ировать", 1),
];

const UNSTRESSED_ENDINGS: &[&str] = &[
    "ого", "его", "ому", "ему", "ыми", "ими", "ая", "яя", "ую", "юю", "ою", "ею", "ые", "ие", "ое",
    "ее", "ый", "ий", "ых", "их", "ым", "им", "ом", "ем",
];

const PRETONIC_AFFIXES: &[&str] = &["тельн", "тель"];

fn vowel_positions(letters: &[char]) -> Vec<usize> {
    letters
        .iter()
        .enumerate()
        .filter(|(_, letter)| is_vowel(**letter))
        .map(|(index, _)| index)
        .collect()
}

pub fn stress_position(word: &str) -> Option<usize> {
    let letters: Vec<char> = word.chars().filter(|c| *c != COMBINING_ACUTE).collect();
    let vowels = vowel_positions(&letters);
    if vowels.is_empty() {
        return None;
    }
    if vowels.len() == 1 {
        return Some(0);
    }

    let mut seen = 0usize;
    let mut previous_was_vowel = false;
    for symbol in word.chars() {
        if symbol == COMBINING_ACUTE {
            if previous_was_vowel {
                return Some(seen - 1);
            }
            continue;
        }
        if is_vowel(symbol) {
            seen += 1;
            previous_was_vowel = true;
        } else {
            previous_was_vowel = false;
        }
    }

    if let Some(index) = vowels.iter().position(|at| letters[*at] == 'ё') {
        return Some(index);
    }

    let plain: String = letters.iter().collect();
    if let Some((_, index)) = STRESS_DICTIONARY.iter().find(|(entry, _)| *entry == plain) {
        return Some((*index).min(vowels.len() - 1));
    }

    for (suffix, from_end) in STRESS_SUFFIXES {
        if plain.ends_with(suffix) && vowels.len() > *from_end {
            return Some(vowels.len() - 1 - from_end);
        }
    }

    for affix in PRETONIC_AFFIXES {
        if let Some(byte) = plain.find(affix) {
            let before = plain[..byte].chars().filter(|c| is_vowel(*c)).count();
            if before > 0 {
                return Some(before - 1);
            }
        }
    }

    for ending in UNSTRESSED_ENDINGS {
        let Some(stem) = plain.strip_suffix(ending) else {
            continue;
        };
        let stem_vowels = stem.chars().filter(|c| is_vowel(*c)).count();
        if stem_vowels > 0 {
            return Some(stem_vowels - 1);
        }
    }

    Some(vowels.len() - 2)
}

fn apply_clusters(word: &str) -> String {
    let mut text = word.to_string();

    if text.chars().count() > 3 {
        if let Some(stem) = text.strip_suffix("ого") {
            text = format!("{stem}ово");
        } else if let Some(stem) = text.strip_suffix("его") {
            text = format!("{stem}ево");
        }
    }
    if text == "что" || text == "чтобы" || text == "конечно" || text == "скучно"
    {
        text = match text.as_str() {
            "что" => "што".to_string(),
            "чтобы" => "штобы".to_string(),
            "конечно" => "конешно".to_string(),
            _ => "скушно".to_string(),
        };
    }

    if text.ends_with("тся") {
        let keep = text.len() - "тся".len();
        text = format!("{}ца", &text[..keep]);
    } else if text.ends_with("ться") {
        let keep = text.len() - "ться".len();
        text = format!("{}ца", &text[..keep]);
    }

    const REPLACEMENTS: &[(&str, &str)] = &[
        ("сч", "щ"),
        ("зч", "щ"),
        ("жч", "щ"),
        ("здн", "зн"),
        ("стн", "сн"),
        ("стл", "сл"),
        ("рдц", "рц"),
        ("лнц", "нц"),
        ("вств", "ств"),
        ("гк", "хк"),
        ("гч", "хч"),
        ("тс", "ц"),
        ("дс", "ц"),
        ("тц", "ц"),
        ("дц", "ц"),
    ];
    for (from, to) in REPLACEMENTS {
        if text.contains(from) {
            text = text.replace(from, to);
        }
    }

    let letters: Vec<char> = text.chars().collect();
    let mut collapsed = String::with_capacity(text.len());
    for (index, letter) in letters.iter().enumerate() {
        if index > 0 && letters[index - 1] == *letter && is_consonant(*letter) {
            continue;
        }
        collapsed.push(*letter);
    }
    collapsed
}

fn apply_voicing(letters: &[char]) -> Vec<char> {
    let mut output = letters.to_vec();
    for index in (0..output.len()).rev() {
        let letter = output[index];
        if !is_consonant(letter) {
            continue;
        }
        let mut next = None;
        for candidate in output.iter().skip(index + 1) {
            if *candidate == 'ъ' || *candidate == 'ь' {
                continue;
            }
            next = Some(*candidate);
            break;
        }

        match next {
            None => {
                if let Some(voiceless) = voiced_pair(letter) {
                    output[index] = voiceless;
                }
            }
            Some(following) if is_consonant(following) => {
                if following == 'в' || matches!(following, 'м' | 'н' | 'л' | 'р' | 'й') {
                    continue;
                }
                if is_voiced_obstruent(following) {
                    if let Some(voiced) = voiceless_pair(letter) {
                        output[index] = voiced;
                    }
                } else if is_voiceless_obstruent(following) {
                    if let Some(voiceless) = voiced_pair(letter) {
                        output[index] = voiceless;
                    }
                }
            }
            _ => {}
        }
    }
    output
}

fn consonant_symbol(letter: char, soft: bool) -> &'static str {
    match letter {
        'б' => "b",
        'в' => "v",
        'г' => "ɡ",
        'д' => "d",
        'ж' => "ʐ",
        'з' => "z",
        'й' => "j",
        'к' => "k",
        'л' => {
            if soft {
                "l"
            } else {
                "ɫ"
            }
        }
        'м' => "m",
        'н' => "n",
        'п' => "p",
        'р' => "r",
        'с' => "s",
        'т' => "t",
        'ф' => "f",
        'х' => "x",
        'ц' => "ts",
        'ч' => "tɕ",
        'ш' => "ʂ",
        'щ' => "ɕː",
        'ʣ' => "dz",
        'ʥ' => "dʑ",
        'ɣ' => "ɣ",
        _ => "",
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Reduction {
    Stressed,
    Pretonic,
    Reduced,
}

pub fn word_to_ipa(word: &str) -> String {
    let stressed_vowel = stress_position(word);
    let cleaned: String = word.chars().filter(|c| *c != COMBINING_ACUTE).collect();
    let letters: Vec<char> = apply_clusters(&cleaned).chars().collect();
    if letters.is_empty() {
        return String::new();
    }

    let vowels = vowel_positions(&letters);
    if vowels.is_empty() {
        let voiced = apply_voicing(&letters);
        let mut output = String::new();
        for letter in voiced {
            output.push_str(consonant_symbol(letter, false));
        }
        return output;
    }
    let stressed_vowel = stressed_vowel
        .unwrap_or(0)
        .min(vowels.len().saturating_sub(1));

    let letters = apply_voicing(&letters);
    let mut output = String::new();
    let mut vowel_ordinal = 0usize;

    for index in 0..letters.len() {
        let letter = letters[index];

        if letter == 'ъ' || letter == 'ь' {
            continue;
        }

        if is_consonant(letter) {
            let mut soft = is_always_soft(letter);
            if !soft && !is_always_hard(letter) {
                let following = letters.get(index + 1).copied();
                soft = matches!(following, Some(next) if is_softening_vowel(next) || next == 'ь');
            }
            output.push_str(consonant_symbol(letter, soft));
            if soft && !is_always_soft(letter) && !is_always_hard(letter) {
                output.push('\u{02b2}');
            }
            continue;
        }

        if !is_vowel(letter) {
            continue;
        }

        let reduction = if vowel_ordinal == stressed_vowel {
            Reduction::Stressed
        } else if vowel_ordinal + 1 == stressed_vowel || index == 0 {
            Reduction::Pretonic
        } else {
            Reduction::Reduced
        };

        let previous = letters[..index].iter().rev().find(|c| **c != '\u{0301}');
        let iotates = is_softening_vowel(letter)
            && letter != 'и'
            && match previous {
                None => true,
                Some('ъ') | Some('ь') => true,
                Some(other) => is_vowel(*other),
            };
        let hard_context = matches!(previous, Some(other) if is_always_hard(*other));

        if iotates {
            output.push('j');
        }
        if reduction == Reduction::Stressed {
            output.push(PRIMARY_STRESS);
        }
        output.push_str(vowel_symbol(letter, reduction, hard_context, previous));
        vowel_ordinal += 1;
    }

    output
}

fn vowel_symbol(
    letter: char,
    reduction: Reduction,
    hard_context: bool,
    previous: Option<&char>,
) -> &'static str {
    let after_soft = matches!(previous, Some(other) if is_always_soft(*other));
    match (letter, reduction) {
        ('а', Reduction::Stressed) => "a",
        ('а', Reduction::Pretonic) => {
            if after_soft {
                "ɪ"
            } else {
                "ɐ"
            }
        }
        ('а', Reduction::Reduced) => {
            if after_soft {
                "ɪ"
            } else {
                "ə"
            }
        }
        ('о', Reduction::Stressed) => "o",
        ('о', Reduction::Pretonic) => "ɐ",
        ('о', Reduction::Reduced) => "ə",
        ('у', Reduction::Stressed) | ('ю', Reduction::Stressed) => "u",
        ('у', _) | ('ю', _) => "ʊ",
        ('ы', Reduction::Stressed) => "ɨ",
        ('ы', _) => "ɨ",
        ('и', Reduction::Stressed) => {
            if hard_context {
                "ɨ"
            } else {
                "i"
            }
        }
        ('и', _) => {
            if hard_context {
                "ɨ"
            } else {
                "ɪ"
            }
        }
        ('э', Reduction::Stressed) => "ɛ",
        ('э', _) => "ɪ",
        ('е', Reduction::Stressed) => {
            if hard_context {
                "ɛ"
            } else {
                "e"
            }
        }
        ('е', _) => {
            if hard_context {
                "ɨ"
            } else {
                "ɪ"
            }
        }
        ('ё', _) => "o",
        ('я', Reduction::Stressed) => "a",
        ('я', _) => "ɪ",
        _ => "",
    }
}

pub fn phonemize(text: &str) -> String {
    let normalised = normalise(text);
    let mut output = String::new();

    let mut word = String::new();
    let flush = |word: &mut String, output: &mut String| {
        if word.is_empty() {
            return;
        }
        let ipa = word_to_ipa(word);
        if !ipa.is_empty() {
            if !output.is_empty() && !output.ends_with(' ') {
                output.push(' ');
            }
            output.push_str(&ipa);
        }
        word.clear();
    };

    for symbol in normalised.chars() {
        if symbol == ' ' {
            flush(&mut word, &mut output);
        } else if punctuation_token(symbol).is_some() && !is_consonant(symbol) {
            flush(&mut word, &mut output);
            output.push(symbol);
        } else {
            word.push(symbol);
        }
    }
    flush(&mut word, &mut output);

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalisation_lowercases_and_keeps_punctuation() {
        assert_eq!(normalise("Привет, мир!"), "привет, мир!");
        assert_eq!(normalise("  Да   \n нет  "), "да нет");
        assert_eq!(normalise("Ё Ж"), "ё ж");
    }

    #[test]
    fn normalisation_drops_unknown_symbols() {
        assert_eq!(normalise("те*кст"), "текст");
        assert_eq!(normalise("«цитата»"), "цитата");
        assert_eq!(normalise(""), "");
        assert_eq!(normalise("@#$%"), "");
    }

    #[test]
    fn latin_is_transliterated() {
        assert_eq!(normalise("video"), "видео");
        assert_eq!(normalise("cutix"), "кутикс");
    }

    #[test]
    fn digits_become_russian_words() {
        assert_eq!(number_to_words(0), "ноль");
        assert_eq!(number_to_words(1), "один");
        assert_eq!(number_to_words(13), "тринадцать");
        assert_eq!(number_to_words(42), "сорок два");
        assert_eq!(number_to_words(100), "сто");
        assert_eq!(number_to_words(256), "двести пятьдесят шесть");
        assert_eq!(number_to_words(1000), "одна тысяча");
        assert_eq!(number_to_words(2000), "две тысячи");
        assert_eq!(number_to_words(5000), "пять тысяч");
        assert_eq!(number_to_words(1_000_000), "один миллион");
    }

    #[test]
    fn digits_inside_text_are_expanded() {
        assert_eq!(normalise("мне 42 года"), "мне сорок два года");
        assert!(normalise("год 2024").contains("тысячи"));
    }

    #[test]
    fn single_vowel_words_are_stressed_on_it() {
        assert_eq!(stress_position("дом"), Some(0));
        assert_eq!(stress_position("кот"), Some(0));
        assert_eq!(stress_position("мир"), Some(0));
    }

    #[test]
    fn words_without_vowels_have_no_stress() {
        assert_eq!(stress_position("вжх"), None);
        assert_eq!(stress_position(""), None);
    }

    #[test]
    fn yo_always_takes_the_stress() {
        assert_eq!(stress_position("самолёт"), Some(2));
        assert_eq!(stress_position("тёмный"), Some(0));
    }

    #[test]
    fn explicit_accent_mark_wins() {
        assert_eq!(stress_position("молоко\u{0301}"), Some(2));
        assert_eq!(stress_position("за\u{0301}мок"), Some(0));
        assert_eq!(stress_position("замо\u{0301}к"), Some(1));
    }

    #[test]
    fn dictionary_entries_override_the_heuristic() {
        assert_eq!(stress_position("привет"), Some(1));
        assert_eq!(stress_position("молоко"), Some(2));
        assert_eq!(stress_position("хорошо"), Some(2));
        assert_eq!(stress_position("это"), Some(0));
    }

    #[test]
    fn unstressed_endings_are_stripped_before_guessing() {
        assert_eq!(stress_position("нового"), Some(0));
        assert_eq!(stress_position("программного"), Some(1));
        assert_eq!(stress_position("постоянного"), Some(2));
        assert_eq!(stress_position("местный"), Some(0));
    }

    #[test]
    fn tel_affix_takes_the_stress_wherever_it_sits() {
        assert_eq!(stress_position("внимательного"), Some(1));
        assert_eq!(stress_position("писатель"), Some(1));
    }

    #[test]
    fn nie_nouns_stress_in_front_of_the_ending() {
        assert_eq!(stress_position("обучения"), Some(2));
        assert_eq!(stress_position("решения"), Some(1));
        assert_eq!(stress_position("название"), Some(1));
    }

    #[test]
    fn unknown_words_fall_back_to_the_penultimate_vowel() {
        assert_eq!(stress_position("барабака"), Some(2));
    }

    #[test]
    fn palatalisation_is_marked_before_soft_vowels() {
        let ipa = word_to_ipa("тётя");
        assert!(ipa.starts_with("tʲ"), "{ipa}");
        assert!(
            word_to_ipa("день").contains("nʲ"),
            "{}",
            word_to_ipa("день")
        );
        assert!(
            word_to_ipa("мир").starts_with("mʲ"),
            "{}",
            word_to_ipa("мир")
        );
    }

    #[test]
    fn always_hard_consonants_never_palatalise() {
        for (word, hard) in [("жир", "ʐ"), ("шить", "ʂ"), ("цирк", "ts")] {
            let ipa = word_to_ipa(word);
            assert!(
                !ipa.contains(&format!("{hard}\u{02b2}")),
                "{word} -> {ipa} palatalised a hard consonant"
            );
            assert!(ipa.contains('ɨ'), "{word} -> {ipa} should back the vowel");
        }
        assert_eq!(word_to_ipa("шить"), "ʂˈɨtʲ");
    }

    #[test]
    fn hard_l_and_soft_l_differ() {
        assert!(
            word_to_ipa("лампа").starts_with('ɫ'),
            "{}",
            word_to_ipa("лампа")
        );
        assert!(
            word_to_ipa("лес").starts_with("lʲ"),
            "{}",
            word_to_ipa("лес")
        );
    }

    #[test]
    fn final_obstruents_are_devoiced() {
        assert!(word_to_ipa("дуб").ends_with('p'), "{}", word_to_ipa("дуб"));
        assert!(word_to_ipa("год").ends_with('t'), "{}", word_to_ipa("год"));
        assert!(word_to_ipa("нож").ends_with('ʂ'), "{}", word_to_ipa("нож"));
        assert!(
            word_to_ipa("глаз").ends_with('s'),
            "{}",
            word_to_ipa("глаз")
        );
    }

    #[test]
    fn voicing_assimilates_regressively() {
        assert!(
            word_to_ipa("сделать").starts_with('z'),
            "{}",
            word_to_ipa("сделать")
        );
        assert!(
            word_to_ipa("лодка").contains('t'),
            "{}",
            word_to_ipa("лодка")
        );
        assert!(
            !word_to_ipa("лодка").contains('d'),
            "{}",
            word_to_ipa("лодка")
        );
    }

    #[test]
    fn sonorants_do_not_trigger_voicing() {
        let ipa = word_to_ipa("отмель");
        assert!(ipa.contains('t'), "{ipa}");
        assert!(!ipa.contains('d'), "{ipa}");
    }

    #[test]
    fn unstressed_o_reduces() {
        let ipa = word_to_ipa("молоко");
        assert_eq!(ipa, "məɫɐkˈo");
    }

    #[test]
    fn unstressed_a_after_soft_becomes_i() {
        assert_eq!(word_to_ipa("часы"), "tɕɪsˈɨ");
    }

    #[test]
    fn iotation_happens_at_word_start_and_after_vowels() {
        assert!(word_to_ipa("я").starts_with('j'), "{}", word_to_ipa("я"));
        assert_eq!(word_to_ipa("моя"), "mɐjˈa");
        assert!(
            word_to_ipa("объект").contains('j'),
            "{}",
            word_to_ipa("объект")
        );
        assert!(
            !word_to_ipa("тётя").starts_with('j'),
            "{}",
            word_to_ipa("тётя")
        );
    }

    #[test]
    fn known_words_transcribe_as_expected() {
        assert_eq!(word_to_ipa("привет"), "prʲɪvʲˈet");
        assert_eq!(word_to_ipa("это"), "ˈɛtə");
        assert_eq!(word_to_ipa("тест"), "tʲˈest");
        assert_eq!(word_to_ipa("дом"), "dˈom");
        assert_eq!(word_to_ipa("мир"), "mʲˈir");
    }

    #[test]
    fn clusters_are_rewritten() {
        assert!(
            word_to_ipa("счастье").starts_with('ɕ'),
            "{}",
            word_to_ipa("счастье")
        );
        assert!(
            word_to_ipa("что").starts_with('ʂ'),
            "{}",
            word_to_ipa("что")
        );
        assert!(
            word_to_ipa("нового").ends_with('ə'),
            "{}",
            word_to_ipa("нового")
        );
        assert!(
            word_to_ipa("нового").contains('v'),
            "{}",
            word_to_ipa("нового")
        );
        assert!(
            !word_to_ipa("местный").contains('t'),
            "{}",
            word_to_ipa("местный")
        );
    }

    #[test]
    fn reflexive_ending_is_hard_tsa() {
        let ipa = word_to_ipa("учится");
        assert!(ipa.ends_with("tsə"), "{ipa}");
        assert!(!ipa.contains('\u{02b2}') || !ipa.ends_with("tsʲə"), "{ipa}");
    }

    #[test]
    fn every_word_carries_exactly_one_stress_mark() {
        for word in [
            "привет",
            "молоко",
            "здравствуйте",
            "синтез",
            "речи",
            "компьютер",
        ] {
            let ipa = word_to_ipa(word);
            assert_eq!(
                ipa.chars().filter(|c| *c == PRIMARY_STRESS).count(),
                1,
                "{word} -> {ipa}"
            );
        }
    }

    #[test]
    fn sentence_keeps_punctuation_as_separate_tokens() {
        let ipa = phonemize("Привет, это тест.");
        assert!(ipa.contains(','), "{ipa}");
        assert!(ipa.ends_with('.'), "{ipa}");
        assert!(ipa.contains(' '), "{ipa}");
        assert_eq!(
            ipa.chars().filter(|c| *c == PRIMARY_STRESS).count(),
            3,
            "{ipa}"
        );
    }

    #[test]
    fn empty_and_punctuation_only_input_yields_nothing_speakable() {
        assert_eq!(phonemize(""), "");
        assert_eq!(phonemize("   "), "");
        assert_eq!(phonemize("!!!"), "!!!");
        assert_eq!(phonemize("@@@"), "");
    }

    #[test]
    fn output_stays_inside_the_expected_inventory() {
        const ALLOWED: &str = "abdefijklmnoprstuvxzɐəɛɡɨɪɫʂʐʑɕʊʣʥɣːʲˈdz ,.!?;:-";
        let ipa = phonemize(
            "Привет, это тест синтеза речи. Двадцать пять файлов, 42 проекта; всё готово!",
        );
        let unexpected: Vec<char> = ipa.chars().filter(|c| !ALLOWED.contains(*c)).collect();
        assert!(unexpected.is_empty(), "unexpected {unexpected:?} in {ipa}");
    }
}
