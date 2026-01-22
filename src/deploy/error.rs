// ABOUTME: Error types for deployment operations using SNAFU with ErrorKind pattern.
// ABOUTME: Provides opaque errors with kind() accessor for stable API.

use chrono::{DateTime, Utc};
use snafu::Snafu;

use crate::runtime::{ContainerError, ImageError, NetworkError};

/// Categories of deployment errors.
///
/// Use `DeployError::kind()` to get this value for programmatic error handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DeployErrorKind {
    ImagePull,
    ContainerCreate,
    ContainerStart,
    ContainerStop,
    ContainerRemove,
    Network,
    NetworkCreation,
    HealthCheck,
    HealthCheckTimeout,
    Rollback,
    NoOldContainer,
    NoPreviousDeployment,
    Config,
    LockHeld,
    Lock,
}

/// Information about who holds a deployment lock.
#[derive(Debug, Clone)]
pub struct LockHolderInfo {
    pub holder: String,
    pub pid: u32,
    pub started_at: DateTime<Utc>,
}

/// Errors that can occur during deployment state transitions.
///
/// This is an opaque error type. Use `kind()` to determine the error category,
/// and specific accessor methods (like `lock_holder_info()`) to get details.
#[derive(Debug)]
pub struct DeployError(InnerDeployError);

impl std::fmt::Display for DeployError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for DeployError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

impl DeployError {
    /// Returns the kind of this error for programmatic handling.
    pub fn kind(&self) -> DeployErrorKind {
        match &self.0 {
            InnerDeployError::ImagePullFailed { .. }
            | InnerDeployError::ImagePullFailedMsg { .. } => DeployErrorKind::ImagePull,
            InnerDeployError::ContainerCreateFailed { .. }
            | InnerDeployError::ContainerCreateFailedMsg { .. } => DeployErrorKind::ContainerCreate,
            InnerDeployError::ContainerStartFailed { .. }
            | InnerDeployError::ContainerStartFailedMsg { .. } => DeployErrorKind::ContainerStart,
            InnerDeployError::ContainerStopFailed { .. }
            | InnerDeployError::ContainerStopFailedMsg { .. } => DeployErrorKind::ContainerStop,
            InnerDeployError::ContainerRemoveFailed { .. }
            | InnerDeployError::ContainerRemoveFailedMsg { .. } => DeployErrorKind::ContainerRemove,
            InnerDeployError::NetworkFailed { .. }
            | InnerDeployError::NetworkFailedMsg { .. } => DeployErrorKind::Network,
            InnerDeployError::NetworkCreationFailed { .. } => DeployErrorKind::NetworkCreation,
            InnerDeployError::HealthCheckFailed { .. } => DeployErrorKind::HealthCheck,
            InnerDeployError::HealthCheckTimeout { .. } => DeployErrorKind::HealthCheckTimeout,
            InnerDeployError::RollbackFailed { .. } => DeployErrorKind::Rollback,
            InnerDeployError::NoOldContainer => DeployErrorKind::NoOldContainer,
            InnerDeployError::NoPreviousDeployment { .. } => DeployErrorKind::NoPreviousDeployment,
            InnerDeployError::ConfigError { .. } => DeployErrorKind::Config,
            InnerDeployError::LockHeld { .. } => DeployErrorKind::LockHeld,
            InnerDeployError::LockError { .. } => DeployErrorKind::Lock,
        }
    }

    /// Returns lock holder information if this is a `LockHeld` error.
    pub fn lock_holder_info(&self) -> Option<LockHolderInfo> {
        match &self.0 {
            InnerDeployError::LockHeld {
                holder,
                pid,
                started_at,
            } => Some(LockHolderInfo {
                holder: holder.clone(),
                pid: *pid,
                started_at: *started_at,
            }),
            _ => None,
        }
    }

    /// Returns the service name if this is a `NoPreviousDeployment` error.
    pub fn service_name(&self) -> Option<&str> {
        match &self.0 {
            InnerDeployError::NoPreviousDeployment { service } => Some(service),
            _ => None,
        }
    }

    /// Returns the timeout duration if this is a `HealthCheckTimeout` error.
    pub fn timeout_seconds(&self) -> Option<u64> {
        match &self.0 {
            InnerDeployError::HealthCheckTimeout { seconds } => Some(*seconds),
            _ => None,
        }
    }
}

/// Internal error type with full context - not exposed in public API.
#[derive(Debug, Snafu)]
enum InnerDeployError {
    // Source-preserving variants (used via context extensions)
    #[snafu(display("failed to pull image: {source}"))]
    ImagePullFailed { source: ImageError },

    #[snafu(display("failed to create container: {source}"))]
    ContainerCreateFailed { source: ContainerError },

    #[snafu(display("failed to start container: {source}"))]
    ContainerStartFailed { source: ContainerError },

    #[snafu(display("failed to stop container: {source}"))]
    ContainerStopFailed { source: ContainerError },

    #[snafu(display("failed to remove container: {source}"))]
    ContainerRemoveFailed { source: ContainerError },

    #[snafu(display("network operation failed: {source}"))]
    NetworkFailed { source: NetworkError },

    // Message-based variants (used via factory methods)
    #[snafu(display("failed to pull image: {message}"))]
    ImagePullFailedMsg { message: String },

    #[snafu(display("failed to create container: {message}"))]
    ContainerCreateFailedMsg { message: String },

    #[snafu(display("failed to start container: {message}"))]
    ContainerStartFailedMsg { message: String },

    #[snafu(display("failed to stop container: {message}"))]
    ContainerStopFailedMsg { message: String },

    #[snafu(display("failed to remove container: {message}"))]
    ContainerRemoveFailedMsg { message: String },

    #[snafu(display("network operation failed: {message}"))]
    NetworkFailedMsg { message: String },

    #[snafu(display("failed to create network: {message}"))]
    NetworkCreationFailed { message: String },

    #[snafu(display("health check failed: {message}"))]
    HealthCheckFailed { message: String },

    #[snafu(display("health check timed out after {seconds} seconds"))]
    HealthCheckTimeout { seconds: u64 },

    #[snafu(display("rollback failed: {message}"))]
    RollbackFailed { message: String },

    #[snafu(display("no old container to rollback to (first deployment)"))]
    NoOldContainer,

    #[snafu(display("no previous deployment exists for service {service}"))]
    NoPreviousDeployment { service: String },

    #[snafu(display("configuration error: {message}"))]
    ConfigError { message: String },

    #[snafu(display("deployment locked by {holder} (pid {pid}) since {started_at}"))]
    LockHeld {
        holder: String,
        pid: u32,
        started_at: DateTime<Utc>,
    },

    #[snafu(display("lock error: {message}"))]
    LockError { message: String },
}

// Context selectors for converting errors at call sites with proper categorization
use snafu::ResultExt;

pub trait ImageErrorExt<T> {
    fn context_image_pull(self) -> Result<T, DeployError>;
}

impl<T> ImageErrorExt<T> for Result<T, ImageError> {
    fn context_image_pull(self) -> Result<T, DeployError> {
        self.context(ImagePullFailedSnafu).map_err(DeployError)
    }
}

pub trait ContainerErrorExt<T> {
    fn context_container_create(self) -> Result<T, DeployError>;
    fn context_container_start(self) -> Result<T, DeployError>;
    fn context_container_stop(self) -> Result<T, DeployError>;
    fn context_container_remove(self) -> Result<T, DeployError>;
}

impl<T> ContainerErrorExt<T> for Result<T, ContainerError> {
    fn context_container_create(self) -> Result<T, DeployError> {
        self.context(ContainerCreateFailedSnafu).map_err(DeployError)
    }

    fn context_container_start(self) -> Result<T, DeployError> {
        self.context(ContainerStartFailedSnafu).map_err(DeployError)
    }

    fn context_container_stop(self) -> Result<T, DeployError> {
        self.context(ContainerStopFailedSnafu).map_err(DeployError)
    }

    fn context_container_remove(self) -> Result<T, DeployError> {
        self.context(ContainerRemoveFailedSnafu).map_err(DeployError)
    }
}

pub trait NetworkErrorExt<T> {
    fn context_network(self) -> Result<T, DeployError>;
}

impl<T> NetworkErrorExt<T> for Result<T, NetworkError> {
    fn context_network(self) -> Result<T, DeployError> {
        self.context(NetworkFailedSnafu).map_err(DeployError)
    }
}

// Factory functions for errors without source
impl DeployError {
    pub fn image_pull_failed(message: impl Into<String>) -> Self {
        DeployError(InnerDeployError::ImagePullFailedMsg {
            message: message.into(),
        })
    }

    pub fn container_create_failed(message: impl Into<String>) -> Self {
        DeployError(InnerDeployError::ContainerCreateFailedMsg {
            message: message.into(),
        })
    }

    pub fn container_start_failed(message: impl Into<String>) -> Self {
        DeployError(InnerDeployError::ContainerStartFailedMsg {
            message: message.into(),
        })
    }

    pub fn container_stop_failed(message: impl Into<String>) -> Self {
        DeployError(InnerDeployError::ContainerStopFailedMsg {
            message: message.into(),
        })
    }

    pub fn container_remove_failed(message: impl Into<String>) -> Self {
        DeployError(InnerDeployError::ContainerRemoveFailedMsg {
            message: message.into(),
        })
    }

    pub fn network_failed(message: impl Into<String>) -> Self {
        DeployError(InnerDeployError::NetworkFailedMsg {
            message: message.into(),
        })
    }

    pub fn network_creation_failed(message: impl Into<String>) -> Self {
        DeployError(InnerDeployError::NetworkCreationFailed {
            message: message.into(),
        })
    }

    pub fn health_check_failed(message: impl Into<String>) -> Self {
        DeployError(InnerDeployError::HealthCheckFailed {
            message: message.into(),
        })
    }

    pub fn health_check_timeout(seconds: u64) -> Self {
        DeployError(InnerDeployError::HealthCheckTimeout { seconds })
    }

    pub fn rollback_failed(message: impl Into<String>) -> Self {
        DeployError(InnerDeployError::RollbackFailed {
            message: message.into(),
        })
    }

    pub fn no_old_container() -> Self {
        DeployError(InnerDeployError::NoOldContainer)
    }

    pub fn no_previous_deployment(service: impl Into<String>) -> Self {
        DeployError(InnerDeployError::NoPreviousDeployment {
            service: service.into(),
        })
    }

    pub fn config_error(message: impl Into<String>) -> Self {
        DeployError(InnerDeployError::ConfigError {
            message: message.into(),
        })
    }

    pub fn lock_held(holder: impl Into<String>, pid: u32, started_at: DateTime<Utc>) -> Self {
        DeployError(InnerDeployError::LockHeld {
            holder: holder.into(),
            pid,
            started_at,
        })
    }

    pub fn lock_error(message: impl Into<String>) -> Self {
        DeployError(InnerDeployError::LockError {
            message: message.into(),
        })
    }
}
