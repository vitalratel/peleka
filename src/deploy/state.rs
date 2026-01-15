// ABOUTME: Deployment state marker types for the type state pattern.
// ABOUTME: Zero-sized types enforce valid state transitions at compile time.

/// Initial state: connected to server, ready to deploy.
/// Available actions: `pull_image()`
#[derive(Debug, Clone, Copy, Default)]
pub struct Initialized;

/// Image pulled: image available on server.
/// Available actions: `start_container()`
#[derive(Debug, Clone, Copy, Default)]
pub struct ImagePulled;

/// Container started: new container running.
/// Available actions: `health_check()`, `rollback()`
#[derive(Debug, Clone, Copy, Default)]
pub struct ContainerStarted;

/// Health checked: health checks passed.
/// Available actions: `cutover()`, `rollback()`
#[derive(Debug, Clone, Copy, Default)]
pub struct HealthChecked;

/// Cut over: traffic switched to new container.
/// Available actions: `cleanup()`
#[derive(Debug, Clone, Copy, Default)]
pub struct CutOver;

/// Completed: deployment finished, old container removed.
/// Available actions: `finish()`
#[derive(Debug, Clone, Copy, Default)]
pub struct Completed;
