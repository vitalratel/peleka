// ABOUTME: Orphan container detection for cleanup.
// ABOUTME: Finds containers from interrupted deployments that need cleanup.

use crate::runtime::{ContainerError, ContainerFilters, ContainerOps, ContainerSummary};
use crate::types::{ContainerId, ServiceName};

/// Result of a cleanup operation.
#[derive(Debug)]
pub struct CleanupResult {
    /// Container IDs that were successfully removed.
    pub succeeded: Vec<ContainerId>,
    /// Containers that failed to be removed, with their errors.
    pub failed: Vec<CleanupFailure>,
}

impl CleanupResult {
    /// Returns true if all containers were cleaned up successfully.
    pub fn all_succeeded(&self) -> bool {
        self.failed.is_empty()
    }

    /// Returns the total number of containers that were attempted.
    pub fn total(&self) -> usize {
        self.succeeded.len() + self.failed.len()
    }
}

/// A single cleanup failure.
#[derive(Debug)]
pub struct CleanupFailure {
    /// The container that failed to be cleaned up.
    pub container_id: ContainerId,
    /// The error that occurred during cleanup.
    pub error: ContainerError,
}

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
    let filters = ContainerFilters::for_service(service, true);

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
/// Stops and removes all provided containers. Returns detailed results
/// for each container, including any failures.
///
/// # Arguments
///
/// * `runtime` - The container runtime
/// * `orphans` - Container IDs to clean up
/// * `force` - Whether to force removal
/// * `stop_timeout` - Timeout for stopping each container
///
/// # Returns
///
/// A `CleanupResult` containing lists of succeeded and failed cleanups.
pub async fn cleanup_orphans<R: ContainerOps>(
    runtime: &R,
    orphans: &[ContainerId],
    force: bool,
    stop_timeout: std::time::Duration,
) -> CleanupResult {
    let mut succeeded = Vec::new();
    let mut failed = Vec::new();

    for container_id in orphans {
        // Try to stop first (ignore errors - container might already be stopped)
        let _ = runtime.stop_container(container_id, stop_timeout).await;

        match runtime.remove_container(container_id, force).await {
            Ok(()) => succeeded.push(container_id.clone()),
            Err(error) => failed.push(CleanupFailure {
                container_id: container_id.clone(),
                error,
            }),
        }
    }

    CleanupResult { succeeded, failed }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_result_all_succeeded_when_no_failures() {
        let result = CleanupResult {
            succeeded: vec![ContainerId::new("abc123".to_string())],
            failed: vec![],
        };
        assert!(result.all_succeeded());
        assert_eq!(result.total(), 1);
    }

    #[test]
    fn cleanup_result_not_all_succeeded_when_failures() {
        let result = CleanupResult {
            succeeded: vec![ContainerId::new("abc123".to_string())],
            failed: vec![CleanupFailure {
                container_id: ContainerId::new("def456".to_string()),
                error: ContainerError::NotFound("def456".to_string()),
            }],
        };
        assert!(!result.all_succeeded());
        assert_eq!(result.total(), 2);
    }

    #[test]
    fn cleanup_result_empty() {
        let result = CleanupResult {
            succeeded: vec![],
            failed: vec![],
        };
        assert!(result.all_succeeded());
        assert_eq!(result.total(), 0);
    }
}
