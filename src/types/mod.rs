// ABOUTME: Type-safe identifiers and validated domain types.
// ABOUTME: Uses phantom types to prevent ID confusion at compile time.

mod id;
mod image_ref;
mod network_alias;
mod service_name;

pub use id::{ContainerId, ImageId, NetworkId, PodId};
pub use image_ref::{ImageRef, ParseImageRefError};
pub use network_alias::{NetworkAlias, NetworkAliasError};
pub use service_name::{ServiceName, ServiceNameError};
