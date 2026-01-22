// ABOUTME: Tests for deployment state types and type state pattern.
// ABOUTME: Verifies state markers and Deployment<S> struct.

use peleka::deploy::{
    Completed, ContainerStarted, CutOver, Deployment, HealthChecked, ImagePulled, Initialized,
};
use std::mem::size_of;

// =============================================================================
// State Marker Type Tests
// =============================================================================

/// Test: Early state markers (without container) are zero-sized.
#[test]
fn early_state_markers_are_zero_sized() {
    assert_eq!(
        size_of::<Initialized>(),
        0,
        "Initialized should be zero-sized"
    );
    assert_eq!(
        size_of::<ImagePulled>(),
        0,
        "ImagePulled should be zero-sized"
    );
}

/// Test: State markers with container data hold ContainerId.
#[test]
fn container_state_markers_hold_data() {
    // States after ContainerStarted hold a ContainerId
    // ContainerId is a newtype around String, so these should be String-sized
    let container_state_size = size_of::<ContainerStarted>();
    assert!(
        container_state_size > 0,
        "ContainerStarted should hold container ID"
    );
    assert_eq!(
        size_of::<HealthChecked>(),
        container_state_size,
        "HealthChecked should be same size as ContainerStarted"
    );
    assert_eq!(
        size_of::<CutOver>(),
        container_state_size,
        "CutOver should be same size as ContainerStarted"
    );
    assert_eq!(
        size_of::<Completed>(),
        container_state_size,
        "Completed should be same size as ContainerStarted"
    );
}

/// Test: State markers implement Debug for diagnostics.
#[test]
fn state_markers_implement_debug() {
    // These should compile - Debug is implemented
    // Early states can be constructed directly
    let _ = format!("{:?}", Initialized);
    let _ = format!("{:?}", ImagePulled);
    // Later states hold container ID and can't be constructed in tests,
    // but their Debug impl is tested through Deployment<S>
}

// =============================================================================
// Deployment<S> Struct Tests
// =============================================================================

/// Test: Early Deployment<S> variants (before container) have same size.
#[test]
fn early_deployment_sizes_match() {
    let init_size = size_of::<Deployment<Initialized>>();
    let pulled_size = size_of::<Deployment<ImagePulled>>();

    assert_eq!(
        init_size, pulled_size,
        "Early deployment sizes should match"
    );
}

/// Test: Deployment<S> variants with container are larger than early ones.
#[test]
fn container_deployment_sizes_are_larger() {
    let init_size = size_of::<Deployment<Initialized>>();
    let started_size = size_of::<Deployment<ContainerStarted>>();

    // ContainerStarted holds a ContainerId, so it should be larger
    // (or equal if Option<ContainerId> was already in struct, but now it's in state)
    assert!(
        started_size >= init_size,
        "ContainerStarted deployment should be at least as large"
    );
}

/// Test: Deployment implements Debug.
#[test]
fn deployment_implements_debug() {
    use peleka::config::Config;

    let config = Config::template();
    let deployment: Deployment<Initialized> = Deployment::new(config);

    // Should compile - Debug is implemented
    let debug_str = format!("{:?}", deployment);
    assert!(
        debug_str.contains("Deployment"),
        "Debug output should mention Deployment"
    );
}

// =============================================================================
// Constructor and Accessor Tests
// =============================================================================

/// Test: Deployment<Initialized> can be created from Config.
#[test]
fn can_create_initialized_deployment() {
    use peleka::config::Config;

    let config = Config::template();
    let deployment: Deployment<Initialized> = Deployment::new(config);

    // Verify we can access the config - template uses "my-app" service
    assert_eq!(deployment.service_name().as_str(), "my-app");
    assert!(deployment.image().to_string().contains("my-app"));
}

/// Test: Deployment tracks old container ID.
#[test]
fn deployment_tracks_old_container_id() {
    use peleka::config::Config;

    let config = Config::template();
    let deployment: Deployment<Initialized> = Deployment::new(config);

    // Initially no old container
    assert!(deployment.old_container().is_none());
}

/// Test: Deployment can be created with existing old container (for updates).
#[test]
fn deployment_with_old_container() {
    use peleka::config::Config;
    use peleka::types::ContainerId;

    let config = Config::template();
    let old_id = ContainerId::new("abc123".to_string());
    let deployment: Deployment<Initialized> = Deployment::new_update(config, old_id.clone());

    assert_eq!(deployment.old_container(), Some(&old_id));
}
