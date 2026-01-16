// ABOUTME: Integration tests for zero-downtime deployment.
// ABOUTME: Tests network creation, blue-green deployment, and orphan detection.

mod support;

use peleka::config::{CleanupConfig, Config};
use peleka::deploy::{Deployment, detect_orphans};
use peleka::runtime::{ContainerFilters, ContainerOps, NetworkOps, RuntimeType};
use peleka::ssh::{Session, SessionConfig};
use std::time::Duration;

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
    let mut deploy_config = Config::template();
    deploy_config.service = peleka::types::ServiceName::new("test-ensure-net").unwrap();
    deploy_config.image = peleka::types::ImageRef::parse("nginx:alpine").unwrap();
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

    let mut deploy_config = Config::template();
    deploy_config.service = peleka::types::ServiceName::new("test-blue").unwrap();
    deploy_config.image = peleka::types::ImageRef::parse("nginx:alpine").unwrap();

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

    // Verify peleka.state label is "blue"
    assert_eq!(
        info.labels.get("peleka.state"),
        Some(&"blue".to_string()),
        "first deployment should be blue"
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
    let mut deploy_config = Config::template();
    deploy_config.service = service_name.clone();
    deploy_config.image = peleka::types::ImageRef::parse("nginx:alpine").unwrap();

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

/// Test: Cleanup with grace period waits before stopping.
#[tokio::test]
async fn cleanup_waits_for_grace_period() {
    let ssh_config = podman_session_config().await;

    let mut session = Session::connect(ssh_config)
        .await
        .expect("connection should succeed");

    let runtime = peleka::runtime::connect_via_session(&mut session, RuntimeType::Podman)
        .await
        .expect("should create Docker runtime");

    // Create "old" container first
    let mut old_config = Config::template();
    old_config.service = peleka::types::ServiceName::new("test-grace").unwrap();
    old_config.image = peleka::types::ImageRef::parse("nginx:alpine").unwrap();
    old_config.network = Some(peleka::config::NetworkConfig {
        name: "peleka-grace-test".to_string(),
        aliases: vec![],
    });

    let old_deployment = Deployment::new(old_config.clone());
    let network_id = old_deployment.ensure_network(&runtime).await.unwrap();

    let d2 = old_deployment.pull_image(&runtime, None).await.unwrap();
    let d3 = d2.start_container(&runtime).await.unwrap();
    let old_container_id = d3.new_container().unwrap().clone();

    // Don't do full deployment - manually transition to simulate having old container
    // For this test, we just verify the grace period config works

    // Create new deployment with old container and short grace period
    let mut new_config = Config::template();
    new_config.service = peleka::types::ServiceName::new("test-grace").unwrap();
    new_config.image = peleka::types::ImageRef::parse("nginx:alpine").unwrap();
    new_config.network = Some(peleka::config::NetworkConfig {
        name: "peleka-grace-test".to_string(),
        aliases: vec![],
    });
    new_config.cleanup = Some(CleanupConfig {
        grace_period: Duration::from_millis(100), // Very short for testing
    });

    let new_deployment = Deployment::new_update(new_config, old_container_id.clone());

    let d2 = new_deployment.pull_image(&runtime, None).await.unwrap();
    let d3 = d2.start_container(&runtime).await.unwrap();
    let d4 = d3
        .health_check(&runtime, Duration::from_secs(5))
        .await
        .unwrap();
    let d5 = d4.cutover(&runtime, &network_id).await.unwrap();

    // Time the cleanup
    let start = std::time::Instant::now();
    let _d6 = d5.cleanup(&runtime).await.unwrap();
    let elapsed = start.elapsed();

    // Should have waited at least the grace period
    assert!(
        elapsed >= Duration::from_millis(100),
        "cleanup should wait for grace period (took {:?})",
        elapsed
    );

    // Verify old container is gone
    let filters = ContainerFilters {
        all: true,
        ..Default::default()
    };
    let containers = runtime.list_containers(&filters).await.unwrap();
    let old_exists = containers.iter().any(|c| c.id == old_container_id);
    assert!(!old_exists, "old container should be removed after cleanup");

    // Clean up network
    let _ = runtime.remove_network(&network_id).await;

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}

/// Test: Full blue-green deployment flow.
#[tokio::test]
async fn full_blue_green_deployment() {
    let ssh_config = podman_session_config().await;

    let mut session = Session::connect(ssh_config)
        .await
        .expect("connection should succeed");

    let runtime = peleka::runtime::connect_via_session(&mut session, RuntimeType::Podman)
        .await
        .expect("should create Docker runtime");

    let service_name = "test-bluegreen";

    // First deployment (blue)
    let mut blue_config = Config::template();
    blue_config.service = peleka::types::ServiceName::new(service_name).unwrap();
    blue_config.image = peleka::types::ImageRef::parse("nginx:alpine").unwrap();
    blue_config.network = Some(peleka::config::NetworkConfig {
        name: "peleka-bluegreen-test".to_string(),
        aliases: vec![],
    });
    blue_config.cleanup = Some(CleanupConfig {
        grace_period: Duration::from_millis(10), // Short for testing
    });

    let blue_deployment = Deployment::new(blue_config.clone());
    let network_id = blue_deployment.ensure_network(&runtime).await.unwrap();

    let d2 = blue_deployment.pull_image(&runtime, None).await.unwrap();
    let d3 = d2.start_container(&runtime).await.unwrap();
    let blue_container_id = d3.new_container().unwrap().clone();

    let d4 = d3
        .health_check(&runtime, Duration::from_secs(5))
        .await
        .unwrap();
    let d5 = d4.cutover(&runtime, &network_id).await.unwrap();
    let _d6 = d5.cleanup(&runtime).await.unwrap();

    // Verify blue container has correct state label
    let blue_info = runtime.inspect_container(&blue_container_id).await.unwrap();
    assert_eq!(
        blue_info.labels.get("peleka.state"),
        Some(&"blue".to_string())
    );

    // Second deployment (green) with blue as old container
    let green_deployment = Deployment::new_update(blue_config, blue_container_id.clone());

    let d2 = green_deployment.pull_image(&runtime, None).await.unwrap();
    let d3 = d2.start_container(&runtime).await.unwrap();
    let green_container_id = d3.new_container().unwrap().clone();

    // Verify green container has correct state label
    let green_info = runtime
        .inspect_container(&green_container_id)
        .await
        .unwrap();
    assert_eq!(
        green_info.labels.get("peleka.state"),
        Some(&"green".to_string())
    );

    let d4 = d3
        .health_check(&runtime, Duration::from_secs(5))
        .await
        .unwrap();
    let d5 = d4.cutover(&runtime, &network_id).await.unwrap();
    let _d6 = d5.cleanup(&runtime).await.unwrap();

    // Verify blue container is gone
    let filters = ContainerFilters {
        all: true,
        ..Default::default()
    };
    let containers = runtime.list_containers(&filters).await.unwrap();
    let blue_exists = containers.iter().any(|c| c.id == blue_container_id);
    assert!(
        !blue_exists,
        "blue container should be removed after green deployment"
    );

    // Verify green container is still running
    let green_exists = containers.iter().any(|c| c.id == green_container_id);
    assert!(green_exists, "green container should still exist");

    // Clean up
    let _ = runtime
        .stop_container(&green_container_id, Duration::from_secs(5))
        .await;
    let _ = runtime.remove_container(&green_container_id, true).await;
    let _ = runtime.remove_network(&network_id).await;

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}
