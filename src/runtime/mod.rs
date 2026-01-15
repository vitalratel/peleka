// ABOUTME: Container runtime detection for Docker and Podman.
// ABOUTME: Auto-detects available runtime or uses explicit config.

mod detection;
mod types;

pub use detection::detect_runtime;
pub use types::{RuntimeConfig, RuntimeInfo, RuntimeType};
