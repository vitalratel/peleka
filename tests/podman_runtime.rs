// ABOUTME: Integration tests for Podman runtime via SSH tunnel.
// ABOUTME: Tests runtime detection, connectivity, and info retrieval.

mod support;

use peleka::runtime::{RuntimeInfoTrait, RuntimeType, detect_runtime};
use peleka::ssh::Session;

/// Test: Detects Podman on server with Podman installed.
#[test_group::group(podman)]
#[tokio::test]
async fn detects_podman() {
    let config = support::podman_container::shared_podman_container()
        .await
        .session_config();

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

/// Test: Get runtime info via SSH tunnel.
#[test_group::group(podman)]
#[tokio::test]
async fn runtime_info() {
    let config = support::podman_container::shared_podman_container()
        .await
        .session_config();

    let session = Session::connect(config)
        .await
        .expect("connection should succeed");

    let runtime = peleka::runtime::connect_via_session(&session, RuntimeType::Podman)
        .await
        .expect("should create Podman runtime");

    let info = runtime.info().await.expect("should get runtime info");

    assert!(
        !info.name.is_empty(),
        "runtime name should not be empty, got: {}",
        info.name
    );

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}

/// Test: Ping Podman daemon via SSH tunnel.
#[test_group::group(podman)]
#[tokio::test]
async fn runtime_ping() {
    let config = support::podman_container::shared_podman_container()
        .await
        .session_config();

    let session = Session::connect(config)
        .await
        .expect("connection should succeed");

    let runtime = peleka::runtime::connect_via_session(&session, RuntimeType::Podman)
        .await
        .expect("should create Podman runtime");

    runtime.ping().await.expect("ping should succeed");

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}
