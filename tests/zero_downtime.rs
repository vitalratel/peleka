// ABOUTME: Integration tests for zero-downtime deployment.
// ABOUTME: Tests network creation, blue-green deployment, and orphan detection.

mod support;

use peleka::deploy::{Deployment, detect_orphans};
use peleka::runtime::{ContainerOps, NetworkOps, RuntimeType};
use peleka::ssh::{Session, SessionConfig};

/// Get SSH config for the shared Podman test container.
async fn podman_session_config() -> SessionConfig {
    support::podman_container::shared_podman_container()
        .await
        .session_config()
}

/// Test: ensure_network creates network if it doesn't exist.
#[tokio::test]
async fn ensure_network_creates_if_not_exists() {
    let ssh_config = podman_session_config().await;

    let mut session = Session::connect(ssh_config)
        .await
        .expect("connection should succeed");

    let runtime = peleka::runtime::connect_via_session(&mut session, RuntimeType::Podman)
        .await
        .expect("should create Docker runtime");

    // Clean up any existing test network
    let test_network_name = "peleka-test-ensure";
    if runtime
        .network_exists(test_network_name)
        .await
        .unwrap_or(false)
    {
        let _ = runtime
            .remove_network(&peleka::types::NetworkId::new(
                test_network_name.to_string(),
            ))
            .await;
    }

    // Create deployment config with specific network
    let mut deploy_config = support::test_config("test-ensure-net");
    deploy_config.network = Some(peleka::config::NetworkConfig {
        name: test_network_name.to_string(),
        aliases: vec![],
    });

    let deployment = Deployment::new(deploy_config);

    // Ensure network - should create it
    let network_id = deployment
        .ensure_network(&runtime)
        .await
        .expect("ensure_network should succeed");

    // Verify network was created
    assert!(
        runtime
            .network_exists(test_network_name)
            .await
            .expect("network_exists should succeed"),
        "network should exist after ensure_network"
    );

    // Calling ensure_network again should return same network
    let network_id2 = deployment
        .ensure_network(&runtime)
        .await
        .expect("second ensure_network should succeed");

    assert_eq!(
        network_id.as_str(),
        network_id2.as_str(),
        "should return same network ID"
    );

    // Clean up
    let _ = runtime.remove_network(&network_id).await;

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}

/// Test: First deployment creates blue container.
#[tokio::test]
async fn first_deployment_creates_blue_container() {
    let ssh_config = podman_session_config().await;

    let mut session = Session::connect(ssh_config)
        .await
        .expect("connection should succeed");

    let runtime = peleka::runtime::connect_via_session(&mut session, RuntimeType::Podman)
        .await
        .expect("should create Docker runtime");

    let deploy_config = support::test_config("test-blue");
    let deployment = Deployment::new(deploy_config);

    // Ensure network
    let network_id = deployment
        .ensure_network(&runtime)
        .await
        .expect("ensure_network should succeed");

    // Pull and start
    let d2 = deployment
        .pull_image(&runtime, None)
        .await
        .expect("pull should succeed");
    let d3 = d2
        .start_container(&runtime)
        .await
        .expect("start should succeed");

    // Get container info to verify labels
    let container_id = d3.new_container().expect("should have container");
    let info = runtime
        .inspect_container(container_id)
        .await
        .expect("inspect should succeed");

    // Verify peleka.slot label is "blue" (first deployment uses blue slot)
    assert_eq!(
        info.labels.get("peleka.slot"),
        Some(&"blue".to_string()),
        "first deployment should use blue slot"
    );
    assert_eq!(
        info.labels.get("peleka.managed"),
        Some(&"true".to_string()),
        "should have peleka.managed label"
    );

    // Clean up
    let _ = d3.rollback(&runtime).await;
    let _ = runtime.remove_network(&network_id).await;

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}

/// Test: Detect orphans finds containers not in known list.
#[tokio::test]
async fn detect_orphans_finds_unknown_containers() {
    let ssh_config = podman_session_config().await;

    let mut session = Session::connect(ssh_config)
        .await
        .expect("connection should succeed");

    let runtime = peleka::runtime::connect_via_session(&mut session, RuntimeType::Podman)
        .await
        .expect("should create Docker runtime");

    let service_name = peleka::types::ServiceName::new("test-orphan").unwrap();

    // Create a deployment and start container (this becomes the "orphan")
    let deploy_config = support::test_config("test-orphan");
    let deployment = Deployment::new(deploy_config);

    let d2 = deployment
        .pull_image(&runtime, None)
        .await
        .expect("pull should succeed");
    let d3 = d2
        .start_container(&runtime)
        .await
        .expect("start should succeed");

    let orphan_container_id = d3.new_container().expect("should have container").clone();

    // Detect orphans with empty known list - should find our container
    let orphans = detect_orphans(&runtime, &service_name, &[])
        .await
        .expect("detect_orphans should succeed");

    assert!(
        orphans.iter().any(|c| c.id == orphan_container_id),
        "should detect container as orphan when not in known list"
    );

    // Detect orphans with our container in known list - should NOT find it
    let orphans2 = detect_orphans(
        &runtime,
        &service_name,
        std::slice::from_ref(&orphan_container_id),
    )
    .await
    .expect("detect_orphans should succeed");

    assert!(
        !orphans2.iter().any(|c| c.id == orphan_container_id),
        "should not detect container as orphan when in known list"
    );

    // Clean up
    let _ = d3.rollback(&runtime).await;

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}
