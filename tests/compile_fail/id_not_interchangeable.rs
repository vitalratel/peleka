// ABOUTME: Compile-fail test verifying ContainerId and NetworkId are not interchangeable.
// ABOUTME: This test should fail to compile, validating type safety.

use peleka::types::{ContainerId, NetworkId};

fn takes_container_id(_id: ContainerId) {}

fn main() {
    let network_id = NetworkId::new("net123".to_string());
    takes_container_id(network_id); // ERROR: expected ContainerId, found NetworkId
}
