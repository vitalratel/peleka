// ABOUTME: State transition methods for deployment orchestration.
// ABOUTME: Each method consumes self and returns the next state on success.

use std::collections::HashMap;
use std::marker::PhantomData;
use std::time::Duration;

use crate::config::Config;
use crate::runtime::{
    ContainerConfig, ContainerOps, ImageOps, NetworkConfig as RuntimeNetworkConfig, NetworkOps,
    RegistryAuth, RestartPolicyConfig, VolumeMount,
};
use crate::types::{ContainerId, NetworkAlias, NetworkId};

use super::Deployment;
use super::error::DeployError;
use super::state::{Completed, ContainerStarted, CutOver, HealthChecked, ImagePulled, Initialized};

/// Result type for transitions that may need rollback on failure.
pub type TransitionResult<T, S> = Result<Deployment<T>, (Deployment<S>, DeployError)>;

// =============================================================================
// Internal Helpers
// =============================================================================

impl<S> Deployment<S> {
    /// Internal helper to transition to a new state.
    fn transition<T>(self) -> Deployment<T> {
        Deployment {
            config: self.config,
            new_container: self.new_container,
            old_container: self.old_container,
            _state: PhantomData,
        }
    }

    /// Internal helper to transition with a new container ID.
    fn transition_with_new_container<T>(self, container_id: ContainerId) -> Deployment<T> {
        Deployment {
            config: self.config,
            new_container: Some(container_id),
            old_container: self.old_container,
            _state: PhantomData,
        }
    }

    /// Generate container name for this deployment.
    fn container_name(&self) -> String {
        // Use blue/green naming for zero-downtime deployment
        // The actual state (active/previous) is tracked via labels
        let suffix = if self.old_container.is_some() {
            "green"
        } else {
            "blue"
        };
        format!("{}-{}", self.config.service, suffix)
    }

    /// Get the network name to use.
    fn network_name(&self) -> String {
        self.config.network_name()
    }

    /// Get the network alias for the service.
    fn service_alias(&self) -> NetworkAlias {
        // The service name is the network alias for discovery
        NetworkAlias::new(self.config.service.as_str()).expect("service name should be valid alias")
    }

    /// Internal helper for rollback - stops and removes new container.
    async fn rollback_new_container<R: ContainerOps>(
        self,
        runtime: &R,
    ) -> Result<Deployment<Initialized>, DeployError> {
        if let Some(container_id) = &self.new_container {
            let _ = runtime
                .stop_container(container_id, Duration::from_secs(10))
                .await;
            runtime.remove_container(container_id, true).await?;
        }

        Ok(Deployment {
            config: self.config,
            new_container: None,
            old_container: self.old_container,
            _state: PhantomData,
        })
    }
}

// =============================================================================
// Initialized -> ImagePulled
// =============================================================================

impl Deployment<Initialized> {
    /// Ensure the deployment network exists, creating it if necessary.
    ///
    /// # Returns
    ///
    /// Returns the `NetworkId` for use in cutover.
    ///
    /// # Errors
    ///
    /// Returns `DeployError::NetworkCreationFailed` if the network cannot be created.
    pub async fn ensure_network<R: NetworkOps>(
        &self,
        runtime: &R,
    ) -> Result<NetworkId, DeployError> {
        use crate::runtime::NetworkError;

        let network_name = self.network_name();

        // Check if network already exists
        if runtime.network_exists(&network_name).await.unwrap_or(false) {
            // Network exists, return name as ID (Docker/Podman accept both)
            return Ok(NetworkId::new(network_name));
        }

        // Try to create the network
        let config = RuntimeNetworkConfig {
            name: network_name.clone(),
            driver: Some("bridge".to_string()),
            labels: std::collections::HashMap::new(),
        };

        match runtime.create_network(&config).await {
            Ok(_) => {
                // Return name as ID for consistency
                Ok(NetworkId::new(network_name))
            }
            Err(NetworkError::AlreadyExists(_)) => {
                // Race condition: network was created between check and create
                Ok(NetworkId::new(network_name))
            }
            Err(e) => Err(DeployError::NetworkCreationFailed(e.to_string())),
        }
    }

    /// Pull the container image from the registry.
    ///
    /// # Errors
    ///
    /// Returns `DeployError::ImagePullFailed` if the image cannot be pulled.
    #[must_use = "deployment state must be used"]
    pub async fn pull_image<R: ImageOps>(
        self,
        runtime: &R,
        auth: Option<&RegistryAuth>,
    ) -> Result<Deployment<ImagePulled>, DeployError> {
        runtime.pull_image(&self.config.image, auth).await?;
        Ok(self.transition())
    }
}

// =============================================================================
// ImagePulled -> ContainerStarted
// =============================================================================

impl Deployment<ImagePulled> {
    /// Create and start a new container.
    ///
    /// # Errors
    ///
    /// Returns error if container creation or start fails.
    #[must_use = "deployment state must be used"]
    pub async fn start_container<R: ContainerOps + NetworkOps>(
        self,
        runtime: &R,
    ) -> Result<Deployment<ContainerStarted>, DeployError> {
        let config = self.build_container_config();
        let container_id = runtime.create_container(&config).await?;

        // Start the container
        if let Err(e) = runtime.start_container(&container_id).await {
            // Clean up the created container on start failure
            let _ = runtime.remove_container(&container_id, true).await;
            return Err(DeployError::ContainerStartFailed(e.to_string()));
        }

        Ok(self.transition_with_new_container(container_id))
    }

    /// Build container configuration from deployment config.
    fn build_container_config(&self) -> ContainerConfig {
        let mut labels = self.config.labels.clone();
        labels.insert(
            "peleka.service".to_string(),
            self.config.service.to_string(),
        );
        labels.insert("peleka.managed".to_string(), "true".to_string());
        // Track deployment slot (blue/green) for zero-downtime deployment
        let slot = if self.old_container.is_some() {
            "green"
        } else {
            "blue"
        };
        labels.insert("peleka.slot".to_string(), slot.to_string());

        // Parse volumes from config
        let volumes: Vec<VolumeMount> = self
            .config
            .volumes
            .iter()
            .filter_map(|v| parse_volume_mount(v))
            .collect();

        // Parse port mappings
        let ports = self
            .config
            .ports
            .iter()
            .filter_map(|p| parse_port_mapping(p))
            .collect();

        // Convert env values to resolved strings
        let env: HashMap<String, String> = self
            .config
            .env
            .iter()
            .map(|(k, v)| (k.clone(), v.resolve().unwrap_or_default()))
            .collect();

        // Convert restart policy
        let restart_policy = match &self.config.restart {
            crate::config::RestartPolicy::No => RestartPolicyConfig::No,
            crate::config::RestartPolicy::Always => RestartPolicyConfig::Always,
            crate::config::RestartPolicy::UnlessStopped => RestartPolicyConfig::UnlessStopped,
            crate::config::RestartPolicy::OnFailure { max_retries } => {
                RestartPolicyConfig::OnFailure {
                    max_retries: *max_retries,
                }
            }
        };

        // Convert healthcheck config - use user-provided command directly
        let healthcheck = self.config.healthcheck.as_ref().map(|hc| {
            let test = vec!["CMD-SHELL".to_string(), hc.cmd.clone()];
            crate::runtime::HealthcheckConfig {
                test,
                interval: hc.interval,
                timeout: hc.timeout,
                retries: hc.retries,
                start_period: hc.start_period,
            }
        });

        // Network aliases - include service name for discovery
        let network_aliases = vec![self.service_alias()];

        ContainerConfig {
            name: self.container_name(),
            image: self.config.image.clone(),
            env,
            labels,
            ports,
            volumes,
            command: self.config.command.clone(),
            entrypoint: None,
            working_dir: None,
            user: None,
            restart_policy,
            resources: self
                .config
                .resources
                .as_ref()
                .map(|r| crate::runtime::ResourceLimits {
                    memory: r.memory.as_ref().and_then(|m| parse_memory_string(m)),
                    cpus: r.cpus.as_ref().and_then(|c| c.parse().ok()),
                }),
            healthcheck,
            stop_timeout: self.config.stop.as_ref().map(|s| s.timeout),
            network: self.config.network.as_ref().map(|_| self.network_name()),
            network_aliases,
        }
    }
}

// =============================================================================
// ContainerStarted -> HealthChecked
// =============================================================================

impl Deployment<ContainerStarted> {
    /// Wait for health checks to pass.
    ///
    /// This method actively triggers health checks rather than waiting for the
    /// container runtime to run them automatically. This is necessary because
    /// some runtimes (e.g., rootless Podman without systemd) don't automatically
    /// execute health check commands.
    ///
    /// # Errors
    ///
    /// Returns `(self, error)` on failure to allow rollback.
    #[must_use = "deployment state must be used"]
    pub async fn health_check<R: ContainerOps>(
        self,
        runtime: &R,
        timeout: Duration,
    ) -> TransitionResult<HealthChecked, ContainerStarted> {
        let container_id = self
            .new_container
            .as_ref()
            .expect("new container must exist");

        // If no healthcheck is configured, skip the check
        let healthcheck = match &self.config.healthcheck {
            Some(hc) => hc,
            None => return Ok(self.transition()),
        };

        // Build the healthcheck command: ["sh", "-c", cmd]
        let healthcheck_cmd = vec!["sh".to_string(), "-c".to_string(), healthcheck.cmd.clone()];

        let start = std::time::Instant::now();
        let poll_interval = healthcheck.interval;
        let mut retries_remaining = healthcheck.retries;

        // Wait for start period before beginning health checks
        if healthcheck.start_period > Duration::ZERO {
            tokio::time::sleep(healthcheck.start_period).await;
        }

        while start.elapsed() < timeout {
            // Manually trigger the healthcheck with a timeout
            let healthcheck_result = tokio::time::timeout(
                healthcheck.timeout,
                runtime.run_healthcheck(container_id, &healthcheck_cmd),
            )
            .await;

            match healthcheck_result {
                Ok(Ok(true)) => {
                    // Healthy
                    return Ok(self.transition());
                }
                Ok(Ok(false)) => {
                    // Unhealthy - decrement retries
                    if retries_remaining == 0 {
                        return Err((
                            self,
                            DeployError::HealthCheckFailed(
                                "container reported unhealthy after retries exhausted".to_string(),
                            ),
                        ));
                    }
                    retries_remaining -= 1;
                }
                Ok(Err(e)) => {
                    // Error running healthcheck - treat as unhealthy
                    if retries_remaining == 0 {
                        return Err((
                            self,
                            DeployError::HealthCheckFailed(format!(
                                "healthcheck exec failed: {}",
                                e
                            )),
                        ));
                    }
                    retries_remaining -= 1;
                }
                Err(_elapsed) => {
                    // Timeout - treat as unhealthy
                    if retries_remaining == 0 {
                        return Err((
                            self,
                            DeployError::HealthCheckFailed(
                                "healthcheck timeout after retries exhausted".to_string(),
                            ),
                        ));
                    }
                    retries_remaining -= 1;
                }
            }

            tokio::time::sleep(poll_interval).await;
        }

        Err((self, DeployError::HealthCheckTimeout(timeout.as_secs())))
    }

    /// Rollback: stop and remove the new container.
    ///
    /// # Errors
    ///
    /// Returns error if cleanup fails.
    #[must_use = "deployment state must be used"]
    pub async fn rollback<R: ContainerOps>(
        self,
        runtime: &R,
    ) -> Result<Deployment<Initialized>, DeployError> {
        self.rollback_new_container(runtime).await
    }
}

// =============================================================================
// HealthChecked -> CutOver
// =============================================================================

impl Deployment<HealthChecked> {
    /// Switch traffic to the new container (update network alias).
    ///
    /// # Errors
    ///
    /// Returns error if network operations fail.
    #[must_use = "deployment state must be used"]
    pub async fn cutover<R: ContainerOps + NetworkOps>(
        self,
        runtime: &R,
        network_id: &NetworkId,
    ) -> Result<Deployment<CutOver>, DeployError> {
        let new_container_id = self
            .new_container
            .as_ref()
            .expect("new container must exist");
        let alias = self.service_alias();

        // If there's an old container, disconnect it from the network first
        if let Some(old_container_id) = &self.old_container {
            // Best effort: disconnect old container (may already be disconnected)
            let _ = runtime
                .disconnect_from_network(old_container_id, network_id)
                .await;
        }

        // Connect new container to network with the service alias.
        // The container may already be connected (created with network set),
        // so ignore "already connected" errors.
        if let Err(e) = runtime
            .connect_to_network(new_container_id, network_id, &[alias])
            .await
        {
            let err_str = e.to_string().to_lowercase();
            if !err_str.contains("already connected") {
                return Err(DeployError::NetworkFailed(e.to_string()));
            }
        }

        Ok(self.transition())
    }

    /// Rollback: stop and remove the new container.
    ///
    /// # Errors
    ///
    /// Returns error if cleanup fails.
    #[must_use = "deployment state must be used"]
    pub async fn rollback<R: ContainerOps>(
        self,
        runtime: &R,
    ) -> Result<Deployment<Initialized>, DeployError> {
        self.rollback_new_container(runtime).await
    }
}

// =============================================================================
// CutOver -> Completed
// =============================================================================

impl Deployment<CutOver> {
    /// Clean up the old container (if any).
    ///
    /// Waits for the configured grace period to allow in-flight requests
    /// to complete before stopping the old container. The old container is
    /// kept (stopped) to enable manual rollback.
    ///
    /// # Errors
    ///
    /// Returns error if cleanup fails.
    #[must_use = "deployment state must be used"]
    pub async fn cleanup<R: ContainerOps>(
        self,
        runtime: &R,
    ) -> Result<Deployment<Completed>, DeployError> {
        if let Some(old_container_id) = &self.old_container {
            // Wait for grace period to allow in-flight requests to complete
            let grace_period = self
                .config
                .cleanup
                .as_ref()
                .map(|c| c.grace_period)
                .unwrap_or_else(|| Duration::from_secs(30));

            if !grace_period.is_zero() {
                tokio::time::sleep(grace_period).await;
            }

            // Stop with configured timeout or default
            let stop_timeout = self
                .config
                .stop
                .as_ref()
                .map(|s| s.timeout)
                .unwrap_or_else(|| Duration::from_secs(30));

            // Stop the old container but keep it for potential rollback
            runtime
                .stop_container(old_container_id, stop_timeout)
                .await?;
            // Note: We intentionally don't remove the old container to enable
            // manual rollback via `peleka rollback`. The stopped container
            // becomes the "previous" version that can be restored.
        }

        Ok(self.transition())
    }
}

// =============================================================================
// Completed - Terminal State
// =============================================================================

impl Deployment<Completed> {
    /// Get the final container ID of the new deployment.
    pub fn deployed_container(&self) -> &ContainerId {
        self.new_container
            .as_ref()
            .expect("completed deployment must have new container")
    }

    /// Consume the deployment and return the config.
    pub fn finish(self) -> Config {
        self.config
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Parse a volume mount string like "source:target" or "source:target:ro".
fn parse_volume_mount(spec: &str) -> Option<VolumeMount> {
    let parts: Vec<&str> = spec.split(':').collect();
    match parts.len() {
        2 => Some(VolumeMount {
            source: parts[0].to_string(),
            target: parts[1].to_string(),
            read_only: false,
        }),
        3 => Some(VolumeMount {
            source: parts[0].to_string(),
            target: parts[1].to_string(),
            read_only: parts[2] == "ro",
        }),
        _ => None,
    }
}

/// Parse a port mapping string like "8080:80" or "8080:80/tcp".
fn parse_port_mapping(spec: &str) -> Option<crate::runtime::PortMapping> {
    let (port_part, protocol) = if spec.contains('/') {
        let parts: Vec<&str> = spec.split('/').collect();
        let proto = match parts.get(1) {
            Some(&"udp") => crate::runtime::Protocol::Udp,
            _ => crate::runtime::Protocol::Tcp,
        };
        (parts[0], proto)
    } else {
        (spec, crate::runtime::Protocol::Tcp)
    };

    let parts: Vec<&str> = port_part.split(':').collect();
    match parts.len() {
        1 => {
            // Container port only
            let container_port = parts[0].parse().ok()?;
            Some(crate::runtime::PortMapping {
                host_port: None,
                container_port,
                protocol,
                host_ip: None,
            })
        }
        2 => {
            // host:container
            let host_port = parts[0].parse().ok()?;
            let container_port = parts[1].parse().ok()?;
            Some(crate::runtime::PortMapping {
                host_port: Some(host_port),
                container_port,
                protocol,
                host_ip: None,
            })
        }
        _ => None,
    }
}

/// Parse a memory string like "512m" or "1g" into bytes.
fn parse_memory_string(spec: &str) -> Option<u64> {
    let spec = spec.to_lowercase();
    let (num_str, multiplier) = if spec.ends_with("g") {
        (&spec[..spec.len() - 1], 1024 * 1024 * 1024)
    } else if spec.ends_with("m") {
        (&spec[..spec.len() - 1], 1024 * 1024)
    } else if spec.ends_with("k") {
        (&spec[..spec.len() - 1], 1024)
    } else {
        (spec.as_str(), 1)
    };

    num_str.parse::<u64>().ok().map(|n| n * multiplier)
}
