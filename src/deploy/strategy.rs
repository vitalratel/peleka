// ABOUTME: Deployment strategy selection based on configuration.
// ABOUTME: Determines whether to use blue-green or recreate strategy.

use crate::config::Config;

/// Strategy for deploying container updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeployStrategy {
    /// Blue-green deployment: start new container, health check, cutover traffic, stop old.
    /// Provides zero-downtime deployments when possible.
    BlueGreen,

    /// Recreate deployment: stop old container first, then start new.
    /// Required when host port bindings prevent running two containers simultaneously.
    Recreate,
}

impl DeployStrategy {
    /// Determine the appropriate deployment strategy based on configuration.
    /// Returns the strategy and a reason if it differs from the default.
    pub fn for_config(config: &Config) -> (Self, Option<&'static str>) {
        if config.has_host_port_bindings() {
            (
                DeployStrategy::Recreate,
                Some("host port bindings prevent blue-green deployment"),
            )
        } else {
            (DeployStrategy::BlueGreen, None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blue_green_for_container_only_ports() {
        let mut config = Config::template();
        config.ports = vec!["8080".to_string()];

        let (strategy, reason) = DeployStrategy::for_config(&config);
        assert_eq!(strategy, DeployStrategy::BlueGreen);
        assert!(reason.is_none());
    }

    #[test]
    fn recreate_for_host_port_bindings() {
        let mut config = Config::template();
        config.ports = vec!["80:8080".to_string()];

        let (strategy, reason) = DeployStrategy::for_config(&config);
        assert_eq!(strategy, DeployStrategy::Recreate);
        assert!(reason.is_some());
        assert!(reason.unwrap().contains("host port"));
    }
}
