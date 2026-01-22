// ABOUTME: Application-wide error types for peleka.
// ABOUTME: Uses thiserror for ergonomic error handling with preserved rich types.

use std::path::PathBuf;
use thiserror::Error;

use crate::deploy::DeployError;
use crate::ssh;

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
    Ssh(#[from] ssh::Error),

    #[error("runtime detection failed: {0}")]
    RuntimeDetection(String),

    #[error("deployment failed: {0}")]
    Deploy(#[from] DeployError),

    #[error("no servers configured")]
    NoServers,

    #[error("hook failed: {0}")]
    Hook(String),
}

impl Error {
    /// Returns the deployment error if this is a `Deploy` variant.
    pub fn as_deploy_error(&self) -> Option<&DeployError> {
        match self {
            Error::Deploy(e) => Some(e),
            _ => None,
        }
    }

    /// Returns the SSH error if this is an `Ssh` variant.
    pub fn as_ssh_error(&self) -> Option<&ssh::Error> {
        match self {
            Error::Ssh(e) => Some(e),
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
