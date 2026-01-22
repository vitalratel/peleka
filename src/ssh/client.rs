// ABOUTME: SSH session management using russh.
// ABOUTME: Handles connection, authentication, and command execution.

use super::error::{Error, Result};
use russh::client::{self, Config, Handle};
use russh::keys::agent::client::AgentClient;
use russh::keys::known_hosts::{
    check_known_hosts, check_known_hosts_path, learn_known_hosts, learn_known_hosts_path,
};
use russh::keys::{PrivateKeyWithHashAlg, load_secret_key, ssh_key};
use russh::{ChannelMsg, Disconnect};
use std::path::PathBuf;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UnixStream;

/// Configuration for establishing an SSH session.
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// Remote host to connect to.
    pub host: String,
    /// SSH port (default: 22).
    pub port: u16,
    /// Username for authentication.
    pub user: String,
    /// Optional path to private key file.
    /// If None, will try SSH agent then default key locations.
    pub key_path: Option<PathBuf>,
    /// Whether to accept unknown hosts (Trust On First Use).
    /// If false, connection to unknown hosts will fail.
    pub trust_on_first_use: bool,
    /// Optional path to known_hosts file.
    /// If None, uses the default ~/.ssh/known_hosts.
    pub known_hosts_path: Option<PathBuf>,
    /// Timeout for command execution (default: 5 minutes).
    pub command_timeout: Duration,
}

impl SessionConfig {
    pub fn new(host: impl Into<String>, user: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            port: 22,
            user: user.into(),
            key_path: None,
            trust_on_first_use: false,
            known_hosts_path: None,
            command_timeout: Duration::from_secs(300), // 5 minutes
        }
    }

    pub fn port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    pub fn key_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.key_path = Some(path.into());
        self
    }

    pub fn trust_on_first_use(mut self, tofu: bool) -> Self {
        self.trust_on_first_use = tofu;
        self
    }

    pub fn known_hosts_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.known_hosts_path = Some(path.into());
        self
    }

    pub fn command_timeout(mut self, timeout: Duration) -> Self {
        self.command_timeout = timeout;
        self
    }
}

/// Output from a remote command execution.
#[derive(Debug, Clone)]
pub struct CommandOutput {
    /// Exit code of the command.
    pub exit_code: u32,
    /// Standard output.
    pub stdout: String,
    /// Standard error.
    pub stderr: String,
}

impl CommandOutput {
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }
}

/// SSH client handler for russh.
pub(crate) struct SshHandler {
    host: String,
    port: u16,
    trust_on_first_use: bool,
    known_hosts_path: Option<PathBuf>,
}

impl SshHandler {
    fn new(host: String, port: u16, trust_on_first_use: bool, known_hosts_path: Option<PathBuf>) -> Self {
        Self {
            host,
            port,
            trust_on_first_use,
            known_hosts_path,
        }
    }
}

impl client::Handler for SshHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &ssh_key::PublicKey,
    ) -> std::result::Result<bool, Self::Error> {
        let check_result = match &self.known_hosts_path {
            Some(path) => check_known_hosts_path(&self.host, self.port, server_public_key, path),
            None => check_known_hosts(&self.host, self.port, server_public_key),
        };

        match check_result {
            Ok(true) => Ok(true),
            Ok(false) => {
                // Host not in known_hosts
                if self.trust_on_first_use {
                    tracing::warn!(
                        "Trust-On-First-Use: accepting unknown host key for {}:{}",
                        self.host,
                        self.port
                    );
                    let learn_result = match &self.known_hosts_path {
                        Some(path) => {
                            learn_known_hosts_path(&self.host, self.port, server_public_key, path)
                        }
                        None => learn_known_hosts(&self.host, self.port, server_public_key),
                    };
                    if let Err(e) = learn_result {
                        tracing::warn!("Failed to save host key to known_hosts: {}", e);
                    }
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Err(russh::keys::Error::KeyChanged { .. }) => Ok(false),
            Err(_) => {
                // Other errors - treat as unknown host
                if self.trust_on_first_use {
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
        }
    }
}

/// Authentication method resolved from config.
enum AuthMethod {
    Agent(AgentClient<UnixStream>),
    KeyFile(Arc<ssh_key::PrivateKey>),
}

/// An established SSH session.
pub struct Session {
    config: SessionConfig,
    handle: Arc<Handle<SshHandler>>,
    /// Active socket forwarders.
    forwarders: Mutex<Vec<super::forward::ForwardHandle>>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("config", &self.config)
            .field("handle", &"<russh::Handle>")
            .finish()
    }
}

impl Session {
    /// Connect to the remote host.
    pub async fn connect(config: SessionConfig) -> Result<Self> {
        // Resolve authentication method
        let auth_method = Self::resolve_auth_method(&config).await?;

        // Configure client
        let russh_config = Config {
            inactivity_timeout: Some(Duration::from_secs(30)),
            ..Default::default()
        };

        let handler = SshHandler::new(
            config.host.clone(),
            config.port,
            config.trust_on_first_use,
            config.known_hosts_path.clone(),
        );

        // Connect
        let mut session = client::connect(
            Arc::new(russh_config),
            (config.host.as_str(), config.port),
            handler,
        )
        .await
        .map_err(|e| {
            if e.to_string().contains("Connection refused") {
                Error::Connection(format!(
                    "connection refused to {}:{}",
                    config.host, config.port
                ))
            } else {
                Error::Connection(e.to_string())
            }
        })?;

        // Authenticate
        let auth_success = Self::authenticate(&mut session, &config, auth_method).await?;
        if !auth_success {
            return Err(Error::AuthenticationFailed);
        }

        Ok(Self {
            config,
            handle: Arc::new(session),
            forwarders: Mutex::new(Vec::new()),
        })
    }

    /// Resolve which authentication method to use.
    async fn resolve_auth_method(config: &SessionConfig) -> Result<AuthMethod> {
        // If key path specified, use that
        if let Some(key_path) = &config.key_path {
            let key = load_secret_key(key_path, None).map_err(|e| Error::KeyLoadFailed {
                path: key_path.clone(),
                reason: e.to_string(),
            })?;
            return Ok(AuthMethod::KeyFile(Arc::new(key)));
        }

        // Try SSH agent
        if let Ok(agent) = AgentClient::connect_env().await {
            return Ok(AuthMethod::Agent(agent));
        }

        // Fall back to default key locations
        let home = std::env::var("HOME").map_err(|_| {
            Error::AgentUnavailable("SSH agent not available and HOME not set".to_string())
        })?;

        let default_keys = [
            format!("{}/.ssh/id_ed25519", home),
            format!("{}/.ssh/id_rsa", home),
            format!("{}/.ssh/id_ecdsa", home),
        ];

        for key_path in &default_keys {
            if let Ok(key) = load_secret_key(key_path, None) {
                return Ok(AuthMethod::KeyFile(Arc::new(key)));
            }
        }

        Err(Error::AgentUnavailable(
            "SSH agent not available and no default keys found".to_string(),
        ))
    }

    /// Authenticate the session.
    async fn authenticate(
        session: &mut Handle<SshHandler>,
        config: &SessionConfig,
        auth_method: AuthMethod,
    ) -> Result<bool> {
        match auth_method {
            AuthMethod::Agent(mut agent) => {
                let keys = agent.request_identities().await.map_err(|e| {
                    Error::AgentUnavailable(format!("failed to list agent keys: {}", e))
                })?;

                if keys.is_empty() {
                    return Err(Error::AgentUnavailable("no keys in SSH agent".to_string()));
                }

                for key in &keys {
                    match session
                        .authenticate_publickey_with(&config.user, key.clone(), None, &mut agent)
                        .await
                    {
                        Ok(result) if result.success() => return Ok(true),
                        _ => continue,
                    }
                }
                Ok(false)
            }
            AuthMethod::KeyFile(key) => {
                let hash_alg = session
                    .best_supported_rsa_hash()
                    .await
                    .map_err(Error::Protocol)?
                    .flatten();

                let result = session
                    .authenticate_publickey(&config.user, PrivateKeyWithHashAlg::new(key, hash_alg))
                    .await
                    .map_err(Error::Protocol)?;

                Ok(result.success())
            }
        }
    }

    /// Check if a file or socket exists on the remote host.
    pub async fn file_exists(&self, path: &str) -> Result<bool> {
        let output = self
            .exec(&format!("test -e {} && echo exists", path))
            .await?;
        Ok(output.success() && output.stdout.trim() == "exists")
    }

    /// Execute a command on the remote host.
    pub async fn exec(&self, command: &str) -> Result<CommandOutput> {
        self.exec_with_timeout(command, self.config.command_timeout)
            .await
    }

    /// Execute a command with a custom timeout.
    pub async fn exec_with_timeout(
        &self,
        command: &str,
        timeout: Duration,
    ) -> Result<CommandOutput> {
        match tokio::time::timeout(timeout, self.exec_inner(command)).await {
            Ok(result) => result,
            Err(_) => Err(Error::CommandTimeout(timeout)),
        }
    }

    async fn exec_inner(&self, command: &str) -> Result<CommandOutput> {
        let mut channel = self
            .handle
            .channel_open_session()
            .await
            .map_err(|e| Error::CommandFailed(format!("failed to open channel: {}", e)))?;

        channel
            .exec(true, command)
            .await
            .map_err(|e| Error::CommandFailed(format!("failed to exec command: {}", e)))?;

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut exit_code = 0u32;

        let mut got_exit_status = false;
        let mut got_eof = false;

        loop {
            match channel.wait().await {
                Some(ChannelMsg::Data { data }) => {
                    stdout.extend_from_slice(&data);
                }
                Some(ChannelMsg::ExtendedData { data, ext }) => {
                    if ext == 1 {
                        // stderr
                        stderr.extend_from_slice(&data);
                    }
                }
                Some(ChannelMsg::ExitStatus { exit_status }) => {
                    exit_code = exit_status;
                    got_exit_status = true;
                    // If we already got EOF, we can exit now
                    if got_eof {
                        break;
                    }
                }
                Some(ChannelMsg::Eof) => {
                    got_eof = true;
                    // If we already got exit status, we can exit now
                    if got_exit_status {
                        break;
                    }
                }
                Some(ChannelMsg::Close) => {
                    break;
                }
                Some(_) => {}
                None => break,
            }
        }

        // If the channel closed without providing an exit status, this indicates
        // an abnormal termination (e.g., connection timeout, network issue)
        if !got_exit_status {
            return Err(Error::ChannelClosed);
        }

        Ok(CommandOutput {
            exit_code,
            stdout: String::from_utf8_lossy(&stdout).to_string(),
            stderr: String::from_utf8_lossy(&stderr).to_string(),
        })
    }

    /// Forward a local Unix socket to a remote Unix socket.
    ///
    /// Creates a local socket that tunnels all connections through SSH to
    /// the specified remote socket path. Returns the path to the local socket.
    pub async fn forward_socket(&self, remote_socket: &str) -> Result<String> {
        let forward_handle =
            super::forward::start_forward(Arc::clone(&self.handle), remote_socket.to_string())
                .await?;
        let path = forward_handle
            .path()
            .ok_or_else(|| Error::SocketForwardFailed("socket path is not valid UTF-8".to_string()))?
            .to_string();
        self.forwarders.lock().push(forward_handle);
        Ok(path)
    }

    /// Disconnect the session.
    pub async fn disconnect(self) -> Result<()> {
        // Stop all forwarders first (drain to Vec to release lock before await)
        let forwarders: Vec<_> = self.forwarders.lock().drain(..).collect();
        for forwarder in forwarders {
            forwarder.stop().await;
        }

        self.handle
            .disconnect(Disconnect::ByApplication, "", "en")
            .await
            .map_err(Error::Protocol)?;
        Ok(())
    }
}
