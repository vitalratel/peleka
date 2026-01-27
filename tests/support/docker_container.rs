// ABOUTME: Docker-in-Docker container helper for integration tests.
// ABOUTME: Provides SSH access to a container running Docker daemon.

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
const IMAGE_NAME: &str = "peleka-docker-ssh:test";

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

/// Shared Docker container for all tests.
static SHARED_CONTAINER: OnceCell<DockerContainer> = OnceCell::const_new();

/// Get the shared Docker container, starting it if needed.
pub async fn shared_docker_container() -> &'static DockerContainer {
    SHARED_CONTAINER
        .get_or_init(|| async {
            DockerContainer::start()
                .await
                .expect("failed to start Docker container")
        })
        .await
}

/// Running Docker-in-Docker container with SSH connection details.
pub struct DockerContainer {
    port: u16,
    known_hosts_path: std::path::PathBuf,
}

impl DockerContainer {
    async fn start() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let docker = Docker::connect_with_local_defaults()?;

        // Build the Docker image
        Self::build_image(&docker).await?;

        // Read public key
        let key_path = super::test_key_path();
        let public_key = std::fs::read_to_string(format!("{}.pub", key_path))?;

        // Find an available port
        let port = Self::find_available_port().await?;

        // Create container
        let container_name = format!("peleka-docker-test-{}", std::process::id());

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
            privileged: Some(true), // Required for Docker-in-Docker
            ..Default::default()
        };

        let env = vec![format!("AUTHORIZED_KEY={}", public_key.trim())];

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
        let known_hosts_path = std::env::temp_dir().join(format!(
            "peleka-docker-test-{}-known_hosts",
            std::process::id()
        ));

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

        // Wait for SSH and Docker to be ready
        Self::wait_for_ready(port).await?;

        Ok(Self {
            port,
            known_hosts_path,
        })
    }

    /// Build the Docker+SSH image if not present.
    async fn build_image(docker: &Docker) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Check if image exists
        if docker.inspect_image(IMAGE_NAME).await.is_ok() {
            return Ok(());
        }

        eprintln!("Building Docker+SSH image (this may take a minute)...");

        let dockerfile_dir = format!("{}/tests/fixtures/docker-ssh", env!("CARGO_MANIFEST_DIR"));

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

        eprintln!("Docker+SSH image built successfully");
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

    async fn find_available_port() -> Result<u16, Box<dyn std::error::Error + Send + Sync>> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        drop(listener);
        Ok(port)
    }

    async fn wait_for_ready(port: u16) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use tokio::io::AsyncReadExt;

        let addr = format!("127.0.0.1:{}", port);

        // Wait for SSH to be ready (up to 90 seconds for Docker startup)
        for _ in 0..180 {
            if let Ok(mut stream) = tokio::net::TcpStream::connect(&addr).await {
                let mut buf = [0u8; 32];
                match tokio::time::timeout(std::time::Duration::from_secs(2), stream.read(&mut buf))
                    .await
                {
                    Ok(Ok(n)) if n > 0 => {
                        let banner = String::from_utf8_lossy(&buf[..n]);
                        if banner.starts_with("SSH-") {
                            // SSH is ready, give Docker extra time to initialize
                            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                            return Ok(());
                        }
                    }
                    _ => {}
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        Err("Docker container did not become ready in time".into())
    }
}
