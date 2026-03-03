use reqwest::Client;
use serde::Deserialize;
use tracing::{error, debug};



#[derive(Deserialize, Debug)]
struct Match {
    replacements: Vec<Replacement>,
    offset: usize,
    length: usize,
}

#[derive(Deserialize, Debug)]
struct Replacement {
    value: String,
}

#[derive(Deserialize, Debug)]
struct CheckResponse {
    matches: Vec<Match>,
}

pub struct GrammarCorrector {
    client: Client,
    url: String,
}

impl GrammarCorrector {
    pub fn new(url: String) -> Self {
        Self {
            client: Client::new(),
            url,
        }
    }

    pub async fn correct(&self, text: &str, lang: &str) -> String {
        // Map "pl" -> "pl-PL", "en" -> "en-US"
        let language = match lang {
            "pl" => "pl-PL",
            "en" => "en-US",
            _ => lang,
        };

        // If URL is empty, skip (disabled)
        if self.url.is_empty() {
            return text.to_string();
        }

        let params = [
            ("text", text),
            ("language", language),
            ("enabledOnly", "false"), // basic check
        ];

        match self.client.post(&self.url)
            .form(&params)
            .send()
            .await 
        {
            Ok(resp) => {
                let check_resp = resp.json::<CheckResponse>().await.ok();
                if let Some(check_resp) = check_resp {
                    // Map character offsets to byte offsets using the original text
                    // We need to do this BEFORE any modifications
                    // Create a vector of (start_byte, end_byte, replacement_text)
                    let mut replacements_to_apply = Vec::new();

                    for m in &check_resp.matches {
                        if let Some(repl) = m.replacements.first() {
                            let start_char = m.offset;
                            let end_char = start_char + m.length;
                            
                            // Find byte offsets
                            // This is O(N) per match, can be optimized but acceptable for short text
                            let start_byte = text.char_indices().nth(start_char).map(|(i, _)| i);
                            let end_byte = text.char_indices().nth(end_char).map(|(i, _)| i);
                            
                            // Special case: end_char might be == length (end of string)
                            // char_indices().nth(len) returns None.
                            let end_byte = if end_char == text.chars().count() {
                                Some(text.len())
                            } else {
                                end_byte
                            };

                            if let (Some(s), Some(e)) = (start_byte, end_byte) {
                                replacements_to_apply.push((s, e, repl.value.clone()));
                            }
                        }
                    }

                    // Sort by start_byte descending to apply from end to start
                    replacements_to_apply.sort_by(|a, b| b.0.cmp(&a.0));

                    let mut result = text.to_string();
                    for (start, end, repl) in replacements_to_apply {
                        if start <= result.len() && end <= result.len() {
                            result.replace_range(start..end, &repl);
                            debug!("Grammar fix applied at {}-{}: {}", start, end, repl);
                        }
                    }
                    return result;
                }
            }
            Err(e) => {
                error!("Grammar correction failed: {}", e);
            }
        }
        text.to_string()
    }
}
