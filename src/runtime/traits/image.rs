// ABOUTME: Image operations trait for container runtimes.
// ABOUTME: Pull, check existence, and remove container images.

use super::sealed::Sealed;
use super::shared_types::RegistryAuth;
use crate::types::ImageRef;
use async_trait::async_trait;

/// Image operations: pull, check existence, remove.
#[async_trait]
pub trait ImageOps: Sealed + Send + Sync {
    /// Pull an image from a registry.
    async fn pull_image(
        &self,
        reference: &ImageRef,
        auth: Option<&RegistryAuth>,
    ) -> Result<(), ImageError>;

    /// Check if an image exists locally.
    async fn image_exists(&self, reference: &ImageRef) -> Result<bool, ImageError>;

    /// Remove an image.
    async fn remove_image(&self, reference: &ImageRef, force: bool) -> Result<(), ImageError>;
}

/// Errors from image operations.
#[derive(Debug, thiserror::Error)]
pub enum ImageError {
    #[error("image not found: {0}")]
    NotFound(String),

    #[error("authentication failed for registry: {0}")]
    AuthenticationFailed(String),

    #[error("pull failed: {0}")]
    PullFailed(String),

    #[error("image in use, cannot remove: {0}")]
    InUse(String),

    #[error("runtime error: {0}")]
    Runtime(String),
}
