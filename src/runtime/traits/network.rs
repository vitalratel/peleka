// ABOUTME: Network operations trait for container runtimes.
// ABOUTME: Create networks, connect/disconnect containers, manage aliases.

use super::sealed::Sealed;
use super::shared_types::NetworkConfig;
use crate::types::{ContainerId, NetworkAlias, NetworkId};
use async_trait::async_trait;

/// Network operations: create, connect, disconnect.
#[async_trait]
pub trait NetworkOps: Sealed + Send + Sync {
    /// Create a network.
    async fn create_network(&self, config: &NetworkConfig) -> Result<NetworkId, NetworkError>;

    /// Remove a network.
    async fn remove_network(&self, id: &NetworkId) -> Result<(), NetworkError>;

    /// Connect a container to a network with optional aliases.
    async fn connect_to_network(
        &self,
        container: &ContainerId,
        network: &NetworkId,
        aliases: &[NetworkAlias],
    ) -> Result<(), NetworkError>;

    /// Disconnect a container from a network.
    async fn disconnect_from_network(
        &self,
        container: &ContainerId,
        network: &NetworkId,
    ) -> Result<(), NetworkError>;

    /// Check if a network exists.
    async fn network_exists(&self, name: &str) -> Result<bool, NetworkError>;
}

/// Errors from network operations.
#[derive(Debug, thiserror::Error)]
pub enum NetworkError {
    #[error("network not found: {0}")]
    NotFound(String),

    #[error("network already exists: {0}")]
    AlreadyExists(String),

    #[error("container not found: {0}")]
    ContainerNotFound(String),

    #[error("container not connected to network: {0}")]
    NotConnected(String),

    #[error("network in use, cannot remove: {0}")]
    InUse(String),

    #[error("runtime error: {0}")]
    Runtime(String),
}
