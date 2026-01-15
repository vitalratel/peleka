// ABOUTME: Deployment orchestration using the type state pattern.
// ABOUTME: Exports state markers and Deployment struct for compile-time safe deployments.

mod deployment;
mod state;

pub use deployment::Deployment;
pub use state::{Completed, ContainerStarted, CutOver, HealthChecked, ImagePulled, Initialized};
