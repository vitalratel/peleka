// ABOUTME: State transition methods for deployment orchestration.
// ABOUTME: Each method consumes self and returns the next state on success.

use std::time::Duration;

use crate::config::{Config, PullPolicy, resolve_env_map};
use crate::runtime::{
    ContainerConfig, ContainerOps, ImageOps, NetworkConfig as RuntimeNetworkConfig, NetworkOps,
    RegistryAuth, RestartPolicyConfig, VolumeMount,
};
use crate::types::{ContainerId, NetworkAlias, NetworkId};

use super::Deployment;
use super::error::{ContainerErrorExt, DeployError, ImageErrorExt};
use super::state::{Completed, ContainerStarted, CutOver, HealthChecked, ImagePulled, Initialized};

/// Result type for transitions that may need rollback on failure.
pub type TransitionResult<T, S> = Result<Deployment<T>, (Deployment<S>, DeployError)>;

/// Result of a single health check poll.
enum HealthPollResult {
    /// Container is healthy.
    Healthy,
    /// Container reported unhealthy (check returned false).
    Unhealthy,
    /// Health check command failed to execute.
    ExecFailed(String),
    /// Health check timed out.
    Timeout,
}

/// Run a single health check poll with timeout.
async fn poll_health_once<R: ContainerOps>(
    runtime: &R,
    container_id: &ContainerId,
    cmd: &[String],
    timeout: Duration,
) -> HealthPollResult {
    match tokio::time::timeout(timeout, runtime.run_healthcheck(container_id, cmd)).await {
        Ok(Ok(true)) => HealthPollResult::Healthy,
        Ok(Ok(false)) => HealthPollResult::Unhealthy,
        Ok(Err(e)) => HealthPollResult::ExecFailed(e.to_string()),
        Err(_) => HealthPollResult::Timeout,
    }
}

// =============================================================================
// Internal Helpers
// =============================================================================

impl<S> Deployment<S> {
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
    fn network_name(&self) -> &str {
        self.config.network_name()
    }

    /// Get the network alias for the service.
    fn service_alias(&self) -> NetworkAlias {
        self.config.service.as_alias()
    }
}

/// Internal helper for rollback - stops and removes a container.
async fn rollback_container<R: ContainerOps>(
    runtime: &R,
    container_id: &ContainerId,
    stop_timeout: Duration,
) -> Result<(), DeployError> {
    if let Err(e) = runtime.stop_container(container_id, stop_timeout).await {
        tracing::warn!("Failed to stop container during rollback: {}", e);
    }
    runtime
        .remove_container(container_id, true)
        .await
        .context_container_remove()?;
    Ok(())
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
        if runtime.network_exists(network_name).await.unwrap_or(false) {
            // Network exists, return name as ID (Docker/Podman accept both)
            return Ok(NetworkId::new(network_name.to_string()));
        }

        // Try to create the network
        let config = RuntimeNetworkConfig {
            name: network_name.to_string(),
            driver: Some("bridge".to_string()),
            labels: std::collections::HashMap::new(),
        };

        match runtime.create_network(&config).await {
            Ok(_) => {
                // Return name as ID for consistency
                Ok(NetworkId::new(network_name.to_string()))
            }
            Err(NetworkError::AlreadyExists(_)) => {
                // Race condition: network was created between check and create
                Ok(NetworkId::new(network_name.to_string()))
            }
            Err(e) => Err(DeployError::network_creation_failed(e.to_string())),
        }
    }

    /// Pull the container image from the registry.
    ///
    /// Respects `pull_policy` configuration:
    /// - `always`: Always pull from registry (default)
    /// - `never`: Skip pulling, use local image only
    ///
    /// # Errors
    ///
    /// Returns `DeployError::ImagePullFailed` if the image cannot be pulled,
    /// or `DeployError::ImagePullTimeout` if the configured timeout is exceeded.
    #[must_use = "deployment state must be used"]
    pub async fn pull_image<R: ImageOps>(
        self,
        runtime: &R,
        auth: Option<&RegistryAuth>,
    ) -> Result<Deployment<ImagePulled>, DeployError> {
        // Skip pull if policy is Never (for local images)
        if self.config.pull_policy == PullPolicy::Never {
            return Ok(Deployment {
                config: self.config,
                old_container: self.old_container,
                state: ImagePulled,
            });
        }

        let pull_future = runtime.pull_image(&self.config.image, auth);

        match self.config.image_pull_timeout {
            Some(timeout) => {
                tokio::time::timeout(timeout, pull_future)
                    .await
                    .map_err(|_| DeployError::image_pull_timeout(timeout.as_secs()))?
                    .context_image_pull()?;
            }
            None => {
                pull_future.await.context_image_pull()?;
            }
        }

        Ok(Deployment {
            config: self.config,
            old_container: self.old_container,
            state: ImagePulled,
        })
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
        let config = self.build_container_config()?;
        let container_id = runtime
            .create_container(&config)
            .await
            .context_container_create()?;

        // Start the container
        if let Err(e) = runtime.start_container(&container_id).await {
            // Clean up the created container on start failure
            let _ = runtime.remove_container(&container_id, true).await;
            return Err(DeployError::container_start_failed(e.to_string()));
        }

        Ok(Deployment {
            config: self.config,
            old_container: self.old_container,
            state: ContainerStarted(container_id),
        })
    }

    /// Build container configuration from deployment config.
    fn build_container_config(&self) -> Result<ContainerConfig, DeployError> {
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

        // Resolve environment variables (fails if required var is missing)
        let env = resolve_env_map(&self.config.env)
            .map_err(|e| DeployError::config_error(e.to_string()))?;

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

        Ok(ContainerConfig {
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
            network: self
                .config
                .network
                .as_ref()
                .map(|_| self.network_name().to_string()),
            network_aliases,
        })
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
        let container_id = self.state.container_id();

        // If no healthcheck is configured, skip the check
        let healthcheck = match &self.config.healthcheck {
            Some(hc) => hc,
            None => {
                return Ok(Deployment {
                    config: self.config,
                    old_container: self.old_container,
                    state: HealthChecked(self.state.0),
                });
            }
        };

        // Build the healthcheck command: ["sh", "-c", cmd]
        let healthcheck_cmd = vec!["sh".to_string(), "-c".to_string(), healthcheck.cmd.clone()];
        let poll_interval = healthcheck.interval;

        // Helper to create the success state transition
        let succeed = || Deployment {
            config: self.config.clone(),
            old_container: self.old_container.clone(),
            state: HealthChecked(self.state.0.clone()),
        };

        // Phase 1: Start period - poll without counting failures.
        // This allows early exit if healthy while tolerating startup failures.
        if healthcheck.start_period > Duration::ZERO {
            let deadline = std::time::Instant::now() + healthcheck.start_period;

            while std::time::Instant::now() < deadline {
                if let HealthPollResult::Healthy =
                    poll_health_once(runtime, container_id, &healthcheck_cmd, healthcheck.timeout)
                        .await
                {
                    return Ok(succeed());
                }
                tokio::time::sleep(poll_interval).await;
            }
        }

        // Phase 2: Main polling with retry counting.
        let start = std::time::Instant::now();
        let mut retries_remaining = healthcheck.retries;

        while start.elapsed() < timeout {
            let failure_reason = match poll_health_once(
                runtime,
                container_id,
                &healthcheck_cmd,
                healthcheck.timeout,
            )
            .await
            {
                HealthPollResult::Healthy => return Ok(succeed()),
                HealthPollResult::Unhealthy => "container reported unhealthy".to_string(),
                HealthPollResult::ExecFailed(e) => format!("healthcheck exec failed: {}", e),
                HealthPollResult::Timeout => "healthcheck command timed out".to_string(),
            };

            if retries_remaining == 0 {
                return Err((self, DeployError::health_check_failed(failure_reason)));
            }
            retries_remaining -= 1;
            tokio::time::sleep(poll_interval).await;
        }

        Err((self, DeployError::health_check_timeout(timeout.as_secs())))
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
        let stop_timeout = self.config.stop_timeout();
        rollback_container(runtime, self.state.container_id(), stop_timeout).await?;
        Ok(Deployment {
            config: self.config,
            old_container: self.old_container,
            state: Initialized,
        })
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
        let new_container_id = self.state.container_id();
        let alias = self.service_alias();

        // If there's an old container, disconnect it from the network first
        if let Some(old_container_id) = &self.old_container
            && let Err(e) = runtime
                .disconnect_from_network(old_container_id, network_id)
                .await
        {
            // Best effort: old container may already be disconnected
            tracing::debug!("Failed to disconnect old container from network: {}", e);
        }

        // Connect new container to network with the service alias.
        // The container may already be connected (created with network set),
        // so ignore "already connected" or "already exists" errors.
        if let Err(e) = runtime
            .connect_to_network(new_container_id, network_id, &[alias])
            .await
        {
            let err_str = e.to_string().to_lowercase();
            if !err_str.contains("already connected") && !err_str.contains("already exists") {
                return Err(DeployError::network_failed(e.to_string()));
            }
        }

        Ok(Deployment {
            config: self.config,
            old_container: self.old_container,
            state: CutOver(self.state.0),
        })
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
        let stop_timeout = self.config.stop_timeout();
        rollback_container(runtime, self.state.container_id(), stop_timeout).await?;
        Ok(Deployment {
            config: self.config,
            old_container: self.old_container,
            state: Initialized,
        })
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
                .await
                .context_container_stop()?;
            // Note: We intentionally don't remove the old container to enable
            // manual rollback via `peleka rollback`. The stopped container
            // becomes the "previous" version that can be restored.
        }

        Ok(Deployment {
            config: self.config,
            old_container: self.old_container,
            state: Completed(self.state.0),
        })
    }
}

// =============================================================================
// Completed - Terminal State
// =============================================================================

impl Deployment<Completed> {
    /// Get the final container ID of the new deployment.
    pub fn deployed_container(&self) -> &ContainerId {
        self.state.container_id()
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
