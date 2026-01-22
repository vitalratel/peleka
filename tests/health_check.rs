// ABOUTME: Integration tests for health check functionality.
// ABOUTME: Tests health check pass/fail scenarios against real containers.

mod support;

use peleka::config::HealthcheckConfig;
use peleka::deploy::Deployment;
use peleka::runtime::{ContainerOps, RuntimeType};
use peleka::ssh::Session;
use std::time::Duration;

/// Test: Health check passes when command succeeds.
#[tokio::test]
async fn health_check_passes() {
    let ssh_config = support::podman_session_config().await;

    let session = Session::connect(ssh_config)
        .await
        .expect("connection should succeed");

    let runtime = peleka::runtime::connect_via_session(&session, RuntimeType::Podman)
        .await
        .expect("should create Podman runtime");

    let mut deploy_config = support::test_config("health-test-pass");
    deploy_config.healthcheck = Some(HealthcheckConfig {
        cmd: "true".to_string(),
        interval: Duration::from_secs(1),
        timeout: Duration::from_secs(5),
        retries: 2,
        start_period: Duration::from_secs(1),
    });

    let d1 = Deployment::new(deploy_config);
    let d2 = d1
        .pull_image(&runtime, None)
        .await
        .expect("pull should succeed");
    let d3 = d2
        .start_container(&runtime)
        .await
        .expect("start should succeed");

    let health_result = d3.health_check(&runtime, Duration::from_secs(10)).await;

    match health_result {
        Ok(d4) => {
            let _ = d4.rollback(&runtime).await;
        }
        Err((d3_back, err)) => {
            let _ = d3_back.rollback(&runtime).await;
            panic!("health check should pass but failed: {:?}", err);
        }
    }

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}

/// Test: Health check fails and rollback cleans up container.
#[tokio::test]
async fn health_check_fails_and_rollback() {
    let ssh_config = support::podman_session_config().await;

    let session = Session::connect(ssh_config)
        .await
        .expect("connection should succeed");

    let runtime = peleka::runtime::connect_via_session(&session, RuntimeType::Podman)
        .await
        .expect("should create Podman runtime");

    let mut deploy_config = support::test_config("health-test-fail");
    deploy_config.healthcheck = Some(HealthcheckConfig {
        cmd: "false".to_string(),
        interval: Duration::from_secs(1),
        timeout: Duration::from_secs(2),
        retries: 2,
        start_period: Duration::from_secs(1),
    });

    let d1 = Deployment::new(deploy_config);
    let d2 = d1
        .pull_image(&runtime, None)
        .await
        .expect("pull should succeed");
    let d3 = d2
        .start_container(&runtime)
        .await
        .expect("start should succeed");

    let container_id = d3.new_container().expect("should have container").clone();

    let health_result = d3.health_check(&runtime, Duration::from_secs(10)).await;

    // Health check should fail
    let (d3_back, _err) = health_result.expect_err("health check should fail");

    // Verify deployment still has container ID for rollback
    assert_eq!(d3_back.new_container().cloned(), Some(container_id.clone()));

    // Rollback should clean up
    let d1_again = d3_back
        .rollback(&runtime)
        .await
        .expect("rollback should succeed");

    assert!(d1_again.new_container().is_none());

    // Verify container was actually removed
    let inspect_result = runtime.inspect_container(&container_id).await;
    assert!(
        inspect_result.is_err(),
        "container should have been removed by rollback"
    );

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}
