// ABOUTME: Test support utilities.
// ABOUTME: Provides container helpers for integration tests.

// Each test binary only uses some of these modules, so allow dead_code.
#[allow(dead_code)]
pub mod dind_container;
#[allow(dead_code)]
pub mod podman_container;
#[allow(dead_code)]
pub mod ssh_container;
