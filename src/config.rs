use anyhow::Result;
use serde::{Deserialize, Deserializer, Serialize};
use std::path::PathBuf;

use crate::theme::Theme;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    /// Which of the five looks the popup wears. See `src/theme.rs`.
    #[serde(default, deserialize_with = "theme_from_name")]
    pub theme: Theme,
}

fn default_hotkey() -> String {
    "cmd+shift+m".to_string()
}

/// Read the theme by name so a typo names the five valid options rather than
/// listing serde's variants.
fn theme_from_name<'de, D: Deserializer<'de>>(d: D) -> Result<Theme, D::Error> {
    let name = String::deserialize(d)?;
    name.parse().map_err(serde::de::Error::custom)
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: default_hotkey(),
            theme: Theme::default(),
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

    #[test]
    fn reads_every_theme_name() {
        assert_eq!(Config::parse("").unwrap().theme, Theme::Spotlight);
        for name in crate::theme::THEME_NAMES {
            let c = Config::parse(&format!("theme = \"{name}\"")).unwrap();
            assert_eq!(c.theme.to_string(), name);
        }
    }

    #[test]
    fn an_unknown_theme_names_the_five_options() {
        let err = Config::parse("theme = \"neon\"").unwrap_err().to_string();
        for name in crate::theme::THEME_NAMES {
            assert!(err.contains(name), "the error should list {name}: {err}");
        }
    }

    #[test]
    fn hotkey_and_theme_coexist() {
        let c = Config::parse("hotkey = \"ctrl+alt+k\"\ntheme = \"brutalist\"\n").unwrap();
        assert_eq!(c.hotkey, "ctrl+alt+k");
        assert_eq!(c.theme, Theme::Brutalist);
    }
}
