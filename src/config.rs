use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    #[serde(default, rename = "watch")]
    pub watches: Vec<Watch>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Watch {
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub allow_processes: Vec<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub recursive: bool,
}

fn is_false(b: &bool) -> bool {
    !b
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config: {}", path.display()))?;
        toml::from_str(&raw)
            .with_context(|| format!("failed to parse config: {}", path.display()))
    }

    pub fn load_all(paths: &[PathBuf]) -> Result<Self> {
        let configs = paths
            .iter()
            .map(|p| Self::load(p))
            .collect::<Result<Vec<_>>>()?;
        let merged = Self::merge(configs);
        if merged.watches.is_empty() {
            return Err(anyhow!("no [[watch]] entries found across all config files"));
        }
        Ok(merged)
    }

    fn merge(configs: Vec<Self>) -> Self {
        let mut watches: Vec<Watch> = Vec::new();
        for config in configs {
            for watch in config.watches {
                if let Some(existing) = watches.iter_mut().find(|w| w.path == watch.path) {
                    for proc in watch.allow_processes {
                        if !existing.allow_processes.contains(&proc) {
                            existing.allow_processes.push(proc);
                        }
                    }
                    existing.recursive = existing.recursive || watch.recursive;
                } else {
                    watches.push(watch);
                }
            }
        }
        Self { watches }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn watch(path: &str, procs: &[&str], recursive: bool) -> Watch {
        Watch {
            name: path.to_string(),
            path: path.to_string(),
            allow_processes: procs.iter().map(|s| s.to_string()).collect(),
            recursive,
        }
    }

    #[test]
    fn merge_unions_allow_processes() {
        let a = Config { watches: vec![watch("/home/alice/.ssh", &["/usr/bin/ssh"], false)] };
        let b = Config { watches: vec![watch("/home/alice/.ssh", &["/usr/bin/git"], false)] };
        let merged = Config::merge(vec![a, b]);
        assert_eq!(merged.watches.len(), 1);
        let procs = &merged.watches[0].allow_processes;
        assert!(procs.contains(&"/usr/bin/ssh".to_string()));
        assert!(procs.contains(&"/usr/bin/git".to_string()));
    }

    #[test]
    fn merge_deduplicates_allow_processes() {
        let a = Config { watches: vec![watch("/tmp/foo", &["/usr/bin/cat"], false)] };
        let b = Config { watches: vec![watch("/tmp/foo", &["/usr/bin/cat", "/usr/bin/ls"], false)] };
        let merged = Config::merge(vec![a, b]);
        assert_eq!(merged.watches[0].allow_processes.len(), 2);
    }

    #[test]
    fn merge_recursive_is_or() {
        let a = Config { watches: vec![watch("/tmp/foo", &[], false)] };
        let b = Config { watches: vec![watch("/tmp/foo", &[], true)] };
        let merged = Config::merge(vec![a, b]);
        assert!(merged.watches[0].recursive);
    }

    #[test]
    fn merge_distinct_paths_kept_separate() {
        let a = Config { watches: vec![watch("/tmp/foo", &[], false)] };
        let b = Config { watches: vec![watch("/tmp/bar", &[], false)] };
        let merged = Config::merge(vec![a, b]);
        assert_eq!(merged.watches.len(), 2);
    }
}

impl Watch {
    pub fn resolved_path(&self) -> Result<PathBuf> {
        if self.path.starts_with('~') {
            anyhow::bail!(
                "watch `{}`: `~` is not supported — use an absolute path (e.g. /home/username/...)",
                self.name
            );
        }
        Ok(PathBuf::from(&self.path))
    }
}
