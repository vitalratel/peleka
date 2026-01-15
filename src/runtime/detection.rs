// ABOUTME: Runtime detection logic for local and remote systems.
// ABOUTME: Checks for Podman sockets first, then Docker.

use super::types::{RuntimeConfig, RuntimeInfo, RuntimeType};
use crate::ssh::Session;
use std::path::Path;

/// Error during runtime detection.
#[derive(Debug, thiserror::Error)]
pub enum DetectionError {
    #[error("no container runtime found (checked Podman and Docker sockets)")]
    NoRuntimeFound,

    #[error("SSH error: {0}")]
    Ssh(#[from] crate::ssh::Error),
}

/// Detect container runtime on the local system.
///
/// Detection order:
/// 1. Rootless Podman socket (`/run/user/$UID/podman/podman.sock`)
/// 2. Rootful Podman socket (`/run/podman/podman.sock`)
/// 3. Docker socket (`/var/run/docker.sock`)
pub fn detect_local() -> Result<RuntimeInfo, DetectionError> {
    // 1. Rootless Podman
    if let Some(uid) = get_uid() {
        let rootless_socket = format!("/run/user/{}/podman/podman.sock", uid);
        if Path::new(&rootless_socket).exists() {
            return Ok(RuntimeInfo {
                runtime_type: RuntimeType::Podman,
                socket_path: rootless_socket,
            });
        }
    }

    // 2. Rootful Podman
    if Path::new(ROOTFUL_PODMAN).exists() {
        return Ok(RuntimeInfo {
            runtime_type: RuntimeType::Podman,
            socket_path: ROOTFUL_PODMAN.to_string(),
        });
    }

    // 3. Docker
    if Path::new(DOCKER_SOCKET).exists() {
        return Ok(RuntimeInfo {
            runtime_type: RuntimeType::Docker,
            socket_path: DOCKER_SOCKET.to_string(),
        });
    }

    Err(DetectionError::NoRuntimeFound)
}

fn get_uid() -> Option<String> {
    std::env::var("UID").ok().or_else(|| {
        // Fall back to reading /proc/self/status
        std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|s| {
                s.lines()
                    .find(|l| l.starts_with("Uid:"))
                    .and_then(|l| l.split_whitespace().nth(1))
                    .map(|s| s.to_string())
            })
    })
}

const ROOTFUL_PODMAN: &str = "/run/podman/podman.sock";
const DOCKER_SOCKET: &str = "/var/run/docker.sock";

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
    if session.file_exists(ROOTFUL_PODMAN).await? {
        return Ok(RuntimeInfo {
            runtime_type: RuntimeType::Podman,
            socket_path: ROOTFUL_PODMAN.to_string(),
        });
    }

    // 3. Docker
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
        RuntimeType::Docker => DOCKER_SOCKET.to_string(),
        RuntimeType::Podman => ROOTFUL_PODMAN.to_string(),
    }
}
