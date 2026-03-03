use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::{info, instrument, error};

use crate::config::ConfigManager;
use crate::stages::{detect, fillers, itn, punctuation, correction, grammar, translation};

#[derive(Clone)]
pub struct TextCleanupPipeline {
    pub config: ConfigManager,
    pub punctuator: Arc<punctuation::PunctuationModel>,
    pub grammar_corrector: Arc<grammar::GrammarCorrector>,
    pub translator: Arc<Mutex<translation::Translator>>,
}

impl TextCleanupPipeline {
    pub fn new(
        model_path: PathBuf,
        tokenizer_path: PathBuf,
        lt_url: String,
    ) -> Self {
        let config = ConfigManager::new();
        let punctuator = Arc::new(punctuation::PunctuationModel::new(model_path, tokenizer_path));
        let grammar_corrector = Arc::new(grammar::GrammarCorrector::new(lt_url));
        let translator = Arc::new(Mutex::new(translation::Translator::new()));

        Self {
            config,
            punctuator,
            grammar_corrector,
            translator,
        }
    }

    #[instrument(skip(self))]
    pub async fn run(&self, text: &str, enable_translation: bool) -> String {
        if text.trim().is_empty() {
            return text.to_string();
        }

        info!("Input:    {:?}", text);

        // 1. Language detection
        let lang = detect::detect_language(text);
        info!("Language: {}", lang);

        // 2. Filler removal
        let mut processed = fillers::remove_fillers(text, &lang);
        info!("Fillers:  {:?}", processed);

        // 3. Inverse text normalization
        processed = itn::inverse_text_normalize(&processed, &lang);
        info!("ITN:      {:?}", processed);

        // 4. Punctuation & capitalization
        // Note: The punctuation model is typically synchronous (CPU/GPU)
        processed = self.punctuator.restore(&processed);
        info!("Punct:    {:?}", processed);

        // 5. Word corrections
        processed = correction::apply_corrections(&processed, &lang);
        info!("Words:    {:?}", processed);

        // 6. User replacements
        let rules = self.config.get_rules();
        for rule in rules {
            if let Some(filter) = &rule.lang_filter {
                if filter != &lang {
                    continue;
                }
            }
            let new_text = rule.pattern.replace_all(&processed, &rule.replacement);
            if new_text != processed {
                 info!("User rule applied: {}", rule.description);
                 processed = new_text.to_string();
            }
        }
        info!("User:     {:?}", processed);

        // 7. Grammar correction (Async)
        processed = self.grammar_corrector.correct(&processed, &lang).await;
        info!("Grammar:  {:?}", processed);

        // 8. Translation (if Polish and enabled)
        if enable_translation && lang == "pl" {
            // We lock the translator mutex. Since generation is CPU intensive and slow,
            // this might block other requests if we have high concurrency.
            // But for a local tool, it's acceptable.
            // Note: candle computations are sync blocking.
            let mut translator = self.translator.lock().unwrap();
            match translator.translate(&processed) {
                Ok(translated) => {
                    info!("Translate: {:?}", translated);
                    processed = translated;
                }
                Err(e) => {
                    error!("Translation failed: {}", e);
                }
            }
        }

        processed
    }
}
