// ABOUTME: Composable capability traits for container runtimes.
// ABOUTME: Defines ImageOps, ContainerOps, NetworkOps, ExecOps, LogOps, RuntimeInfo.

mod container;
mod exec;
mod image;
mod logs;
mod network;
mod runtime_info;
mod sealed;
mod shared_types;

pub use container::{ContainerError, ContainerFilters, ContainerOps, ContainerSummary};
pub use exec::{ExecError, ExecOps};
pub use image::{ImageError, ImageOps};
pub use logs::{LogError, LogLine, LogOps, LogOptions, LogStream};
pub use network::{NetworkError, NetworkOps};
pub use runtime_info::{RuntimeInfo, RuntimeInfoError};
pub use shared_types::*;

/// Full runtime capability - convenience trait combining all sub-traits.
///
/// Auto-implemented for any type that implements all the required traits.
pub trait FullRuntime:
    ImageOps + ContainerOps + NetworkOps + ExecOps + LogOps + RuntimeInfo
{
}

impl<T> FullRuntime for T where
    T: ImageOps + ContainerOps + NetworkOps + ExecOps + LogOps + RuntimeInfo
{
}
