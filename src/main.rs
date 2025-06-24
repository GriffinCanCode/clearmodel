use anyhow::Result;
use clap::Parser;
use tracing::{info, error};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod config;
mod environment;
mod cache_cleaner;
mod resource_manager;
mod security;
mod errors;

use config::ClearModelConfig;
use environment::EnvironmentManager;
use cache_cleaner::CacheCleaner;

#[derive(Parser)]
#[command(name = "clearmodel")]
#[command(about = "Secure ML model cache cleaner with path traversal protection")]
#[command(version = "0.1.0")]
struct Cli {
    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,
    
    /// Configuration file path
    #[arg(short, long)]
    config: Option<String>,
    
    /// Dry run - show what would be cleaned without actually cleaning
    #[arg(short = 'n', long)]
    dry_run: bool,
    
    /// Verbose output
    #[arg(short, long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // Initialize logging
    init_logging(cli.debug, cli.verbose)?;
    
    info!("Starting clearmodel - ML cache cleaner");
    
    // Load environment and configuration
    let env_manager = EnvironmentManager::new().await?;
    let config = ClearModelConfig::load(cli.config.as_deref()).await?;
    
    // Initialize cache cleaner
    let cache_cleaner = CacheCleaner::new(config, env_manager).await?;
    
    // Perform cache cleaning
    match cache_cleaner.clean_all_caches(cli.dry_run).await {
        Ok(_) => {
            info!("Model cache cleaning completed successfully!");
        }
        Err(e) => {
            error!("Error during cache cleaning: {}", e);
            std::process::exit(1);
        }
    }
    
    Ok(())
}

fn init_logging(debug: bool, verbose: bool) -> Result<()> {
    let log_level = if debug {
        "debug"
    } else if verbose {
        "info"
    } else {
        "warn"
    };
    
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("clearmodel={}", log_level).into()),
        )
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .init();
    
    Ok(())
} 