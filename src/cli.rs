// ABOUTME: Command-line interface definition using clap derive macros.
// ABOUTME: Defines all subcommands and their arguments.

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "peleka")]
#[command(about = "Zero-downtime container deployment for Docker and Podman")]
#[command(version)]
pub struct Cli {
    /// Enable verbose output for debugging
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize a new peleka.yml configuration file
    Init {
        /// Service name to use in config
        #[arg(long)]
        service: Option<String>,

        /// Container image to use
        #[arg(long)]
        image: Option<String>,

        /// Overwrite existing config file
        #[arg(long)]
        force: bool,
    },

    /// Deploy the service to configured servers
    Deploy {
        /// Target destination (defined in config)
        #[arg(short, long)]
        destination: Option<String>,
    },

    /// Rollback to the previous deployment
    Rollback {
        /// Target destination (defined in config)
        #[arg(short, long)]
        destination: Option<String>,
    },

    /// Show deployment status
    Status,

    /// Stream container logs
    Logs {
        /// Number of lines to show from end
        #[arg(long)]
        tail: Option<u64>,

        /// Follow log output (like tail -f)
        #[arg(short, long)]
        follow: bool,

        /// Show logs since timestamp (e.g., 2024-01-15T10:00:00)
        #[arg(long)]
        since: Option<String>,

        /// Show log disk usage statistics
        #[arg(long)]
        stats: bool,
    },
}
