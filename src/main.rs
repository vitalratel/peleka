// ABOUTME: Entry point for the peleka CLI application.
// ABOUTME: Parses arguments and dispatches to appropriate command handlers.

mod cli;

use clap::Parser;
use cli::{Cli, Commands};
use peleka::config::{self, Config};
use std::env;
use tracing_subscriber::EnvFilter;

fn main() {
    let cli = Cli::parse();

    // Initialize tracing subscriber based on verbose flag
    let filter = if cli.verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::new("warn")
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .init();

    let result = match cli.command {
        Commands::Init {
            service,
            image,
            force,
        } => {
            let cwd = env::current_dir().expect("Failed to get current directory");
            config::init_config(&cwd, service.as_deref(), image.as_deref(), force)
        }
        Commands::Deploy { destination: _ } => {
            eprintln!("Deploy not yet implemented");
            Ok(())
        }
        Commands::Status => {
            let cwd = env::current_dir().expect("Failed to get current directory");
            Config::discover(&cwd).map(|config| {
                println!("Service: {}", config.service);
                println!("Image: {}", config.image);
                println!("Servers: {}", config.servers.len());
                // TODO: Connect to servers and show container status
            })
        }
        Commands::Logs {
            tail,
            follow,
            since,
            stats,
        } => {
            let cwd = env::current_dir().expect("Failed to get current directory");
            Config::discover(&cwd).map(|config| {
                println!("Service: {}", config.service);
                println!(
                    "Options: tail={:?}, follow={}, since={:?}, stats={}",
                    tail, follow, since, stats
                );
                // TODO: Connect to servers and stream logs
            })
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
