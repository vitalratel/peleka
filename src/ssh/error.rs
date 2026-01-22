// ABOUTME: SSH-specific error types.
// ABOUTME: Covers connection, authentication, and host key verification failures.

use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("connection failed: {0}")]
    Connection(String),

    #[error("authentication failed: no valid credentials")]
    AuthenticationFailed,

    #[error("SSH agent not available: {0}")]
    AgentUnavailable(String),

    #[error("failed to load key from {path}: {reason}")]
    KeyLoadFailed { path: PathBuf, reason: String },

    #[error("command execution failed: {0}")]
    CommandFailed(String),

    #[error("command timed out after {0:?}")]
    CommandTimeout(std::time::Duration),

    #[error("channel closed unexpectedly without exit status")]
    ChannelClosed,

    #[error("socket forwarding failed: {0}")]
    SocketForwardFailed(String),

    #[error("SSH protocol error: {0}")]
    Protocol(#[from] russh::Error),

    #[error("SSH key error: {0}")]
    Key(#[from] russh::keys::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
