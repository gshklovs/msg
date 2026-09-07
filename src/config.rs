use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
}

fn default_hotkey() -> String {
    "cmd+shift+m".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: default_hotkey(),
        }
    }
}

impl Config {
    pub fn path() -> Option<PathBuf> {
        Some(dirs::home_dir()?.join(".config/msg/config.toml"))
    }

    /// Load `~/.config/msg/config.toml`. A missing file is not an error.
    pub fn load() -> Result<Self> {
        let Some(path) = Self::path() else {
            return Ok(Self::default());
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Ok(Self::default());
        };
        Self::parse(&text)
    }

    pub fn parse(text: &str) -> Result<Self> {
        Ok(toml::from_str(text)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_uses_defaults() {
        assert_eq!(Config::parse("").unwrap(), Config::default());
        assert_eq!(Config::default().hotkey, "cmd+shift+m");
    }

    #[test]
    fn reads_hotkey_override() {
        let c = Config::parse("hotkey = \"ctrl+alt+k\"").unwrap();
        assert_eq!(c.hotkey, "ctrl+alt+k");
    }

    #[test]
    fn rejects_wrong_type() {
        assert!(Config::parse("hotkey = 3").is_err());
    }
}
