// ABOUTME: Entry point for the peleka CLI application.
// ABOUTME: Parses arguments and dispatches to appropriate command handlers.

mod cli;
mod commands;

use clap::Parser;
use cli::{Cli, Commands};
use peleka::config::{self, Config};
use peleka::error::{Error, Result};
use peleka::output::{Output, OutputMode};
use std::env;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
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

    // Determine output mode
    let output_mode = if cli.json {
        OutputMode::Json
    } else if cli.quiet {
        OutputMode::Quiet
    } else {
        OutputMode::Normal
    };
    let output = Output::new(output_mode);

    let result = run(cli, output).await;

    if let Err(e) = result {
        handle_error(e);
    }
}

/// Handle errors with programmatic error types and helpful hints.
fn handle_error(e: Error) -> ! {
    use peleka::deploy::DeployErrorKind;

    match &e {
        Error::Deploy(deploy_err) => match deploy_err.kind() {
            DeployErrorKind::LockHeld => {
                if let Some(info) = deploy_err.lock_holder_info() {
                    eprintln!(
                        "Error: Deployment locked by {} (pid {})",
                        info.holder, info.pid
                    );
                    eprintln!("       Started at: {}", info.started_at);
                    eprintln!("       Tip: Use --force to break the lock");
                } else {
                    eprintln!("Error: {e}");
                }
                std::process::exit(2);
            }
            DeployErrorKind::HealthCheckTimeout => {
                if let Some(secs) = deploy_err.timeout_seconds() {
                    eprintln!("Error: Health check timed out after {}s", secs);
                    eprintln!("       Tip: Increase health_timeout in peleka.yml");
                } else {
                    eprintln!("Error: {e}");
                }
                std::process::exit(3);
            }
            DeployErrorKind::NoPreviousDeployment => {
                if let Some(service) = deploy_err.service_name() {
                    eprintln!(
                        "Error: No previous deployment exists for service '{}'",
                        service
                    );
                    eprintln!("       Tip: Deploy first, then use rollback");
                } else {
                    eprintln!("Error: {e}");
                }
                std::process::exit(4);
            }
            _ => {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        },
        Error::Ssh(ssh_err) => {
            eprintln!("Error: SSH connection failed: {}", ssh_err);
            if format!("{ssh_err}").contains("authentication") {
                eprintln!("       Tip: Check SSH key and user configuration");
            }
            std::process::exit(5);
        }
        Error::ConfigNotFound(path) => {
            eprintln!("Error: Configuration file not found in {}", path.display());
            eprintln!("       Tip: Run 'peleka init' to create peleka.yml");
            std::process::exit(6);
        }
        Error::NoServers => {
            eprintln!("Error: No servers configured");
            eprintln!("       Tip: Add servers to peleka.yml");
            std::process::exit(7);
        }
        _ => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }
}

async fn run(cli: Cli, output: Output) -> Result<()> {
    match cli.command {
        Commands::Init {
            service,
            image,
            force,
        } => {
            let cwd = env::current_dir()?;
            config::init_config(&cwd, service.as_deref(), image.as_deref(), force)
        }
        Commands::Deploy { destination, force } => {
            let cwd = env::current_dir()?;
            let config = Config::discover(&cwd)?;

            // Apply destination overrides if specified
            let config = if let Some(dest) = destination {
                config.for_destination(&dest)?
            } else {
                config
            };

            commands::deploy(config, force, output).await
        }
        Commands::Rollback { destination } => {
            let cwd = env::current_dir()?;
            let config = Config::discover(&cwd)?;

            // Apply destination overrides if specified
            let config = if let Some(dest) = destination {
                config.for_destination(&dest)?
            } else {
                config
            };

            commands::rollback(config, output).await
        }
        Commands::Exec {
            destination,
            command,
        } => {
            let cwd = env::current_dir()?;
            let config = Config::discover(&cwd)?;

            // Apply destination overrides if specified
            let config = if let Some(dest) = destination {
                config.for_destination(&dest)?
            } else {
                config
            };

            commands::exec_command(config, command, output).await
        }
    }
}
