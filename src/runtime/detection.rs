// ABOUTME: Runtime detection logic over SSH.
// ABOUTME: Checks for Podman sockets first, then Docker.

use super::types::{RuntimeConfig, RuntimeInfo, RuntimeType};
use crate::ssh::Session;

/// Error during runtime detection.
#[derive(Debug, thiserror::Error)]
pub enum DetectionError {
    #[error("no container runtime found (checked Podman and Docker sockets)")]
    NoRuntimeFound,

    #[error("SSH error: {0}")]
    Ssh(#[from] crate::ssh::Error),
}

/// Detect the container runtime on the remote server.
///
/// Detection order (when not explicitly configured):
/// 1. Rootless Podman socket (`/run/user/$UID/podman/podman.sock`)
/// 2. Rootful Podman socket (`/run/podman/podman.sock`)
/// 3. Docker socket (`/var/run/docker.sock`)
///
/// If `config` is provided with explicit values, those take precedence.
pub async fn detect_runtime(
    session: &Session,
    config: Option<&RuntimeConfig>,
) -> Result<RuntimeInfo, DetectionError> {
    // Check for explicit override
    if let Some(cfg) = config
        && let Some(runtime_type) = cfg.runtime
    {
        let socket_path = cfg
            .socket
            .clone()
            .unwrap_or_else(|| default_socket_path(runtime_type));
        return Ok(RuntimeInfo {
            runtime_type,
            socket_path,
        });
    }

    // Auto-detect: check sockets in order

    // 1. Rootless Podman
    let uid_output = session.exec("id -u").await?;
    if uid_output.success() {
        let uid = uid_output.stdout.trim();
        let rootless_socket = format!("/run/user/{}/podman/podman.sock", uid);
        if session.file_exists(&rootless_socket).await? {
            return Ok(RuntimeInfo {
                runtime_type: RuntimeType::Podman,
                socket_path: rootless_socket,
            });
        }
    }

    // 2. Rootful Podman
    const ROOTFUL_PODMAN: &str = "/run/podman/podman.sock";
    if session.file_exists(ROOTFUL_PODMAN).await? {
        return Ok(RuntimeInfo {
            runtime_type: RuntimeType::Podman,
            socket_path: ROOTFUL_PODMAN.to_string(),
        });
    }

    // 3. Docker
    const DOCKER_SOCKET: &str = "/var/run/docker.sock";
    if session.file_exists(DOCKER_SOCKET).await? {
        return Ok(RuntimeInfo {
            runtime_type: RuntimeType::Docker,
            socket_path: DOCKER_SOCKET.to_string(),
        });
    }

    Err(DetectionError::NoRuntimeFound)
}

fn default_socket_path(runtime: RuntimeType) -> String {
    match runtime {
        RuntimeType::Docker => "/var/run/docker.sock".to_string(),
        RuntimeType::Podman => "/run/podman/podman.sock".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_docker_socket() {
        assert_eq!(
            default_socket_path(RuntimeType::Docker),
            "/var/run/docker.sock"
        );
    }

    #[test]
    fn default_podman_socket() {
        assert_eq!(
            default_socket_path(RuntimeType::Podman),
            "/run/podman/podman.sock"
        );
    }
}
