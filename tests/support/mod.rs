// ABOUTME: Test support utilities.
// ABOUTME: Provides container helpers for integration tests.

use std::sync::Once;

// Each test binary only uses some of these modules, so allow dead_code.
#[allow(dead_code)]
pub mod podman_container;
#[allow(dead_code)]
pub mod ssh_container;

static TRACING_INIT: Once = Once::new();

/// Initialize tracing for tests. Safe to call multiple times.
#[allow(dead_code)]
pub fn init_tracing() {
    TRACING_INIT.call_once(|| {
        use tracing_subscriber::EnvFilter;
        let filter = EnvFilter::from_default_env()
            .add_directive("peleka=debug".parse().unwrap())
            .add_directive("russh=debug".parse().unwrap());
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_test_writer()
            .try_init()
            .ok();
    });
}
