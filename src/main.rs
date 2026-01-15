// ABOUTME: Entry point for the peleka CLI application.
// ABOUTME: Parses arguments and dispatches to appropriate command handlers.

mod cli;

use clap::Parser;
use cli::{Cli, Commands};
use peleka::config;
use std::env;

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Init => {
            let cwd = env::current_dir().expect("Failed to get current directory");
            config::init_config(&cwd)
        }
        Commands::Deploy { destination: _ } => {
            eprintln!("Deploy not yet implemented");
            Ok(())
        }
        Commands::Status => {
            eprintln!("Status not yet implemented");
            Ok(())
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
