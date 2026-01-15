// ABOUTME: DockerRuntime struct and factory function.
// ABOUTME: Connects to Docker daemon via SSH-forwarded Unix socket.

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
use docker_api::Docker;
use docker_api::opts::{
    ContainerConnectionOpts, ContainerCreateOpts, ContainerDisconnectionOpts, ContainerFilter,
    ContainerListOpts, ContainerRemoveOpts, ContainerStopOpts, ExecCreateOpts, ExecStartOpts,
    ImageRemoveOpts, LogsOpts, NetworkCreateOpts, PullOpts,
};
use futures::{Stream, StreamExt};
use std::pin::Pin;
use std::time::Duration;

// =============================================================================
// Error Mapping Helpers
// =============================================================================

fn map_image_remove_error(e: docker_api::Error, image_name: &str) -> ImageError {
    match e {
        docker_api::Error::Fault { code, message } if code == 404 => ImageError::NotFound(message),
        _ => ImageError::Runtime(format!("failed to remove {}: {}", image_name, e)),
    }
}

fn map_container_create_error(e: docker_api::Error) -> ContainerError {
    match e {
        docker_api::Error::Fault { code, message } if code == 404 => {
            ContainerError::ImageNotFound(message)
        }
        docker_api::Error::Fault { code, message } if code == 409 => {
            ContainerError::AlreadyExists(message)
        }
        _ => ContainerError::Runtime(e.to_string()),
    }
}

fn map_container_start_error(e: docker_api::Error) -> ContainerError {
    match e {
        docker_api::Error::Fault { code, message } if code == 404 => {
            ContainerError::NotFound(message)
        }
        docker_api::Error::Fault { code, message } if code == 304 => {
            ContainerError::AlreadyRunning(message)
        }
        _ => ContainerError::Runtime(e.to_string()),
    }
}

fn map_container_stop_error(e: docker_api::Error) -> ContainerError {
    match e {
        docker_api::Error::Fault { code, message } if code == 404 => {
            ContainerError::NotFound(message)
        }
        docker_api::Error::Fault { code, message } if code == 304 => {
            ContainerError::NotRunning(message)
        }
        _ => ContainerError::Runtime(e.to_string()),
    }
}

fn map_container_not_found_error(e: docker_api::Error) -> ContainerError {
    match e {
        docker_api::Error::Fault { code, message } if code == 404 => {
            ContainerError::NotFound(message)
        }
        _ => ContainerError::Runtime(e.to_string()),
    }
}

fn map_container_rename_error(e: docker_api::Error) -> ContainerError {
    match e {
        docker_api::Error::Fault { code, message } if code == 404 => {
            ContainerError::NotFound(message)
        }
        docker_api::Error::Fault { code, message } if code == 409 => {
            ContainerError::AlreadyExists(message)
        }
        _ => ContainerError::Runtime(e.to_string()),
    }
}

fn map_network_create_error(e: docker_api::Error) -> NetworkError {
    match e {
        docker_api::Error::Fault { code, message } if code == 409 => {
            NetworkError::AlreadyExists(message)
        }
        _ => NetworkError::Runtime(e.to_string()),
    }
}

fn map_network_remove_error(e: docker_api::Error) -> NetworkError {
    match e {
        docker_api::Error::Fault { code, message } if code == 404 => {
            NetworkError::NotFound(message)
        }
        docker_api::Error::Fault { code, message } if code == 403 => {
            NetworkError::InUse(message)
        }
        _ => NetworkError::Runtime(e.to_string()),
    }
}

fn map_network_connect_error(e: docker_api::Error) -> NetworkError {
    match e {
        docker_api::Error::Fault { code, message } if code == 404 => {
            NetworkError::NotFound(message)
        }
        _ => NetworkError::Runtime(e.to_string()),
    }
}

fn map_network_disconnect_error(e: docker_api::Error) -> NetworkError {
    match e {
        docker_api::Error::Fault { code, message } if code == 404 => {
            NetworkError::NotFound(message)
        }
        docker_api::Error::Fault { code, message } if code == 403 => {
            NetworkError::NotConnected(message)
        }
        _ => NetworkError::Runtime(e.to_string()),
    }
}

fn map_exec_create_error(e: docker_api::Error) -> ExecError {
    match e {
        docker_api::Error::Fault { code, message } if code == 404 => {
            ExecError::ContainerNotFound(message)
        }
        docker_api::Error::Fault { code, message } if code == 409 => {
            ExecError::ContainerNotRunning(message)
        }
        _ => ExecError::Runtime(e.to_string()),
    }
}

fn map_exec_not_found_error(e: docker_api::Error) -> ExecError {
    match e {
        docker_api::Error::Fault { code, message } if code == 404 => {
            ExecError::ExecNotFound(message)
        }
        _ => ExecError::Runtime(e.to_string()),
    }
}

// =============================================================================
// DockerRuntime
// =============================================================================

/// Docker runtime implementation.
pub struct DockerRuntime {
    client: Docker,
}

impl DockerRuntime {
    /// Create a new DockerRuntime from a Docker client.
    pub fn new(client: Docker) -> Self {
        Self { client }
    }
}

/// Connect to Docker runtime via SSH session.
///
/// Forwards the Docker socket from the remote server and creates a DockerRuntime
/// that communicates through the tunnel.
pub async fn connect_via_session(
    session: &mut Session,
    runtime_type: RuntimeType,
) -> Result<DockerRuntime, RuntimeInfoError> {
    // Determine remote socket path based on runtime type
    let remote_socket = match runtime_type {
        RuntimeType::Docker => "/var/run/docker.sock".to_string(),
        RuntimeType::Podman => {
            // For rootless Podman, get user ID
            let uid_output = session
                .exec("id -u")
                .await
                .map_err(|e| RuntimeInfoError::ConnectionFailed(e.to_string()))?;
            let uid = uid_output.stdout.trim();
            format!("/run/user/{}/podman/podman.sock", uid)
        }
    };

    // Forward the socket via SSH
    let local_socket = session
        .forward_socket(&remote_socket)
        .await
        .map_err(|e| RuntimeInfoError::ConnectionFailed(e.to_string()))?;

    // Create Docker client connected to the local forwarded socket
    let client = Docker::unix(&local_socket);

    Ok(DockerRuntime::new(client))
}

// Implement Sealed trait to allow runtime trait implementations
impl Sealed for DockerRuntime {}

#[async_trait]
impl RuntimeInfo for DockerRuntime {
    async fn info(&self) -> Result<RuntimeMetadata, RuntimeInfoError> {
        let info = self
            .client
            .info()
            .await
            .map_err(|e| RuntimeInfoError::ConnectionFailed(e.to_string()))?;

        Ok(RuntimeMetadata {
            name: "Docker".to_string(),
            version: info.server_version.unwrap_or_default(),
            api_version: docker_api::LATEST_API_VERSION.to_string(),
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
impl ImageOps for DockerRuntime {
    async fn pull_image(
        &self,
        reference: &ImageRef,
        auth: Option<&RegistryAuth>,
    ) -> Result<(), ImageError> {
        let image_name = reference.to_string();

        let mut opts = PullOpts::builder().image(&image_name);

        if let Some(auth) = auth {
            opts = opts.auth(docker_api::opts::RegistryAuth::Password {
                username: auth.username.clone(),
                password: auth.password.clone(),
                email: None,
                server_address: auth.server.clone(),
            });
        }

        let opts = opts.build();

        // Pull returns a stream of progress updates - consume it
        let images = self.client.images();
        let mut stream = images.pull(&opts);
        while let Some(result) = stream.next().await {
            result.map_err(|e| ImageError::PullFailed(format!("{}: {}", image_name, e)))?;
        }

        Ok(())
    }

    async fn image_exists(&self, reference: &ImageRef) -> Result<bool, ImageError> {
        let image_name = reference.to_string();

        match self.client.images().get(&image_name).inspect().await {
            Ok(_) => Ok(true),
            Err(docker_api::Error::Fault { code, .. }) if code == 404 => Ok(false),
            Err(e) => Err(ImageError::Runtime(format!(
                "failed to inspect {}: {}",
                image_name, e
            ))),
        }
    }

    async fn remove_image(&self, reference: &ImageRef, force: bool) -> Result<(), ImageError> {
        let image_name = reference.to_string();

        let opts = ImageRemoveOpts::builder().force(force).build();

        self.client
            .images()
            .get(&image_name)
            .remove(&opts)
            .await
            .map_err(|e| map_image_remove_error(e, &image_name))?;

        Ok(())
    }
}

#[async_trait]
impl ContainerOps for DockerRuntime {
    async fn create_container(
        &self,
        config: &ContainerConfig,
    ) -> Result<ContainerId, ContainerError> {
        let image_name = config.image.to_string();

        // Build container options
        let mut opts = ContainerCreateOpts::builder()
            .name(&config.name)
            .image(&image_name);

        // Set environment variables
        for (key, value) in &config.env {
            opts = opts.env([format!("{}={}", key, value)]);
        }

        // Set labels
        opts = opts.labels(&config.labels);

        // Set command
        if let Some(ref cmd) = config.command {
            opts = opts.command(cmd);
        }

        // Set entrypoint
        if let Some(ref entrypoint) = config.entrypoint {
            opts = opts.entrypoint(entrypoint);
        }

        // Set working directory
        if let Some(ref working_dir) = config.working_dir {
            opts = opts.working_dir(working_dir);
        }

        // Set user
        if let Some(ref user) = config.user {
            opts = opts.user(user);
        }

        // Set network
        if let Some(ref network) = config.network {
            opts = opts.network_mode(network);
        }

        // Set restart policy
        match &config.restart_policy {
            RestartPolicyConfig::No => {
                opts = opts.restart_policy("no", 0);
            }
            RestartPolicyConfig::Always => {
                opts = opts.restart_policy("always", 0);
            }
            RestartPolicyConfig::UnlessStopped => {
                opts = opts.restart_policy("unless-stopped", 0);
            }
            RestartPolicyConfig::OnFailure { max_retries } => {
                opts = opts.restart_policy("on-failure", max_retries.unwrap_or(0) as u64);
            }
        }

        // Set resource limits
        if let Some(ref resources) = config.resources {
            if let Some(memory) = resources.memory {
                opts = opts.memory(memory);
            }
            if let Some(cpus) = resources.cpus {
                opts = opts.nano_cpus((cpus * 1_000_000_000.0) as u64);
            }
        }

        // Set stop timeout
        if let Some(timeout) = config.stop_timeout {
            opts = opts.stop_timeout(timeout);
        }

        // Set volumes
        for mount in &config.volumes {
            let mount_str = if mount.read_only {
                format!("{}:{}:ro", mount.source, mount.target)
            } else {
                format!("{}:{}", mount.source, mount.target)
            };
            opts = opts.volumes([mount_str]);
        }

        // Set port mappings
        for port in &config.ports {
            let publish_port = match port.protocol {
                Protocol::Tcp => docker_api::opts::PublishPort::tcp(port.container_port.into()),
                Protocol::Udp => docker_api::opts::PublishPort::udp(port.container_port.into()),
            };
            if let Some(host_port) = port.host_port {
                opts = opts.expose(publish_port, u32::from(host_port));
            }
        }

        let opts = opts.build();

        let container = self
            .client
            .containers()
            .create(&opts)
            .await
            .map_err(map_container_create_error)?;

        Ok(ContainerId::new(container.id().to_string()))
    }

    async fn start_container(&self, id: &ContainerId) -> Result<(), ContainerError> {
        self.client
            .containers()
            .get(id.as_str())
            .start()
            .await
            .map_err(map_container_start_error)
    }

    async fn stop_container(
        &self,
        id: &ContainerId,
        timeout: Duration,
    ) -> Result<(), ContainerError> {
        let opts = ContainerStopOpts::builder().wait(timeout).build();

        self.client
            .containers()
            .get(id.as_str())
            .stop(&opts)
            .await
            .map_err(map_container_stop_error)
    }

    async fn remove_container(&self, id: &ContainerId, force: bool) -> Result<(), ContainerError> {
        let opts = ContainerRemoveOpts::builder().force(force).build();

        self.client
            .containers()
            .get(id.as_str())
            .remove(&opts)
            .await
            .map_err(map_container_not_found_error)?;

        Ok(())
    }

    async fn inspect_container(&self, id: &ContainerId) -> Result<ContainerInfo, ContainerError> {
        let details = self
            .client
            .containers()
            .get(id.as_str())
            .inspect()
            .await
            .map_err(map_container_not_found_error)?;

        // Parse state
        let state = details
            .state
            .as_ref()
            .and_then(|s| s.status.as_ref())
            .map(|s| match s.as_str() {
                "created" => ContainerState::Created,
                "running" => ContainerState::Running,
                "paused" => ContainerState::Paused,
                "restarting" => ContainerState::Restarting,
                "removing" => ContainerState::Removing,
                "exited" => ContainerState::Exited,
                "dead" => ContainerState::Dead,
                _ => ContainerState::Exited,
            })
            .unwrap_or(ContainerState::Exited);

        // Parse health status
        let health = details
            .state
            .as_ref()
            .and_then(|s| s.health.as_ref())
            .and_then(|h| h.status.as_ref())
            .map(|s| match s.as_str() {
                "starting" => HealthState::Starting,
                "healthy" => HealthState::Healthy,
                "unhealthy" => HealthState::Unhealthy,
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
            created: details.created.unwrap_or_default(),
            labels: details.config.and_then(|c| c.labels).unwrap_or_default(),
            network_settings: NetworkSettings { networks },
        })
    }

    async fn list_containers(
        &self,
        filters: &ContainerFilters,
    ) -> Result<Vec<ContainerSummary>, ContainerError> {
        let mut opts = ContainerListOpts::builder().all(filters.all);

        // Add name filter
        if let Some(ref name) = filters.name {
            opts = opts.filter([ContainerFilter::Name(name.clone())]);
        }

        // Add label filters
        for (key, value) in &filters.labels {
            opts = opts.filter([ContainerFilter::Label(key.clone(), value.clone())]);
        }

        let opts = opts.build();

        let containers = self
            .client
            .containers()
            .list(&opts)
            .await
            .map_err(|e| ContainerError::Runtime(e.to_string()))?;

        Ok(containers
            .into_iter()
            .map(|c| {
                let id = c.id.unwrap_or_default();
                let names = c.names.unwrap_or_default();
                let name = names
                    .first()
                    .map(|n| n.trim_start_matches('/').to_string())
                    .unwrap_or_default();

                ContainerSummary {
                    id: ContainerId::new(id),
                    name,
                    image: c.image.unwrap_or_default(),
                    state: c.state.unwrap_or_default(),
                    status: c.status.unwrap_or_default(),
                    labels: c.labels.unwrap_or_default(),
                }
            })
            .collect())
    }

    async fn rename_container(
        &self,
        id: &ContainerId,
        new_name: &str,
    ) -> Result<(), ContainerError> {
        self.client
            .containers()
            .get(id.as_str())
            .rename(new_name)
            .await
            .map_err(map_container_rename_error)
    }
}

#[async_trait]
impl NetworkOps for DockerRuntime {
    async fn create_network(&self, config: &NetworkConfig) -> Result<NetworkId, NetworkError> {
        let mut opts = NetworkCreateOpts::builder(&config.name);

        // Set driver if specified
        if let Some(ref driver) = config.driver {
            opts = opts.driver(driver);
        }

        // Set labels
        opts = opts.labels(&config.labels);

        let opts = opts.build();

        let network = self
            .client
            .networks()
            .create(&opts)
            .await
            .map_err(map_network_create_error)?;

        Ok(NetworkId::new(network.id().to_string()))
    }

    async fn remove_network(&self, id: &NetworkId) -> Result<(), NetworkError> {
        self.client
            .networks()
            .get(id.as_str())
            .delete()
            .await
            .map_err(map_network_remove_error)
    }

    async fn connect_to_network(
        &self,
        container: &ContainerId,
        network: &NetworkId,
        aliases: &[NetworkAlias],
    ) -> Result<(), NetworkError> {
        let mut opts = ContainerConnectionOpts::builder(container.as_str());

        // Add aliases
        if !aliases.is_empty() {
            let alias_strings: Vec<&str> = aliases.iter().map(|a| a.as_str()).collect();
            opts = opts.aliases(alias_strings);
        }

        let opts = opts.build();

        self.client
            .networks()
            .get(network.as_str())
            .connect(&opts)
            .await
            .map_err(map_network_connect_error)
    }

    async fn disconnect_from_network(
        &self,
        container: &ContainerId,
        network: &NetworkId,
    ) -> Result<(), NetworkError> {
        let opts = ContainerDisconnectionOpts::builder(container.as_str()).build();

        self.client
            .networks()
            .get(network.as_str())
            .disconnect(&opts)
            .await
            .map_err(map_network_disconnect_error)
    }

    async fn network_exists(&self, name: &str) -> Result<bool, NetworkError> {
        match self.client.networks().get(name).inspect().await {
            Ok(_) => Ok(true),
            Err(docker_api::Error::Fault { code, .. }) if code == 404 => Ok(false),
            Err(e) => Err(NetworkError::Runtime(e.to_string())),
        }
    }
}

#[async_trait]
impl ExecOps for DockerRuntime {
    async fn exec(
        &self,
        container: &ContainerId,
        config: &ExecConfig,
    ) -> Result<ExecResult, ExecError> {
        // Build exec options
        let mut opts = ExecCreateOpts::builder()
            .command(&config.cmd)
            .attach_stdout(config.attach_stdout)
            .attach_stderr(config.attach_stderr)
            .attach_stdin(config.attach_stdin)
            .tty(config.tty)
            .privileged(config.privileged);

        // Set environment
        if !config.env.is_empty() {
            opts = opts.env(&config.env);
        }

        // Set working directory
        if let Some(ref working_dir) = config.working_dir {
            opts = opts.working_dir(working_dir);
        }

        // Set user
        if let Some(ref user) = config.user {
            opts = opts.user(user);
        }

        let opts = opts.build();

        // Run exec and collect output
        let mut multiplexer = self
            .client
            .containers()
            .get(container.as_str())
            .exec(&opts, &ExecStartOpts::default())
            .await
            .map_err(map_exec_create_error)?;

        // Collect output from multiplexer
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        while let Some(result) = multiplexer.next().await {
            match result {
                Ok(chunk) => match chunk {
                    docker_api::conn::TtyChunk::StdOut(data) => stdout.extend(data),
                    docker_api::conn::TtyChunk::StdErr(data) => stderr.extend(data),
                    docker_api::conn::TtyChunk::StdIn(_) => {}
                },
                Err(e) => {
                    return Err(ExecError::Failed(e.to_string()));
                }
            }
        }

        // For simple exec, we assume success (exit code 0) if no error
        // The docker-api crate doesn't expose exec ID from this method
        Ok(ExecResult {
            exit_code: 0,
            stdout,
            stderr,
        })
    }

    async fn exec_create(
        &self,
        container: &ContainerId,
        config: &ExecConfig,
    ) -> Result<String, ExecError> {
        let mut opts = ExecCreateOpts::builder()
            .command(&config.cmd)
            .attach_stdout(config.attach_stdout)
            .attach_stderr(config.attach_stderr)
            .attach_stdin(config.attach_stdin)
            .tty(config.tty)
            .privileged(config.privileged);

        // Set environment
        if !config.env.is_empty() {
            opts = opts.env(&config.env);
        }

        // Set working directory
        if let Some(ref working_dir) = config.working_dir {
            opts = opts.working_dir(working_dir);
        }

        // Set user
        if let Some(ref user) = config.user {
            opts = opts.user(user);
        }

        let opts = opts.build();

        let exec = docker_api::Exec::create(self.client.clone(), container.as_str(), &opts)
            .await
            .map_err(map_exec_create_error)?;

        // Get the exec ID via inspect since Exec::id() is private
        let details = exec
            .inspect()
            .await
            .map_err(|e| ExecError::Runtime(e.to_string()))?;

        Ok(details.id.unwrap_or_default())
    }

    async fn exec_start(&self, exec_id: &str) -> Result<ExecResult, ExecError> {
        let exec = docker_api::Exec::get(self.client.clone(), exec_id);
        let opts = ExecStartOpts::default();

        let mut multiplexer = exec.start(&opts).await.map_err(map_exec_not_found_error)?;

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        while let Some(result) = multiplexer.next().await {
            match result {
                Ok(chunk) => match chunk {
                    docker_api::conn::TtyChunk::StdOut(data) => stdout.extend(data),
                    docker_api::conn::TtyChunk::StdErr(data) => stderr.extend(data),
                    docker_api::conn::TtyChunk::StdIn(_) => {}
                },
                Err(e) => {
                    return Err(ExecError::Failed(e.to_string()));
                }
            }
        }

        // Get exit code from inspect
        let info = self.exec_inspect(exec_id).await?;
        let exit_code = info.exit_code.unwrap_or(0);

        Ok(ExecResult {
            exit_code,
            stdout,
            stderr,
        })
    }

    async fn exec_inspect(&self, exec_id: &str) -> Result<ExecInfo, ExecError> {
        let exec = docker_api::Exec::get(self.client.clone(), exec_id);
        let details = exec.inspect().await.map_err(map_exec_not_found_error)?;

        Ok(ExecInfo {
            id: exec_id.to_string(),
            running: details.running.unwrap_or(false),
            exit_code: details.exit_code.map(|e| e as i64),
            container_id: ContainerId::new(details.container_id.unwrap_or_default()),
        })
    }
}

#[async_trait]
impl LogOps for DockerRuntime {
    async fn container_logs(
        &self,
        id: &ContainerId,
        opts: &LogOptions,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<LogLine, LogError>> + Send>>, LogError> {
        let mut log_opts = LogsOpts::builder()
            .stdout(opts.stdout)
            .stderr(opts.stderr)
            .follow(opts.follow)
            .timestamps(opts.timestamps);

        // Set tail option
        if let Some(n) = opts.tail {
            log_opts = log_opts.n_lines(n as usize);
        } else {
            log_opts = log_opts.all();
        }

        let log_opts = log_opts.build();

        // Clone client and container ID to avoid lifetime issues
        let client = self.client.clone();
        let container_id = id.to_string();

        // Create a stream that owns its dependencies using async_stream
        let stream = futures::stream::unfold(
            (client, container_id, log_opts, false),
            |(client, container_id, log_opts, started)| async move {
                if started {
                    return None;
                }

                // Get the log stream
                let container = client.containers().get(&container_id);
                let mut stream = container.logs(&log_opts);

                // Collect all chunks into a vec to avoid lifetime issues
                let mut items = Vec::new();
                while let Some(result) = stream.next().await {
                    items.push(result);
                }

                Some((items, (client, container_id, log_opts, true)))
            },
        )
        .flat_map(futures::stream::iter)
        .map(|result| {
            result
                .map(|chunk| {
                    let (stream_type, data) = match chunk {
                        docker_api::conn::TtyChunk::StdOut(data) => (LogStream::Stdout, data),
                        docker_api::conn::TtyChunk::StdErr(data) => (LogStream::Stderr, data),
                        docker_api::conn::TtyChunk::StdIn(data) => (LogStream::Stdout, data),
                    };

                    LogLine {
                        content: String::from_utf8_lossy(&data).to_string(),
                        stream: stream_type,
                        timestamp: None, // Docker API embeds timestamp in content if requested
                    }
                })
                .map_err(|e| LogError::StreamError(e.to_string()))
        });

        Ok(Box::pin(stream))
    }
}
