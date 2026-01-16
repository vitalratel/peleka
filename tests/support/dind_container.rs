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
const IMAGE_NAME: &str = "peleka-dind-ssh:test";

/// Container info needed for cleanup.
struct ContainerInfo {
    container_id: String,
}

/// Shared container info for cleanup.
static CONTAINER_INFO: OnceLock<ContainerInfo> = OnceLock::new();

/// Cleanup on process exit.
#[ctor::dtor]
fn cleanup_on_exit() {
    let Some(info) = CONTAINER_INFO.get() else {
        return;
    };
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

/// Shared DinD container for all tests.
static SHARED_CONTAINER: OnceCell<DindContainer> = OnceCell::const_new();

/// Get the shared DinD container, starting it if needed.
pub async fn shared_dind_container() -> &'static DindContainer {
    SHARED_CONTAINER
        .get_or_init(|| async {
            DindContainer::start()
                .await
                .expect("failed to start DinD container")
        })
        .await
}

/// Running DinD container with SSH connection details.
pub struct DindContainer {
    port: u16,
}

impl DindContainer {
    async fn start() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let docker = Docker::connect_with_local_defaults()?;

        // Build the DinD image
        Self::build_image(&docker).await?;

        // Read public key
        let key_path = Self::test_key_path();
        let public_key = std::fs::read_to_string(format!("{}.pub", key_path))?;

        // Find an available port
        let port = Self::find_available_port().await?;

        // Create container
        let container_name = format!("peleka-dind-test-{}", std::process::id());

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
            privileged: Some(true), // Required for DinD
            ..Default::default()
        };

        let mut env = Vec::new();
        env.push(format!("AUTHORIZED_KEY={}", public_key.trim()));

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

        // Store container ID for cleanup
        let _ = CONTAINER_INFO.set(ContainerInfo {
            container_id: container.id.clone(),
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

        Ok(Self { port })
    }

    /// Build the DinD+SSH image if not present.
    async fn build_image(docker: &Docker) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Check if image exists
        if docker.inspect_image(IMAGE_NAME).await.is_ok() {
            return Ok(());
        }

        eprintln!("Building DinD+SSH image (this may take a minute)...");

        let dockerfile_dir = format!("{}/tests/fixtures/dind-ssh", env!("CARGO_MANIFEST_DIR"));

        // Create tar archive of build context
        let tar_data = Self::create_build_context(&dockerfile_dir)?;

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

        eprintln!("DinD+SSH image built successfully");
        Ok(())
    }

    /// Create a tar archive of the build context.
    fn create_build_context(
        dir: &str,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        let mut ar = tar::Builder::new(Vec::new());

        // Add Dockerfile
        let dockerfile_path = format!("{}/Dockerfile", dir);
        let dockerfile_content = std::fs::read(&dockerfile_path)?;
        let mut header = tar::Header::new_gnu();
        header.set_path("Dockerfile")?;
        header.set_size(dockerfile_content.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        ar.append(&header, dockerfile_content.as_slice())?;

        // Add entrypoint.sh
        let entrypoint_path = format!("{}/entrypoint.sh", dir);
        let entrypoint_content = std::fs::read(&entrypoint_path)?;
        let mut header = tar::Header::new_gnu();
        header.set_path("entrypoint.sh")?;
        header.set_size(entrypoint_content.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        ar.append(&header, entrypoint_content.as_slice())?;

        ar.into_inner().map_err(Into::into)
    }

    /// Get SessionConfig for connecting to this container.
    pub fn session_config(&self) -> SessionConfig {
        SessionConfig::new("127.0.0.1", TEST_USER)
            .port(self.port)
            .key_path(Self::test_key_path())
            .trust_on_first_use(true)
    }

    fn test_key_path() -> String {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        format!("{}/tests/fixtures/test_key", manifest_dir)
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

        // Wait for SSH to be ready (up to 60 seconds for DinD startup)
        for _ in 0..120 {
            if let Ok(mut stream) = tokio::net::TcpStream::connect(&addr).await {
                let mut buf = [0u8; 32];
                match tokio::time::timeout(std::time::Duration::from_secs(2), stream.read(&mut buf))
                    .await
                {
                    Ok(Ok(n)) if n > 0 => {
                        let banner = String::from_utf8_lossy(&buf[..n]);
                        if banner.starts_with("SSH-") {
                            // SSH is ready, now wait for Docker daemon to be ready
                            // by actually connecting and checking via SSH
                            if Self::wait_for_docker_ready(port).await.is_ok() {
                                return Ok(());
                            }
                        }
                    }
                    _ => {}
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        Err("DinD container did not become ready in time".into())
    }

    /// Wait for Docker daemon to be ready inside the DinD container via SSH.
    async fn wait_for_docker_ready(
        port: u16,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let config = SessionConfig::new("127.0.0.1", TEST_USER)
            .port(port)
            .key_path(Self::test_key_path())
            .trust_on_first_use(true);

        // Try connecting and checking Docker status for up to 30 seconds
        for _ in 0..30 {
            if let Ok(session) = peleka::ssh::Session::connect(config.clone()).await {
                // Try to run 'docker info' to check if daemon is ready
                if let Ok(result) = session.exec("docker info").await {
                    if result.exit_code == 0 {
                        let _ = session.disconnect().await;
                        return Ok(());
                    }
                }
                let _ = session.disconnect().await;
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
        Err("Docker daemon did not become ready in time".into())
    }
}
