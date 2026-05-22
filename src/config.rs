use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default, rename = "watch")]
    pub watches: Vec<Watch>,
}

#[derive(Debug, Deserialize)]
pub struct Watch {
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub allow_processes: Vec<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub recursive: bool,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config: {}", path.display()))?;
        let cfg: Config = toml::from_str(&raw)
            .with_context(|| format!("failed to parse config: {}", path.display()))?;
        if cfg.watches.is_empty() {
            return Err(anyhow!("config has no [[watch]] entries"));
        }
        Ok(cfg)
    }
}

impl Watch {
    pub fn resolved_path(&self) -> Result<PathBuf> {
        expand_home(&self.path)
    }
}

fn expand_home(raw: &str) -> Result<PathBuf> {
    if raw == "~" {
        let home = std::env::var("HOME").context("HOME not set, cannot expand `~`")?;
        return Ok(PathBuf::from(home));
    }
    if let Some(rest) = raw.strip_prefix("~/") {
        let home = std::env::var("HOME").context("HOME not set, cannot expand `~/`")?;
        return Ok(Path::new(&home).join(rest));
    }
    Ok(PathBuf::from(raw))
}
