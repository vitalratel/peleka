// ABOUTME: Generic deployment struct parameterized by state marker.
// ABOUTME: Uses PhantomData to track deployment state at compile time.

use std::marker::PhantomData;

use crate::config::Config;
use crate::types::{ContainerId, ImageRef, ServiceName};

use super::state::Initialized;

/// A deployment in progress, parameterized by its current state.
///
/// The state type parameter `S` is a zero-sized marker type that
/// indicates which deployment phase we're in. This enables compile-time
/// enforcement of valid state transitions.
#[derive(Debug)]
pub struct Deployment<S> {
    config: Config,
    new_container: Option<ContainerId>,
    old_container: Option<ContainerId>,
    _state: PhantomData<S>,
}

impl Deployment<Initialized> {
    /// Create a new deployment (first deploy, no existing container).
    pub fn new(config: Config) -> Self {
        Deployment {
            config,
            new_container: None,
            old_container: None,
            _state: PhantomData,
        }
    }

    /// Create a deployment that updates an existing container.
    pub fn new_update(config: Config, old_container: ContainerId) -> Self {
        Deployment {
            config,
            new_container: None,
            old_container: Some(old_container),
            _state: PhantomData,
        }
    }
}

impl<S> Deployment<S> {
    /// Get the service name from config.
    pub fn service_name(&self) -> &ServiceName {
        &self.config.service
    }

    /// Get the image reference from config.
    pub fn image(&self) -> &ImageRef {
        &self.config.image
    }

    /// Get the config.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Get the new container ID (set after container is started).
    pub fn new_container(&self) -> Option<&ContainerId> {
        self.new_container.as_ref()
    }

    /// Get the old container ID (None on first deploy).
    pub fn old_container(&self) -> Option<&ContainerId> {
        self.old_container.as_ref()
    }
}
