//! Accessibility settings for TontooOS.
//!
//! - Reduce Transparency: Disables glass/blur effects, replaces with solid opaque backgrounds.
//! - Reduce Motion: Disables animations (workspace transitions, window open/close, etc.).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessibilitySettings {
    pub reduce_transparency: bool,
    pub reduce_motion: bool,
}

impl Default for AccessibilitySettings {
    fn default() -> Self {
        Self {
            reduce_transparency: false,
            reduce_motion: false,
        }
    }
}

impl AccessibilitySettings {
    pub fn load() -> Self {
        dirs::config_dir()
            .map(|p| p.join("tontoo").join("accessibility.json"))
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let dir = dirs::config_dir().ok_or("No config dir")?.join("tontoo");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("accessibility.json");
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }
}
