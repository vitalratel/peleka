// ABOUTME: Test support utilities.
// ABOUTME: Provides container helpers for integration tests.

use std::sync::Once;

// Each test binary only uses some of these modules, so allow dead_code.
#[allow(dead_code)]
pub mod podman_container;
#[allow(dead_code)]
pub mod ssh_container;

/// Test image registry. Change this single constant to switch all tests.
pub const TEST_IMAGE: &str = "host.containers.internal:3000/vitalratel/alpine:3.19";

/// Create a test deployment config with sensible defaults.
///
/// The returned config has:
/// - Service name from the parameter
/// - Test image from TEST_IMAGE constant
/// - Command that keeps container running (`sleep infinity`)
#[allow(dead_code)]
pub fn test_config(service_name: &str) -> peleka::config::Config {
    let mut config = peleka::config::Config::template();
    config.service = peleka::types::ServiceName::new(service_name).unwrap();
    config.image = peleka::types::ImageRef::parse(TEST_IMAGE).unwrap();
    config.command = Some(vec![
        "sh".to_string(),
        "-c".to_string(),
        "sleep infinity".to_string(),
    ]);
    config
}

/// Detect the Podman socket path on a remote host via SSH session.
/// Checks for rootful socket first, then falls back to rootless.
#[allow(dead_code)]
pub async fn detect_podman_socket(session: &mut peleka::ssh::Session) -> String {
    let rootful_socket = "/run/podman/podman.sock";
    let check_result = session
        .exec(&format!("test -S {} && echo exists", rootful_socket))
        .await;

    if check_result
        .map(|r| r.stdout.contains("exists"))
        .unwrap_or(false)
    {
        rootful_socket.to_string()
    } else {
        let uid_output = session.exec("id -u").await.expect("should get uid");
        let uid = uid_output.stdout.trim();
        format!("/run/user/{}/podman/podman.sock", uid)
    }
}

static TRACING_INIT: Once = Once::new();

/// Initialize tracing for tests. Safe to call multiple times.
#[allow(dead_code)]
pub fn init_tracing() {
    TRACING_INIT.call_once(|| {
        use tracing_subscriber::EnvFilter;
        let filter = EnvFilter::from_default_env()
            .add_directive("peleka=debug".parse().unwrap())
            .add_directive("russh=debug".parse().unwrap());
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_test_writer()
            .try_init()
            .ok();
    });
}

/// Path to the test SSH key.
#[allow(dead_code)]
pub fn test_key_path() -> String {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    format!("{}/tests/fixtures/test_key", manifest_dir)
}

/// Create a tar archive of a Docker build context directory.
/// Includes Dockerfile and entrypoint.sh with appropriate permissions.
#[allow(dead_code)]
pub fn create_build_context(
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
