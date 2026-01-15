// ABOUTME: SSH container helper for integration tests.
// ABOUTME: Uses bollard to manage a shared SSH server container.

use bollard::Docker;
use bollard::models::ContainerCreateBody;
use bollard::query_parameters::{
    CreateContainerOptions, CreateImageOptions, RemoveContainerOptions, StopContainerOptions,
};
use futures::StreamExt;
use peleka::ssh::SessionConfig;
use std::collections::HashMap;
use std::sync::OnceLock;

const IMAGE: &str = "lscr.io/linuxserver/openssh-server:latest";
const SSH_PORT: u16 = 2222;
const TEST_USER: &str = "testuser";

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
}

impl SshContainer {
    async fn start() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let docker = Docker::connect_with_local_defaults()?;

        // Read public key
        let key_path = Self::test_key_path();
        let public_key = std::fs::read_to_string(format!("{}.pub", key_path))?;

        // Pull image if needed
        let mut pull_stream = docker.create_image(
            Some(CreateImageOptions {
                from_image: Some(IMAGE.to_string()),
                ..Default::default()
            }),
            None,
            None,
        );
        while let Some(result) = pull_stream.next().await {
            result?;
        }

        // Find an available port
        let port = Self::find_available_port().await?;

        // Create container
        let container_name = format!("peleka-ssh-test-{}", std::process::id());
        let mut env = Vec::new();
        env.push("PUID=1000".to_string());
        env.push("PGID=1000".to_string());
        env.push(format!("USER_NAME={}", TEST_USER));
        env.push(format!("PUBLIC_KEY={}", public_key.trim()));

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
            image: Some(IMAGE.to_string()),
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

        // Wait for SSH to be ready
        Self::wait_for_ssh(port).await?;

        Ok(Self { port })
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
}
