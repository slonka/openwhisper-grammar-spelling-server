use lingua::{Language, LanguageDetector, LanguageDetectorBuilder};
use lazy_static::lazy_static;

lazy_static! {
    static ref DETECTOR: LanguageDetector = {
        let languages = vec![Language::English, Language::Polish];
        LanguageDetectorBuilder::from_languages(&languages)
            .build()
    };
}

pub fn detect_language(text: &str) -> String {
    if let Some(lang) = DETECTOR.detect_language_of(text) {
        match lang {
            Language::English => "en".to_string(),
            Language::Polish => "pl".to_string(),
            _ => "pl".to_string(), // Default fallback
        }
    } else {
        "pl".to_string()
    }
}
