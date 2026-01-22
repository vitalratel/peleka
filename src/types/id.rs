// ABOUTME: Phantom-typed identifiers for compile-time type safety.
// ABOUTME: Prevents accidental swapping of container, network, image, and pod IDs.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

/// Marker types for phantom type parameters.
/// Using empty enums prevents instantiation and requires no trait bounds.
pub enum ContainerMarker {}
pub enum NetworkMarker {}
pub enum ImageMarker {}
pub enum PodMarker {}

/// A type-safe identifier that prevents accidental mixing of different ID types.
///
/// Using phantom types, this ensures you can't accidentally pass a `ContainerId`
/// where a `NetworkId` is expected, catching bugs at compile time.
#[must_use = "IDs reference resources and should not be ignored"]
pub struct Id<T> {
    value: String,
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

// Manual trait implementations that don't require T to implement the trait.
// This is necessary because T is only used as a phantom type marker.

impl<T> std::fmt::Debug for Id<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Id").field("value", &self.value).finish()
    }
}

impl<T> Clone for Id<T> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            _marker: PhantomData,
        }
    }
}

impl<T> PartialEq for Id<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<T> Eq for Id<T> {}

impl<T> Hash for Id<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}

impl<T> std::fmt::Display for Id<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl<T> Serialize for Id<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.value.serialize(serializer)
    }
}

impl<'de, T> Deserialize<'de> for Id<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Ok(Self::new(value))
    }
}

pub type ContainerId = Id<ContainerMarker>;
pub type NetworkId = Id<NetworkMarker>;
pub type ImageId = Id<ImageMarker>;
pub type PodId = Id<PodMarker>;
