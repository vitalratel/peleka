// ABOUTME: Phantom-typed identifiers for compile-time type safety.
// ABOUTME: Prevents accidental swapping of container, network, image, and pod IDs.

use std::marker::PhantomData;

pub struct ContainerMarker;
pub struct NetworkMarker;
pub struct ImageMarker;
pub struct PodMarker;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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

impl<T> std::fmt::Display for Id<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

pub type ContainerId = Id<ContainerMarker>;
pub type NetworkId = Id<NetworkMarker>;
pub type ImageId = Id<ImageMarker>;
pub type PodId = Id<PodMarker>;
