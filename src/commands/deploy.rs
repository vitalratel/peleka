// ABOUTME: Deploy command implementation.
// ABOUTME: Handles deployment orchestration, hooks, and state machine execution.

use super::runtime_connection::connect_to_runtime;
use peleka::config::{Config, ServerConfig};
use peleka::deploy::{
    ContainerErrorExt, DeployError, DeployLock, DeployStrategy, Deployment, Initialized,
    cleanup_orphans, detect_orphans,
};
use peleka::diagnostics::{Diagnostics, Warning};
use peleka::error::{Error, Result};
use peleka::hooks::{HookContext, HookPoint, HookRunner};
use peleka::output::Output;
use peleka::runtime::{BollardRuntime, ContainerFilters, ContainerOps};
use peleka::ssh::Session;
use std::env;

/// Deploy to all configured servers.
pub async fn deploy(config: Config, force: bool, mut output: Output) -> Result<()> {
    if config.servers.is_empty() {
        return Err(Error::NoServers);
    }

    output.start_timer();
    let cwd = env::current_dir()?;
    let hook_runner = HookRunner::new(&cwd);
    let mut diag = Diagnostics::default();

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
        if let Err(e) = deploy_to_server(&config, server, force, &output, &mut diag).await {
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

    // Emit collected warnings
    for warning in diag.warnings() {
        output.warning(&warning.message);
    }

    output.success("Deployment complete!");
    Ok(())
}

/// Deploy to a single server.
async fn deploy_to_server(
    config: &Config,
    server: &ServerConfig,
    force: bool,
    output: &Output,
    diag: &mut Diagnostics,
) -> Result<()> {
    output.progress(&format!("  → Connecting to {}...", server.host));

    let session = Session::connect(server.ssh_session_config()).await?;

    // Run deployment with lock, ensuring cleanup on error or panic
    output.progress("  → Acquiring deploy lock...");
    let result = DeployLock::with_lock(&session, &config.service, force, async {
        deploy_to_server_inner(config, server, &session, output).await
    })
    .await;

    // Disconnect SSH session (non-fatal if it fails)
    if let Err(e) = session.disconnect().await {
        diag.warn(Warning::ssh_disconnect(format!(
            "SSH disconnect failed for {}: {}",
            server.host, e
        )));
    }

    result
}

/// Inner deployment logic (runs while holding lock).
async fn deploy_to_server_inner(
    config: &Config,
    server: &ServerConfig,
    session: &Session,
    output: &Output,
) -> Result<()> {
    let runtime = connect_to_runtime(session, server, output).await?;

    // Determine deployment strategy
    let (strategy, reason) = DeployStrategy::for_config(config);
    if let Some(reason) = reason {
        output.warning(&format!(
            "Using recreate strategy (brief downtime): {}",
            reason
        ));
    }

    // Find existing container for this service
    let old_container = find_existing_container(&runtime, &config.service).await?;

    if let Some(ref id) = old_container {
        output.progress(&format!("  → Found existing container: {}", id));
    } else {
        output.progress("  → No existing container (first deploy)");
    }

    // Handle strategy-specific pre-deployment and create deployment state machine.
    // Using a single match to properly transfer ownership without cloning.
    let (deployment, old_to_remove): (Deployment<Initialized>, Option<_>) =
        match (strategy, old_container) {
            (DeployStrategy::Recreate, Some(old_id)) => {
                output.progress("  → Stopping old container (recreate strategy)...");
                let stop_timeout = config.stop_timeout();
                runtime
                    .stop_container(&old_id, stop_timeout)
                    .await
                    .context_container_stop()?;
                // Don't track old container in deployment - it's already stopped
                // Keep ownership for removal after successful deploy
                (Deployment::new(config.clone()), Some(old_id))
            }
            (DeployStrategy::BlueGreen, Some(old_id)) => {
                // Give ownership to deployment for blue-green cutover
                (Deployment::new_update(config.clone(), old_id), None)
            }
            (_, None) => (Deployment::new(config.clone()), None),
        };

    // Run deployment state machine
    run_deployment(deployment, &runtime, config, output).await?;

    // For recreate strategy, remove the stopped old container after successful deploy
    if let Some(old_id) = old_to_remove {
        output.progress("  → Removing old container...");
        if let Err(e) = runtime.remove_container(&old_id, true).await {
            // Non-fatal: log and continue
            tracing::warn!("Failed to remove old container: {}", e);
        }
    }

    Ok(())
}

/// Find existing container for a service.
pub async fn find_existing_container(
    runtime: &BollardRuntime,
    service: &peleka::types::ServiceName,
) -> Result<Option<peleka::types::ContainerId>> {
    let filters = ContainerFilters::for_service(service, false);

    let containers = runtime
        .list_containers(&filters)
        .await
        .map_err(|e| DeployError::config_error(format!("failed to list containers: {}", e)))?;

    // Return the first running container found
    Ok(containers.into_iter().next().map(|c| c.id))
}

/// Run the deployment state machine.
async fn run_deployment(
    deployment: Deployment<Initialized>,
    runtime: &BollardRuntime,
    config: &Config,
    output: &Output,
) -> Result<()> {
    // Ensure network exists
    output.progress("  → Ensuring network exists...");
    let network_id = deployment.ensure_network(runtime).await?;

    // Pull image
    output.progress("  → Pulling image...");
    let deployment = deployment.pull_image(runtime, None).await?;

    // Start container
    output.progress("  → Starting container...");
    let deployment = deployment.start_container(runtime).await?;

    // Health check
    output.progress("  → Waiting for health check...");
    let health_timeout = deployment.config().health_timeout;
    let deployment = match deployment.health_check(runtime, health_timeout).await {
        Ok(d) => d,
        Err((failed_deployment, e)) => {
            eprintln!("  ✗ Health check failed: {}", e);
            output.progress("  → Rolling back...");
            failed_deployment.rollback(runtime).await?;
            return Err(e.into());
        }
    };

    // Cutover
    output.progress("  → Cutting over traffic...");
    let deployment = deployment.cutover(runtime, &network_id).await?;

    // Cleanup old container
    output.progress("  → Cleaning up...");
    let deployment = deployment.cleanup(runtime).await?;

    // Detect and cleanup orphaned containers
    let deployed_id = deployment.deployed_container().clone();
    let old_id = deployment.config().service.clone();
    let deployment_config = deployment.finish();

    // Build list of known containers (newly deployed + old if any)
    let mut known_containers = vec![deployed_id.clone()];
    if let Some(ref old_container) = find_existing_container(runtime, &old_id).await? {
        known_containers.push(old_container.clone());
    }

    let orphans = detect_orphans(runtime, &config.service, &known_containers)
        .await
        .map_err(|e| DeployError::config_error(format!("failed to detect orphans: {}", e)))?;

    if !orphans.is_empty() {
        output.progress(&format!(
            "  → Cleaning up {} orphaned container(s)...",
            orphans.len()
        ));
        let orphan_ids: Vec<_> = orphans.iter().map(|o| o.id.clone()).collect();
        let result =
            cleanup_orphans(runtime, &orphan_ids, true, deployment_config.stop_timeout()).await;

        if !result.all_succeeded() {
            for failure in &result.failed {
                tracing::warn!(
                    "Failed to cleanup orphan container {}: {}",
                    failure.container_id,
                    failure.error
                );
            }
        }
    }

    output.progress(&format!("  ✓ Deployed container: {}", deployed_id));

    Ok(())
}
