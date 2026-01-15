// ABOUTME: Deployment orchestration using the type state pattern.
// ABOUTME: Exports state markers and Deployment struct for compile-time safe deployments.

mod deployment;
mod error;
mod orphans;
mod state;
mod transitions;

pub use deployment::Deployment;
pub use error::DeployError;
pub use orphans::{cleanup_orphans, detect_orphans};
pub use state::{Completed, ContainerStarted, CutOver, HealthChecked, ImagePulled, Initialized};
pub use transitions::TransitionResult;
