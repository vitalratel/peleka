// ABOUTME: Sealed trait pattern for runtime traits.
// ABOUTME: Prevents external implementations, allowing non-breaking evolution.

/// Sealed trait to prevent external implementations.
///
/// This pattern allows us to add new methods to traits without breaking semver.
/// Only types that implement Sealed (our internal runtime types) can implement
/// the runtime traits.
pub trait Sealed {}
