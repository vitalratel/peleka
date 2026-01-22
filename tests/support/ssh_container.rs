// ABOUTME: SSH container helper for integration tests.
// ABOUTME: Builds from local Dockerfile using Gitea registry to avoid Docker Hub rate limits.

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

const IMAGE_NAME: &str = "peleka-ssh-only:test";
const SSH_PORT: u16 = 22;
const TEST_USER: &str = "testuser";

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
        if let Ok(docker) = Docker::connect_with_local_defaults() {
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

/// Shared SSH container for all tests.
static SHARED_CONTAINER: tokio::sync::OnceCell<SshContainer> = tokio::sync::OnceCell::const_new();

/// Get the shared SSH container, starting it if needed.
pub async fn shared_container() -> &'static SshContainer {
    SHARED_CONTAINER
        .get_or_init(|| async {
            SshContainer::start()
                .await
                .expect("failed to start SSH container")
        })
        .await
}

/// Running SSH container with connection details.
pub struct SshContainer {
    port: u16,
    known_hosts_path: std::path::PathBuf,
}

impl SshContainer {
    async fn start() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let docker = Docker::connect_with_local_defaults()?;

        // Build the SSH image
        Self::build_image(&docker).await?;

        // Read public key
        let key_path = super::test_key_path();
        let public_key = std::fs::read_to_string(format!("{}.pub", key_path))?;

        // Find an available port
        let port = Self::find_available_port().await?;

        // Create container
        let container_name = format!("peleka-ssh-test-{}", std::process::id());
        let mut env = Vec::new();
        env.push(format!("AUTHORIZED_KEY={}", public_key.trim()));

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
            ..Default::default()
        };

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
            std::env::temp_dir().join(format!("peleka-test-{}-known_hosts", std::process::id()));

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

        // Wait for SSH to be ready
        Self::wait_for_ssh(port).await?;

        Ok(Self {
            port,
            known_hosts_path,
        })
    }

    /// Get SessionConfig for connecting to this container.
    pub fn session_config(&self) -> SessionConfig {
        SessionConfig::new("127.0.0.1", TEST_USER)
            .port(self.port)
            .key_path(super::test_key_path())
            .trust_on_first_use(true)
            .known_hosts_path(&self.known_hosts_path)
    }

    async fn find_available_port() -> Result<u16, Box<dyn std::error::Error + Send + Sync>> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        drop(listener);
        Ok(port)
    }

    async fn wait_for_ssh(port: u16) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use tokio::io::AsyncReadExt;

        let addr = format!("127.0.0.1:{}", port);
        for _ in 0..60 {
            if let Ok(mut stream) = tokio::net::TcpStream::connect(&addr).await {
                // Try to read SSH banner to confirm SSH is actually ready
                let mut buf = [0u8; 32];
                match tokio::time::timeout(std::time::Duration::from_secs(2), stream.read(&mut buf))
                    .await
                {
                    Ok(Ok(n)) if n > 0 => {
                        let banner = String::from_utf8_lossy(&buf[..n]);
                        if banner.starts_with("SSH-") {
                            // SSH is ready, give it a moment to stabilize
                            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                            return Ok(());
                        }
                    }
                    _ => {}
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        Err("SSH container did not become ready in time".into())
    }

    /// Build the SSH image if not present.
    async fn build_image(docker: &Docker) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Check if image exists
        if docker.inspect_image(IMAGE_NAME).await.is_ok() {
            return Ok(());
        }

        eprintln!("Building SSH-only image...");

        let dockerfile_dir = format!("{}/tests/fixtures/ssh-only", env!("CARGO_MANIFEST_DIR"));

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

        eprintln!("SSH-only image built successfully");
        Ok(())
    }
}
