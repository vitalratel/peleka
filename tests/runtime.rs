// ABOUTME: Integration tests for runtime detection.
// ABOUTME: Tests run against real servers with Docker/Podman installed.

mod support;

use peleka::runtime::{RuntimeType, detect_runtime};
use peleka::ssh::{Session, SessionConfig};

/// Get SSH config for the shared DinD test container.
async fn dind_session_config() -> SessionConfig {
    support::dind_container::shared_dind_container()
        .await
        .session_config()
}

/// Get SSH config for the shared Podman test container.
async fn podman_session_config() -> SessionConfig {
    support::podman_container::shared_podman_container()
        .await
        .session_config()
}

/// Test: Detects Podman on server with Podman installed.
#[tokio::test]
async fn detects_podman_on_podman_server() {
    let config = podman_session_config().await;
    let session = Session::connect(config)
        .await
        .expect("connection should succeed");

    let runtime = detect_runtime(&session, None)
        .await
        .expect("detection should succeed");

    assert!(
        matches!(runtime.runtime_type, RuntimeType::Podman),
        "expected Podman, got {:?}",
        runtime.runtime_type
    );
    assert!(
        runtime.socket_path.contains("podman"),
        "socket path should contain 'podman': {}",
        runtime.socket_path
    );

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}

/// Test: Config override takes precedence over auto-detection.
#[tokio::test]
async fn config_override_takes_precedence() {
    let config = dind_session_config().await;
    let session = Session::connect(config)
        .await
        .expect("connection should succeed");

    // Force Docker even if Podman is available
    let override_config = peleka::runtime::RuntimeConfig {
        runtime: Some(RuntimeType::Docker),
        socket: Some("/var/run/docker.sock".to_string()),
    };

    let runtime = detect_runtime(&session, Some(&override_config))
        .await
        .expect("detection should succeed");

    assert!(
        matches!(runtime.runtime_type, RuntimeType::Docker),
        "override should force Docker"
    );
    assert_eq!(runtime.socket_path, "/var/run/docker.sock");

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}

/// Test: Prefers Podman when both runtimes are available.
/// Note: With our Podman container, only Podman is present, so this verifies Podman is detected.
#[tokio::test]
async fn prefers_podman_when_both_present() {
    let config = podman_session_config().await;
    let session = Session::connect(config)
        .await
        .expect("connection should succeed");

    // Auto-detect without override - should prefer Podman
    let runtime = detect_runtime(&session, None)
        .await
        .expect("detection should succeed");

    assert!(
        matches!(runtime.runtime_type, RuntimeType::Podman),
        "should prefer Podman when both are present, got {:?}",
        runtime.runtime_type
    );

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}

/// Test: Can detect Docker when forced via config override.
#[tokio::test]
async fn detects_docker_with_override() {
    let config = dind_session_config().await;
    let session = Session::connect(config)
        .await
        .expect("connection should succeed");

    // Force Docker via override
    let override_config = peleka::runtime::RuntimeConfig {
        runtime: Some(RuntimeType::Docker),
        socket: None, // Let it use default
    };

    let runtime = detect_runtime(&session, Some(&override_config))
        .await
        .expect("detection should succeed");

    assert!(
        matches!(runtime.runtime_type, RuntimeType::Docker),
        "override should force Docker"
    );
    assert_eq!(runtime.socket_path, "/var/run/docker.sock");

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}

/// Test: RuntimeType serialization/deserialization.
#[test]
fn runtime_type_serde_roundtrip() {
    use serde_json;

    let podman = RuntimeType::Podman;
    let json = serde_json::to_string(&podman).expect("serialize");
    assert_eq!(json, "\"podman\"");

    let back: RuntimeType = serde_json::from_str(&json).expect("deserialize");
    assert!(matches!(back, RuntimeType::Podman));

    let docker = RuntimeType::Docker;
    let json = serde_json::to_string(&docker).expect("serialize");
    assert_eq!(json, "\"docker\"");

    let back: RuntimeType = serde_json::from_str(&json).expect("deserialize");
    assert!(matches!(back, RuntimeType::Docker));
}
