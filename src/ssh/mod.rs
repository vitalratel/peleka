// ABOUTME: SSH client module for remote server connections.
// ABOUTME: Supports SSH agent and key-based authentication with known_hosts verification.

mod client;
mod error;

pub use client::{CommandOutput, Session, SessionConfig};
pub use error::{Error, Result};
