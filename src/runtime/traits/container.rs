// ABOUTME: Container operations trait for container runtimes.
// ABOUTME: Create, start, stop, remove, inspect, and list containers.

use super::sealed::Sealed;
use super::shared_types::{ContainerConfig, ContainerInfo};
use crate::types::ContainerId;
use async_trait::async_trait;
use std::collections::HashMap;
use std::time::Duration;

/// Container lifecycle operations.
#[async_trait]
pub trait ContainerOps: Sealed + Send + Sync {
    /// Create a container from the given configuration.
    async fn create_container(
        &self,
        config: &ContainerConfig,
    ) -> Result<ContainerId, ContainerError>;

    /// Start a created container.
    async fn start_container(&self, id: &ContainerId) -> Result<(), ContainerError>;

    /// Stop a running container.
    async fn stop_container(
        &self,
        id: &ContainerId,
        timeout: Duration,
    ) -> Result<(), ContainerError>;

    /// Remove a container.
    async fn remove_container(&self, id: &ContainerId, force: bool) -> Result<(), ContainerError>;

    /// Get detailed information about a container.
    async fn inspect_container(&self, id: &ContainerId) -> Result<ContainerInfo, ContainerError>;

    /// List containers matching the given filters.
    async fn list_containers(
        &self,
        filters: &ContainerFilters,
    ) -> Result<Vec<ContainerSummary>, ContainerError>;

    /// Rename a container.
    async fn rename_container(
        &self,
        id: &ContainerId,
        new_name: &str,
    ) -> Result<(), ContainerError>;
}

/// Filters for listing containers.
#[derive(Debug, Clone, Default)]
pub struct ContainerFilters {
    /// Filter by label (key=value).
    pub labels: HashMap<String, String>,
    /// Filter by name (supports partial match).
    pub name: Option<String>,
    /// Include stopped containers.
    pub all: bool,
}

/// Summary information about a container.
#[derive(Debug, Clone)]
pub struct ContainerSummary {
    /// Container ID.
    pub id: ContainerId,
    /// Container name.
    pub name: String,
    /// Image used.
    pub image: String,
    /// Current state.
    pub state: String,
    /// Status message.
    pub status: String,
    /// Labels.
    pub labels: HashMap<String, String>,
}

/// Errors from container operations.
#[derive(Debug, thiserror::Error)]
pub enum ContainerError {
    #[error("container not found: {0}")]
    NotFound(String),

    #[error("container already exists: {0}")]
    AlreadyExists(String),

    #[error("container not running: {0}")]
    NotRunning(String),

    #[error("container already running: {0}")]
    AlreadyRunning(String),

    #[error("image not found: {0}")]
    ImageNotFound(String),

    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("runtime error: {0}")]
    Runtime(String),
}
