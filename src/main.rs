mod config;
mod event;
mod watcher;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "cerbera", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Start monitoring. v0.1 is watch-only — unauthorized access is logged, not blocked.
    Run {
        #[arg(long, short)]
        config: PathBuf,
    },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    match cli.command {
        Cmd::Run { config } => run(config),
    }
}

fn run(config_path: PathBuf) -> Result<()> {
    let cfg = config::Config::load(&config_path)?;
    tracing::info!(watches = cfg.watches.len(), "loaded config");

    let watcher = watcher::Watcher::new()?;
    for w in &cfg.watches {
        watcher.add_watch(w)?;
        tracing::info!(name = %w.name, path = %w.path, "watching");
    }

    let index = event::AllowIndex::from_watches(&cfg.watches)?;

    tracing::warn!("MODE: watch-only (v0.1) — access is always allowed; only logging");
    event::run_loop(&watcher, &index)
}
