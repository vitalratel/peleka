// ABOUTME: Container image reference parsing and validation.
// ABOUTME: Handles formats like nginx, nginx:tag, registry/image:tag@digest.

use std::fmt;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseImageRefError {
    #[error("image reference cannot be empty")]
    Empty,

    #[error("invalid character in image reference: {0}")]
    InvalidChar(char),

    #[error("invalid image reference format: {0}")]
    InvalidFormat(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageRef {
    registry: Option<String>,
    name: String,
    tag: Option<String>,
    digest: Option<String>,
}

impl ImageRef {
    pub fn parse(input: &str) -> Result<Self, ParseImageRefError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(ParseImageRefError::Empty);
        }

        // Check for invalid characters
        for c in input.chars() {
            if !c.is_ascii_alphanumeric()
                && c != '/'
                && c != ':'
                && c != '.'
                && c != '-'
                && c != '_'
                && c != '@'
            {
                return Err(ParseImageRefError::InvalidChar(c));
            }
        }

        // Split off digest if present
        let (without_digest, digest) = match input.split_once('@') {
            Some((before, after)) => (before, Some(after.to_string())),
            None => (input, None),
        };

        // Split off tag if present
        let (without_tag, tag) = match without_digest.rsplit_once(':') {
            Some((before, after)) => {
                // Check if the colon is part of a port number in the registry
                // by seeing if 'after' looks like a tag (no slashes)
                if after.contains('/') {
                    (without_digest, None)
                } else {
                    (before, Some(after.to_string()))
                }
            }
            None => (without_digest, None),
        };

        // Parse registry and name
        let (registry, name) = Self::parse_registry_and_name(without_tag)?;

        // Default tag to "latest" if no tag and no digest
        let tag = match (&tag, &digest) {
            (None, None) => Some("latest".to_string()),
            _ => tag,
        };

        Ok(Self {
            registry,
            name,
            tag,
            digest,
        })
    }

    fn parse_registry_and_name(
        input: &str,
    ) -> Result<(Option<String>, String), ParseImageRefError> {
        // A registry is present if the first component contains a dot or colon,
        // or is "localhost"
        let parts: Vec<&str> = input.splitn(2, '/').collect();

        match parts.as_slice() {
            [name] => Ok((None, (*name).to_string())),
            [first, rest] => {
                if first.contains('.') || first.contains(':') || *first == "localhost" {
                    Ok((Some((*first).to_string()), (*rest).to_string()))
                } else {
                    // No registry, the whole thing is the name (e.g., "library/nginx")
                    Ok((None, input.to_string()))
                }
            }
            _ => Err(ParseImageRefError::InvalidFormat(input.to_string())),
        }
    }

    pub fn registry(&self) -> Option<&str> {
        self.registry.as_deref()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn tag(&self) -> Option<&str> {
        self.tag.as_deref()
    }

    pub fn digest(&self) -> Option<&str> {
        self.digest.as_deref()
    }
}

impl fmt::Display for ImageRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref registry) = self.registry {
            write!(f, "{}/", registry)?;
        }
        write!(f, "{}", self.name)?;
        if let Some(ref tag) = self.tag {
            write!(f, ":{}", tag)?;
        }
        if let Some(ref digest) = self.digest {
            write!(f, "@{}", digest)?;
        }
        Ok(())
    }
}
