use std::path::{Path, PathBuf};
use lazy_static::lazy_static;
use regex::Regex;
use spellbook::Dictionary;
use tracing::{debug, warn, info};

lazy_static! {
    /// Polish: insert comma before subordinating/coordinating conjunctions
    /// when not already preceded by a comma.
    static ref PL_COMMA_RE: Regex = Regex::new(
        r"(?i)\b(\w+)\s+(ale|lecz|jednak|natomiast|bo|gdyż|ponieważ|więc|zatem|tedy|albowiem)\b"
    ).unwrap();

    /// English contractions (unambiguous cases only)
    static ref EN_CONTRACTIONS: Vec<(Regex, &'static str)> = vec![
        (Regex::new(r"(?i)\bdont\b").unwrap(), "don't"),
        (Regex::new(r"(?i)\bcant\b").unwrap(), "can't"),
        (Regex::new(r"(?i)\bwont\b").unwrap(), "won't"),
        (Regex::new(r"(?i)\bshouldnt\b").unwrap(), "shouldn't"),
        (Regex::new(r"(?i)\bcouldnt\b").unwrap(), "couldn't"),
        (Regex::new(r"(?i)\bwouldnt\b").unwrap(), "wouldn't"),
        (Regex::new(r"(?i)\bisnt\b").unwrap(), "isn't"),
        (Regex::new(r"(?i)\barent\b").unwrap(), "aren't"),
        (Regex::new(r"(?i)\bwasnt\b").unwrap(), "wasn't"),
        (Regex::new(r"(?i)\bwerent\b").unwrap(), "weren't"),
        (Regex::new(r"(?i)\bhasnt\b").unwrap(), "hasn't"),
        (Regex::new(r"(?i)\bhavent\b").unwrap(), "haven't"),
        (Regex::new(r"(?i)\bhadnt\b").unwrap(), "hadn't"),
        (Regex::new(r"(?i)\bdidnt\b").unwrap(), "didn't"),
        (Regex::new(r"(?i)\bdoesnt\b").unwrap(), "doesn't"),
    ];
}

pub struct GrammarCorrector {
    pl_dict: Option<Dictionary>,
    en_dict: Option<Dictionary>,
}

impl GrammarCorrector {
    pub fn new(dict_dir: PathBuf) -> Self {
        let pl_dict = load_dictionary(&dict_dir, "pl");
        let en_dict = load_dictionary(&dict_dir, "en");
        Self { pl_dict, en_dict }
    }

    pub fn correct(&self, text: &str, lang: &str) -> String {
        let mut result = text.to_string();

        // 1. Spell check
        let dict = match lang {
            "pl" => self.pl_dict.as_ref(),
            "en" => self.en_dict.as_ref(),
            _ => None,
        };

        if let Some(dict) = dict {
            result = spell_check(&result, dict);
        }

        // 2. Pattern rules
        match lang {
            "pl" => result = apply_pl_commas(&result),
            "en" => result = apply_en_contractions(&result),
            _ => {}
        }

        result
    }
}

fn load_dictionary(dict_dir: &Path, lang: &str) -> Option<Dictionary> {
    let aff_path = dict_dir.join(lang).join("index.aff");
    let dic_path = dict_dir.join(lang).join("index.dic");

    let aff = match std::fs::read_to_string(&aff_path) {
        Ok(s) => s,
        Err(e) => {
            warn!("Failed to read {}: {} - {} spell checking disabled", aff_path.display(), e, lang);
            return None;
        }
    };
    let dic = match std::fs::read_to_string(&dic_path) {
        Ok(s) => s,
        Err(e) => {
            warn!("Failed to read {}: {} - {} spell checking disabled", dic_path.display(), e, lang);
            return None;
        }
    };

    match Dictionary::new(&aff, &dic) {
        Ok(d) => {
            info!("Loaded {} dictionary", lang);
            Some(d)
        }
        Err(e) => {
            warn!("Failed to parse {} dictionary: {} - spell checking disabled", lang, e);
            None
        }
    }
}

/// Tokenize text into words and non-word segments, spell-check each word,
/// and reconstruct the text.
fn spell_check(text: &str, dict: &Dictionary) -> String {
    // Split into tokens: sequences of word chars (Unicode letters/digits/apostrophes)
    // vs everything else. We preserve all original spacing/punctuation.
    let mut result = String::with_capacity(text.len());
    let mut suggestions = Vec::new();

    let mut chars = text.char_indices().peekable();

    while let Some(&(start, ch)) = chars.peek() {
        if is_word_char(ch) {
            // Consume the whole word
            let mut end = start;
            while let Some(&(i, c)) = chars.peek() {
                if is_word_char(c) {
                    end = i + c.len_utf8();
                    chars.next();
                } else {
                    break;
                }
            }
            let word = &text[start..end];

            // Skip very short words (1-2 chars) and words with digits
            if word.chars().count() <= 2 || word.chars().any(|c| c.is_ascii_digit()) {
                result.push_str(word);
            } else {
                // Always normalize to lowercase before checking/suggesting.
                // The punctuation model can produce garbage mixed-case like
                // "ChcIalbym", "bylO", "gotoWe" which confuses the spell checker.
                let lower = word.to_lowercase();
                if dict.check(&lower) {
                    result.push_str(word);
                } else {
                    suggestions.clear();
                    dict.suggest(&lower, &mut suggestions);
                    if let Some(suggestion) = suggestions.first() {
                        let corrected = preserve_case(word, suggestion);
                        debug!("Spell fix: {:?} -> {:?}", word, corrected);
                        result.push_str(&corrected);
                    } else {
                        // No suggestion - keep original
                        result.push_str(word);
                    }
                }
            }
        } else {
            result.push(ch);
            chars.next();
        }
    }

    result
}

fn is_word_char(c: char) -> bool {
    c.is_alphabetic() || c == '\'' || c == '\u{2019}' // include apostrophes
}

/// Preserve the case pattern of the original word in the replacement.
fn preserve_case(original: &str, replacement: &str) -> String {
    let orig_chars: Vec<char> = original.chars().collect();

    // All uppercase -> return replacement in uppercase
    if orig_chars.iter().all(|c| c.is_uppercase() || !c.is_alphabetic()) {
        return replacement.to_uppercase();
    }

    // Title case (first char upper, rest lower) -> title case the replacement
    if orig_chars.first().map(|c| c.is_uppercase()).unwrap_or(false)
        && orig_chars[1..].iter().all(|c| c.is_lowercase() || !c.is_alphabetic())
    {
        let mut chars = replacement.chars();
        return match chars.next() {
            Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
            None => String::new(),
        };
    }

    // Otherwise keep replacement as-is (usually lowercase)
    replacement.to_string()
}

/// Insert comma before Polish conjunctions when missing.
fn apply_pl_commas(text: &str) -> String {
    let result = PL_COMMA_RE.replace_all(text, |caps: &regex::Captures| {
        let before = caps.get(1).unwrap().as_str();
        let conjunction = caps.get(2).unwrap().as_str();

        // Check if there's already a comma right before the conjunction
        let full_match = caps.get(0).unwrap();
        let before_match = if full_match.start() > 0 {
            &text[..full_match.start()]
        } else {
            ""
        };

        // If the text immediately before already ends with a comma, don't add another
        if before_match.trim_end().ends_with(',') {
            format!("{} {}", before, conjunction)
        } else {
            format!("{}, {}", before, conjunction)
        }
    });

    if result != text {
        debug!("Polish comma insertion applied");
    }

    result.to_string()
}

/// Fix unambiguous English contractions.
fn apply_en_contractions(text: &str) -> String {
    let mut result = text.to_string();
    for (pattern, replacement) in EN_CONTRACTIONS.iter() {
        let new = pattern.replace_all(&result, |caps: &regex::Captures| {
            let matched = caps.get(0).unwrap().as_str();
            preserve_case(matched, replacement)
        });
        if new != result {
            debug!("Contraction fix: {:?}", replacement);
            result = new.to_string();
        }
    }
    result
}
