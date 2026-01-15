// ABOUTME: Container runtime detection and trait abstractions.
// ABOUTME: Auto-detects available runtime, defines composable capability traits.

mod bollard;
mod detection;
pub mod traits;
mod types;

pub use bollard::{BollardRuntime, connect_via_session};
pub use detection::{DetectionError, detect_local, detect_runtime};
pub use types::{RuntimeConfig, RuntimeInfo, RuntimeType};

// Re-export traits at runtime level for convenience
pub use traits::{
    ContainerConfig, ContainerError, ContainerFilters, ContainerInfo, ContainerOps, ContainerState,
    ContainerSummary, ExecConfig, ExecError, ExecOps, ExecResult, FullRuntime, HealthState,
    HealthcheckConfig, ImageError, ImageOps, LogError, LogLine, LogOps, LogOptions, LogStream,
    NetworkConfig, NetworkError, NetworkOps, PortMapping, Protocol, RegistryAuth, ResourceLimits,
    RestartPolicyConfig, RuntimeInfo as RuntimeInfoTrait, RuntimeInfoError, RuntimeMetadata,
    VolumeMount,
};
