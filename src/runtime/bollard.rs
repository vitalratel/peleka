// ABOUTME: Bollard-based container runtime implementation.
// ABOUTME: Supports both Docker and Podman via Docker-compatible API.

use crate::runtime::traits::sealed::Sealed;
use crate::runtime::traits::{
    ContainerConfig, ContainerError, ContainerFilters, ContainerInfo, ContainerOps, ContainerState,
    ContainerSummary, ExecConfig, ExecError, ExecInfo, ExecOps, ExecResult, HealthState,
    ImageError, ImageOps, LogError, LogLine, LogOps, LogOptions, LogStream, NetworkConfig,
    NetworkError, NetworkInfo, NetworkOps, NetworkSettings, Protocol, RegistryAuth,
    RestartPolicyConfig, RuntimeInfo, RuntimeInfoError, RuntimeMetadata,
};
use crate::runtime::types::RuntimeType;
use crate::ssh::Session;
use crate::types::{ContainerId, ImageRef, NetworkAlias, NetworkId};
use async_trait::async_trait;
use bollard::Docker;
use bollard::exec::StartExecOptions;
use bollard::models::{
    ContainerCreateBody, EndpointSettings, HealthConfig, HostConfig, Mount, MountTypeEnum,
    PortBinding, RestartPolicy, RestartPolicyNameEnum,
};
use bollard::query_parameters::{
    CreateContainerOptions, CreateImageOptions, InspectContainerOptions, ListContainersOptions,
    LogsOptions, RemoveContainerOptions, RemoveImageOptions, StopContainerOptions,
};
use futures::{Stream, StreamExt};
use hyper_util::rt::TokioIo;
use std::collections::HashMap;
use std::pin::Pin;
use std::time::Duration;
use tokio::net::UnixStream;

// =============================================================================
// Error Mapping Helpers
// =============================================================================

fn map_image_pull_error(e: bollard::errors::Error, image_name: &str) -> ImageError {
    ImageError::PullFailed(format!("{}: {}", image_name, e))
}

fn map_image_remove_error(e: bollard::errors::Error, image_name: &str) -> ImageError {
    match &e {
        bollard::errors::Error::DockerResponseServerError { status_code, .. }
            if *status_code == 404 =>
        {
            ImageError::NotFound(image_name.to_string())
        }
        _ => ImageError::Runtime(format!("failed to remove {}: {}", image_name, e)),
    }
}

fn map_container_create_error(e: bollard::errors::Error) -> ContainerError {
    match &e {
        bollard::errors::Error::DockerResponseServerError {
            status_code,
            message,
        } if *status_code == 404 => ContainerError::ImageNotFound(message.clone()),
        bollard::errors::Error::DockerResponseServerError {
            status_code,
            message,
        } if *status_code == 409 => ContainerError::AlreadyExists(message.clone()),
        _ => ContainerError::Runtime(e.to_string()),
    }
}

fn map_container_start_error(e: bollard::errors::Error) -> ContainerError {
    match &e {
        bollard::errors::Error::DockerResponseServerError {
            status_code,
            message,
        } if *status_code == 404 => ContainerError::NotFound(message.clone()),
        bollard::errors::Error::DockerResponseServerError {
            status_code,
            message,
        } if *status_code == 304 => ContainerError::AlreadyRunning(message.clone()),
        _ => ContainerError::Runtime(e.to_string()),
    }
}

fn map_container_stop_error(e: bollard::errors::Error) -> ContainerError {
    match &e {
        bollard::errors::Error::DockerResponseServerError {
            status_code,
            message,
        } if *status_code == 404 => ContainerError::NotFound(message.clone()),
        bollard::errors::Error::DockerResponseServerError {
            status_code,
            message,
        } if *status_code == 304 => ContainerError::NotRunning(message.clone()),
        _ => ContainerError::Runtime(e.to_string()),
    }
}

fn map_container_not_found_error(e: bollard::errors::Error) -> ContainerError {
    match &e {
        bollard::errors::Error::DockerResponseServerError {
            status_code,
            message,
        } if *status_code == 404 => ContainerError::NotFound(message.clone()),
        _ => ContainerError::Runtime(e.to_string()),
    }
}

fn map_container_rename_error(e: bollard::errors::Error) -> ContainerError {
    match &e {
        bollard::errors::Error::DockerResponseServerError {
            status_code,
            message,
        } if *status_code == 404 => ContainerError::NotFound(message.clone()),
        bollard::errors::Error::DockerResponseServerError {
            status_code,
            message,
        } if *status_code == 409 => ContainerError::AlreadyExists(message.clone()),
        _ => ContainerError::Runtime(e.to_string()),
    }
}

fn map_network_create_error(e: bollard::errors::Error) -> NetworkError {
    match &e {
        bollard::errors::Error::DockerResponseServerError {
            status_code,
            message,
        } if *status_code == 409 => NetworkError::AlreadyExists(message.clone()),
        _ => NetworkError::Runtime(e.to_string()),
    }
}

fn map_network_remove_error(e: bollard::errors::Error) -> NetworkError {
    match &e {
        bollard::errors::Error::DockerResponseServerError {
            status_code,
            message,
        } if *status_code == 404 => NetworkError::NotFound(message.clone()),
        bollard::errors::Error::DockerResponseServerError {
            status_code,
            message,
        } if *status_code == 403 => NetworkError::InUse(message.clone()),
        _ => NetworkError::Runtime(e.to_string()),
    }
}

fn map_network_connect_error(e: bollard::errors::Error) -> NetworkError {
    match &e {
        bollard::errors::Error::DockerResponseServerError {
            status_code,
            message,
        } if *status_code == 404 => NetworkError::NotFound(message.clone()),
        _ => NetworkError::Runtime(e.to_string()),
    }
}

fn map_network_disconnect_error(e: bollard::errors::Error) -> NetworkError {
    match &e {
        bollard::errors::Error::DockerResponseServerError {
            status_code,
            message,
        } if *status_code == 404 => NetworkError::NotFound(message.clone()),
        bollard::errors::Error::DockerResponseServerError {
            status_code,
            message,
        } if *status_code == 403 => NetworkError::NotConnected(message.clone()),
        _ => NetworkError::Runtime(e.to_string()),
    }
}

fn map_exec_create_error(e: bollard::errors::Error) -> ExecError {
    match &e {
        bollard::errors::Error::DockerResponseServerError {
            status_code,
            message,
        } if *status_code == 404 => ExecError::ContainerNotFound(message.clone()),
        bollard::errors::Error::DockerResponseServerError {
            status_code,
            message,
        } if *status_code == 409 => ExecError::ContainerNotRunning(message.clone()),
        _ => ExecError::Runtime(e.to_string()),
    }
}

fn map_exec_not_found_error(e: bollard::errors::Error) -> ExecError {
    match &e {
        bollard::errors::Error::DockerResponseServerError {
            status_code,
            message,
        } if *status_code == 404 => ExecError::ExecNotFound(message.clone()),
        _ => ExecError::Runtime(e.to_string()),
    }
}

// =============================================================================
// BollardRuntime
// =============================================================================

/// Container runtime implementation using bollard.
///
/// Supports both Docker and Podman via Docker-compatible API.
/// For Podman, uses native libpod API for features not in Docker API.
pub struct BollardRuntime {
    client: Docker,
    runtime_type: RuntimeType,
    socket_path: Option<String>,
}

impl BollardRuntime {
    /// Create a new BollardRuntime from a Docker client.
    pub fn new(client: Docker, runtime_type: RuntimeType) -> Self {
        Self {
            client,
            runtime_type,
            socket_path: None,
        }
    }

    /// Create a new BollardRuntime with socket path for libpod API access.
    pub fn new_with_socket(client: Docker, runtime_type: RuntimeType, socket_path: String) -> Self {
        Self {
            client,
            runtime_type,
            socket_path: Some(socket_path),
        }
    }

    /// Connect to a container runtime using detected runtime info.
    ///
    /// Use with `detect_local()` or `detect_runtime()` to connect to a runtime.
    pub fn connect(info: &super::types::RuntimeInfo) -> Result<Self, RuntimeInfoError> {
        let client =
            Docker::connect_with_unix(&info.socket_path, 120, bollard::API_DEFAULT_VERSION)
                .map_err(|e| RuntimeInfoError::ConnectionFailed(e.to_string()))?;
        Ok(Self::new_with_socket(client, info.runtime_type, info.socket_path.clone()))
    }

    /// Pull image using Podman's native libpod API with tlsVerify=false.
    /// This allows pulling from insecure (HTTP) registries.
    async fn pull_image_libpod(&self, image_name: &str) -> Result<(), ImageError> {
        let socket_path = self.socket_path.as_ref().ok_or_else(|| {
            ImageError::PullFailed("socket path not available for libpod API".to_string())
        })?;

        let stream = UnixStream::connect(socket_path).await.map_err(|e| {
            ImageError::PullFailed(format!("failed to connect to socket: {}", e))
        })?;

        let io = TokioIo::new(stream);

        let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await.map_err(|e| {
            ImageError::PullFailed(format!("HTTP handshake failed: {}", e))
        })?;

        // Spawn connection handler
        tokio::spawn(async move {
            if let Err(e) = conn.await {
                tracing::warn!("libpod connection error: {}", e);
            }
        });

        // Build request to libpod API
        let encoded_ref = urlencoding::encode(image_name);
        let uri = format!(
            "/v4.0.0/libpod/images/pull?reference={}&tlsVerify=false",
            encoded_ref
        );

        let req = hyper::Request::builder()
            .method("POST")
            .uri(&uri)
            .header("Host", "localhost")
            .body(http_body_util::Empty::<bytes::Bytes>::new())
            .map_err(|e| ImageError::PullFailed(format!("failed to build request: {}", e)))?;

        let resp = sender.send_request(req).await.map_err(|e| {
            ImageError::PullFailed(format!("request failed: {}", e))
        })?;

        use http_body_util::BodyExt;

        if !resp.status().is_success() {
            // Read error body
            let body = resp.into_body().collect().await.map_err(|e| {
                ImageError::PullFailed(format!("failed to read error response: {}", e))
            })?;
            let body_bytes = body.to_bytes();
            let error_text = String::from_utf8_lossy(&body_bytes);
            return Err(ImageError::PullFailed(format!(
                "{}: libpod API error: {}",
                image_name, error_text
            )));
        }

        // Consume response body (it contains progress JSON)
        let body = resp.into_body().collect().await.map_err(|e| {
            ImageError::PullFailed(format!("failed to read response: {}", e))
        })?;

        // Check for error in response
        let body_bytes = body.to_bytes();
        let body_text = String::from_utf8_lossy(&body_bytes);
        if body_text.contains("\"error\"") && !body_text.contains("\"error\":null") {
            return Err(ImageError::PullFailed(format!(
                "{}: {}",
                image_name, body_text
            )));
        }

        Ok(())
    }

    /// Get the runtime type (Docker or Podman).
    pub fn runtime_type(&self) -> RuntimeType {
        self.runtime_type
    }

    /// Execute in detached mode and poll for completion.
    /// Used for Podman which has issues with attached exec streams not closing.
    async fn exec_start_detached(&self, exec_id: &str) -> Result<ExecResult, ExecError> {
        let opts = StartExecOptions {
            detach: true,
            ..Default::default()
        };

        self.client
            .start_exec(exec_id, Some(opts))
            .await
            .map_err(map_exec_not_found_error)?;

        // Poll for completion
        let poll_interval = std::time::Duration::from_millis(100);
        let max_wait = std::time::Duration::from_secs(300); // 5 minute max
        let start = std::time::Instant::now();

        loop {
            let info = self.exec_inspect_internal(exec_id).await?;
            if !info.running {
                return Ok(ExecResult {
                    exit_code: info.exit_code.unwrap_or(0),
                    stdout: Vec::new(), // Output not captured in detached mode
                    stderr: Vec::new(),
                });
            }

            if start.elapsed() > max_wait {
                return Err(ExecError::Failed("exec timed out".to_string()));
            }

            tokio::time::sleep(poll_interval).await;
        }
    }

    /// Internal exec_inspect that doesn't require trait bounds.
    async fn exec_inspect_internal(&self, exec_id: &str) -> Result<ExecInfo, ExecError> {
        let details = self
            .client
            .inspect_exec(exec_id)
            .await
            .map_err(map_exec_not_found_error)?;

        Ok(ExecInfo {
            id: exec_id.to_string(),
            running: details.running.unwrap_or(false),
            exit_code: details.exit_code,
            container_id: ContainerId::new(details.container_id.unwrap_or_default()),
        })
    }
}

/// Connect to container runtime via SSH session.
///
/// Forwards the Docker/Podman socket from the remote server and creates a
/// BollardRuntime that communicates through the tunnel.
pub async fn connect_via_session(
    session: &mut Session,
    runtime_type: RuntimeType,
) -> Result<BollardRuntime, RuntimeInfoError> {
    // Determine remote socket path based on runtime type
    let remote_socket = match runtime_type {
        RuntimeType::Docker => "/var/run/docker.sock".to_string(),
        RuntimeType::Podman => {
            // Check for rootful Podman first, then rootless
            let rootful_socket = "/run/podman/podman.sock";
            let check_result = session
                .exec(&format!("test -S {} && echo exists", rootful_socket))
                .await;

            if check_result
                .map(|r| r.stdout.contains("exists"))
                .unwrap_or(false)
            {
                rootful_socket.to_string()
            } else {
                // Fall back to rootless Podman
                let uid_output = session
                    .exec("id -u")
                    .await
                    .map_err(|e| RuntimeInfoError::ConnectionFailed(e.to_string()))?;
                let uid = uid_output.stdout.trim();
                format!("/run/user/{}/podman/podman.sock", uid)
            }
        }
    };

    // Forward the socket via SSH
    let local_socket = session
        .forward_socket(&remote_socket)
        .await
        .map_err(|e| RuntimeInfoError::ConnectionFailed(e.to_string()))?;

    // Create Docker client connected to the local forwarded socket
    let client = Docker::connect_with_unix(&local_socket, 120, bollard::API_DEFAULT_VERSION)
        .map_err(|e| RuntimeInfoError::ConnectionFailed(e.to_string()))?;

    Ok(BollardRuntime::new(client, runtime_type))
}

// Implement Sealed trait to allow runtime trait implementations
impl Sealed for BollardRuntime {}

#[async_trait]
impl RuntimeInfo for BollardRuntime {
    async fn info(&self) -> Result<RuntimeMetadata, RuntimeInfoError> {
        let info = self
            .client
            .info()
            .await
            .map_err(|e| RuntimeInfoError::ConnectionFailed(e.to_string()))?;

        let name = match self.runtime_type {
            RuntimeType::Docker => "Docker".to_string(),
            RuntimeType::Podman => "Podman".to_string(),
        };

        Ok(RuntimeMetadata {
            name,
            version: info.server_version.unwrap_or_default(),
            api_version: bollard::API_DEFAULT_VERSION.to_string(),
            os: info.operating_system.unwrap_or_default(),
            arch: info.architecture.unwrap_or_default(),
        })
    }

    async fn ping(&self) -> Result<(), RuntimeInfoError> {
        self.client
            .ping()
            .await
            .map_err(|e| RuntimeInfoError::ConnectionFailed(e.to_string()))?;
        Ok(())
    }
}

#[async_trait]
impl ImageOps for BollardRuntime {
    async fn pull_image(
        &self,
        reference: &ImageRef,
        auth: Option<&RegistryAuth>,
    ) -> Result<(), ImageError> {
        let image_name = reference.to_string();

        // For Podman, use native libpod API which supports tlsVerify=false
        // This allows pulling from insecure (HTTP) registries
        if self.runtime_type == RuntimeType::Podman && self.socket_path.is_some() {
            return self.pull_image_libpod(&image_name).await;
        }

        // Docker-compatible API (works for Docker and Podman with HTTPS registries)
        let opts = CreateImageOptions {
            from_image: Some(image_name.clone()),
            ..Default::default()
        };

        let credentials = auth.map(|a| bollard::auth::DockerCredentials {
            username: Some(a.username.clone()),
            password: Some(a.password.clone()),
            serveraddress: a.server.clone(),
            ..Default::default()
        });

        // Pull returns a stream of progress updates - consume it
        let mut stream = self.client.create_image(Some(opts), None, credentials);
        while let Some(result) = stream.next().await {
            result.map_err(|e| map_image_pull_error(e, &image_name))?;
        }

        Ok(())
    }

    async fn image_exists(&self, reference: &ImageRef) -> Result<bool, ImageError> {
        let image_name = reference.to_string();

        match self.client.inspect_image(&image_name).await {
            Ok(_) => Ok(true),
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 404, ..
            }) => Ok(false),
            Err(e) => Err(ImageError::Runtime(format!(
                "failed to inspect {}: {}",
                image_name, e
            ))),
        }
    }

    async fn remove_image(&self, reference: &ImageRef, force: bool) -> Result<(), ImageError> {
        let image_name = reference.to_string();

        let opts = RemoveImageOptions {
            force,
            ..Default::default()
        };

        self.client
            .remove_image(&image_name, Some(opts), None)
            .await
            .map_err(|e| map_image_remove_error(e, &image_name))?;

        Ok(())
    }
}

#[async_trait]
impl ContainerOps for BollardRuntime {
    async fn create_container(
        &self,
        config: &ContainerConfig,
    ) -> Result<ContainerId, ContainerError> {
        let image_name = config.image.to_string();

        // Build environment variables
        let env: Vec<String> = config
            .env
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();

        // Build labels
        let labels: HashMap<String, String> = config.labels.clone();

        // Build host config with restart policy
        let mut host_config = HostConfig {
            restart_policy: Some(RestartPolicy {
                name: Some(match &config.restart_policy {
                    RestartPolicyConfig::No => RestartPolicyNameEnum::NO,
                    RestartPolicyConfig::Always => RestartPolicyNameEnum::ALWAYS,
                    RestartPolicyConfig::UnlessStopped => RestartPolicyNameEnum::UNLESS_STOPPED,
                    RestartPolicyConfig::OnFailure { .. } => RestartPolicyNameEnum::ON_FAILURE,
                }),
                maximum_retry_count: match &config.restart_policy {
                    RestartPolicyConfig::OnFailure { max_retries } => max_retries.map(|r| r as i64),
                    _ => None,
                },
            }),
            ..Default::default()
        };

        // Set resource limits
        if let Some(ref resources) = config.resources {
            if let Some(memory) = resources.memory {
                host_config.memory = Some(memory as i64);
            }
            if let Some(cpus) = resources.cpus {
                host_config.nano_cpus = Some((cpus * 1_000_000_000.0) as i64);
            }
        }

        // Set stop timeout
        // Note: stop_timeout is on ContainerConfig, not HostConfig in bollard

        // Set volumes/mounts
        let mounts: Vec<Mount> = config
            .volumes
            .iter()
            .map(|m| Mount {
                source: Some(m.source.clone()),
                target: Some(m.target.clone()),
                typ: Some(MountTypeEnum::BIND),
                read_only: Some(m.read_only),
                ..Default::default()
            })
            .collect();
        if !mounts.is_empty() {
            host_config.mounts = Some(mounts);
        }

        // Set port bindings
        let mut port_bindings: HashMap<String, Option<Vec<PortBinding>>> = HashMap::new();
        let mut exposed_ports: Vec<String> = Vec::new();
        for port in &config.ports {
            let proto = match port.protocol {
                Protocol::Tcp => "tcp",
                Protocol::Udp => "udp",
            };
            let port_key = format!("{}/{}", port.container_port, proto);

            exposed_ports.push(port_key.clone());

            if let Some(host_port) = port.host_port {
                port_bindings.insert(
                    port_key,
                    Some(vec![PortBinding {
                        host_ip: port.host_ip.clone(),
                        host_port: Some(host_port.to_string()),
                    }]),
                );
            }
        }
        if !port_bindings.is_empty() {
            host_config.port_bindings = Some(port_bindings);
        }

        // Set network mode
        if let Some(ref network) = config.network {
            host_config.network_mode = Some(network.clone());
        }

        // Build healthcheck config
        let healthcheck = config.healthcheck.as_ref().map(|hc| HealthConfig {
            test: Some(hc.test.clone()),
            interval: Some(hc.interval.as_nanos() as i64),
            timeout: Some(hc.timeout.as_nanos() as i64),
            retries: Some(hc.retries as i64),
            start_period: Some(hc.start_period.as_nanos() as i64),
            start_interval: None,
        });

        // Build networking config with aliases
        let networking_config = if config.network.is_some() && !config.network_aliases.is_empty() {
            let network_name = config.network.as_ref().unwrap().clone();
            let aliases: Vec<String> = config
                .network_aliases
                .iter()
                .map(|a| a.to_string())
                .collect();
            let mut endpoints: HashMap<String, EndpointSettings> = HashMap::new();
            endpoints.insert(
                network_name,
                EndpointSettings {
                    aliases: Some(aliases),
                    ..Default::default()
                },
            );
            Some(bollard::models::NetworkingConfig {
                endpoints_config: Some(endpoints),
            })
        } else {
            None
        };

        // Build container config
        let container_config = ContainerCreateBody {
            image: Some(image_name),
            env: if env.is_empty() { None } else { Some(env) },
            labels: if labels.is_empty() {
                None
            } else {
                Some(labels)
            },
            cmd: config.command.clone(),
            entrypoint: config.entrypoint.clone(),
            working_dir: config.working_dir.clone(),
            user: config.user.clone(),
            host_config: Some(host_config),
            healthcheck,
            exposed_ports: if exposed_ports.is_empty() {
                None
            } else {
                Some(exposed_ports)
            },
            networking_config,
            stop_timeout: config.stop_timeout.map(|d| d.as_secs() as i64),
            ..Default::default()
        };

        let opts = CreateContainerOptions {
            name: Some(config.name.clone()),
            ..Default::default()
        };

        let response = self
            .client
            .create_container(Some(opts), container_config)
            .await
            .map_err(map_container_create_error)?;

        Ok(ContainerId::new(response.id))
    }

    async fn start_container(&self, id: &ContainerId) -> Result<(), ContainerError> {
        self.client
            .start_container(
                id.as_str(),
                None::<bollard::query_parameters::StartContainerOptions>,
            )
            .await
            .map_err(map_container_start_error)
    }

    async fn stop_container(
        &self,
        id: &ContainerId,
        timeout: Duration,
    ) -> Result<(), ContainerError> {
        let opts = StopContainerOptions {
            t: Some(timeout.as_secs() as i32),
            signal: None,
        };

        self.client
            .stop_container(id.as_str(), Some(opts))
            .await
            .map_err(map_container_stop_error)
    }

    async fn remove_container(&self, id: &ContainerId, force: bool) -> Result<(), ContainerError> {
        let opts = RemoveContainerOptions {
            force,
            ..Default::default()
        };

        self.client
            .remove_container(id.as_str(), Some(opts))
            .await
            .map_err(map_container_not_found_error)?;

        Ok(())
    }

    async fn inspect_container(&self, id: &ContainerId) -> Result<ContainerInfo, ContainerError> {
        let details = self
            .client
            .inspect_container(id.as_str(), None::<InspectContainerOptions>)
            .await
            .map_err(map_container_not_found_error)?;

        // Parse state
        let state = details
            .state
            .as_ref()
            .and_then(|s| s.status)
            .map(|s| match s {
                bollard::models::ContainerStateStatusEnum::CREATED => ContainerState::Created,
                bollard::models::ContainerStateStatusEnum::RUNNING => ContainerState::Running,
                bollard::models::ContainerStateStatusEnum::PAUSED => ContainerState::Paused,
                bollard::models::ContainerStateStatusEnum::RESTARTING => ContainerState::Restarting,
                bollard::models::ContainerStateStatusEnum::REMOVING => ContainerState::Removing,
                bollard::models::ContainerStateStatusEnum::EXITED => ContainerState::Exited,
                bollard::models::ContainerStateStatusEnum::DEAD => ContainerState::Dead,
                _ => ContainerState::Exited,
            })
            .unwrap_or(ContainerState::Exited);

        // Parse health status
        let health = details
            .state
            .as_ref()
            .and_then(|s| s.health.as_ref())
            .and_then(|h| h.status)
            .map(|s| match s {
                bollard::models::HealthStatusEnum::STARTING => HealthState::Starting,
                bollard::models::HealthStatusEnum::HEALTHY => HealthState::Healthy,
                bollard::models::HealthStatusEnum::UNHEALTHY => HealthState::Unhealthy,
                _ => HealthState::None,
            });

        // Parse network settings
        let mut networks = std::collections::HashMap::new();
        if let Some(ref network_settings) = details.network_settings
            && let Some(ref nets) = network_settings.networks
        {
            for (name, endpoint) in nets {
                networks.insert(
                    name.clone(),
                    NetworkInfo {
                        network_id: endpoint.network_id.clone().unwrap_or_default(),
                        ip_address: endpoint.ip_address.clone().unwrap_or_default(),
                        gateway: endpoint.gateway.clone().unwrap_or_default(),
                        aliases: endpoint.aliases.clone().unwrap_or_default(),
                    },
                );
            }
        }

        Ok(ContainerInfo {
            id: id.clone(),
            name: details
                .name
                .unwrap_or_default()
                .trim_start_matches('/')
                .to_string(),
            image: details
                .config
                .as_ref()
                .and_then(|c| c.image.clone())
                .unwrap_or_default(),
            state,
            health,
            created: details.created.map(|dt| dt.to_string()).unwrap_or_default(),
            labels: details.config.and_then(|c| c.labels).unwrap_or_default(),
            network_settings: NetworkSettings { networks },
        })
    }

    async fn list_containers(
        &self,
        filters: &ContainerFilters,
    ) -> Result<Vec<ContainerSummary>, ContainerError> {
        let mut filter_map: HashMap<String, Vec<String>> = HashMap::new();

        // Add name filter
        if let Some(ref name) = filters.name {
            filter_map.insert("name".to_string(), vec![name.clone()]);
        }

        // Add label filters
        for (key, value) in &filters.labels {
            filter_map
                .entry("label".to_string())
                .or_default()
                .push(format!("{}={}", key, value));
        }

        let opts = ListContainersOptions {
            all: filters.all,
            filters: Some(filter_map.clone()),
            ..Default::default()
        };

        // Podman reports "stopping" as a container state during shutdown, but bollard
        // doesn't recognize it and fails deserialization. Retry after a short delay
        // since "stopping" is a transient state.
        let mut last_error = None;
        for attempt in 0..3 {
            match self.client.list_containers(Some(opts.clone())).await {
                Ok(containers) => {
                    return Ok(containers
                        .into_iter()
                        .map(|c| {
                            let id = c.id.unwrap_or_default();
                            let names = c.names.unwrap_or_default();
                            let name = names
                                .first()
                                .map(|n| n.trim_start_matches('/').to_string())
                                .unwrap_or_default();

                            let state_str = c
                                .state
                                .map(|s| format!("{:?}", s).to_lowercase())
                                .unwrap_or_default();

                            ContainerSummary {
                                id: ContainerId::new(id),
                                name,
                                image: c.image.unwrap_or_default(),
                                state: state_str,
                                status: c.status.unwrap_or_default(),
                                labels: c.labels.unwrap_or_default(),
                            }
                        })
                        .collect());
                }
                Err(e) => {
                    let err_str = e.to_string();
                    // Podman's "stopping"/"stopped" states cause deserialization failure
                    if (err_str.contains("unknown variant `stopping`")
                        || err_str.contains("unknown variant `stopped`"))
                        && attempt < 2
                    {
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        last_error = Some(err_str);
                        continue;
                    }
                    return Err(ContainerError::Runtime(err_str));
                }
            }
        }

        Err(ContainerError::Runtime(
            last_error.unwrap_or_else(|| "list_containers failed".to_string()),
        ))
    }

    async fn rename_container(
        &self,
        id: &ContainerId,
        new_name: &str,
    ) -> Result<(), ContainerError> {
        self.client
            .rename_container(
                id.as_str(),
                bollard::query_parameters::RenameContainerOptions {
                    name: new_name.to_string(),
                },
            )
            .await
            .map_err(map_container_rename_error)
    }

    async fn run_healthcheck(
        &self,
        id: &ContainerId,
        cmd: &[String],
    ) -> Result<bool, ContainerError> {
        // Build exec config for the healthcheck command
        let exec_config = crate::runtime::ExecConfig {
            cmd: cmd.to_vec(),
            attach_stdout: true,
            attach_stderr: true,
            ..Default::default()
        };

        // Run the healthcheck command via exec
        match self.exec(id, &exec_config).await {
            Ok(result) => {
                // Exit code 0 means healthy
                Ok(result.exit_code == 0)
            }
            Err(e) => Err(ContainerError::Runtime(format!(
                "healthcheck exec failed: {}",
                e
            ))),
        }
    }
}

#[async_trait]
impl NetworkOps for BollardRuntime {
    async fn create_network(&self, config: &NetworkConfig) -> Result<NetworkId, NetworkError> {
        let opts = bollard::models::NetworkCreateRequest {
            name: config.name.clone(),
            driver: config.driver.clone(),
            labels: if config.labels.is_empty() {
                None
            } else {
                Some(config.labels.clone())
            },
            ..Default::default()
        };

        let response = self
            .client
            .create_network(opts)
            .await
            .map_err(map_network_create_error)?;

        Ok(NetworkId::new(response.id))
    }

    async fn remove_network(&self, id: &NetworkId) -> Result<(), NetworkError> {
        self.client
            .remove_network(id.as_str())
            .await
            .map_err(map_network_remove_error)
    }

    async fn connect_to_network(
        &self,
        container: &ContainerId,
        network: &NetworkId,
        aliases: &[NetworkAlias],
    ) -> Result<(), NetworkError> {
        let config = bollard::models::NetworkConnectRequest {
            container: container.to_string(),
            endpoint_config: Some(EndpointSettings {
                aliases: if aliases.is_empty() {
                    None
                } else {
                    Some(aliases.iter().map(|a| a.to_string()).collect())
                },
                ..Default::default()
            }),
        };

        self.client
            .connect_network(network.as_str(), config)
            .await
            .map_err(map_network_connect_error)
    }

    async fn disconnect_from_network(
        &self,
        container: &ContainerId,
        network: &NetworkId,
    ) -> Result<(), NetworkError> {
        let config = bollard::models::NetworkDisconnectRequest {
            container: container.to_string(),
            force: Some(false),
        };

        self.client
            .disconnect_network(network.as_str(), config)
            .await
            .map_err(map_network_disconnect_error)
    }

    async fn network_exists(&self, name: &str) -> Result<bool, NetworkError> {
        match self
            .client
            .inspect_network(
                name,
                None::<bollard::query_parameters::InspectNetworkOptions>,
            )
            .await
        {
            Ok(_) => Ok(true),
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 404, ..
            }) => Ok(false),
            Err(e) => Err(NetworkError::Runtime(e.to_string())),
        }
    }
}

#[async_trait]
impl ExecOps for BollardRuntime {
    async fn exec(
        &self,
        container: &ContainerId,
        config: &ExecConfig,
    ) -> Result<ExecResult, ExecError> {
        // Create exec instance
        let exec_id = self.exec_create(container, config).await?;

        // Start and get output
        self.exec_start(&exec_id).await
    }

    async fn exec_create(
        &self,
        container: &ContainerId,
        config: &ExecConfig,
    ) -> Result<String, ExecError> {
        let opts = bollard::models::ExecConfig {
            cmd: Some(config.cmd.clone()),
            env: if config.env.is_empty() {
                None
            } else {
                Some(config.env.clone())
            },
            working_dir: config.working_dir.clone(),
            user: config.user.clone(),
            attach_stdin: Some(config.attach_stdin),
            attach_stdout: Some(config.attach_stdout),
            attach_stderr: Some(config.attach_stderr),
            tty: Some(config.tty),
            privileged: Some(config.privileged),
            ..Default::default()
        };

        let response = self
            .client
            .create_exec(container.as_str(), opts)
            .await
            .map_err(map_exec_create_error)?;

        Ok(response.id)
    }

    async fn exec_start(&self, exec_id: &str) -> Result<ExecResult, ExecError> {
        // Podman has issues with exec output streams not closing properly,
        // causing attached mode to hang. Use detached mode + polling for Podman.
        if self.runtime_type == RuntimeType::Podman {
            return self.exec_start_detached(exec_id).await;
        }

        // Docker: use attached mode to capture stdout/stderr
        let opts = StartExecOptions {
            detach: false,
            ..Default::default()
        };

        let result = self
            .client
            .start_exec(exec_id, Some(opts))
            .await
            .map_err(map_exec_not_found_error)?;

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        // Handle the StartExecResults enum
        if let bollard::exec::StartExecResults::Attached { mut output, .. } = result {
            while let Some(item) = output.next().await {
                match item {
                    Ok(bollard::container::LogOutput::StdOut { message }) => {
                        stdout.extend(message);
                    }
                    Ok(bollard::container::LogOutput::StdErr { message }) => {
                        stderr.extend(message);
                    }
                    Ok(_) => {}
                    Err(e) => {
                        return Err(ExecError::Failed(e.to_string()));
                    }
                }
            }
        }

        // Get exit code from inspect
        let info = self.exec_inspect_internal(exec_id).await?;
        let exit_code = info.exit_code.unwrap_or(0);

        Ok(ExecResult {
            exit_code,
            stdout,
            stderr,
        })
    }
}

#[async_trait]
impl LogOps for BollardRuntime {
    async fn container_logs(
        &self,
        id: &ContainerId,
        opts: &LogOptions,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<LogLine, LogError>> + Send>>, LogError> {
        let log_opts = LogsOptions {
            stdout: opts.stdout,
            stderr: opts.stderr,
            follow: opts.follow,
            timestamps: opts.timestamps,
            tail: opts
                .tail
                .map(|n| n.to_string())
                .unwrap_or_else(|| "all".to_string()),
            ..Default::default()
        };

        let stream = self.client.logs(id.as_str(), Some(log_opts));

        let mapped_stream = stream.map(|result| {
            result
                .map(|output| {
                    let (stream_type, data) = match output {
                        bollard::container::LogOutput::StdOut { message } => {
                            (LogStream::Stdout, message)
                        }
                        bollard::container::LogOutput::StdErr { message } => {
                            (LogStream::Stderr, message)
                        }
                        bollard::container::LogOutput::StdIn { message } => {
                            (LogStream::Stdout, message)
                        }
                        bollard::container::LogOutput::Console { message } => {
                            (LogStream::Stdout, message)
                        }
                    };

                    LogLine {
                        content: String::from_utf8_lossy(&data).to_string(),
                        stream: stream_type,
                        timestamp: None, // Docker API embeds timestamp in content if requested
                    }
                })
                .map_err(|e| LogError::StreamError(e.to_string()))
        });

        Ok(Box::pin(mapped_stream))
    }
}
