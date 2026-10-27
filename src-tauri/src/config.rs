//! App settings stored as JSON in the user config folder
//! (`~/.config/rummage` on Linux, `%APPDATA%\rummage` on Windows).

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const APP_DIR: &str = "rummage";
/// The folder used before the app was renamed; its settings are read if there are no new ones.
const OLD_APP_DIR: &str = "media-catalog";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Config {
    /// Folder holding the catalog database and thumbnail cache. None until first-run setup.
    pub catalog_dir: Option<PathBuf>,
}

fn config_path(app_dir: &str) -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(app_dir)
        .join("config.json")
}

pub fn default_catalog_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_DIR)
}

pub fn load() -> Config {
    [APP_DIR, OLD_APP_DIR]
        .iter()
        .find_map(|dir| std::fs::read_to_string(config_path(dir)).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save(config: &Config) -> Result<()> {
    let path = config_path(APP_DIR);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(config)?)?;
    Ok(())
}
