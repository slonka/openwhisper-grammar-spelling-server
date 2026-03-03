use ndarray::{Array2, Array3};
use ort::session::Session;
use ort::value::Value;
use std::path::Path;
use std::sync::Mutex;
use tokenizers::Tokenizer;
use tracing::{error, info, warn};

// Hardcoded labels for pcs_47lang (derived from dump)
const PRE_LABELS: &[&str] = &["<NULL>", "¿"];
const POST_LABELS: &[&str] = &[
    "<NULL>", ".", ",", "?", "？", "，", "。", "、", "・", "।", "؟", "،", ";", "።", "፣", "፧",
];
const NULL_TOKEN: &str = "<NULL>";

pub struct PunctuationModel {
    session: Option<Mutex<Session>>,
    tokenizer: Option<Tokenizer>,
}

impl PunctuationModel {
    pub fn new<P: AsRef<Path>>(model_path: P, tokenizer_path: P) -> Self {
        let model_path = model_path.as_ref();
        let tokenizer_path = tokenizer_path.as_ref();

        let session = if model_path.exists() {
            match Session::builder() {
                Ok(builder) => match builder.commit_from_file(model_path) {
                    Ok(s) => {
                        info!("Loaded punctuation model from {:?}", model_path);
                        Some(Mutex::new(s))
                    }
                    Err(e) => {
                        error!("Failed to load punctuation model: {}", e);
                        None
                    }
                },
                Err(e) => {
                    error!("Failed to create SessionBuilder: {}", e);
                    None
                }
            }
        } else {
            warn!(
                "Punctuation model not found at {:?}, disabling stage.",
                model_path
            );
            None
        };

        let tokenizer = if tokenizer_path.exists() {
            match Tokenizer::from_file(tokenizer_path) {
                Ok(t) => {
                    info!("Loaded tokenizer from {:?}", tokenizer_path);
                    Some(t)
                }
                Err(e) => {
                    error!("Failed to load tokenizer: {}", e);
                    None
                }
            }
        } else {
            warn!(
                "Tokenizer not found at {:?}, disabling punctuation stage.",
                tokenizer_path
            );
            None
        };

        Self { session, tokenizer }
    }

    pub fn restore(&self, text: &str) -> String {
        if let (Some(session_mutex), Some(tokenizer)) = (&self.session, &self.tokenizer) {
            let lower = text.to_lowercase();
            match tokenizer.encode(lower.as_str(), true) {
                Ok(encoding) => {
                    let ids = encoding.get_ids();
                    let len = ids.len();
                    if len < 2 {
                        return text.to_string();
                    }

                    // Prepare input tensor: [1, seq_len]
                    let input_ids =
                        Array2::from_shape_vec((1, len), ids.iter().map(|&id| id as i64).collect())
                            .unwrap();

                    // Create Value from array (owned)
                    let input_value = match Value::from_array(input_ids) {
                        Ok(v) => v,
                        Err(e) => {
                            error!("Failed to create input tensor: {}", e);
                            return text.to_string();
                        }
                    };

                    // Prepare inputs (references input_value)
                    let inputs = ort::inputs!["input_ids" => &input_value];

                    // Run inference and extract outputs within mutex scope
                    let (pre_array, post_array, cap_array) = {
                        let mut session = session_mutex.lock().unwrap();
                        let outputs = match session.run(inputs) {
                            Ok(o) => o,
                            Err(e) => {
                                error!("Inference failed: {}", e);
                                return text.to_string();
                            }
                        };

                        // Extract tensors
                        let (pre_shape, pre_data) =
                            outputs["pre_preds"].try_extract_tensor::<i64>().unwrap();
                        let (post_shape, post_data) =
                            outputs["post_preds"].try_extract_tensor::<i64>().unwrap();
                        let (cap_shape, cap_data) =
                            outputs["cap_preds"].try_extract_tensor::<bool>().unwrap();

                        // Copy data to owned arrays
                        let pre_dim = (pre_shape[0] as usize, pre_shape[1] as usize);
                        let post_dim = (post_shape[0] as usize, post_shape[1] as usize);
                        let cap_dim = (
                            cap_shape[0] as usize,
                            cap_shape[1] as usize,
                            cap_shape[2] as usize,
                        );

                        let pre_array = Array2::from_shape_vec(pre_dim, pre_data.to_vec()).unwrap();
                        let post_array =
                            Array2::from_shape_vec(post_dim, post_data.to_vec()).unwrap();
                        let cap_array = Array3::from_shape_vec(cap_dim, cap_data.to_vec()).unwrap();

                        (pre_array, post_array, cap_array)
                    };

                    // Create ndarray views
                    let pre_view = pre_array.view();
                    let post_view = post_array.view();
                    let cap_view = cap_array.view();

                    let mut output_chars = String::new();

                    // Iterate tokens, skipping first and last (BOS/EOS if present)
                    let start_idx = 1;
                    let end_idx = len - 1;

                    if start_idx >= end_idx {
                        return text.to_string();
                    }

                    for i in start_idx..end_idx {
                        let id = ids[i];
                        let token_str = tokenizer.id_to_token(id).unwrap_or("".to_string());

                        // Handle SentencePiece special prefix
                        let is_start_word = token_str.starts_with('\u{2581}');
                        let clean_token = if is_start_word {
                            &token_str['\u{2581}'.len_utf8()..]
                        } else {
                            &token_str
                        };

                        // Add space if needed
                        if is_start_word && !output_chars.is_empty() {
                            output_chars.push(' ');
                        }

                        // Pre-punctuation
                        let pre_idx = pre_view[[0, i]] as usize;
                        if let Some(label) = PRE_LABELS.get(pre_idx) {
                            if *label != NULL_TOKEN {
                                output_chars.push_str(label);
                            }
                        }

                        // Capitalization and char append
                        for (char_idx, char) in clean_token.chars().enumerate() {
                            let actual_char_idx = if is_start_word {
                                char_idx + 1
                            } else {
                                char_idx
                            };

                            let should_cap = if actual_char_idx < 16 {
                                cap_view[[0, i, actual_char_idx]]
                            } else {
                                false
                            };

                            if should_cap {
                                output_chars.extend(char.to_uppercase());
                            } else {
                                output_chars.push(char);
                            }
                        }

                        // Post-punctuation
                        let post_idx = post_view[[0, i]] as usize;
                        if let Some(label) = POST_LABELS.get(post_idx) {
                            if *label != NULL_TOKEN {
                                output_chars.push_str(label);
                            }
                        }
                    }

                    output_chars
                }
                Err(e) => {
                    error!("Tokenization failed: {}", e);
                    text.to_string()
                }
            }
        } else {
            text.to_string()
        }
    }
}
