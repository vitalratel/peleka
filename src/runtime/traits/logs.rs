// ABOUTME: Log operations trait for container runtimes.
// ABOUTME: Stream container logs with filtering options.

use super::sealed::Sealed;
use crate::types::ContainerId;
use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;
use std::time::SystemTime;

/// Log streaming operations.
#[async_trait]
pub trait LogOps: Sealed + Send + Sync {
    /// Stream logs from a container.
    async fn container_logs(
        &self,
        id: &ContainerId,
        opts: &LogOptions,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<LogLine, LogError>> + Send>>, LogError>;
}

/// Options for log streaming.
#[derive(Debug, Clone, Default)]
pub struct LogOptions {
    /// Include stdout.
    pub stdout: bool,
    /// Include stderr.
    pub stderr: bool,
    /// Follow log output (like `tail -f`).
    pub follow: bool,
    /// Show timestamps.
    pub timestamps: bool,
    /// Number of lines to show from end (0 = all).
    pub tail: Option<u64>,
    /// Show logs since this time.
    pub since: Option<SystemTime>,
    /// Show logs until this time.
    pub until: Option<SystemTime>,
}

impl LogOptions {
    /// Create options for following all logs.
    pub fn follow_all() -> Self {
        Self {
            stdout: true,
            stderr: true,
            follow: true,
            timestamps: true,
            tail: None,
            since: None,
            until: None,
        }
    }

    /// Create options for tailing the last N lines.
    pub fn tail(n: u64) -> Self {
        Self {
            stdout: true,
            stderr: true,
            follow: false,
            timestamps: false,
            tail: Some(n),
            since: None,
            until: None,
        }
    }
}

/// A single log line from a container.
#[derive(Debug, Clone)]
pub struct LogLine {
    /// The log content.
    pub content: String,
    /// Whether this is from stdout or stderr.
    pub stream: LogStream,
    /// Timestamp (if requested).
    pub timestamp: Option<SystemTime>,
}

/// Log stream type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogStream {
    Stdout,
    Stderr,
}

/// Errors from log operations.
#[derive(Debug, thiserror::Error)]
pub enum LogError {
    #[error("container not found: {0}")]
    ContainerNotFound(String),

    #[error("stream error: {0}")]
    StreamError(String),

    #[error("runtime error: {0}")]
    Runtime(String),
}
