use anyhow::{Error as E, Result};
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::marian::{Config, MTModel};
use tokenizers::Tokenizer;
use tracing::info;

pub struct Translator {
    model: Option<MTModel>,
    tokenizer: Option<Tokenizer>,
    config: Option<Config>,
    device: Device,
}

impl Translator {
    pub fn new() -> Self {
        // We use CPU for now to ensure compatibility.
        // On macOS with candle-core features "metal", it could use GPU.
        // But for this refactor, let's stick to CPU to avoid complexity unless requested.
        let device = Device::Cpu;

        Self {
            model: None,
            tokenizer: None,
            config: None,
            device,
        }
    }

    pub fn ensure_loaded(&mut self) -> Result<()> {
        if self.model.is_some() {
            return Ok(());
        }

        info!("Loading translation model (Helsinki-NLP/opus-mt-pl-en)...");

        // All model files are local (from mise run setup-models)
        let model_path = std::path::PathBuf::from("models/opus-mt-pl-en.safetensors");
        if !model_path.exists() {
            return Err(E::msg("models/opus-mt-pl-en.safetensors not found. Run: mise run setup-models"));
        }
        let tokenizer_path = std::path::PathBuf::from("models/tokenizer_pl_en.json");
        if !tokenizer_path.exists() {
            return Err(E::msg("models/tokenizer_pl_en.json not found. Run: mise run setup-models"));
        }
        let config_path = std::path::PathBuf::from("models/opus-mt-pl-en-config.json");
        if !config_path.exists() {
            return Err(E::msg("models/opus-mt-pl-en-config.json not found. Run: mise run setup-models"));
        }

        let config: Config = serde_json::from_str(&std::fs::read_to_string(config_path)?)?;
        let tokenizer = Tokenizer::from_file(tokenizer_path).map_err(E::msg)?;

        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(&[model_path], DType::F32, &self.device)?
        };
        let model = MTModel::new(&config, vb)?;

        self.model = Some(model);
        self.tokenizer = Some(tokenizer);
        self.config = Some(config);

        info!("Translation model loaded.");
        Ok(())
    }

    pub fn translate(&mut self, text: &str) -> Result<String> {
        if self.model.is_none() {
            self.ensure_loaded()?;
        }

        let model = self.model.as_mut().unwrap();
        let tokenizer = self.tokenizer.as_ref().unwrap();
        let config = self.config.as_ref().unwrap();

        let tokens = tokenizer
            .encode(text, true)
            .map_err(E::msg)?
            .get_ids()
            .to_vec();

        if tokens.is_empty() {
            return Ok(String::new());
        }

        let input_tensor = Tensor::new(tokens.as_slice(), &self.device)?.unsqueeze(0)?;

        // Encoder
        let encoder_output = model.encoder().forward(&input_tensor, 0)?;

        // Greedy decoder with multiple stopping guards.
        // The model wants beam search (num_beams=6) but batched beams are too
        // expensive without proper KV cache support in candle.
        let mut decoder_token = config.decoder_start_token_id;
        let mut output_tokens: Vec<u32> = vec![];
        let eos_id = config.eos_token_id;
        let repetition_penalty: f32 = 1.5;
        let no_repeat_ngram: usize = 3;
        let max_len = (tokens.len() * 2).max(20).min(512);

        model.reset_kv_cache();

        for step in 0..max_len {
            let decoder_input = Tensor::new(&[decoder_token], &self.device)?.unsqueeze(0)?;
            let logits = model.decode(&decoder_input, &encoder_output, step)?;
            let logits = logits.squeeze(0)?.squeeze(0)?;
            let mut logits_vec = logits.to_vec1::<f32>()?;

            // Repetition penalty
            for &tid in &output_tokens {
                let idx = tid as usize;
                if idx < logits_vec.len() {
                    if logits_vec[idx] > 0.0 {
                        logits_vec[idx] /= repetition_penalty;
                    } else {
                        logits_vec[idx] *= repetition_penalty;
                    }
                }
            }

            // N-gram blocking
            if output_tokens.len() >= no_repeat_ngram - 1 {
                let prefix = &output_tokens[output_tokens.len() - (no_repeat_ngram - 1)..];
                for start in 0..output_tokens.len().saturating_sub(no_repeat_ngram - 1) {
                    if &output_tokens[start..start + no_repeat_ngram - 1] == prefix {
                        let blocked = output_tokens[start + no_repeat_ngram - 1] as usize;
                        if blocked < logits_vec.len() {
                            logits_vec[blocked] = f32::NEG_INFINITY;
                        }
                    }
                }
            }

            // Boost EOS logit as output grows beyond input length
            // Encourages stopping at natural length rather than rambling
            if output_tokens.len() > tokens.len() {
                let overshoot = (output_tokens.len() - tokens.len()) as f32;
                logits_vec[eos_id as usize] += overshoot * 2.0;
            }

            // Argmax
            let next_token_id = logits_vec
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(idx, _)| idx as u32)
                .unwrap_or(eos_id);

            if next_token_id == eos_id {
                break;
            }

            output_tokens.push(next_token_id);
            decoder_token = next_token_id;
        }

        let result = tokenizer.decode(&output_tokens, true).map_err(E::msg)?;
        Ok(result)
    }
}
