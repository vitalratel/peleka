// ABOUTME: Integration tests for manual rollback command.
// ABOUTME: Tests swapping between active and previous containers.

mod support;

use peleka::deploy::{Deployment, manual_rollback};
use peleka::runtime::{ContainerFilters, ContainerOps, NetworkOps, RuntimeType};
use peleka::ssh::Session;
use peleka::types::ServiceName;
use std::collections::HashMap;
use std::time::Duration;

/// Helper to find containers for a service and separate by running state.
async fn find_service_containers(
    runtime: &peleka::runtime::BollardRuntime,
    service: &str,
) -> (
    Vec<peleka::types::ContainerId>,
    Vec<peleka::types::ContainerId>,
) {
    let mut labels = HashMap::new();
    labels.insert("peleka.service".to_string(), service.to_string());
    labels.insert("peleka.managed".to_string(), "true".to_string());

    let filters = ContainerFilters {
        labels,
        all: true, // Include stopped containers
        ..Default::default()
    };

    let containers = runtime.list_containers(&filters).await.unwrap_or_default();

    let (running, stopped): (Vec<_>, Vec<_>) =
        containers.into_iter().partition(|c| c.state == "running");

    (
        running.into_iter().map(|c| c.id).collect(),
        stopped.into_iter().map(|c| c.id).collect(),
    )
}

/// Test: Manual rollback swaps active and previous containers.
#[test_group::group(podman)]
#[tokio::test]
async fn manual_rollback_swaps_containers() {
    let ssh_config = support::podman_session_config().await;

    let session = Session::connect(ssh_config)
        .await
        .expect("connection should succeed");

    let runtime = peleka::runtime::connect_via_session(&session, RuntimeType::Podman)
        .await
        .expect("should create Podman runtime");

    let service_name = ServiceName::new("test-rollback-swap").unwrap();

    // Create first deployment config
    let mut deploy_config = support::test_config("test-rollback-swap");
    deploy_config.cleanup = Some(peleka::config::CleanupConfig {
        grace_period: Duration::from_secs(0),
    });
    deploy_config.network = Some(peleka::config::NetworkConfig {
        name: "peleka-test-rollback-swap".to_string(),
        aliases: vec![],
    });
    deploy_config.stop = Some(peleka::config::StopConfig {
        timeout: Duration::from_secs(5),
        signal: "SIGTERM".to_string(),
    });

    // First deployment - creates "active" container
    let d1 = Deployment::new(deploy_config.clone());
    let network_id = d1
        .ensure_network(&runtime)
        .await
        .expect("network should work");
    let d2 = d1
        .pull_image(&runtime, None)
        .await
        .expect("pull should succeed");
    let d3 = d2
        .start_container(&runtime)
        .await
        .expect("start should succeed");
    let d4 = d3
        .health_check(&runtime, Duration::from_secs(5))
        .await
        .expect("health check should pass");
    let d5 = d4
        .cutover(&runtime, &network_id)
        .await
        .expect("cutover should succeed");
    let d6 = d5.cleanup(&runtime).await.expect("cleanup should succeed");
    let first_container_id = d6.deployed_container().clone();

    // Second deployment - first becomes stopped (previous), second becomes running (active)
    let d1 = Deployment::new_update(deploy_config.clone(), first_container_id.clone());
    let d2 = d1
        .pull_image(&runtime, None)
        .await
        .expect("pull should succeed");
    let d3 = d2
        .start_container(&runtime)
        .await
        .expect("start should succeed");
    let d4 = d3
        .health_check(&runtime, Duration::from_secs(5))
        .await
        .expect("health check should pass");
    let d5 = d4
        .cutover(&runtime, &network_id)
        .await
        .expect("cutover should succeed");
    let d6 = d5.cleanup(&runtime).await.expect("cleanup should succeed");
    let second_container_id = d6.deployed_container().clone();

    // Verify: second is running (active), first is stopped (previous)
    let (running, stopped) = find_service_containers(&runtime, "test-rollback-swap").await;
    assert!(
        running.contains(&second_container_id),
        "second should be running (active)"
    );
    assert!(
        stopped.contains(&first_container_id),
        "first should be stopped (previous)"
    );

    // Manual rollback
    manual_rollback(
        &runtime,
        &service_name,
        &network_id,
        deploy_config.stop_timeout(),
    )
    .await
    .expect("rollback should succeed");

    // Verify: first is now running (active), second is now stopped (previous)
    let (running_after, stopped_after) =
        find_service_containers(&runtime, "test-rollback-swap").await;
    assert!(
        running_after.contains(&first_container_id),
        "first should be running after rollback"
    );
    assert!(
        stopped_after.contains(&second_container_id),
        "second should be stopped after rollback"
    );

    // Clean up
    let _ = runtime
        .stop_container(&first_container_id, Duration::from_secs(5))
        .await;
    let _ = runtime.remove_container(&first_container_id, true).await;
    let _ = runtime.remove_container(&second_container_id, true).await;
    let _ = runtime.remove_network(&network_id).await;

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}

/// Test: Rollback fails if no previous container exists.
#[test_group::group(podman)]
#[tokio::test]
async fn rollback_fails_without_previous() {
    let ssh_config = support::podman_session_config().await;

    let session = Session::connect(ssh_config)
        .await
        .expect("connection should succeed");

    let runtime = peleka::runtime::connect_via_session(&session, RuntimeType::Podman)
        .await
        .expect("should create Podman runtime");

    let service_name = ServiceName::new("test-rollback-no-prev").unwrap();

    // Create first deployment only (no previous)
    let mut deploy_config = support::test_config("test-rollback-no-prev");
    deploy_config.network = Some(peleka::config::NetworkConfig {
        name: "peleka-test-rollback-no-prev".to_string(),
        aliases: vec![],
    });
    deploy_config.stop = Some(peleka::config::StopConfig {
        timeout: Duration::from_secs(5),
        signal: "SIGTERM".to_string(),
    });

    let d1 = Deployment::new(deploy_config.clone());
    let network_id = d1
        .ensure_network(&runtime)
        .await
        .expect("network should work");
    let d2 = d1
        .pull_image(&runtime, None)
        .await
        .expect("pull should succeed");
    let d3 = d2
        .start_container(&runtime)
        .await
        .expect("start should succeed");
    let d4 = d3
        .health_check(&runtime, Duration::from_secs(5))
        .await
        .expect("health check should pass");
    let d5 = d4
        .cutover(&runtime, &network_id)
        .await
        .expect("cutover should succeed");
    let d6 = d5.cleanup(&runtime).await.expect("cleanup should succeed");
    let container_id = d6.deployed_container().clone();

    // Try rollback - should fail
    let result = manual_rollback(
        &runtime,
        &service_name,
        &network_id,
        deploy_config.stop_timeout(),
    )
    .await;
    assert!(
        result.is_err(),
        "rollback should fail without previous container"
    );

    // Clean up
    let _ = runtime
        .stop_container(&container_id, Duration::from_secs(5))
        .await;
    let _ = runtime.remove_container(&container_id, true).await;
    let _ = runtime.remove_network(&network_id).await;

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}

/// Test: Double rollback swaps back (ping-pong).
#[test_group::group(podman)]
#[tokio::test]
async fn double_rollback_swaps_back() {
    let ssh_config = support::podman_session_config().await;

    let session = Session::connect(ssh_config)
        .await
        .expect("connection should succeed");

    let runtime = peleka::runtime::connect_via_session(&session, RuntimeType::Podman)
        .await
        .expect("should create Podman runtime");

    let service_name = ServiceName::new("test-rollback-pingpong").unwrap();

    // Create deployment config
    let mut deploy_config = support::test_config("test-rollback-pingpong");
    deploy_config.cleanup = Some(peleka::config::CleanupConfig {
        grace_period: Duration::from_secs(0),
    });
    deploy_config.network = Some(peleka::config::NetworkConfig {
        name: "peleka-test-rollback-pingpong".to_string(),
        aliases: vec![],
    });
    deploy_config.stop = Some(peleka::config::StopConfig {
        timeout: Duration::from_secs(5),
        signal: "SIGTERM".to_string(),
    });

    // First deployment
    let d1 = Deployment::new(deploy_config.clone());
    let network_id = d1
        .ensure_network(&runtime)
        .await
        .expect("network should work");
    let d2 = d1
        .pull_image(&runtime, None)
        .await
        .expect("pull should succeed");
    let d3 = d2
        .start_container(&runtime)
        .await
        .expect("start should succeed");
    let d4 = d3
        .health_check(&runtime, Duration::from_secs(5))
        .await
        .expect("health check should pass");
    let d5 = d4
        .cutover(&runtime, &network_id)
        .await
        .expect("cutover should succeed");
    let d6 = d5.cleanup(&runtime).await.expect("cleanup should succeed");
    let first_container_id = d6.deployed_container().clone();

    // Second deployment
    let d1 = Deployment::new_update(deploy_config.clone(), first_container_id.clone());
    let d2 = d1
        .pull_image(&runtime, None)
        .await
        .expect("pull should succeed");
    let d3 = d2
        .start_container(&runtime)
        .await
        .expect("start should succeed");
    let d4 = d3
        .health_check(&runtime, Duration::from_secs(5))
        .await
        .expect("health check should pass");
    let d5 = d4
        .cutover(&runtime, &network_id)
        .await
        .expect("cutover should succeed");
    let d6 = d5.cleanup(&runtime).await.expect("cleanup should succeed");
    let second_container_id = d6.deployed_container().clone();

    // First rollback: second -> stopped, first -> running
    manual_rollback(
        &runtime,
        &service_name,
        &network_id,
        deploy_config.stop_timeout(),
    )
    .await
    .expect("first rollback should succeed");

    // Second rollback: first -> stopped, second -> running (back to original)
    manual_rollback(
        &runtime,
        &service_name,
        &network_id,
        deploy_config.stop_timeout(),
    )
    .await
    .expect("second rollback should succeed");

    // Verify: back to original state (second running, first stopped)
    let (running, stopped) = find_service_containers(&runtime, "test-rollback-pingpong").await;
    assert!(
        running.contains(&second_container_id),
        "second should be running after double rollback"
    );
    assert!(
        stopped.contains(&first_container_id),
        "first should be stopped after double rollback"
    );

    // Clean up
    let _ = runtime
        .stop_container(&second_container_id, Duration::from_secs(5))
        .await;
    let _ = runtime.remove_container(&first_container_id, true).await;
    let _ = runtime.remove_container(&second_container_id, true).await;
    let _ = runtime.remove_network(&network_id).await;

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}
