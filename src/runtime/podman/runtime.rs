// ABOUTME: PodmanRuntime struct and trait implementations.
// ABOUTME: Connects to Podman daemon via SSH-forwarded Unix socket.

use crate::runtime::traits::sealed::Sealed;
use crate::runtime::traits::{
    ContainerConfig, ContainerError, ContainerFilters, ContainerInfo, ContainerOps, ContainerState,
    ContainerSummary, ExecConfig, ExecError, ExecInfo, ExecOps, ExecResult, HealthState,
    ImageError, ImageOps, LogError, LogLine, LogOps, LogOptions, LogStream, NetworkConfig,
    NetworkError, NetworkInfo, NetworkOps, NetworkSettings, Protocol, RegistryAuth,
    RestartPolicyConfig, RuntimeInfo, RuntimeInfoError, RuntimeMetadata,
};
use crate::ssh::Session;
use crate::types::{ContainerId, ImageRef, NetworkAlias, NetworkId};
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use podman_api::Podman;
use podman_api::opts::{
    ContainerCreateOpts, ContainerDeleteOpts, ContainerListOpts, ContainerLogsOpts,
    ContainerRestartPolicy, ContainerStopOpts, ExecCreateOpts, ExecStartOpts, NetworkConnectOpts,
    NetworkCreateOpts, NetworkDisconnectOpts, PullOpts,
};
use std::collections::HashMap;
use std::pin::Pin;
use std::time::Duration;

// =============================================================================
// Error Mapping Helpers
// =============================================================================

fn map_image_remove_error(e: podman_api::Error, image_name: &str) -> ImageError {
    match e {
        podman_api::Error::Fault { code, message } if code == 404 => ImageError::NotFound(message),
        _ => ImageError::Runtime(format!("failed to remove {}: {}", image_name, e)),
    }
}

fn map_container_create_error(e: podman_api::Error) -> ContainerError {
    match e {
        podman_api::Error::Fault { code, message } if code == 404 => {
            ContainerError::ImageNotFound(message)
        }
        podman_api::Error::Fault { code, message } if code == 409 => {
            ContainerError::AlreadyExists(message)
        }
        _ => ContainerError::Runtime(e.to_string()),
    }
}

fn map_container_start_error(e: podman_api::Error) -> ContainerError {
    match e {
        podman_api::Error::Fault { code, message } if code == 404 => {
            ContainerError::NotFound(message)
        }
        podman_api::Error::Fault { code, message } if code == 304 => {
            ContainerError::AlreadyRunning(message)
        }
        _ => ContainerError::Runtime(e.to_string()),
    }
}

fn map_container_stop_error(e: podman_api::Error) -> ContainerError {
    match e {
        podman_api::Error::Fault { code, message } if code == 404 => {
            ContainerError::NotFound(message)
        }
        podman_api::Error::Fault { code, message } if code == 304 => {
            ContainerError::NotRunning(message)
        }
        _ => ContainerError::Runtime(e.to_string()),
    }
}

fn map_container_not_found_error(e: podman_api::Error) -> ContainerError {
    match e {
        podman_api::Error::Fault { code, message } if code == 404 => {
            ContainerError::NotFound(message)
        }
        _ => ContainerError::Runtime(e.to_string()),
    }
}

fn map_container_rename_error(e: podman_api::Error) -> ContainerError {
    match e {
        podman_api::Error::Fault { code, message } if code == 404 => {
            ContainerError::NotFound(message)
        }
        podman_api::Error::Fault { code, message } if code == 409 => {
            ContainerError::AlreadyExists(message)
        }
        _ => ContainerError::Runtime(e.to_string()),
    }
}

fn map_network_create_error(e: podman_api::Error) -> NetworkError {
    match e {
        podman_api::Error::Fault { code, message } if code == 409 => {
            NetworkError::AlreadyExists(message)
        }
        _ => NetworkError::Runtime(e.to_string()),
    }
}

fn map_network_remove_error(e: podman_api::Error) -> NetworkError {
    match e {
        podman_api::Error::Fault { code, message } if code == 404 => {
            NetworkError::NotFound(message)
        }
        podman_api::Error::Fault { code, message } if code == 403 => {
            NetworkError::InUse(message)
        }
        _ => NetworkError::Runtime(e.to_string()),
    }
}

fn map_network_connect_error(e: podman_api::Error) -> NetworkError {
    match e {
        podman_api::Error::Fault { code, message } if code == 404 => {
            NetworkError::NotFound(message)
        }
        _ => NetworkError::Runtime(e.to_string()),
    }
}

fn map_network_disconnect_error(e: podman_api::Error) -> NetworkError {
    match e {
        podman_api::Error::Fault { code, message } if code == 404 => {
            NetworkError::NotFound(message)
        }
        podman_api::Error::Fault { code, message } if code == 403 => {
            NetworkError::NotConnected(message)
        }
        _ => NetworkError::Runtime(e.to_string()),
    }
}

fn map_exec_create_error(e: podman_api::Error) -> ExecError {
    match e {
        podman_api::Error::Fault { code, message } if code == 404 => {
            ExecError::ContainerNotFound(message)
        }
        podman_api::Error::Fault { code, message } if code == 409 => {
            ExecError::ContainerNotRunning(message)
        }
        _ => ExecError::Runtime(e.to_string()),
    }
}

fn map_exec_not_found_error(e: podman_api::Error) -> ExecError {
    match e {
        podman_api::Error::Fault { code, message } if code == 404 => {
            ExecError::ExecNotFound(message)
        }
        _ => ExecError::Runtime(e.to_string()),
    }
}

// =============================================================================
// PodmanRuntime
// =============================================================================

/// Podman runtime implementation.
pub struct PodmanRuntime {
    client: Podman,
}

impl PodmanRuntime {
    /// Create a new PodmanRuntime from a Podman client.
    pub fn new(client: Podman) -> Self {
        Self { client }
    }

    /// Connect to Podman runtime via SSH session.
    ///
    /// Forwards the rootless Podman socket from the remote server and creates
    /// a PodmanRuntime that communicates through the tunnel.
    pub async fn connect_via_session(
        session: &mut Session,
    ) -> Result<PodmanRuntime, RuntimeInfoError> {
        // For rootless Podman, get user ID and use their socket
        let uid_output = session
            .exec("id -u")
            .await
            .map_err(|e| RuntimeInfoError::ConnectionFailed(e.to_string()))?;
        let uid = uid_output.stdout.trim();
        let remote_socket = format!("/run/user/{}/podman/podman.sock", uid);

        // Forward the socket via SSH
        let local_socket = session
            .forward_socket(&remote_socket)
            .await
            .map_err(|e| RuntimeInfoError::ConnectionFailed(e.to_string()))?;

        // Create Podman client connected to the local forwarded socket
        let client = Podman::unix(&local_socket);

        Ok(PodmanRuntime::new(client))
    }
}

// Implement Sealed trait to allow runtime trait implementations
impl Sealed for PodmanRuntime {}

#[async_trait]
impl RuntimeInfo for PodmanRuntime {
    async fn info(&self) -> Result<RuntimeMetadata, RuntimeInfoError> {
        let info = self
            .client
            .info()
            .await
            .map_err(|e| RuntimeInfoError::ConnectionFailed(e.to_string()))?;

        Ok(RuntimeMetadata {
            name: "Podman".to_string(),
            version: info
                .version
                .as_ref()
                .and_then(|v| v.version.clone())
                .unwrap_or_default(),
            api_version: info
                .version
                .as_ref()
                .and_then(|v| v.api_version.clone())
                .unwrap_or_default(),
            os: info
                .host
                .as_ref()
                .and_then(|h| h.os.clone())
                .unwrap_or_default(),
            arch: info
                .host
                .as_ref()
                .and_then(|h| h.arch.clone())
                .unwrap_or_default(),
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
impl ImageOps for PodmanRuntime {
    async fn pull_image(
        &self,
        reference: &ImageRef,
        auth: Option<&RegistryAuth>,
    ) -> Result<(), ImageError> {
        let image_name = reference.to_string();

        let mut opts = PullOpts::builder().reference(&image_name);

        if let Some(auth) = auth {
            // Use credentials in username:password format
            let credentials = format!("{}:{}", auth.username, auth.password);
            opts = opts.credentials(&credentials);
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

        match self.client.images().get(&image_name).exists().await {
            Ok(exists) => Ok(exists),
            Err(e) => Err(ImageError::Runtime(format!(
                "failed to check {}: {}",
                image_name, e
            ))),
        }
    }

    async fn remove_image(&self, reference: &ImageRef, force: bool) -> Result<(), ImageError> {
        let image_name = reference.to_string();

        let result = if force {
            self.client.images().get(&image_name).remove().await
        } else {
            self.client.images().get(&image_name).delete().await
        };

        result.map_err(|e| map_image_remove_error(e, &image_name))?;

        Ok(())
    }
}

#[async_trait]
impl ContainerOps for PodmanRuntime {
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
        let env_pairs: Vec<(&str, &str)> = config
            .env
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        if !env_pairs.is_empty() {
            opts = opts.env(env_pairs);
        }

        // Set labels
        let label_pairs: Vec<(&str, &str)> = config
            .labels
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        if !label_pairs.is_empty() {
            opts = opts.labels(label_pairs);
        }

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
            opts = opts.work_dir(working_dir);
        }

        // Set user
        if let Some(ref user) = config.user {
            opts = opts.user(user);
        }

        // Set network
        if let Some(ref network) = config.network {
            let mut net_map: HashMap<String, podman_api::models::PerNetworkOptions> =
                HashMap::new();
            net_map.insert(
                network.clone(),
                podman_api::models::PerNetworkOptions {
                    aliases: None,
                    interface_name: None,
                    static_ips: None,
                    static_mac: None,
                },
            );
            opts = opts.networks(net_map);
        }

        // Set restart policy
        opts = match &config.restart_policy {
            RestartPolicyConfig::No => opts.restart_policy(ContainerRestartPolicy::No),
            RestartPolicyConfig::Always => opts.restart_policy(ContainerRestartPolicy::Always),
            RestartPolicyConfig::UnlessStopped => {
                opts.restart_policy(ContainerRestartPolicy::UnlessStopped)
            }
            RestartPolicyConfig::OnFailure { .. } => {
                opts.restart_policy(ContainerRestartPolicy::OnFailure)
            }
        };

        // Set stop timeout
        if let Some(timeout) = config.stop_timeout {
            opts = opts.stop_timeout(timeout.as_secs());
        }

        // Set volumes using mounts (bind style)
        for mount in &config.volumes {
            let mount_config = podman_api::models::ContainerMount {
                destination: Some(mount.target.clone()),
                source: Some(mount.source.clone()),
                _type: Some("bind".to_string()),
                options: if mount.read_only {
                    Some(vec!["ro".to_string()])
                } else {
                    None
                },
                gid_mappings: None,
                uid_mappings: None,
            };
            opts = opts.mounts([mount_config]);
        }

        // Set port mappings
        let port_mappings: Vec<podman_api::models::PortMapping> = config
            .ports
            .iter()
            .map(|port| podman_api::models::PortMapping {
                container_port: Some(port.container_port),
                host_port: port.host_port,
                protocol: Some(match port.protocol {
                    Protocol::Tcp => "tcp".to_string(),
                    Protocol::Udp => "udp".to_string(),
                }),
                host_ip: port.host_ip.clone(),
                range: None,
            })
            .collect();
        if !port_mappings.is_empty() {
            opts = opts.portmappings(port_mappings);
        }

        let opts = opts.build();

        let container = self
            .client
            .containers()
            .create(&opts)
            .await
            .map_err(map_container_create_error)?;

        Ok(ContainerId::new(container.id))
    }

    async fn start_container(&self, id: &ContainerId) -> Result<(), ContainerError> {
        self.client
            .containers()
            .get(id.as_str())
            .start(None)
            .await
            .map_err(map_container_start_error)
    }

    async fn stop_container(
        &self,
        id: &ContainerId,
        timeout: Duration,
    ) -> Result<(), ContainerError> {
        let opts = ContainerStopOpts::builder()
            .timeout(timeout.as_secs() as usize)
            .build();

        self.client
            .containers()
            .get(id.as_str())
            .stop(&opts)
            .await
            .map_err(map_container_stop_error)
    }

    async fn remove_container(&self, id: &ContainerId, force: bool) -> Result<(), ContainerError> {
        let result = if force {
            self.client.containers().get(id.as_str()).remove().await
        } else {
            let opts = ContainerDeleteOpts::builder().build();
            self.client
                .containers()
                .get(id.as_str())
                .delete(&opts)
                .await
        };

        result.map_err(map_container_not_found_error)
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

        // Convert DateTime to string
        let created_str = details
            .created
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_default();

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
            created: created_str,
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
            opts = opts.filter([podman_api::opts::ContainerListFilter::Name(name.clone())]);
        }

        // Add label filters
        for (key, value) in &filters.labels {
            opts = opts.filter([podman_api::opts::ContainerListFilter::LabelKeyVal(
                key.clone(),
                value.clone(),
            )]);
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
impl NetworkOps for PodmanRuntime {
    async fn create_network(&self, config: &NetworkConfig) -> Result<NetworkId, NetworkError> {
        let mut opts = NetworkCreateOpts::builder().name(&config.name);

        // Set driver if specified
        if let Some(ref driver) = config.driver {
            opts = opts.driver(driver);
        }

        // Set labels
        let label_pairs: Vec<(&str, &str)> = config
            .labels
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        if !label_pairs.is_empty() {
            opts = opts.labels(label_pairs);
        }

        let opts = opts.build();

        let network = self
            .client
            .networks()
            .create(&opts)
            .await
            .map_err(map_network_create_error)?;

        // Podman returns the network name as ID in the response
        Ok(NetworkId::new(
            network.name.unwrap_or_else(|| config.name.clone()),
        ))
    }

    async fn remove_network(&self, id: &NetworkId) -> Result<(), NetworkError> {
        self.client
            .networks()
            .get(id.as_str())
            .delete()
            .await
            .map_err(map_network_remove_error)?;
        Ok(())
    }

    async fn connect_to_network(
        &self,
        container: &ContainerId,
        network: &NetworkId,
        aliases: &[NetworkAlias],
    ) -> Result<(), NetworkError> {
        let mut opts = NetworkConnectOpts::builder().container(container.as_str());

        // Add aliases
        if !aliases.is_empty() {
            let alias_strings: Vec<&str> = aliases.iter().map(|a| a.as_str()).collect();
            opts = opts.aliases(alias_strings);
        }

        let opts = opts.build();

        self.client
            .networks()
            .get(network.as_str())
            .connect_container(&opts)
            .await
            .map_err(map_network_connect_error)
    }

    async fn disconnect_from_network(
        &self,
        container: &ContainerId,
        network: &NetworkId,
    ) -> Result<(), NetworkError> {
        let opts = NetworkDisconnectOpts::builder()
            .container(container.as_str())
            .build();

        self.client
            .networks()
            .get(network.as_str())
            .disconnect_container(&opts)
            .await
            .map_err(map_network_disconnect_error)
    }

    async fn network_exists(&self, name: &str) -> Result<bool, NetworkError> {
        match self.client.networks().get(name).exists().await {
            Ok(exists) => Ok(exists),
            Err(e) => Err(NetworkError::Runtime(e.to_string())),
        }
    }
}

#[async_trait]
impl ExecOps for PodmanRuntime {
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

        // Set environment as key=value pairs
        if !config.env.is_empty() {
            // Parse "KEY=VALUE" strings into tuples
            let env_pairs: Vec<(&str, &str)> = config
                .env
                .iter()
                .filter_map(|s| {
                    let parts: Vec<&str> = s.splitn(2, '=').collect();
                    if parts.len() == 2 {
                        Some((parts[0], parts[1]))
                    } else {
                        None
                    }
                })
                .collect();
            if !env_pairs.is_empty() {
                opts = opts.env(env_pairs);
            }
        }

        // Set working directory
        if let Some(ref working_dir) = config.working_dir {
            opts = opts.working_dir(working_dir);
        }

        // Set user
        if let Some(ref user) = config.user {
            opts = opts.user(podman_api::opts::UserOpt::User(user.clone()));
        }

        let opts = opts.build();

        // Create exec
        let exec = self
            .client
            .containers()
            .get(container.as_str())
            .create_exec(&opts)
            .await
            .map_err(map_exec_create_error)?;

        // Get exec ID from response
        let exec_id = exec.id().to_string();

        // Start the exec
        let start_opts = ExecStartOpts::builder().build();
        let exec_handle = self.client.execs().get(&exec_id);
        let multiplexer = exec_handle
            .start(&start_opts)
            .await
            .map_err(|e| ExecError::Runtime(e.to_string()))?;

        // Collect output - handle Option<Multiplexer>
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        if let Some(mut stream) = multiplexer {
            while let Some(result) = stream.next().await {
                match result {
                    Ok(chunk) => match chunk {
                        podman_api::conn::TtyChunk::StdOut(data) => stdout.extend(data),
                        podman_api::conn::TtyChunk::StdErr(data) => stderr.extend(data),
                        podman_api::conn::TtyChunk::StdIn(_) => {}
                    },
                    Err(e) => {
                        return Err(ExecError::Failed(e.to_string()));
                    }
                }
            }
        }

        // Get exit code from inspect
        let info = self.exec_inspect(&exec_id).await?;
        let exit_code = info.exit_code.unwrap_or(0);

        Ok(ExecResult {
            exit_code,
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

        // Set environment as key=value pairs
        if !config.env.is_empty() {
            let env_pairs: Vec<(&str, &str)> = config
                .env
                .iter()
                .filter_map(|s| {
                    let parts: Vec<&str> = s.splitn(2, '=').collect();
                    if parts.len() == 2 {
                        Some((parts[0], parts[1]))
                    } else {
                        None
                    }
                })
                .collect();
            if !env_pairs.is_empty() {
                opts = opts.env(env_pairs);
            }
        }

        // Set working directory
        if let Some(ref working_dir) = config.working_dir {
            opts = opts.working_dir(working_dir);
        }

        // Set user
        if let Some(ref user) = config.user {
            opts = opts.user(podman_api::opts::UserOpt::User(user.clone()));
        }

        let opts = opts.build();

        let exec = self
            .client
            .containers()
            .get(container.as_str())
            .create_exec(&opts)
            .await
            .map_err(map_exec_create_error)?;

        Ok(exec.id().to_string())
    }

    async fn exec_start(&self, exec_id: &str) -> Result<ExecResult, ExecError> {
        let start_opts = ExecStartOpts::builder().build();

        let exec_handle = self.client.execs().get(exec_id);
        let multiplexer = exec_handle
            .start(&start_opts)
            .await
            .map_err(map_exec_not_found_error)?;

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        if let Some(mut stream) = multiplexer {
            while let Some(result) = stream.next().await {
                match result {
                    Ok(chunk) => match chunk {
                        podman_api::conn::TtyChunk::StdOut(data) => stdout.extend(data),
                        podman_api::conn::TtyChunk::StdErr(data) => stderr.extend(data),
                        podman_api::conn::TtyChunk::StdIn(_) => {}
                    },
                    Err(e) => {
                        return Err(ExecError::Failed(e.to_string()));
                    }
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
        let details = self
            .client
            .execs()
            .get(exec_id)
            .inspect()
            .await
            .map_err(map_exec_not_found_error)?;

        // The inspect result is a JSON Value, so we need to extract fields carefully
        let running = details
            .get("Running")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let exit_code = details.get("ExitCode").and_then(|v| v.as_i64());
        let container_id = details
            .get("ContainerID")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        Ok(ExecInfo {
            id: exec_id.to_string(),
            running,
            exit_code,
            container_id: ContainerId::new(container_id),
        })
    }
}

#[async_trait]
impl LogOps for PodmanRuntime {
    async fn container_logs(
        &self,
        id: &ContainerId,
        opts: &LogOptions,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<LogLine, LogError>> + Send>>, LogError> {
        let mut log_opts = ContainerLogsOpts::builder()
            .stdout(opts.stdout)
            .stderr(opts.stderr)
            .follow(opts.follow)
            .timestamps(opts.timestamps);

        // Set tail option
        if let Some(n) = opts.tail {
            log_opts = log_opts.tail(n.to_string());
        }

        let log_opts = log_opts.build();

        // Clone client and container ID to avoid lifetime issues
        let client = self.client.clone();
        let container_id = id.to_string();

        // Create a stream that owns its dependencies using unfold
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
                        podman_api::conn::TtyChunk::StdOut(data) => (LogStream::Stdout, data),
                        podman_api::conn::TtyChunk::StdErr(data) => (LogStream::Stderr, data),
                        podman_api::conn::TtyChunk::StdIn(data) => (LogStream::Stdout, data),
                    };

                    LogLine {
                        content: String::from_utf8_lossy(&data).to_string(),
                        stream: stream_type,
                        timestamp: None, // Podman API embeds timestamp in content if requested
                    }
                })
                .map_err(|e| LogError::StreamError(e.to_string()))
        });

        Ok(Box::pin(stream))
    }
}

/// Quadlet unit file content.
#[derive(Debug, Clone)]
pub struct QuadletUnit {
    /// The systemd unit file content.
    pub content: String,
}

/// Extension trait for Podman-specific features.
#[async_trait]
pub trait PodmanExt {
    /// Generate a Quadlet unit file for a container.
    ///
    /// Quadlet is Podman's systemd integration that allows managing containers
    /// as systemd units.
    async fn generate_quadlet(
        &self,
        container: &ContainerId,
    ) -> Result<QuadletUnit, ContainerError>;
}

#[async_trait]
impl PodmanExt for PodmanRuntime {
    async fn generate_quadlet(
        &self,
        container: &ContainerId,
    ) -> Result<QuadletUnit, ContainerError> {
        // Use Podman's generate systemd command to create a unit file
        // The podman-api crate has generate_systemd_units which returns JSON with unit file content
        let opts = podman_api::opts::SystemdUnitsOpts::builder().build();
        let response = self
            .client
            .containers()
            .get(container.as_str())
            .generate_systemd_units(&opts)
            .await
            .map_err(|e| match e {
                podman_api::Error::Fault { code, message } if code == 404 => {
                    ContainerError::NotFound(message)
                }
                _ => ContainerError::Runtime(e.to_string()),
            })?;

        // The response is a JSON Value that contains unit name -> content mapping
        // Extract the first unit file content
        let content = if let Some(obj) = response.as_object() {
            obj.values()
                .next()
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string()
        } else {
            String::new()
        };

        Ok(QuadletUnit { content })
    }
}
