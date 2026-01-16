// ABOUTME: Application-wide error types for peleka.
// ABOUTME: Uses thiserror for ergonomic error handling.

use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("file already exists: {0}")]
    AlreadyExists(PathBuf),

    #[error("configuration file not found in {0}")]
    ConfigNotFound(PathBuf),

    #[error("unknown destination: {0}")]
    UnknownDestination(String),

    #[error("missing required environment variable: {0}")]
    MissingEnvVar(String),

    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("YAML parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("SSH error: {0}")]
    Ssh(String),

    #[error("runtime detection failed: {0}")]
    RuntimeDetection(String),

    #[error("deployment failed: {0}")]
    Deploy(String),

    #[error("no servers configured")]
    NoServers,
}

pub type Result<T> = std::result::Result<T, Error>;
