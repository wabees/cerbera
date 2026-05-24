mod config;
mod event;
mod learn;
mod watcher;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::time::Duration;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "cerbera", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Start monitoring. Watch-only mode: unauthorized access is logged, not blocked.
    Run {
        #[arg(long, short)]
        config: PathBuf,
    },
    /// Observe accesses to watched paths and generate an allow-list config.
    Learn {
        #[arg(long, short)]
        config: PathBuf,
        /// Observation duration in seconds.
        #[arg(long, short, default_value = "60")]
        duration: u64,
        /// Write output to this file instead of stdout.
        #[arg(long, short)]
        output: Option<PathBuf>,
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
        Cmd::Learn { config, duration, output } => {
            let cfg = config::Config::load(&config)?;
            learn::run_learn(&cfg, Duration::from_secs(duration), output.as_deref())
        }
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

    tracing::warn!("MODE: watch-only — access is always allowed; only logging");
    event::run_loop(&watcher, &index)
}
