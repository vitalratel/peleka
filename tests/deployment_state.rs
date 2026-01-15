// ABOUTME: Tests for deployment state types and type state pattern.
// ABOUTME: Verifies zero-sized state markers and Deployment<S> struct.

use peleka::deploy::{
    Completed, ContainerStarted, CutOver, Deployment, HealthChecked, ImagePulled, Initialized,
};
use std::mem::size_of;

// =============================================================================
// State Marker Type Tests
// =============================================================================

/// Test: All state marker types are zero-sized.
#[test]
fn state_markers_are_zero_sized() {
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
    assert_eq!(
        size_of::<ContainerStarted>(),
        0,
        "ContainerStarted should be zero-sized"
    );
    assert_eq!(
        size_of::<HealthChecked>(),
        0,
        "HealthChecked should be zero-sized"
    );
    assert_eq!(size_of::<CutOver>(), 0, "CutOver should be zero-sized");
    assert_eq!(size_of::<Completed>(), 0, "Completed should be zero-sized");
}

/// Test: State markers implement Debug for diagnostics.
#[test]
fn state_markers_implement_debug() {
    // These should compile - Debug is implemented
    let _ = format!("{:?}", Initialized);
    let _ = format!("{:?}", ImagePulled);
    let _ = format!("{:?}", ContainerStarted);
    let _ = format!("{:?}", HealthChecked);
    let _ = format!("{:?}", CutOver);
    let _ = format!("{:?}", Completed);
}

// =============================================================================
// Deployment<S> Struct Tests
// =============================================================================

/// Test: PhantomData state marker doesn't affect Deployment size.
#[test]
fn phantom_data_doesnt_affect_size() {
    // All Deployment<S> variants should have the same size regardless of state
    let init_size = size_of::<Deployment<Initialized>>();
    let pulled_size = size_of::<Deployment<ImagePulled>>();
    let started_size = size_of::<Deployment<ContainerStarted>>();
    let checked_size = size_of::<Deployment<HealthChecked>>();
    let cutover_size = size_of::<Deployment<CutOver>>();
    let completed_size = size_of::<Deployment<Completed>>();

    assert_eq!(init_size, pulled_size, "Deployment sizes should match");
    assert_eq!(pulled_size, started_size, "Deployment sizes should match");
    assert_eq!(started_size, checked_size, "Deployment sizes should match");
    assert_eq!(checked_size, cutover_size, "Deployment sizes should match");
    assert_eq!(
        cutover_size, completed_size,
        "Deployment sizes should match"
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

/// Test: Deployment tracks new and old container IDs.
#[test]
fn deployment_tracks_container_ids() {
    use peleka::config::Config;

    let config = Config::template();
    let deployment: Deployment<Initialized> = Deployment::new(config);

    // Initially no containers
    assert!(deployment.new_container().is_none());
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

    assert!(deployment.new_container().is_none());
    assert_eq!(deployment.old_container(), Some(&old_id));
}
