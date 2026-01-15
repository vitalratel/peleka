// ABOUTME: Compile-fail test verifying cutover cannot be called on Initialized.
// ABOUTME: This test should fail to compile, validating state machine safety.

use peleka::config::Config;
use peleka::deploy::{Deployment, Initialized};
use peleka::types::NetworkId;

async fn try_invalid_cutover<R: peleka::runtime::ContainerOps + peleka::runtime::NetworkOps>(
    runtime: &R,
) {
    let config = Config::template();
    let deployment: Deployment<Initialized> = Deployment::new(config);
    let network_id = NetworkId::new("test".to_string());

    // ERROR: cutover() method doesn't exist on Deployment<Initialized>
    deployment.cutover(runtime, &network_id).await;
}

fn main() {}
