use crate::error::PalpError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

pub const CONFIG_FILE: &str = ".palprc.json";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InsetsConfig {
    pub status_bar: u32,
    pub navigation_bar: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PalpConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insets: Option<InsetsConfig>,
    #[serde(default)]
    pub targets: HashMap<String, (u32, u32)>,
}

impl PalpConfig {
    /// Loads configuration from the current working directory or any ancestor directory.
    pub fn load() -> Self {
        if let Some(path) = find_config_file() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(config) = serde_json::from_str::<PalpConfig>(&content) {
                    return config;
                }
            }
        }
        Self::default()
    }

    /// Saves configuration to `.palprc.json` in the current directory.
    pub fn save_to_cwd(&self) -> Result<PathBuf, PalpError> {
        let path = PathBuf::from(CONFIG_FILE);
        let json = serde_json::to_string_pretty(self)?;
        fs::write(&path, json)?;
        Ok(path)
    }

    /// Resolves target name to coordinates `(x, y)` if defined in config.
    pub fn get_target(&self, name: &str) -> Option<(u32, u32)> {
        self.targets.get(name).copied()
    }
}

fn find_config_file() -> Option<PathBuf> {
    let mut current = std::env::current_dir().ok()?;
    loop {
        let candidate = current.join(CONFIG_FILE);
        if candidate.is_file() {
            return Some(candidate);
        }
        if !current.pop() {
            break;
        }
    }
    None
}
