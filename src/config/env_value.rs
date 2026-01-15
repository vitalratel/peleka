// ABOUTME: Environment variable value types with interpolation support.
// ABOUTME: Handles literal values and references to environment variables.

use crate::error::{Error, Result};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum EnvValue {
    Literal(String),
    FromEnv {
        #[serde(rename = "env")]
        var: String,
        #[serde(default)]
        default: Option<String>,
    },
}

impl EnvValue {
    pub fn resolve(&self) -> Result<String> {
        match self {
            EnvValue::Literal(s) => Ok(s.clone()),
            EnvValue::FromEnv { var, default } => match std::env::var(var) {
                Ok(val) => Ok(val),
                Err(_) => default
                    .clone()
                    .ok_or_else(|| Error::MissingEnvVar(var.clone())),
            },
        }
    }
}

pub fn resolve_env_map(map: &HashMap<String, EnvValue>) -> Result<HashMap<String, String>> {
    map.iter()
        .map(|(k, v)| v.resolve().map(|resolved| (k.clone(), resolved)))
        .collect()
}
