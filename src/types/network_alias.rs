// ABOUTME: Validated network alias for container networking.
// ABOUTME: Ensures aliases are non-empty and contain only valid characters.

use std::fmt;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetworkAliasError {
    #[error("network alias cannot be empty")]
    Empty,

    #[error("invalid character in network alias: '{0}'")]
    InvalidChar(char),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NetworkAlias(String);

impl NetworkAlias {
    pub fn new(value: &str) -> Result<Self, NetworkAliasError> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(NetworkAliasError::Empty);
        }

        // Valid characters: alphanumeric, hyphen, underscore, dot
        for c in trimmed.chars() {
            if !c.is_ascii_alphanumeric() && c != '-' && c != '_' && c != '.' {
                return Err(NetworkAliasError::InvalidChar(c));
            }
        }

        Ok(Self(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NetworkAlias {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
