// ABOUTME: Entry point for the peleka CLI application.
// ABOUTME: Parses arguments and dispatches to appropriate command handlers.

mod cli;

use clap::Parser;
use cli::{Cli, Commands};
use peleka::config::{self, Config, ServerConfig};
use peleka::deploy::{DeployLock, Deployment, Initialized, manual_rollback};
use peleka::error::{Error, Result};
use peleka::hooks::{HookContext, HookPoint, HookRunner};
use peleka::runtime::{
    BollardRuntime, ContainerFilters, ContainerOps, connect_via_session, detect_runtime,
};
use peleka::ssh::{Session, SessionConfig};
use std::collections::HashMap;
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

    let result = run(cli).await;

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Init {
            service,
            image,
            force,
        } => {
            let cwd = env::current_dir().expect("Failed to get current directory");
            config::init_config(&cwd, service.as_deref(), image.as_deref(), force)
        }
        Commands::Deploy { destination, force } => {
            let cwd = env::current_dir().expect("Failed to get current directory");
            let config = Config::discover(&cwd)?;

            // Apply destination overrides if specified
            let config = if let Some(dest) = destination {
                config.for_destination(&dest)?
            } else {
                config
            };

            deploy(config, force).await
        }
        Commands::Rollback { destination } => {
            let cwd = env::current_dir().expect("Failed to get current directory");
            let config = Config::discover(&cwd)?;

            // Apply destination overrides if specified
            let config = if let Some(dest) = destination {
                config.for_destination(&dest)?
            } else {
                config
            };

            rollback(config).await
        }
    }
}

/// Deploy to all configured servers.
async fn deploy(config: Config, force: bool) -> Result<()> {
    if config.servers.is_empty() {
        return Err(Error::NoServers);
    }

    let cwd = env::current_dir().expect("Failed to get current directory");
    let hook_runner = HookRunner::new(&cwd);

    println!(
        "Deploying {} ({}) to {} server(s)",
        config.service,
        config.image,
        config.servers.len()
    );

    // Run pre-deploy hook for each server
    for server in &config.servers {
        let hook_context = HookContext {
            service: config.service.clone(),
            image: config.image.to_string(),
            server: server.host.clone(),
            runtime: server
                .runtime
                .as_ref()
                .map(|r| r.to_string())
                .unwrap_or_else(|| "auto".to_string()),
            previous_version: None, // TODO: Could detect from running container
        };

        if let Some(result) = hook_runner.run(HookPoint::PreDeploy, &hook_context).await
            && !result.success
        {
            eprintln!("Pre-deploy hook failed for {}", server.host);
            if !result.stderr.is_empty() {
                eprintln!("{}", result.stderr);
            }
            return Err(Error::Hook("pre-deploy hook failed".to_string()));
        }
    }

    // Deploy to each server
    let mut deploy_error = None;
    for server in &config.servers {
        if let Err(e) = deploy_to_server(&config, server, force).await {
            eprintln!("Failed to deploy to {}: {}", server.host, e);

            // Run on-error hook
            let hook_context = HookContext {
                service: config.service.clone(),
                image: config.image.to_string(),
                server: server.host.clone(),
                runtime: server
                    .runtime
                    .as_ref()
                    .map(|r| r.to_string())
                    .unwrap_or_else(|| "auto".to_string()),
                previous_version: None,
            };

            if let Some(result) = hook_runner.run(HookPoint::OnError, &hook_context).await
                && !result.success
            {
                eprintln!("Warning: on-error hook failed");
            }

            deploy_error = Some(e);
            break;
        }
    }

    if let Some(e) = deploy_error {
        return Err(e);
    }

    // Run post-deploy hook for each server
    for server in &config.servers {
        let hook_context = HookContext {
            service: config.service.clone(),
            image: config.image.to_string(),
            server: server.host.clone(),
            runtime: server
                .runtime
                .as_ref()
                .map(|r| r.to_string())
                .unwrap_or_else(|| "auto".to_string()),
            previous_version: None,
        };

        if let Some(result) = hook_runner.run(HookPoint::PostDeploy, &hook_context).await
            && !result.success
        {
            eprintln!("Warning: post-deploy hook failed for {}", server.host);
        }
    }

    println!("Deployment complete!");
    Ok(())
}

/// Rollback to previous deployment on all configured servers.
async fn rollback(config: Config) -> Result<()> {
    if config.servers.is_empty() {
        return Err(Error::NoServers);
    }

    println!(
        "Rolling back {} on {} server(s)",
        config.service,
        config.servers.len()
    );

    for server in &config.servers {
        if let Err(e) = rollback_on_server(&config, server).await {
            eprintln!("Failed to rollback on {}: {}", server.host, e);
            return Err(e);
        }
    }

    println!("Rollback complete!");
    Ok(())
}

/// Rollback on a single server.
async fn rollback_on_server(config: &Config, server: &ServerConfig) -> Result<()> {
    println!("  → Connecting to {}...", server.host);

    // Create SSH session
    let user = server
        .user
        .clone()
        .unwrap_or_else(|| env::var("USER").unwrap_or_else(|_| "root".to_string()));

    let ssh_config = SessionConfig::new(&server.host, &user)
        .port(server.port)
        .trust_on_first_use(server.trust_first_connection);

    let mut session = Session::connect(ssh_config)
        .await
        .map_err(|e| Error::Ssh(e.to_string()))?;

    // Detect runtime
    println!("  → Detecting runtime...");
    let runtime_info = detect_runtime(&session, Some(&server.runtime_config()))
        .await
        .map_err(|e| Error::RuntimeDetection(e.to_string()))?;

    println!(
        "  → Found {} at {}",
        runtime_info.runtime_type, runtime_info.socket_path
    );

    // Connect to runtime via SSH tunnel
    let runtime = connect_via_session(&mut session, runtime_info.runtime_type)
        .await
        .map_err(|e| Error::RuntimeDetection(e.to_string()))?;

    // Get network ID
    let network_name = config
        .network
        .as_ref()
        .map(|n| n.name.clone())
        .unwrap_or_else(|| "peleka".to_string());
    let network_id = peleka::types::NetworkId::new(network_name);

    // Perform rollback
    println!("  → Swapping containers...");
    manual_rollback(&runtime, &config.service, &network_id)
        .await
        .map_err(|e| Error::Deploy(e.to_string()))?;

    println!("  ✓ Rollback successful");

    // Disconnect SSH session
    session
        .disconnect()
        .await
        .map_err(|e| Error::Ssh(e.to_string()))?;

    Ok(())
}

/// Deploy to a single server.
async fn deploy_to_server(config: &Config, server: &ServerConfig, force: bool) -> Result<()> {
    println!("  → Connecting to {}...", server.host);

    // Create SSH session
    let user = server
        .user
        .clone()
        .unwrap_or_else(|| env::var("USER").unwrap_or_else(|_| "root".to_string()));

    let ssh_config = SessionConfig::new(&server.host, &user)
        .port(server.port)
        .trust_on_first_use(server.trust_first_connection);

    let session = Session::connect(ssh_config)
        .await
        .map_err(|e| Error::Ssh(e.to_string()))?;

    // Acquire deploy lock
    println!("  → Acquiring deploy lock...");
    let lock = DeployLock::acquire(&session, &config.service, force)
        .await
        .map_err(|e| Error::Deploy(e.to_string()))?;

    // Run deployment with lock, ensuring cleanup on error
    let result = deploy_to_server_inner(config, server, &session).await;

    // Release lock (always, even on error)
    lock.release()
        .await
        .map_err(|e| Error::Deploy(e.to_string()))?;

    // Disconnect SSH session
    session
        .disconnect()
        .await
        .map_err(|e| Error::Ssh(e.to_string()))?;

    result
}

/// Inner deployment logic (runs while holding lock).
async fn deploy_to_server_inner(
    config: &Config,
    server: &ServerConfig,
    session: &Session,
) -> Result<()> {
    // Detect runtime
    println!("  → Detecting runtime...");
    let runtime_info = detect_runtime(session, Some(&server.runtime_config()))
        .await
        .map_err(|e| Error::RuntimeDetection(e.to_string()))?;

    println!(
        "  → Found {} at {}",
        runtime_info.runtime_type, runtime_info.socket_path
    );

    // Connect to runtime via SSH tunnel
    // Note: We need a mutable reference for the tunnel, but session is borrowed immutably
    // This is a limitation - we need to restructure to avoid this
    // For now, create a new session for the tunnel
    let user = server
        .user
        .clone()
        .unwrap_or_else(|| env::var("USER").unwrap_or_else(|_| "root".to_string()));

    let ssh_config = SessionConfig::new(&server.host, &user)
        .port(server.port)
        .trust_on_first_use(server.trust_first_connection);

    let mut tunnel_session = Session::connect(ssh_config)
        .await
        .map_err(|e| Error::Ssh(e.to_string()))?;

    let runtime = connect_via_session(&mut tunnel_session, runtime_info.runtime_type)
        .await
        .map_err(|e| Error::RuntimeDetection(e.to_string()))?;

    // Find existing container for this service
    let old_container = find_existing_container(&runtime, &config.service).await?;

    if let Some(ref id) = old_container {
        println!("  → Found existing container: {}", id);
    } else {
        println!("  → No existing container (first deploy)");
    }

    // Create deployment
    let deployment: Deployment<Initialized> = if let Some(old_id) = old_container {
        Deployment::new_update(config.clone(), old_id)
    } else {
        Deployment::new(config.clone())
    };

    // Run deployment state machine
    run_deployment(deployment, &runtime).await?;

    // Disconnect tunnel session
    tunnel_session
        .disconnect()
        .await
        .map_err(|e| Error::Ssh(e.to_string()))?;

    Ok(())
}

/// Find existing container for a service.
async fn find_existing_container(
    runtime: &BollardRuntime,
    service: &peleka::types::ServiceName,
) -> Result<Option<peleka::types::ContainerId>> {
    let mut labels = HashMap::new();
    labels.insert("peleka.service".to_string(), service.to_string());
    labels.insert("peleka.managed".to_string(), "true".to_string());

    let filters = ContainerFilters {
        labels,
        all: false, // Only running containers
        ..Default::default()
    };

    let containers = runtime
        .list_containers(&filters)
        .await
        .map_err(|e| Error::Deploy(format!("failed to list containers: {}", e)))?;

    // Return the first running container found
    Ok(containers.into_iter().next().map(|c| c.id))
}

/// Run the deployment state machine.
async fn run_deployment(
    deployment: Deployment<Initialized>,
    runtime: &BollardRuntime,
) -> Result<()> {
    // Ensure network exists
    println!("  → Ensuring network exists...");
    let network_id = deployment
        .ensure_network(runtime)
        .await
        .map_err(|e| Error::Deploy(e.to_string()))?;

    // Pull image
    println!("  → Pulling image...");
    let deployment = deployment
        .pull_image(runtime, None)
        .await
        .map_err(|e| Error::Deploy(e.to_string()))?;

    // Start container
    println!("  → Starting container...");
    let deployment = deployment
        .start_container(runtime)
        .await
        .map_err(|e| Error::Deploy(e.to_string()))?;

    // Health check
    println!("  → Waiting for health check...");
    let health_timeout = deployment.config().health_timeout;
    let deployment = match deployment.health_check(runtime, health_timeout).await {
        Ok(d) => d,
        Err((failed_deployment, e)) => {
            eprintln!("  ✗ Health check failed: {}", e);
            println!("  → Rolling back...");
            failed_deployment
                .rollback(runtime)
                .await
                .map_err(|e| Error::Deploy(format!("rollback failed: {}", e)))?;
            return Err(Error::Deploy(e.to_string()));
        }
    };

    // Cutover
    println!("  → Cutting over traffic...");
    let deployment = deployment
        .cutover(runtime, &network_id)
        .await
        .map_err(|e| Error::Deploy(e.to_string()))?;

    // Cleanup old container
    println!("  → Cleaning up...");
    let deployment = deployment
        .cleanup(runtime)
        .await
        .map_err(|e| Error::Deploy(e.to_string()))?;

    println!(
        "  ✓ Deployed container: {}",
        deployment.deployed_container()
    );

    Ok(())
}
