// ABOUTME: Phantom-typed identifiers for compile-time type safety.
// ABOUTME: Prevents accidental swapping of container, network, image, and pod IDs.

use serde::{Deserialize, Serialize};
use std::marker::PhantomData;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContainerMarker;
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NetworkMarker;
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ImageMarker;
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PodMarker;

/// A type-safe identifier that prevents accidental mixing of different ID types.
///
/// Using phantom types, this ensures you can't accidentally pass a `ContainerId`
/// where a `NetworkId` is expected, catching bugs at compile time.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use = "IDs reference resources and should not be ignored"]
#[serde(transparent)]
pub struct Id<T> {
    value: String,
    #[serde(skip)]
    _marker: PhantomData<T>,
}

impl<T> Id<T> {
    pub fn new(value: String) -> Self {
        Self {
            value,
            _marker: PhantomData,
        }
    }

    pub fn as_str(&self) -> &str {
        &self.value
    }

    pub fn into_inner(self) -> String {
        self.value
    }
}

impl<T> std::fmt::Display for Id<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

pub type ContainerId = Id<ContainerMarker>;
pub type NetworkId = Id<NetworkMarker>;
pub type ImageId = Id<ImageMarker>;
pub type PodId = Id<PodMarker>;
