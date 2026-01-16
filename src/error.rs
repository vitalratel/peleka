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
}

pub type Result<T> = std::result::Result<T, Error>;
