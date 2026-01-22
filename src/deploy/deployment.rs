// ABOUTME: Generic deployment struct parameterized by state marker.
// ABOUTME: State types carry their own data for compile-time guarantees.

use crate::config::Config;
use crate::types::{ContainerId, ImageRef, ServiceName};

use super::state::{Completed, ContainerStarted, CutOver, HealthChecked, Initialized};

/// A deployment in progress, parameterized by its current state.
///
/// The state type parameter `S` carries state-specific data (like container IDs)
/// directly in the state type. This enables compile-time enforcement that
/// container IDs exist when they should.
#[derive(Debug)]
pub struct Deployment<S> {
    pub(crate) config: Config,
    pub(crate) old_container: Option<ContainerId>,
    pub(crate) state: S,
}

impl Deployment<Initialized> {
    /// Create a new deployment (first deploy, no existing container).
    pub fn new(config: Config) -> Self {
        Deployment {
            config,
            old_container: None,
            state: Initialized,
        }
    }

    /// Create a deployment that updates an existing container.
    pub fn new_update(config: Config, old_container: ContainerId) -> Self {
        Deployment {
            config,
            old_container: Some(old_container),
            state: Initialized,
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

    /// Get the old container ID (None on first deploy).
    pub fn old_container(&self) -> Option<&ContainerId> {
        self.old_container.as_ref()
    }
}

// State-specific accessors for container ID
impl Deployment<ContainerStarted> {
    /// Get the new container ID.
    pub fn new_container(&self) -> &ContainerId {
        self.state.container_id()
    }
}

impl Deployment<HealthChecked> {
    /// Get the new container ID.
    pub fn new_container(&self) -> &ContainerId {
        self.state.container_id()
    }
}

impl Deployment<CutOver> {
    /// Get the new container ID.
    pub fn new_container(&self) -> &ContainerId {
        self.state.container_id()
    }
}

impl Deployment<Completed> {
    /// Get the new container ID.
    pub fn new_container(&self) -> &ContainerId {
        self.state.container_id()
    }
}
