use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use regex::Regex;
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tracing::{info, warn};

#[derive(Debug, Clone)]
pub struct ReplacementRule {
    pub pattern: Regex,
    pub replacement: String,
    pub description: String,
    pub lang_filter: Option<String>,
}

#[derive(Deserialize)]
struct RawRule {
    from: String,
    to: String,
    lang: Option<String>,
}

#[derive(Deserialize)]
struct RawConfig {
    rules: Vec<RawRule>,
}

#[derive(Clone)]
pub struct ConfigManager {
    rules: Arc<RwLock<Vec<ReplacementRule>>>,
    config_path: PathBuf,
}

impl ConfigManager {
    pub fn new() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let config_path = PathBuf::from(home)
            .join(".config")
            .join("openwhisper-cleanup")
            .join("replacements.json");

        let manager = Self {
            rules: Arc::new(RwLock::new(Vec::new())),
            config_path,
        };

        manager.load_rules();
        manager.start_watcher();

        manager
    }

    fn load_rules(&self) {
        if !self.config_path.exists() {
            return;
        }

        match fs::read_to_string(&self.config_path) {
            Ok(content) => {
                let raw_rules: Vec<RawRule> = if let Ok(data) = serde_json::from_str::<RawConfig>(&content) {
                    data.rules
                } else if let Ok(data) = serde_json::from_str::<Vec<RawRule>>(&content) {
                    data
                } else {
                    warn!("Failed to parse replacements.json");
                    return;
                };

                let mut rules = Vec::new();
                for r in raw_rules {
                    // Python logic: pattern = re.compile(r"\b" + re.escape(from_text) + r"\b", re.IGNORECASE)
                    let escaped = regex::escape(&r.from);
                    let pattern_str = format!(r"(?i)\b{}\b", escaped);
                    
                    match Regex::new(&pattern_str) {
                        Ok(re) => {
                            rules.push(ReplacementRule {
                                pattern: re,
                                replacement: r.to.clone(),
                                description: format!("{} -> {}", r.from, r.to),
                                lang_filter: r.lang,
                            });
                        }
                        Err(e) => {
                            warn!("Invalid regex for rule '{}': {}", r.from, e);
                        }
                    }
                }

                if let Ok(mut lock) = self.rules.write() {
                    *lock = rules;
                    info!("Loaded {} user replacement rules", lock.len());
                }
            }
            Err(e) => {
                warn!("Failed to read replacements config: {}", e);
            }
        }
    }

    fn start_watcher(&self) {
        let manager = self.clone();
        let path = self.config_path.clone();
        
        // Spawn a background task for watching
        tokio::spawn(async move {
            let (tx, rx) = std::sync::mpsc::channel();
            
            // Need to create parent dir if it doesn't exist to watch it? 
            // Usually we watch the file or parent dir. If file doesn't exist, we might need to watch parent.
            if let Some(parent) = path.parent() {
                 let _ = fs::create_dir_all(parent);
                 
                 let mut watcher = RecommendedWatcher::new(tx, Config::default()).ok();
                 
                 if let Some(w) = &mut watcher {
                     if let Err(e) = w.watch(parent, RecursiveMode::NonRecursive) {
                         warn!("Failed to watch config directory: {}", e);
                         return;
                     }
                     
                     info!("Watching config directory: {:?}", parent);
                     
                     for res in rx {
                         match res {
                             Ok(event) => {
                                 // Simple logic: if any event touches our file, reload
                                 if event.paths.iter().any(|p| p.ends_with("replacements.json")) {
                                     info!("Config file changed, reloading...");
                                     manager.load_rules();
                                 }
                             }
                             Err(e) => warn!("Watch error: {}", e),
                         }
                     }
                 }
            }
        });
    }

    pub fn get_rules(&self) -> Vec<ReplacementRule> {
        self.rules.read().unwrap().clone()
    }
}
