// ABOUTME: Podman container helper for integration tests.
// ABOUTME: Provides SSH access to a container running Podman daemon.

use bollard::Docker;
use bollard::models::ContainerCreateBody;
use bollard::query_parameters::{
    BuildImageOptions, CreateContainerOptions, RemoveContainerOptions, StopContainerOptions,
};
use bytes::Bytes;
use futures::StreamExt;
use http_body_util::{Either, Full};
use peleka::ssh::SessionConfig;
use std::collections::HashMap;
use std::sync::OnceLock;
use tokio::sync::OnceCell;

const SSH_PORT: u16 = 22;
const TEST_USER: &str = "testuser";
const IMAGE_NAME: &str = "localhost/peleka-podman-ssh:test";

/// Container info needed for cleanup.
struct ContainerInfo {
    container_id: String,
    known_hosts_path: std::path::PathBuf,
}

/// Shared container info for cleanup.
static CONTAINER_INFO: OnceLock<ContainerInfo> = OnceLock::new();

/// Cleanup on process exit.
#[ctor::dtor]
fn cleanup_on_exit() {
    let Some(info) = CONTAINER_INFO.get() else {
        return;
    };

    // Clean up temp known_hosts file
    let _ = std::fs::remove_file(&info.known_hosts_path);

    let Ok(rt) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return;
    };
    rt.block_on(async {
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
            .unwrap_or_else(|_| format!("/run/user/{}", std::process::id()));
        let socket_path = format!("{}/podman/podman.sock", runtime_dir);
        if let Ok(docker) = Docker::connect_with_socket(&socket_path, 120, bollard::API_DEFAULT_VERSION) {
            let _ = docker
                .stop_container(&info.container_id, None::<StopContainerOptions>)
                .await;
            let _ = docker
                .remove_container(
                    &info.container_id,
                    Some(RemoveContainerOptions {
                        force: true,
                        ..Default::default()
                    }),
                )
                .await;
        }
    });
}

/// Shared Podman container for all tests.
static SHARED_CONTAINER: OnceCell<PodmanContainer> = OnceCell::const_new();

/// Get the shared Podman container, starting it if needed.
pub async fn shared_podman_container() -> &'static PodmanContainer {
    SHARED_CONTAINER
        .get_or_init(|| async {
            PodmanContainer::start()
                .await
                .expect("failed to start Podman container")
        })
        .await
}

/// Running Podman container with SSH connection details.
pub struct PodmanContainer {
    port: u16,
    known_hosts_path: std::path::PathBuf,
}

impl PodmanContainer {
    /// Get path to rootless podman socket.
    fn podman_socket_path() -> String {
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
            .unwrap_or_else(|_| format!("/run/user/{}", std::process::id()));
        format!("{}/podman/podman.sock", runtime_dir)
    }

    async fn start() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Connect to rootless podman socket
        let socket_path = Self::podman_socket_path();
        let docker = Docker::connect_with_socket(&socket_path, 120, bollard::API_DEFAULT_VERSION)?;

        // Build the Podman image
        Self::build_image(&docker).await?;

        // Read public key
        let key_path = super::test_key_path();
        let public_key = std::fs::read_to_string(format!("{}.pub", key_path))?;

        // Find an available port
        let port = Self::find_available_port().await?;

        // Create container
        let container_name = format!("peleka-podman-test-{}", std::process::id());

        let mut port_bindings = HashMap::new();
        port_bindings.insert(
            format!("{}/tcp", SSH_PORT),
            Some(vec![bollard::models::PortBinding {
                host_ip: Some("127.0.0.1".to_string()),
                host_port: Some(port.to_string()),
            }]),
        );

        let host_config = bollard::models::HostConfig {
            port_bindings: Some(port_bindings),
            privileged: Some(true), // Required for Podman-in-container
            // Allow container to reach host via host.containers.internal
            extra_hosts: Some(vec!["host.containers.internal:host-gateway".to_string()]),
            ..Default::default()
        };

        let mut env = Vec::new();
        env.push(format!("AUTHORIZED_KEY={}", public_key.trim()));

        // Pass host's external IP for NAT redirect in container
        if let Some(host_ip) = Self::get_host_external_ip() {
            env.push(format!("GITEA_HOST_IP={}", host_ip));
        }

        let config = ContainerCreateBody {
            image: Some(IMAGE_NAME.to_string()),
            env: Some(env),
            host_config: Some(host_config),
            ..Default::default()
        };

        let container = docker
            .create_container(
                Some(CreateContainerOptions {
                    name: Some(container_name),
                    ..Default::default()
                }),
                config,
            )
            .await?;

        // Create temp known_hosts file path for test isolation
        let known_hosts_path =
            std::env::temp_dir().join(format!("peleka-podman-test-{}-known_hosts", std::process::id()));

        // Store container ID and known_hosts_path for cleanup
        let _ = CONTAINER_INFO.set(ContainerInfo {
            container_id: container.id.clone(),
            known_hosts_path: known_hosts_path.clone(),
        });

        // Start container
        docker
            .start_container(
                &container.id,
                None::<bollard::query_parameters::StartContainerOptions>,
            )
            .await?;

        // Wait for SSH and Podman to be ready
        Self::wait_for_ready(port).await?;

        Ok(Self {
            port,
            known_hosts_path,
        })
    }

    /// Build the Podman+SSH image if not present.
    async fn build_image(docker: &Docker) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Check if image exists
        if docker.inspect_image(IMAGE_NAME).await.is_ok() {
            return Ok(());
        }

        eprintln!("Building Podman+SSH image (this may take a minute)...");

        let dockerfile_dir = format!("{}/tests/fixtures/podman-ssh", env!("CARGO_MANIFEST_DIR"));

        // Create tar archive of build context
        let tar_data = super::create_build_context(&dockerfile_dir)?;

        let options = BuildImageOptions {
            dockerfile: "Dockerfile".to_string(),
            t: Some(IMAGE_NAME.to_string()),
            ..Default::default()
        };

        let body = Either::Left(Full::new(Bytes::from(tar_data)));
        let mut build_stream = docker.build_image(options, None, Some(body));

        while let Some(result) = build_stream.next().await {
            match result {
                Ok(output) => {
                    if let Some(error_detail) = output.error_detail {
                        return Err(format!("Build error: {:?}", error_detail).into());
                    }
                }
                Err(e) => return Err(e.into()),
            }
        }

        eprintln!("Podman+SSH image built successfully");
        Ok(())
    }

    /// Get SessionConfig for connecting to this container.
    pub fn session_config(&self) -> SessionConfig {
        SessionConfig::new("127.0.0.1", TEST_USER)
            .port(self.port)
            .key_path(super::test_key_path())
            .trust_on_first_use(true)
            .known_hosts_path(&self.known_hosts_path)
    }

    /// Get the host's external IP address (first non-loopback IPv4).
    fn get_host_external_ip() -> Option<String> {
        use std::process::Command;

        // Use ip command to get the first global scope IPv4 address
        let output = Command::new("ip")
            .args(["-4", "addr", "show", "scope", "global"])
            .output()
            .ok()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let line = line.trim();
            if line.starts_with("inet ") {
                // Parse "inet 192.168.0.106/24 ..."
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let ip_cidr = parts[1];
                    let ip = ip_cidr.split('/').next()?;
                    return Some(ip.to_string());
                }
            }
        }
        None
    }

    async fn find_available_port() -> Result<u16, Box<dyn std::error::Error + Send + Sync>> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        drop(listener);
        Ok(port)
    }

    async fn wait_for_ready(port: u16) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use tokio::io::AsyncReadExt;

        let addr = format!("127.0.0.1:{}", port);

        // Wait for SSH to be ready (up to 90 seconds for Podman image pull + startup)
        for _ in 0..180 {
            if let Ok(mut stream) = tokio::net::TcpStream::connect(&addr).await {
                let mut buf = [0u8; 32];
                match tokio::time::timeout(std::time::Duration::from_secs(2), stream.read(&mut buf))
                    .await
                {
                    Ok(Ok(n)) if n > 0 => {
                        let banner = String::from_utf8_lossy(&buf[..n]);
                        if banner.starts_with("SSH-") {
                            // SSH is ready, give Podman extra time to initialize
                            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                            return Ok(());
                        }
                    }
                    _ => {}
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        Err("Podman container did not become ready in time".into())
    }
}
