// ABOUTME: Runtime type definitions for Docker and Podman.
// ABOUTME: Includes RuntimeType enum and RuntimeInfo struct.

use serde::{Deserialize, Serialize};

/// The container runtime type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeType {
    Docker,
    Podman,
}

impl std::fmt::Display for RuntimeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeType::Docker => write!(f, "docker"),
            RuntimeType::Podman => write!(f, "podman"),
        }
    }
}

/// Detected runtime information.
#[derive(Debug, Clone)]
pub struct RuntimeInfo {
    /// The type of runtime detected.
    pub runtime_type: RuntimeType,
    /// Path to the runtime socket.
    pub socket_path: String,
}

/// Configuration for explicit runtime override.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RuntimeConfig {
    /// Explicit runtime type (overrides auto-detection).
    pub runtime: Option<RuntimeType>,
    /// Explicit socket path (overrides default).
    pub socket: Option<String>,
}
