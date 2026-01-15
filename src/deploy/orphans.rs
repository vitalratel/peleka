// ABOUTME: Orphan container detection for cleanup.
// ABOUTME: Finds containers from interrupted deployments that need cleanup.

use crate::runtime::{ContainerFilters, ContainerOps, ContainerSummary};
use crate::types::{ContainerId, ServiceName};

/// Detect orphaned containers for a service.
///
/// An orphan is a container that:
/// - Is managed by peleka (`peleka.managed=true`)
/// - Belongs to the specified service (`peleka.service=<service>`)
/// - Is not in the provided list of known containers (old/new)
///
/// # Arguments
///
/// * `runtime` - The container runtime to query
/// * `service` - The service name to check for orphans
/// * `known_containers` - Container IDs that are known/expected (current deployment)
///
/// # Returns
///
/// List of orphaned container summaries.
pub async fn detect_orphans<R: ContainerOps>(
    runtime: &R,
    service: &ServiceName,
    known_containers: &[ContainerId],
) -> Result<Vec<ContainerSummary>, crate::runtime::ContainerError> {
    let mut labels = std::collections::HashMap::new();
    labels.insert("peleka.service".to_string(), service.to_string());
    labels.insert("peleka.managed".to_string(), "true".to_string());

    let filters = ContainerFilters {
        labels,
        all: true, // Include stopped containers
        ..Default::default()
    };

    let containers = runtime.list_containers(&filters).await?;

    // Filter out known containers
    let orphans: Vec<ContainerSummary> = containers
        .into_iter()
        .filter(|c| !known_containers.iter().any(|k| k == &c.id))
        .collect();

    Ok(orphans)
}

/// Clean up orphaned containers.
///
/// Stops and removes all provided containers.
///
/// # Arguments
///
/// * `runtime` - The container runtime
/// * `orphans` - Container IDs to clean up
/// * `force` - Whether to force removal
///
/// # Returns
///
/// Number of containers cleaned up.
pub async fn cleanup_orphans<R: ContainerOps>(
    runtime: &R,
    orphans: &[ContainerId],
    force: bool,
) -> Result<usize, crate::runtime::ContainerError> {
    let mut cleaned = 0;

    for container_id in orphans {
        // Best effort: try to stop first, then remove
        let _ = runtime
            .stop_container(container_id, std::time::Duration::from_secs(10))
            .await;

        if runtime.remove_container(container_id, force).await.is_ok() {
            cleaned += 1;
        }
    }

    Ok(cleaned)
}
