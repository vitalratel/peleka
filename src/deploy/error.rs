// ABOUTME: Error types for deployment operations.
// ABOUTME: Covers image pull, container, network, and health check failures.

use crate::runtime::{ContainerError, ImageError, NetworkError};

/// Errors that can occur during deployment state transitions.
#[derive(Debug, thiserror::Error)]
pub enum DeployError {
    /// Image pull failed.
    #[error("failed to pull image: {0}")]
    ImagePullFailed(String),

    /// Container creation failed.
    #[error("failed to create container: {0}")]
    ContainerCreateFailed(String),

    /// Container start failed.
    #[error("failed to start container: {0}")]
    ContainerStartFailed(String),

    /// Container stop failed.
    #[error("failed to stop container: {0}")]
    ContainerStopFailed(String),

    /// Container removal failed.
    #[error("failed to remove container: {0}")]
    ContainerRemoveFailed(String),

    /// Network operation failed.
    #[error("network operation failed: {0}")]
    NetworkFailed(String),

    /// Network creation failed.
    #[error("failed to create network: {0}")]
    NetworkCreationFailed(String),

    /// Health check failed.
    #[error("health check failed: {0}")]
    HealthCheckFailed(String),

    /// Health check timed out.
    #[error("health check timed out after {0} seconds")]
    HealthCheckTimeout(u64),

    /// Rollback failed.
    #[error("rollback failed: {0}")]
    RollbackFailed(String),

    /// No old container to rollback to.
    #[error("no old container to rollback to (first deployment)")]
    NoOldContainer,

    /// Configuration error.
    #[error("configuration error: {0}")]
    ConfigError(String),
}

impl From<ImageError> for DeployError {
    fn from(err: ImageError) -> Self {
        DeployError::ImagePullFailed(err.to_string())
    }
}

impl From<ContainerError> for DeployError {
    fn from(err: ContainerError) -> Self {
        DeployError::ContainerCreateFailed(err.to_string())
    }
}

impl From<NetworkError> for DeployError {
    fn from(err: NetworkError) -> Self {
        DeployError::NetworkFailed(err.to_string())
    }
}
