// ABOUTME: Integration tests for Docker runtime via SSH tunnel.
// ABOUTME: Tests runtime detection, connectivity, and info retrieval.

mod support;

use peleka::runtime::{RuntimeInfoTrait, RuntimeType};
use peleka::ssh::Session;

/// Test: Get runtime info via SSH tunnel to Docker-in-Docker.
#[tokio::test]
async fn runtime_info() {
    let config = support::docker_container::shared_docker_container()
        .await
        .session_config();

    let session = Session::connect(config)
        .await
        .expect("connection should succeed");

    let runtime = peleka::runtime::connect_via_session(&session, RuntimeType::Docker)
        .await
        .expect("should create Docker runtime");

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

/// Test: Ping Docker daemon via SSH tunnel.
#[tokio::test]
async fn runtime_ping() {
    let config = support::docker_container::shared_docker_container()
        .await
        .session_config();

    let session = Session::connect(config)
        .await
        .expect("connection should succeed");

    let runtime = peleka::runtime::connect_via_session(&session, RuntimeType::Docker)
        .await
        .expect("should create Docker runtime");

    runtime.ping().await.expect("ping should succeed");

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}
