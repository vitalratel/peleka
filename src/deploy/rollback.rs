// ABOUTME: Manual rollback functionality for restoring previous deployments.
// ABOUTME: Swaps active and previous containers for a service.

use std::time::Duration;

use crate::runtime::{ContainerFilters, ContainerOps, NetworkOps};
use crate::types::{NetworkAlias, NetworkId, ServiceName};

use super::DeployError;

/// Manual rollback - swap active and previous containers.
///
/// This function:
/// 1. Finds all peleka-managed containers for the service
/// 2. Identifies running (active) and stopped (previous) containers
/// 3. Starts the previous container
/// 4. Updates network aliases to point to the previous container
/// 5. Stops the previously active container
///
/// After rollback, what was "previous" becomes "active" and vice versa.
/// This enables ping-pong behavior: double rollback returns to original state.
///
/// # Arguments
///
/// * `runtime` - The container runtime
/// * `service` - The service name to rollback
/// * `network_id` - The network to reconnect containers to
/// * `stop_timeout` - Timeout for stopping the active container
///
/// # Errors
///
/// Returns error if:
/// - No active container found
/// - No previous container found (nothing to roll back to)
/// - Container operations fail
pub async fn manual_rollback<R: ContainerOps + NetworkOps>(
    runtime: &R,
    service: &ServiceName,
    network_id: &NetworkId,
    stop_timeout: Duration,
) -> Result<(), DeployError> {
    // Find all containers for this service
    let filters = ContainerFilters::for_service(service, true);

    let containers = runtime
        .list_containers(&filters)
        .await
        .map_err(|e| DeployError::rollback_failed(format!("failed to list containers: {}", e)))?;

    // Separate running (active) and stopped (previous) containers
    let (running, stopped): (Vec<_>, Vec<_>) =
        containers.into_iter().partition(|c| c.state == "running");

    let active = running.into_iter().next().ok_or_else(|| {
        DeployError::rollback_failed("no running container found for service".to_string())
    })?;

    let previous = stopped
        .into_iter()
        .next()
        .ok_or_else(|| DeployError::no_previous_deployment(service.to_string()))?;

    // Start the previous container
    runtime.start_container(&previous.id).await.map_err(|e| {
        DeployError::rollback_failed(format!("failed to start previous container: {}", e))
    })?;

    // Get the service alias
    let alias = NetworkAlias::new(service.as_str()).map_err(|e| {
        DeployError::rollback_failed(format!("invalid service name for alias: {}", e))
    })?;

    // Disconnect active container from network
    let _ = runtime
        .disconnect_from_network(&active.id, network_id)
        .await;

    // Connect previous container to network with service alias.
    // The container may already be connected, so ignore "already connected"
    // or "already exists" errors (Docker uses different wording).
    if let Err(e) = runtime
        .connect_to_network(&previous.id, network_id, &[alias])
        .await
    {
        let err_str = e.to_string().to_lowercase();
        if !err_str.contains("already connected") && !err_str.contains("already exists") {
            return Err(DeployError::rollback_failed(format!(
                "failed to connect previous container to network: {}",
                e
            )));
        }
    }

    // Stop the previously active container
    runtime
        .stop_container(&active.id, stop_timeout)
        .await
        .map_err(|e| {
            DeployError::rollback_failed(format!("failed to stop active container: {}", e))
        })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_rollback_exists() {
        // Type check - ensure the function signature is correct
        fn _check<'a, R: ContainerOps + NetworkOps>(
            runtime: &'a R,
            service: &'a ServiceName,
            network_id: &'a NetworkId,
            stop_timeout: Duration,
        ) -> impl std::future::Future<Output = Result<(), DeployError>> + 'a {
            manual_rollback(runtime, service, network_id, stop_timeout)
        }
    }
}
