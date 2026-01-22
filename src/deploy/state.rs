// ABOUTME: Deployment state marker types for the type state pattern.
// ABOUTME: States carry their own data, enforcing valid transitions at compile time.

use crate::types::ContainerId;

/// Initial state: connected to server, ready to deploy.
/// Available actions: `pull_image()`
#[derive(Debug, Clone, Default)]
pub struct Initialized;

/// Image pulled: image available on server.
/// Available actions: `start_container()`
#[derive(Debug, Clone, Default)]
pub struct ImagePulled;

/// Container started: new container running.
/// Available actions: `health_check()`, `rollback()`
#[derive(Debug, Clone)]
pub struct ContainerStarted(pub(crate) ContainerId);

impl ContainerStarted {
    /// Get the container ID.
    pub fn container_id(&self) -> &ContainerId {
        &self.0
    }
}

/// Health checked: health checks passed.
/// Available actions: `cutover()`, `rollback()`
#[derive(Debug, Clone)]
pub struct HealthChecked(pub(crate) ContainerId);

impl HealthChecked {
    /// Get the container ID.
    pub fn container_id(&self) -> &ContainerId {
        &self.0
    }
}

/// Cut over: traffic switched to new container.
/// Available actions: `cleanup()`
#[derive(Debug, Clone)]
pub struct CutOver(pub(crate) ContainerId);

impl CutOver {
    /// Get the container ID.
    pub fn container_id(&self) -> &ContainerId {
        &self.0
    }
}

/// Completed: deployment finished, old container stopped.
/// Available actions: `finish()`
#[derive(Debug, Clone)]
pub struct Completed(pub(crate) ContainerId);

impl Completed {
    /// Get the container ID.
    pub fn container_id(&self) -> &ContainerId {
        &self.0
    }
}
