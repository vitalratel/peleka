// ABOUTME: Entry point for the peleka CLI application.
// ABOUTME: Parses arguments and dispatches to appropriate command handlers.

mod cli;

use clap::Parser;
use cli::{Cli, Commands};
use peleka::config::{self, Config, ServerConfig};
use peleka::deploy::{DeployLock, Deployment, Initialized, manual_rollback};
use peleka::error::{Error, Result};
use peleka::hooks::{HookContext, HookPoint, HookRunner};
use peleka::output::{Output, OutputMode};
use peleka::runtime::{
    BollardRuntime, ContainerFilters, ContainerOps, ExecConfig, ExecOps, connect_via_session,
    detect_runtime,
};
use peleka::ssh::Session;
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
        eprintln!("Error: {e}");
        std::process::exit(1);
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

            deploy(config, force, output).await
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

            rollback(config, output).await
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

            exec_command(config, command, output).await
        }
    }
}

/// Deploy to all configured servers.
async fn deploy(config: Config, force: bool, mut output: Output) -> Result<()> {
    if config.servers.is_empty() {
        return Err(Error::NoServers);
    }

    output.start_timer();
    let cwd = env::current_dir()?;
    let hook_runner = HookRunner::new(&cwd);

    output.progress(&format!(
        "Deploying {} ({}) to {} server(s)",
        config.service,
        config.image,
        config.servers.len()
    ));

    // Run pre-deploy hook for each server
    for server in &config.servers {
        let hook_context = HookContext::new(&config, server);

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
        if let Err(e) = deploy_to_server(&config, server, force, &output).await {
            eprintln!("Failed to deploy to {}: {}", server.host, e);

            // Run on-error hook
            let hook_context = HookContext::new(&config, server);

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
        let hook_context = HookContext::new(&config, server);

        if let Some(result) = hook_runner.run(HookPoint::PostDeploy, &hook_context).await
            && !result.success
        {
            eprintln!("Warning: post-deploy hook failed for {}", server.host);
        }
    }

    output.success("Deployment complete!");
    Ok(())
}

/// Rollback to previous deployment on all configured servers.
async fn rollback(config: Config, mut output: Output) -> Result<()> {
    if config.servers.is_empty() {
        return Err(Error::NoServers);
    }

    output.start_timer();

    output.progress(&format!(
        "Rolling back {} on {} server(s)",
        config.service,
        config.servers.len()
    ));

    for server in &config.servers {
        if let Err(e) = rollback_on_server(&config, server, &output).await {
            eprintln!("Failed to rollback on {}: {}", server.host, e);
            return Err(e);
        }
    }

    output.success("Rollback complete!");
    Ok(())
}

/// Execute a command in the service container.
async fn exec_command(config: Config, command: Vec<String>, output: Output) -> Result<()> {
    if config.servers.is_empty() {
        return Err(Error::NoServers);
    }

    // Execute on first server only
    let server = &config.servers[0];
    exec_on_server(&config, server, &command, &output).await
}

/// Execute a command on a single server.
async fn exec_on_server(
    config: &Config,
    server: &ServerConfig,
    command: &[String],
    output: &Output,
) -> Result<()> {
    output.progress(&format!("  → Connecting to {}...", server.host));

    let session = Session::connect(server.ssh_session_config())
        .await
        .map_err(|e| Error::Ssh(e.to_string()))?;

    // Detect runtime
    output.progress("  → Detecting runtime...");
    let runtime_info = detect_runtime(&session, Some(&server.runtime_config()))
        .await
        .map_err(|e| Error::RuntimeDetection(e.to_string()))?;

    output.progress(&format!(
        "  → Found {} at {}",
        runtime_info.runtime_type, runtime_info.socket_path
    ));

    // Connect to runtime via SSH tunnel
    let runtime = connect_via_session(&session, runtime_info.runtime_type)
        .await
        .map_err(|e| Error::RuntimeDetection(e.to_string()))?;

    // Find running container for this service
    let container_id = find_existing_container(&runtime, &config.service)
        .await?
        .ok_or_else(|| Error::Deploy("no running container found for service".to_string()))?;

    output.progress(&format!("  → Executing in container {}...", container_id));

    // Build exec config
    let exec_config = ExecConfig {
        cmd: command.to_vec(),
        env: vec![],
        working_dir: None,
        user: None,
        attach_stdin: false,
        attach_stdout: true,
        attach_stderr: true,
        tty: false,
        privileged: false,
        timeout: None, // No timeout for CLI exec commands
    };

    // Execute command
    let result = runtime
        .exec(&container_id, &exec_config)
        .await
        .map_err(|e| Error::Deploy(format!("exec failed: {}", e)))?;

    // Print output
    if !result.stdout.is_empty() {
        let stdout = String::from_utf8_lossy(&result.stdout);
        print!("{}", stdout);
    }
    if !result.stderr.is_empty() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        eprint!("{}", stderr);
    }

    // Check exit code
    if result.exit_code != 0 {
        return Err(Error::Deploy(format!(
            "command exited with code {}",
            result.exit_code
        )));
    }

    // Disconnect SSH session
    session
        .disconnect()
        .await
        .map_err(|e| Error::Ssh(e.to_string()))?;

    Ok(())
}

/// Rollback on a single server.
async fn rollback_on_server(config: &Config, server: &ServerConfig, output: &Output) -> Result<()> {
    output.progress(&format!("  → Connecting to {}...", server.host));

    let session = Session::connect(server.ssh_session_config())
        .await
        .map_err(|e| Error::Ssh(e.to_string()))?;

    // Detect runtime
    output.progress("  → Detecting runtime...");
    let runtime_info = detect_runtime(&session, Some(&server.runtime_config()))
        .await
        .map_err(|e| Error::RuntimeDetection(e.to_string()))?;

    output.progress(&format!(
        "  → Found {} at {}",
        runtime_info.runtime_type, runtime_info.socket_path
    ));

    // Connect to runtime via SSH tunnel
    let runtime = connect_via_session(&session, runtime_info.runtime_type)
        .await
        .map_err(|e| Error::RuntimeDetection(e.to_string()))?;

    // Get network ID
    let network_id = peleka::types::NetworkId::new(config.network_name());

    // Perform rollback
    output.progress("  → Swapping containers...");
    manual_rollback(&runtime, &config.service, &network_id)
        .await
        .map_err(|e| Error::Deploy(e.to_string()))?;

    output.progress("  ✓ Rollback successful");

    // Disconnect SSH session
    session
        .disconnect()
        .await
        .map_err(|e| Error::Ssh(e.to_string()))?;

    Ok(())
}

/// Deploy to a single server.
async fn deploy_to_server(
    config: &Config,
    server: &ServerConfig,
    force: bool,
    output: &Output,
) -> Result<()> {
    output.progress(&format!("  → Connecting to {}...", server.host));

    let session = Session::connect(server.ssh_session_config())
        .await
        .map_err(|e| Error::Ssh(e.to_string()))?;

    // Acquire deploy lock
    output.progress("  → Acquiring deploy lock...");
    let lock = DeployLock::acquire(&session, &config.service, force)
        .await
        .map_err(|e| Error::Deploy(e.to_string()))?;

    // Run deployment with lock, ensuring cleanup on error
    let result = deploy_to_server_inner(config, server, &session, output).await;

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
    output: &Output,
) -> Result<()> {
    // Detect runtime
    output.progress("  → Detecting runtime...");
    let runtime_info = detect_runtime(session, Some(&server.runtime_config()))
        .await
        .map_err(|e| Error::RuntimeDetection(e.to_string()))?;

    output.progress(&format!(
        "  → Found {} at {}",
        runtime_info.runtime_type, runtime_info.socket_path
    ));

    // Connect to runtime via SSH tunnel
    let runtime = connect_via_session(session, runtime_info.runtime_type)
        .await
        .map_err(|e| Error::RuntimeDetection(e.to_string()))?;

    // Find existing container for this service
    let old_container = find_existing_container(&runtime, &config.service).await?;

    if let Some(ref id) = old_container {
        output.progress(&format!("  → Found existing container: {}", id));
    } else {
        output.progress("  → No existing container (first deploy)");
    }

    // Create deployment
    let deployment: Deployment<Initialized> = if let Some(old_id) = old_container {
        Deployment::new_update(config.clone(), old_id)
    } else {
        Deployment::new(config.clone())
    };

    // Run deployment state machine
    run_deployment(deployment, &runtime, output).await?;

    Ok(())
}

/// Find existing container for a service.
async fn find_existing_container(
    runtime: &BollardRuntime,
    service: &peleka::types::ServiceName,
) -> Result<Option<peleka::types::ContainerId>> {
    let filters = ContainerFilters::for_service(service, false);

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
    output: &Output,
) -> Result<()> {
    // Ensure network exists
    output.progress("  → Ensuring network exists...");
    let network_id = deployment
        .ensure_network(runtime)
        .await
        .map_err(|e| Error::Deploy(e.to_string()))?;

    // Pull image
    output.progress("  → Pulling image...");
    let deployment = deployment
        .pull_image(runtime, None)
        .await
        .map_err(|e| Error::Deploy(e.to_string()))?;

    // Start container
    output.progress("  → Starting container...");
    let deployment = deployment
        .start_container(runtime)
        .await
        .map_err(|e| Error::Deploy(e.to_string()))?;

    // Health check
    output.progress("  → Waiting for health check...");
    let health_timeout = deployment.config().health_timeout;
    let deployment = match deployment.health_check(runtime, health_timeout).await {
        Ok(d) => d,
        Err((failed_deployment, e)) => {
            eprintln!("  ✗ Health check failed: {}", e);
            output.progress("  → Rolling back...");
            failed_deployment
                .rollback(runtime)
                .await
                .map_err(|e| Error::Deploy(format!("rollback failed: {}", e)))?;
            return Err(Error::Deploy(e.to_string()));
        }
    };

    // Cutover
    output.progress("  → Cutting over traffic...");
    let deployment = deployment
        .cutover(runtime, &network_id)
        .await
        .map_err(|e| Error::Deploy(e.to_string()))?;

    // Cleanup old container
    output.progress("  → Cleaning up...");
    let deployment = deployment
        .cleanup(runtime)
        .await
        .map_err(|e| Error::Deploy(e.to_string()))?;

    output.progress(&format!(
        "  ✓ Deployed container: {}",
        deployment.deployed_container()
    ));

    Ok(())
}
