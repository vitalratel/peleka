// ABOUTME: DNS-compatible service name validation.
// ABOUTME: Ensures service names follow RFC 1123 label requirements.

use std::fmt;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ServiceNameError {
    #[error("service name cannot be empty")]
    Empty,

    #[error("service name exceeds maximum length of 63 characters")]
    TooLong,

    #[error("service name cannot start with a hyphen")]
    StartsWithHyphen,

    #[error("service name cannot end with a hyphen")]
    EndsWithHyphen,

    #[error("service name must be lowercase")]
    NotLowercase,

    #[error("invalid character in service name: '{0}'")]
    InvalidChar(char),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ServiceName(String);

impl ServiceName {
    pub fn new(value: &str) -> Result<Self, ServiceNameError> {
        if value.is_empty() {
            return Err(ServiceNameError::Empty);
        }

        if value.len() > 63 {
            return Err(ServiceNameError::TooLong);
        }

        if value.starts_with('-') {
            return Err(ServiceNameError::StartsWithHyphen);
        }

        if value.ends_with('-') {
            return Err(ServiceNameError::EndsWithHyphen);
        }

        for c in value.chars() {
            if c.is_ascii_uppercase() {
                return Err(ServiceNameError::NotLowercase);
            }
            if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '-' {
                return Err(ServiceNameError::InvalidChar(c));
            }
        }

        Ok(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ServiceName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
