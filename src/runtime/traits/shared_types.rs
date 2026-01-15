// ABOUTME: Shared types used across runtime trait definitions.
// ABOUTME: ContainerConfig, ContainerInfo, NetworkConfig, RegistryAuth, etc.

use crate::types::{ContainerId, ImageRef, NetworkAlias};
use std::collections::HashMap;
use std::time::Duration;

/// Configuration for creating a container.
#[derive(Debug, Clone)]
pub struct ContainerConfig {
    /// Name for the container.
    pub name: String,
    /// Image to run.
    pub image: ImageRef,
    /// Environment variables.
    pub env: HashMap<String, String>,
    /// Labels to apply.
    pub labels: HashMap<String, String>,
    /// Port mappings (host:container).
    pub ports: Vec<PortMapping>,
    /// Volume mounts.
    pub volumes: Vec<VolumeMount>,
    /// Command to run (overrides image CMD).
    pub command: Option<Vec<String>>,
    /// Entrypoint (overrides image ENTRYPOINT).
    pub entrypoint: Option<Vec<String>>,
    /// Working directory.
    pub working_dir: Option<String>,
    /// User to run as.
    pub user: Option<String>,
    /// Restart policy.
    pub restart_policy: RestartPolicyConfig,
    /// Resource limits.
    pub resources: Option<ResourceLimits>,
    /// Healthcheck configuration.
    pub healthcheck: Option<HealthcheckConfig>,
    /// Stop timeout.
    pub stop_timeout: Option<Duration>,
    /// Network to connect to.
    pub network: Option<String>,
    /// Network aliases.
    pub network_aliases: Vec<NetworkAlias>,
}

/// Port mapping configuration.
#[derive(Debug, Clone)]
pub struct PortMapping {
    /// Host port (or range).
    pub host_port: Option<u16>,
    /// Container port.
    pub container_port: u16,
    /// Protocol (tcp/udp).
    pub protocol: Protocol,
    /// Host IP to bind to.
    pub host_ip: Option<String>,
}

/// Network protocol.
#[derive(Debug, Clone, Copy, Default)]
pub enum Protocol {
    #[default]
    Tcp,
    Udp,
}

/// Volume mount configuration.
#[derive(Debug, Clone)]
pub struct VolumeMount {
    /// Source path or volume name.
    pub source: String,
    /// Target path in container.
    pub target: String,
    /// Read-only flag.
    pub read_only: bool,
}

/// Restart policy configuration.
#[derive(Debug, Clone, Default)]
pub enum RestartPolicyConfig {
    /// Never restart.
    No,
    /// Always restart.
    Always,
    /// Restart unless explicitly stopped.
    #[default]
    UnlessStopped,
    /// Restart on failure with optional max retries.
    OnFailure { max_retries: Option<u32> },
}

/// Resource limits for a container.
#[derive(Debug, Clone, Default)]
pub struct ResourceLimits {
    /// Memory limit in bytes.
    pub memory: Option<u64>,
    /// CPU quota (1.0 = 1 CPU).
    pub cpus: Option<f64>,
}

/// Healthcheck configuration for a container.
#[derive(Debug, Clone)]
pub struct HealthcheckConfig {
    /// Command to run for health check.
    pub test: Vec<String>,
    /// Interval between checks.
    pub interval: Duration,
    /// Timeout for each check.
    pub timeout: Duration,
    /// Retries before unhealthy.
    pub retries: u32,
    /// Start period before health checks begin.
    pub start_period: Duration,
}

/// Information about a running container.
#[derive(Debug, Clone)]
pub struct ContainerInfo {
    /// Container ID.
    pub id: ContainerId,
    /// Container name.
    pub name: String,
    /// Image used.
    pub image: String,
    /// Current state.
    pub state: ContainerState,
    /// Health status (if healthcheck configured).
    pub health: Option<HealthState>,
    /// Creation timestamp.
    pub created: String,
    /// Labels.
    pub labels: HashMap<String, String>,
    /// Network settings.
    pub network_settings: NetworkSettings,
}

/// Container state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerState {
    Created,
    Running,
    Paused,
    Restarting,
    Removing,
    Exited,
    Dead,
}

/// Health state of a container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthState {
    Starting,
    Healthy,
    Unhealthy,
    None,
}

/// Network settings for a container.
#[derive(Debug, Clone, Default)]
pub struct NetworkSettings {
    /// IP addresses by network name.
    pub networks: HashMap<String, NetworkInfo>,
}

/// Network information for a container.
#[derive(Debug, Clone)]
pub struct NetworkInfo {
    /// Network ID.
    pub network_id: String,
    /// IP address in this network.
    pub ip_address: String,
    /// Gateway.
    pub gateway: String,
    /// Aliases in this network.
    pub aliases: Vec<String>,
}

/// Configuration for creating a network.
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    /// Network name.
    pub name: String,
    /// Network driver (bridge, host, overlay, etc.).
    pub driver: Option<String>,
    /// Labels.
    pub labels: HashMap<String, String>,
}

/// Registry authentication credentials.
#[derive(Debug, Clone)]
pub struct RegistryAuth {
    /// Username.
    pub username: String,
    /// Password or token.
    pub password: String,
    /// Registry server (e.g., "ghcr.io").
    pub server: Option<String>,
}

/// Runtime metadata.
#[derive(Debug, Clone)]
pub struct RuntimeMetadata {
    /// Runtime name (e.g., "docker", "podman").
    pub name: String,
    /// Runtime version.
    pub version: String,
    /// API version.
    pub api_version: String,
    /// Operating system.
    pub os: String,
    /// Architecture.
    pub arch: String,
}

/// Exec configuration for running commands in containers.
#[derive(Debug, Clone)]
pub struct ExecConfig {
    /// Command and arguments to run.
    pub cmd: Vec<String>,
    /// Environment variables.
    pub env: Vec<String>,
    /// Working directory.
    pub working_dir: Option<String>,
    /// User to run as.
    pub user: Option<String>,
    /// Attach stdin.
    pub attach_stdin: bool,
    /// Attach stdout.
    pub attach_stdout: bool,
    /// Attach stderr.
    pub attach_stderr: bool,
    /// Allocate a TTY.
    pub tty: bool,
    /// Run in privileged mode.
    pub privileged: bool,
}

impl Default for ExecConfig {
    fn default() -> Self {
        Self {
            cmd: Vec::new(),
            env: Vec::new(),
            working_dir: None,
            user: None,
            attach_stdin: false,
            attach_stdout: true,
            attach_stderr: true,
            tty: false,
            privileged: false,
        }
    }
}

/// Result of an exec operation.
#[derive(Debug, Clone)]
pub struct ExecResult {
    /// Exit code.
    pub exit_code: i64,
    /// Standard output.
    pub stdout: Vec<u8>,
    /// Standard error.
    pub stderr: Vec<u8>,
}

/// Exec instance information.
#[derive(Debug, Clone)]
pub struct ExecInfo {
    /// Exec ID.
    pub id: String,
    /// Whether the exec is running.
    pub running: bool,
    /// Exit code (if finished).
    pub exit_code: Option<i64>,
    /// Container ID.
    pub container_id: ContainerId,
}
