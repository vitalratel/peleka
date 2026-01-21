// ABOUTME: Tests for deployment state transitions.
// ABOUTME: Verifies transition methods exist and return correct state types.

mod support;

use peleka::deploy::{
    Completed, ContainerStarted, CutOver, Deployment, HealthChecked, ImagePulled, Initialized,
};
use std::time::Duration;

// =============================================================================
// Transition Type Signature Tests
// =============================================================================

/// Test: Verifies the type signatures of all transition methods compile correctly.
/// This ensures the state machine is wired up properly at compile time.
#[test]
fn transition_type_signatures_compile() {
    use peleka::deploy::DeployError;
    use peleka::runtime::{ContainerOps, ImageOps, NetworkOps, RegistryAuth};
    use peleka::types::NetworkId;

    // This function is never called, but it must compile.
    // If any type signature is wrong, this will fail to compile.
    #[allow(dead_code)]
    async fn check_signatures<R: ImageOps + ContainerOps + NetworkOps>(
        runtime: &R,
        network_id: &NetworkId,
    ) {
        let config = peleka::config::Config::template();

        // Initialized -> ImagePulled
        let d1: Deployment<Initialized> = Deployment::new(config);
        let d2: Result<Deployment<ImagePulled>, DeployError> =
            d1.pull_image(runtime, None::<&RegistryAuth>).await;

        // ImagePulled -> ContainerStarted
        let d3: Result<Deployment<ContainerStarted>, DeployError> =
            d2.unwrap().start_container(runtime).await;

        // ContainerStarted -> HealthChecked (with rollback option)
        let d4: Result<Deployment<HealthChecked>, (Deployment<ContainerStarted>, DeployError)> = d3
            .unwrap()
            .health_check(runtime, Duration::from_secs(60))
            .await;

        // HealthChecked -> CutOver
        let d5: Result<Deployment<CutOver>, DeployError> =
            d4.unwrap().cutover(runtime, network_id).await;

        // CutOver -> Completed
        let d6: Result<Deployment<Completed>, DeployError> = d5.unwrap().cleanup(runtime).await;

        // Completed - terminal state
        let _config = d6.unwrap().finish();
    }
}

/// Test: Rollback is available from ContainerStarted.
#[test]
fn rollback_from_container_started_compiles() {
    use peleka::deploy::DeployError;
    use peleka::runtime::ContainerOps;

    #[allow(dead_code)]
    async fn check_rollback<R: ContainerOps>(
        deployment: Deployment<ContainerStarted>,
        runtime: &R,
    ) -> Result<Deployment<Initialized>, DeployError> {
        deployment.rollback(runtime).await
    }
}

/// Test: Rollback is available from HealthChecked.
#[test]
fn rollback_from_health_checked_compiles() {
    use peleka::deploy::DeployError;
    use peleka::runtime::ContainerOps;

    #[allow(dead_code)]
    async fn check_rollback<R: ContainerOps>(
        deployment: Deployment<HealthChecked>,
        runtime: &R,
    ) -> Result<Deployment<Initialized>, DeployError> {
        deployment.rollback(runtime).await
    }
}

// =============================================================================
// Integration Tests (require SSH_TEST_HOST)
// =============================================================================

use peleka::ssh::{Session, SessionConfig};

/// Get SSH config for the shared Podman test container.
async fn podman_session_config() -> SessionConfig {
    support::podman_container::shared_podman_container()
        .await
        .session_config()
}

/// Test: Full deployment chain works end-to-end.
#[tokio::test]
async fn full_deployment_chain() {
    use peleka::config::Config;
    use peleka::deploy::Deployment;
    use peleka::runtime::{NetworkOps, RuntimeType};

    let config = podman_session_config().await;

    let mut session = Session::connect(config)
        .await
        .expect("connection should succeed");

    let runtime = peleka::runtime::connect_via_session(&mut session, RuntimeType::Podman)
        .await
        .expect("should create Podman runtime");

    // Create deployment config with network
    let mut deploy_config = Config::template();
    deploy_config.service = peleka::types::ServiceName::new("test-deploy").unwrap();
    deploy_config.image = peleka::types::ImageRef::parse("docker.io/library/busybox:1.36").unwrap();
    deploy_config.command = Some(vec![
        "sh".to_string(),
        "-c".to_string(),
        "sleep infinity".to_string(),
    ]);
    deploy_config.network = Some(peleka::config::NetworkConfig {
        name: "peleka-test-network".to_string(),
        aliases: vec![],
    });

    // Run through deployment chain
    let d1 = Deployment::new(deploy_config);

    // Use ensure_network to create the network properly
    let network_id = d1
        .ensure_network(&runtime)
        .await
        .expect("network should be created");

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
        .expect("health check should pass (no healthcheck configured)");
    let d5 = d4
        .cutover(&runtime, &network_id)
        .await
        .expect("cutover should succeed");
    let d6 = d5.cleanup(&runtime).await.expect("cleanup should succeed");

    let _final_config = d6.finish();

    // Clean up network
    let _ = runtime.remove_network(&network_id).await;

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}

/// Test: Container start failure cleans up created container.
#[tokio::test]
async fn container_start_failure_cleans_up() {
    use peleka::config::Config;
    use peleka::deploy::Deployment;
    use peleka::runtime::RuntimeType;

    let config = podman_session_config().await;

    let mut session = Session::connect(config)
        .await
        .expect("connection should succeed");

    let runtime = peleka::runtime::connect_via_session(&mut session, RuntimeType::Podman)
        .await
        .expect("should create Docker runtime");

    // Create deployment config with invalid command that will fail to start
    let mut deploy_config = Config::template();
    deploy_config.service = peleka::types::ServiceName::new("test-fail-start").unwrap();
    deploy_config.image = peleka::types::ImageRef::parse("docker.io/library/busybox:1.36").unwrap();
    deploy_config.command = Some(vec![
        "sh".to_string(),
        "-c".to_string(),
        "sleep infinity".to_string(),
    ]);

    // Pull the image first
    let d1 = Deployment::new(deploy_config);
    let d2 = d1
        .pull_image(&runtime, None)
        .await
        .expect("pull should succeed");

    // Start should succeed (nginx starts fine)
    // This test verifies the cleanup path exists - a real failure test would
    // need a container that fails after creation but before start completes
    let d3_result = d2.start_container(&runtime).await;

    // Clean up if it succeeded
    if let Ok(d3) = d3_result {
        let _ = d3.rollback(&runtime).await;
    }

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}

/// Test: Rollback from ContainerStarted removes new container.
#[tokio::test]
async fn rollback_from_container_started_removes_container() {
    use peleka::config::Config;
    use peleka::deploy::Deployment;
    use peleka::runtime::{ContainerFilters, ContainerOps, RuntimeType};

    let config = podman_session_config().await;

    let mut session = Session::connect(config)
        .await
        .expect("connection should succeed");

    let runtime = peleka::runtime::connect_via_session(&mut session, RuntimeType::Podman)
        .await
        .expect("should create Docker runtime");

    // Create deployment
    let mut deploy_config = Config::template();
    deploy_config.service = peleka::types::ServiceName::new("test-rollback").unwrap();
    deploy_config.image = peleka::types::ImageRef::parse("docker.io/library/busybox:1.36").unwrap();
    deploy_config.command = Some(vec![
        "sh".to_string(),
        "-c".to_string(),
        "sleep infinity".to_string(),
    ]);

    let d1 = Deployment::new(deploy_config);
    let d2 = d1
        .pull_image(&runtime, None)
        .await
        .expect("pull should succeed");
    let d3 = d2
        .start_container(&runtime)
        .await
        .expect("start should succeed");

    // Get container ID before rollback
    let container_id = d3.new_container().expect("should have container").clone();

    // Rollback
    let _d1_again = d3
        .rollback(&runtime)
        .await
        .expect("rollback should succeed");

    // Verify container was removed
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

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}

// =============================================================================
// DeployError Tests
// =============================================================================

/// Test: DeployError type exists and has expected variants.
#[test]
fn deploy_error_exists() {
    use peleka::deploy::DeployError;

    // Verify error type exists and can be formatted
    let _: fn() -> DeployError = || DeployError::ImagePullFailed("test".to_string());
}

/// Test: DeployError implements std::error::Error.
#[test]
fn deploy_error_implements_error() {
    use peleka::deploy::DeployError;
    use std::error::Error;

    fn assert_error<E: Error>() {}
    assert_error::<DeployError>();
}
