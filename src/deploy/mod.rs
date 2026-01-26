// ABOUTME: Deployment orchestration using the type state pattern.
// ABOUTME: Exports state markers and Deployment struct for compile-time safe deployments.

mod deployment;
mod error;
mod lock;
mod orphans;
mod rollback;
mod state;
mod strategy;
mod transitions;

pub use deployment::Deployment;
pub use error::{
    ContainerErrorExt, DeployError, DeployErrorKind, ImageErrorExt, LockHolderInfo, NetworkErrorExt,
};
pub use lock::{DeployLock, LockInfo};
pub use orphans::{CleanupFailure, CleanupResult, cleanup_orphans, detect_orphans};
pub use rollback::manual_rollback;
pub use state::{Completed, ContainerStarted, CutOver, HealthChecked, ImagePulled, Initialized};
pub use strategy::DeployStrategy;
pub use transitions::TransitionResult;
