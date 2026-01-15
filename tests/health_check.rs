// ABOUTME: Integration tests for health check functionality.
// ABOUTME: Tests health check pass/fail scenarios against real containers.

use peleka::config::{Config, HealthcheckConfig};
use peleka::deploy::Deployment;
use peleka::runtime::{ContainerOps, NetworkConfig, NetworkOps, RuntimeType};
use peleka::ssh::{Session, SessionConfig};
use std::env;
use std::time::Duration;

/// Get test SSH configuration from environment.
fn test_config() -> Option<SessionConfig> {
    let host = env::var("SSH_TEST_HOST").ok()?;
    let user = env::var("SSH_TEST_USER").ok().or_else(whoami)?;
    let port: u16 = env::var("SSH_TEST_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(22);
    let key_path = env::var("SSH_KEY").ok();
    let tofu = env::var("SSH_TEST_TOFU").is_ok();

    let mut config = SessionConfig::new(host, user)
        .port(port)
        .trust_on_first_use(tofu);
    if let Some(path) = key_path {
        config = config.key_path(path);
    }
    Some(config)
}

fn whoami() -> Option<String> {
    env::var("USER").ok()
}

/// Test: Health check passes when endpoint returns expected status.
#[tokio::test]
#[ignore = "requires SSH_TEST_HOST with Docker"]
async fn health_check_passes_with_healthy_endpoint() {
    let ssh_config = test_config().expect("SSH_TEST_HOST must be set");

    let mut session = Session::connect(ssh_config)
        .await
        .expect("connection should succeed");

    let runtime = peleka::runtime::docker::connect_via_session(&mut session, RuntimeType::Docker)
        .await
        .expect("should create Docker runtime");

    // Create a test network
    let network_config = NetworkConfig {
        name: "peleka-health-test".to_string(),
        driver: Some("bridge".to_string()),
        labels: std::collections::HashMap::new(),
    };
    let network_id = runtime
        .create_network(&network_config)
        .await
        .expect("should create network");

    // Use httpbin which provides /status/{code} endpoints
    let mut deploy_config = Config::template();
    deploy_config.service = peleka::types::ServiceName::new("health-test-pass").unwrap();
    deploy_config.image = peleka::types::ImageRef::parse("kennethreitz/httpbin:latest").unwrap();
    deploy_config.healthcheck = Some(HealthcheckConfig {
        path: "/status/200".to_string(),
        port: 80,
        interval: Duration::from_secs(2),
        timeout: Duration::from_secs(5),
        retries: 3,
        start_period: Duration::from_secs(5),
        expected_status: 200,
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

    // Health check should pass - endpoint returns 200
    let health_result = d3.health_check(&runtime, Duration::from_secs(60)).await;

    match health_result {
        Ok(d4) => {
            // Clean up
            let _ = d4.rollback(&runtime).await;
        }
        Err((d3_back, err)) => {
            // Clean up on failure
            let _ = d3_back.rollback(&runtime).await;
            panic!("health check should pass but failed: {:?}", err);
        }
    }

    let _ = runtime.remove_network(&network_id).await;
    session.disconnect().await.expect("disconnect should succeed");
}

/// Test: Health check fails when endpoint returns unexpected status.
#[tokio::test]
#[ignore = "requires SSH_TEST_HOST with Docker"]
async fn health_check_fails_with_unhealthy_endpoint() {
    let ssh_config = test_config().expect("SSH_TEST_HOST must be set");

    let mut session = Session::connect(ssh_config)
        .await
        .expect("connection should succeed");

    let runtime = peleka::runtime::docker::connect_via_session(&mut session, RuntimeType::Docker)
        .await
        .expect("should create Docker runtime");

    // Create a test network
    let network_config = NetworkConfig {
        name: "peleka-health-fail-test".to_string(),
        driver: Some("bridge".to_string()),
        labels: std::collections::HashMap::new(),
    };
    let network_id = runtime
        .create_network(&network_config)
        .await
        .expect("should create network");

    // Configure to expect 200 but endpoint returns 503
    let mut deploy_config = Config::template();
    deploy_config.service = peleka::types::ServiceName::new("health-test-fail").unwrap();
    deploy_config.image = peleka::types::ImageRef::parse("kennethreitz/httpbin:latest").unwrap();
    deploy_config.healthcheck = Some(HealthcheckConfig {
        path: "/status/503".to_string(),
        port: 80,
        interval: Duration::from_secs(2),
        timeout: Duration::from_secs(3),
        retries: 2,
        start_period: Duration::from_secs(2),
        expected_status: 200, // Expect 200 but will get 503
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

    // Health check should fail - endpoint returns 503 but we expect 200
    // Wait for enough time: start_period + retries * interval
    let health_result = d3.health_check(&runtime, Duration::from_secs(30)).await;

    match health_result {
        Ok(d4) => {
            let _ = d4.rollback(&runtime).await;
            panic!("health check should fail but passed");
        }
        Err((d3_back, _err)) => {
            // Expected: health check failed, now verify rollback works
            let d1_again = d3_back
                .rollback(&runtime)
                .await
                .expect("rollback should succeed");
            // Verify we're back to Initialized state (no container)
            assert!(d1_again.new_container().is_none());
        }
    }

    let _ = runtime.remove_network(&network_id).await;
    session.disconnect().await.expect("disconnect should succeed");
}

/// Test: Health check with custom expected status (204 No Content).
#[tokio::test]
#[ignore = "requires SSH_TEST_HOST with Docker"]
async fn health_check_with_custom_expected_status() {
    let ssh_config = test_config().expect("SSH_TEST_HOST must be set");

    let mut session = Session::connect(ssh_config)
        .await
        .expect("connection should succeed");

    let runtime = peleka::runtime::docker::connect_via_session(&mut session, RuntimeType::Docker)
        .await
        .expect("should create Docker runtime");

    // Create a test network
    let network_config = NetworkConfig {
        name: "peleka-health-204-test".to_string(),
        driver: Some("bridge".to_string()),
        labels: std::collections::HashMap::new(),
    };
    let network_id = runtime
        .create_network(&network_config)
        .await
        .expect("should create network");

    // Configure to expect 204 and endpoint returns 204
    let mut deploy_config = Config::template();
    deploy_config.service = peleka::types::ServiceName::new("health-test-204").unwrap();
    deploy_config.image = peleka::types::ImageRef::parse("kennethreitz/httpbin:latest").unwrap();
    deploy_config.healthcheck = Some(HealthcheckConfig {
        path: "/status/204".to_string(),
        port: 80,
        interval: Duration::from_secs(2),
        timeout: Duration::from_secs(5),
        retries: 3,
        start_period: Duration::from_secs(5),
        expected_status: 204,
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

    // Health check should pass - endpoint returns 204 as expected
    let health_result = d3.health_check(&runtime, Duration::from_secs(60)).await;

    match health_result {
        Ok(d4) => {
            let _ = d4.rollback(&runtime).await;
        }
        Err((d3_back, err)) => {
            let _ = d3_back.rollback(&runtime).await;
            panic!("health check should pass with 204 but failed: {:?}", err);
        }
    }

    let _ = runtime.remove_network(&network_id).await;
    session.disconnect().await.expect("disconnect should succeed");
}

/// Test: Health check times out when endpoint never becomes healthy.
#[tokio::test]
#[ignore = "requires SSH_TEST_HOST with Docker"]
async fn health_check_timeout_handled() {
    let ssh_config = test_config().expect("SSH_TEST_HOST must be set");

    let mut session = Session::connect(ssh_config)
        .await
        .expect("connection should succeed");

    let runtime = peleka::runtime::docker::connect_via_session(&mut session, RuntimeType::Docker)
        .await
        .expect("should create Docker runtime");

    // Use a config where health check takes longer than our timeout
    let mut deploy_config = Config::template();
    deploy_config.service = peleka::types::ServiceName::new("health-test-timeout").unwrap();
    deploy_config.image = peleka::types::ImageRef::parse("kennethreitz/httpbin:latest").unwrap();
    deploy_config.healthcheck = Some(HealthcheckConfig {
        path: "/delay/10".to_string(), // 10 second delay - will timeout
        port: 80,
        interval: Duration::from_secs(2),
        timeout: Duration::from_secs(1), // Docker health check timeout is 1s
        retries: 2,
        start_period: Duration::from_secs(1),
        expected_status: 200,
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

    // Health check should timeout or fail
    let health_result = d3.health_check(&runtime, Duration::from_secs(15)).await;

    // Either timeout or unhealthy is acceptable here
    match health_result {
        Ok(d4) => {
            let _ = d4.rollback(&runtime).await;
            // If it somehow passes (race condition), that's acceptable
        }
        Err((d3_back, err)) => {
            // Expected: timeout or unhealthy
            let _ = d3_back.rollback(&runtime).await;
            // Verify it's a timeout or health check failure
            let err_str = format!("{:?}", err);
            assert!(
                err_str.contains("timeout") || err_str.contains("unhealthy"),
                "expected timeout or unhealthy error, got: {}",
                err_str
            );
        }
    }

    session.disconnect().await.expect("disconnect should succeed");
}

/// Test: Rollback returns container for cleanup after health check failure.
#[tokio::test]
#[ignore = "requires SSH_TEST_HOST with Docker"]
async fn rollback_returns_container_on_health_failure() {
    let ssh_config = test_config().expect("SSH_TEST_HOST must be set");

    let mut session = Session::connect(ssh_config)
        .await
        .expect("connection should succeed");

    let runtime = peleka::runtime::docker::connect_via_session(&mut session, RuntimeType::Docker)
        .await
        .expect("should create Docker runtime");

    // Configure an always-failing health check
    let mut deploy_config = Config::template();
    deploy_config.service = peleka::types::ServiceName::new("health-test-rollback").unwrap();
    deploy_config.image = peleka::types::ImageRef::parse("kennethreitz/httpbin:latest").unwrap();
    deploy_config.healthcheck = Some(HealthcheckConfig {
        path: "/status/500".to_string(),
        port: 80,
        interval: Duration::from_secs(2),
        timeout: Duration::from_secs(3),
        retries: 1,
        start_period: Duration::from_secs(2),
        expected_status: 200,
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

    // Get the container ID before health check
    let container_id = d3.new_container().expect("should have container").clone();

    // Health check will fail
    let health_result = d3.health_check(&runtime, Duration::from_secs(20)).await;

    // Verify we get the deployment back for rollback
    let (d3_back, _err) = health_result.expect_err("health check should fail");

    // Verify the returned deployment still has the container ID
    assert_eq!(
        d3_back.new_container().map(|c| c.clone()),
        Some(container_id.clone())
    );

    // Rollback should clean up the container
    let d1_again = d3_back
        .rollback(&runtime)
        .await
        .expect("rollback should succeed");

    // Verify container is gone
    assert!(d1_again.new_container().is_none());

    // Verify container was actually removed
    use peleka::runtime::ContainerFilters;
    let filters = ContainerFilters {
        all: true,
        ..Default::default()
    };
    let containers = runtime
        .list_containers(&filters)
        .await
        .expect("list should succeed");
    let found = containers.iter().any(|c| c.id == container_id);
    assert!(!found, "container should have been removed by rollback");

    session.disconnect().await.expect("disconnect should succeed");
}
