// ABOUTME: Compile-fail test verifying rollback cannot be called on Completed.
// ABOUTME: This test should fail to compile, validating state machine safety.

use peleka::deploy::{Completed, Deployment};

async fn try_invalid_rollback<R: peleka::runtime::ContainerOps>(
    deployment: Deployment<Completed>,
    runtime: &R,
) {
    // ERROR: rollback() method doesn't exist on Deployment<Completed>
    deployment.rollback(runtime).await;
}

fn main() {}
