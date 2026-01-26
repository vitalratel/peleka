// ABOUTME: Configuration types and parsing for peleka.yml.
// ABOUTME: Handles YAML parsing, env var interpolation, and destination merging.

mod deserialize;
mod env_value;
mod healthcheck;
mod init;
mod restart_policy;
mod server;
mod stop;

pub use env_value::{EnvValue, resolve_env_map};
pub use healthcheck::HealthcheckConfig;
pub use init::init_config;
pub use restart_policy::RestartPolicy;
pub use server::ServerConfig;
pub use stop::StopConfig;

use crate::error::{Error, Result};
use crate::types::{ImageRef, ServiceName};
use nonempty::NonEmpty;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

pub const CONFIG_FILENAME: &str = "peleka.yml";
pub const CONFIG_FILENAME_ALT: &str = "peleka.yaml";
pub const CONFIG_FILENAME_DIR: &str = ".peleka/config.yml";

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(deserialize_with = "deserialize::deserialize_service_name")]
    pub service: ServiceName,

    #[serde(deserialize_with = "deserialize::deserialize_image_ref")]
    pub image: ImageRef,

    #[serde(deserialize_with = "deserialize::deserialize_servers")]
    pub servers: NonEmpty<ServerConfig>,

    #[serde(default)]
    pub ports: Vec<String>,

    #[serde(default)]
    pub volumes: Vec<String>,

    #[serde(default)]
    pub env: HashMap<String, EnvValue>,

    #[serde(default)]
    pub labels: HashMap<String, String>,

    #[serde(default)]
    pub command: Option<Vec<String>>,

    #[serde(default)]
    pub healthcheck: Option<HealthcheckConfig>,

    #[serde(default = "default_health_timeout", with = "humantime_serde")]
    pub health_timeout: Duration,

    #[serde(default, with = "humantime_serde::option")]
    pub image_pull_timeout: Option<Duration>,

    #[serde(default)]
    pub resources: Option<ResourcesConfig>,

    #[serde(default)]
    pub network: Option<NetworkConfig>,

    #[serde(default)]
    pub restart: RestartPolicy,

    #[serde(default)]
    pub stop: Option<StopConfig>,

    #[serde(default)]
    pub cleanup: Option<CleanupConfig>,

    #[serde(default)]
    pub logging: Option<LoggingConfig>,

    /// Deployment strategy. If not specified, auto-detected based on config.
    #[serde(default)]
    pub strategy: Option<StrategyConfig>,

    #[serde(default)]
    pub destinations: HashMap<String, Destination>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Destination {
    #[serde(default, deserialize_with = "deserialize::deserialize_servers_option")]
    pub servers: Option<NonEmpty<ServerConfig>>,

    #[serde(default)]
    pub env: HashMap<String, EnvValue>,

    #[serde(default)]
    pub labels: HashMap<String, String>,

    #[serde(default)]
    pub ports: Option<Vec<String>>,

    #[serde(default)]
    pub volumes: Option<Vec<String>>,

    #[serde(default)]
    pub healthcheck: Option<HealthcheckConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResourcesConfig {
    pub memory: Option<String>,
    pub cpus: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NetworkConfig {
    #[serde(default = "default_network_name")]
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
}

fn default_network_name() -> String {
    "peleka".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct CleanupConfig {
    #[serde(default = "default_grace_period", with = "humantime_serde")]
    pub grace_period: Duration,
}

fn default_grace_period() -> Duration {
    Duration::from_secs(30)
}

fn default_health_timeout() -> Duration {
    Duration::from_secs(120)
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingConfig {
    #[serde(default = "default_log_driver")]
    pub driver: String,
    #[serde(default)]
    pub options: HashMap<String, String>,
}

fn default_log_driver() -> String {
    "json-file".to_string()
}

/// Deployment strategy specified in configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StrategyConfig {
    /// Blue-green: zero-downtime deployment (default for stateless apps).
    BlueGreen,
    /// Recreate: stop old first, brief downtime (for stateful single-instance apps).
    Recreate,
}

impl Config {
    pub fn from_yaml(yaml: &str) -> Result<Self> {
        serde_yaml::from_str(yaml).map_err(Error::from)
    }

    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::from_yaml(&content)
    }

    pub fn discover(dir: &Path) -> Result<Self> {
        let candidates = [
            dir.join(CONFIG_FILENAME),
            dir.join(CONFIG_FILENAME_ALT),
            dir.join(CONFIG_FILENAME_DIR),
        ];

        for path in &candidates {
            if path.exists() {
                let config = Self::load(path)?;
                config.validate_placeholders()?;
                return Ok(config);
            }
        }

        Err(Error::ConfigNotFound(dir.to_path_buf()))
    }

    /// Validate that placeholder values from the template have been customized.
    fn validate_placeholders(&self) -> Result<()> {
        // Error on placeholder server host - this would definitely fail
        for server in self.servers.iter() {
            if server.host == "server.example.com" {
                return Err(Error::InvalidConfig(
                    "server host 'server.example.com' is a placeholder - please configure a real server".to_string()
                ));
            }
        }

        // Warn on placeholder service name
        if self.service.as_str() == "my-app" {
            tracing::warn!("service name 'my-app' appears to be a placeholder");
        }

        // Warn on placeholder image references
        let image_str = self.image.to_string();
        if image_str.contains("my-registry") || image_str.contains("my-app") {
            tracing::warn!(
                "image '{}' appears to contain placeholder values",
                image_str
            );
        }

        Ok(())
    }

    /// Apply destination overrides if specified, otherwise return self unchanged.
    pub fn with_optional_destination(self, dest: Option<&str>) -> Result<Config> {
        match dest {
            Some(name) => self.for_destination(name),
            None => Ok(self),
        }
    }

    pub fn for_destination(&self, name: &str) -> Result<Config> {
        let dest = self
            .destinations
            .get(name)
            .ok_or_else(|| Error::UnknownDestination(name.to_string()))?;

        let mut merged = self.clone();

        // Replace servers if destination specifies them
        if let Some(ref servers) = dest.servers {
            merged.servers = servers.clone();
        }

        // Deep merge env
        for (k, v) in &dest.env {
            merged.env.insert(k.clone(), v.clone());
        }

        // Deep merge labels
        for (k, v) in &dest.labels {
            merged.labels.insert(k.clone(), v.clone());
        }

        // Replace ports if specified
        if let Some(ref ports) = dest.ports {
            merged.ports = ports.clone();
        }

        // Replace volumes if specified
        if let Some(ref volumes) = dest.volumes {
            merged.volumes = volumes.clone();
        }

        // Override healthcheck if specified
        if dest.healthcheck.is_some() {
            merged.healthcheck = dest.healthcheck.clone();
        }

        Ok(merged)
    }

    /// Get the network name for this deployment.
    /// Uses configured network name or falls back to "peleka".
    pub fn network_name(&self) -> &str {
        self.network
            .as_ref()
            .map(|n| n.name.as_str())
            .unwrap_or("peleka")
    }

    /// Get the stop timeout for containers.
    /// Uses configured timeout or falls back to 30 seconds.
    pub fn stop_timeout(&self) -> Duration {
        self.stop
            .as_ref()
            .map(|s| s.timeout)
            .unwrap_or_else(|| Duration::from_secs(30))
    }

    /// Check if any port mappings bind to a host port.
    /// Host port bindings (e.g. "80:8080") prevent blue-green deployment
    /// because only one container can bind to a host port at a time.
    pub fn has_host_port_bindings(&self) -> bool {
        self.ports.iter().any(|p| {
            // Parse port spec: "8080" (container only) vs "80:8080" (host:container)
            let port_part = p.split('/').next().unwrap_or(p);
            port_part.contains(':')
        })
    }

    pub fn template() -> Self {
        Config {
            service: ServiceName::new("my-app").unwrap(),
            image: ImageRef::parse("my-registry/my-app:latest").unwrap(),
            servers: NonEmpty::new(ServerConfig {
                host: "server.example.com".to_string(),
                port: 22,
                user: Some("deploy".to_string()),
                runtime: None,
                socket: None,
                trust_first_connection: false,
            }),
            ports: vec![],
            volumes: vec![],
            env: HashMap::new(),
            labels: HashMap::new(),
            command: None,
            healthcheck: None,
            health_timeout: default_health_timeout(),
            image_pull_timeout: None,
            resources: None,
            network: None,
            restart: RestartPolicy::default(),
            stop: None,
            cleanup: None,
            logging: None,
            strategy: None,
            destinations: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_host_port_bindings_detects_host_ports() {
        let mut config = Config::template();

        // No ports - no host bindings
        config.ports = vec![];
        assert!(!config.has_host_port_bindings());

        // Container-only port - no host binding
        config.ports = vec!["8080".to_string()];
        assert!(!config.has_host_port_bindings());

        // Multiple container-only ports
        config.ports = vec!["8080".to_string(), "9090".to_string()];
        assert!(!config.has_host_port_bindings());

        // Host:container binding
        config.ports = vec!["80:8080".to_string()];
        assert!(config.has_host_port_bindings());

        // Mixed - one host binding is enough
        config.ports = vec!["8080".to_string(), "80:80".to_string()];
        assert!(config.has_host_port_bindings());
    }

    #[test]
    fn has_host_port_bindings_handles_protocol_suffix() {
        let mut config = Config::template();

        // Container port with protocol
        config.ports = vec!["8080/tcp".to_string()];
        assert!(!config.has_host_port_bindings());

        // Host binding with protocol
        config.ports = vec!["80:8080/tcp".to_string()];
        assert!(config.has_host_port_bindings());

        // UDP port
        config.ports = vec!["53:53/udp".to_string()];
        assert!(config.has_host_port_bindings());
    }
}
