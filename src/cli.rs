// ABOUTME: Command-line interface definition using clap derive macros.
// ABOUTME: Defines all subcommands and their arguments.

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "peleka")]
#[command(about = "Zero-downtime container deployment for Docker and Podman")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize a new peleka.yml configuration file
    Init,

    /// Deploy the service to configured servers
    Deploy {
        /// Target destination (defined in config)
        #[arg(short, long)]
        destination: Option<String>,
    },

    /// Show deployment status
    Status,
}
