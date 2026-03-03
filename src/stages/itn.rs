use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashMap;
use text2num::{replace_numbers_in_text, Language};

lazy_static! {
    static ref EN_LANG: Language = Language::english();

    // Polish ITN resources
    static ref PL_NUMBER_WORDS: HashMap<&'static str, u64> = {
        let mut m = HashMap::new();
        // Units
        m.insert("zero", 0);
        m.insert("jeden", 1); m.insert("jedna", 1); m.insert("jedno", 1);
        m.insert("dwa", 2); m.insert("dwie", 2); m.insert("dwaj", 2);
        m.insert("trzy", 3);
        m.insert("cztery", 4);
        m.insert("pięć", 5);
        m.insert("sześć", 6);
        m.insert("siedem", 7);
        m.insert("osiem", 8);
        m.insert("dziewięć", 9);
        // Teens
        m.insert("dziesięć", 10);
        m.insert("jedenaście", 11);
        m.insert("dwanaście", 12);
        m.insert("trzynaście", 13);
        m.insert("czternaście", 14);
        m.insert("piętnaście", 15);
        m.insert("szesnaście", 16);
        m.insert("siedemnaście", 17);
        m.insert("osiemnaście", 18);
        m.insert("dziewiętnaście", 19);
        // Tens
        m.insert("dwadzieścia", 20);
        m.insert("trzydzieści", 30);
        m.insert("czterdzieści", 40);
        m.insert("pięćdziesiąt", 50);
        m.insert("sześćdziesiąt", 60);
        m.insert("siedemdziesiąt", 70);
        m.insert("osiemdziesiąt", 80);
        m.insert("dziewięćdziesiąt", 90);
        // Hundreds
        m.insert("sto", 100);
        m.insert("dwieście", 200);
        m.insert("trzysta", 300);
        m.insert("czterysta", 400);
        m.insert("pięćset", 500);
        m.insert("sześćset", 600);
        m.insert("siedemset", 700);
        m.insert("osiemset", 800);
        m.insert("dziewięćset", 900);
        m
    };

    static ref PL_MULTIPLIERS: HashMap<&'static str, u64> = {
        let mut m = HashMap::new();
        m.insert("tysiąc", 1000);
        m.insert("tysiące", 1000);
        m.insert("tysięcy", 1000);
        m.insert("milion", 1_000_000);
        m.insert("miliony", 1_000_000);
        m.insert("milionów", 1_000_000);
        m.insert("miliard", 1_000_000_000);
        m.insert("miliardy", 1_000_000_000);
        m.insert("miliardów", 1_000_000_000);
        m
    };

    // Regex to split text into words, preserving spaces
    static ref TOKENIZER_RE: Regex = Regex::new(r"[\w\u00C0-\u017F]+|[^\w\u00C0-\u017F]+").unwrap();
}

pub fn inverse_text_normalize(text: &str, lang: &str) -> String {
    match lang {
        "en" => replace_numbers_in_text(text, &*EN_LANG, 0.0),
        "pl" => apply_polish_itn(text),
        _ => text.to_string(),
    }
}

fn apply_polish_itn(text: &str) -> String {
    let mut result = String::new();
    let mut current_number_words: Vec<String> = Vec::new();
    let mut parsing_number = false;

    // Tokenize
    for cap in TOKENIZER_RE.captures_iter(text) {
        let token = cap.get(0).unwrap().as_str();
        let lower_token = token.to_lowercase();

        let is_number_word = PL_NUMBER_WORDS.contains_key(lower_token.as_str())
            || PL_MULTIPLIERS.contains_key(lower_token.as_str());

        // Skip purely whitespace tokens when deciding if we are "inside" a number sequence
        // but keep them in the accumulator if we are
        let is_space = token.trim().is_empty();

        if is_number_word {
            parsing_number = true;
            current_number_words.push(token.to_string());
        } else if is_space && parsing_number {
            current_number_words.push(token.to_string());
        } else {
            // End of sequence
            if parsing_number {
                let (val, ok) = parse_polish_number_sequence(&current_number_words);
                if ok {
                    result.push_str(&val.to_string());
                } else {
                    // Fallback: restore original text if parsing failed (unlikely here)
                    result.push_str(&current_number_words.join(""));
                }
                current_number_words.clear();
                parsing_number = false;
            }
            result.push_str(token);
        }
    }

    // Flush remaining
    if parsing_number {
        let (val, ok) = parse_polish_number_sequence(&current_number_words);
        if ok {
            result.push_str(&val.to_string());
        } else {
            result.push_str(&current_number_words.join(""));
        }
    }

    result
}

fn parse_polish_number_sequence(words: &[String]) -> (u64, bool) {
    let mut total_value: u64 = 0;
    let mut current_segment: u64 = 0;

    // We clean words (remove spaces)
    let clean_words: Vec<String> = words
        .iter()
        .map(|w| w.trim())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_lowercase())
        .collect();

    if clean_words.is_empty() {
        return (0, false);
    }

    for word in clean_words {
        if let Some(&val) = PL_NUMBER_WORDS.get(word.as_str()) {
            current_segment += val;
        } else if let Some(&mult) = PL_MULTIPLIERS.get(word.as_str()) {
            if current_segment == 0 {
                current_segment = 1;
            }
            // If multiplier is greater than previous multipliers (not strictly enforced here but generally),
            // we multiply segment and add to total.
            // Simplified logic:
            // "dwa tysiące" -> 2 * 1000 -> add 2000 to total, reset segment
            // "sto milionów" -> 100 * 1000000 -> add to total, reset segment
            // "dwa tysiące pięćset" -> 2000 added, then 500 in segment.

            // Logic:
            // If total_value has a larger multiplier, we just add.
            // E.g. "milion tysiąc" is invalid.
            // "dwa miliony trzysta tysięcy"
            // 1. dwa (2)
            // 2. miliony (*10^6) -> total += 2*10^6, segment=0
            // 3. trzysta (300)
            // 4. tysięcy (*1000) -> total += 300*1000, segment=0

            total_value += current_segment * mult;
            current_segment = 0;
        }
    }

    total_value += current_segment;
    (total_value, true)
}
