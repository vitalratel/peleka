// ABOUTME: Exec operations trait for container runtimes.
// ABOUTME: Execute commands inside running containers.

use super::sealed::Sealed;
use super::shared_types::{ExecConfig, ExecResult};
use crate::types::ContainerId;
use async_trait::async_trait;

/// Exec operations: run commands in containers.
#[async_trait]
pub trait ExecOps: Sealed + Send + Sync {
    /// Create and run an exec instance, returning the result.
    async fn exec(
        &self,
        container: &ContainerId,
        config: &ExecConfig,
    ) -> Result<ExecResult, ExecError>;

    /// Create an exec instance without starting it.
    async fn exec_create(
        &self,
        container: &ContainerId,
        config: &ExecConfig,
    ) -> Result<String, ExecError>;

    /// Start a created exec instance.
    async fn exec_start(&self, exec_id: &str) -> Result<ExecResult, ExecError>;
}

/// Errors from exec operations.
#[derive(Debug, thiserror::Error)]
pub enum ExecError {
    #[error("container not found: {0}")]
    ContainerNotFound(String),

    #[error("container not running: {0}")]
    ContainerNotRunning(String),

    #[error("exec instance not found: {0}")]
    ExecNotFound(String),

    #[error("exec failed: {0}")]
    Failed(String),

    #[error("runtime error: {0}")]
    Runtime(String),
}
