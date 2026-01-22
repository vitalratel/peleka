// ABOUTME: Runtime error types with SNAFU pattern.
// ABOUTME: Unifies detection and connection errors for programmatic handling.

use snafu::Snafu;

use super::detection::DetectionError;
use super::traits::RuntimeInfoError;

/// Unified runtime error for detection and connection failures.
#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum RuntimeError {
    #[snafu(display("runtime detection failed: {source}"))]
    Detection { source: DetectionError },

    #[snafu(display("runtime connection failed: {source}"))]
    Connection { source: RuntimeInfoError },
}

/// Error kind for programmatic handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeErrorKind {
    /// No container runtime found on the system.
    NoRuntimeFound,
    /// SSH error during runtime detection.
    SshError,
    /// Failed to connect to runtime socket.
    ConnectionFailed,
    /// Runtime operation error.
    RuntimeOperation,
}

impl RuntimeError {
    /// Returns the error kind for programmatic handling.
    pub fn kind(&self) -> RuntimeErrorKind {
        match self {
            RuntimeError::Detection { source } => match source {
                DetectionError::NoRuntimeFound => RuntimeErrorKind::NoRuntimeFound,
                DetectionError::Ssh(_) => RuntimeErrorKind::SshError,
            },
            RuntimeError::Connection { source } => match source {
                RuntimeInfoError::ConnectionFailed(_) => RuntimeErrorKind::ConnectionFailed,
                RuntimeInfoError::Runtime(_) => RuntimeErrorKind::RuntimeOperation,
            },
        }
    }

    /// Returns connection error details if this is a connection failure.
    pub fn connection_details(&self) -> Option<&str> {
        match self {
            RuntimeError::Connection {
                source: RuntimeInfoError::ConnectionFailed(msg),
            } => Some(msg),
            _ => None,
        }
    }
}

impl From<DetectionError> for RuntimeError {
    fn from(source: DetectionError) -> Self {
        RuntimeError::Detection { source }
    }
}

impl From<RuntimeInfoError> for RuntimeError {
    fn from(source: RuntimeInfoError) -> Self {
        RuntimeError::Connection { source }
    }
}
