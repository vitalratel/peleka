// ABOUTME: Runtime info trait for container runtimes.
// ABOUTME: Query runtime version and metadata.

use super::sealed::Sealed;
use super::shared_types::RuntimeMetadata;
use async_trait::async_trait;

/// Runtime metadata operations.
#[async_trait]
pub trait RuntimeInfo: Sealed + Send + Sync {
    /// Get runtime version and metadata.
    async fn info(&self) -> Result<RuntimeMetadata, RuntimeInfoError>;

    /// Ping the runtime to check connectivity.
    async fn ping(&self) -> Result<(), RuntimeInfoError>;
}

/// Errors from runtime info operations.
#[derive(Debug, thiserror::Error)]
pub enum RuntimeInfoError {
    #[error("connection failed: {0}")]
    ConnectionFailed(String),

    #[error("runtime error: {0}")]
    Runtime(String),
}
