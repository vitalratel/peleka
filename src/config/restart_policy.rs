// ABOUTME: Container restart policy configuration.
// ABOUTME: Supports no, always, unless-stopped, and on-failure[:max-retries].

use serde::de::{self, Deserialize, Deserializer};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum RestartPolicy {
    No,
    Always,
    #[default]
    UnlessStopped,
    OnFailure {
        max_retries: Option<u32>,
    },
}

impl FromStr for RestartPolicy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "no" => Ok(RestartPolicy::No),
            "always" => Ok(RestartPolicy::Always),
            "unless-stopped" => Ok(RestartPolicy::UnlessStopped),
            "on-failure" => Ok(RestartPolicy::OnFailure { max_retries: None }),
            s if s.starts_with("on-failure:") => {
                let retries_str = &s["on-failure:".len()..];
                let retries = retries_str
                    .parse::<u32>()
                    .map_err(|_| format!("invalid max retries: {}", retries_str))?;
                Ok(RestartPolicy::OnFailure {
                    max_retries: Some(retries),
                })
            }
            _ => Err(format!("unknown restart policy: {}", s)),
        }
    }
}

impl fmt::Display for RestartPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RestartPolicy::No => write!(f, "no"),
            RestartPolicy::Always => write!(f, "always"),
            RestartPolicy::UnlessStopped => write!(f, "unless-stopped"),
            RestartPolicy::OnFailure { max_retries: None } => write!(f, "on-failure"),
            RestartPolicy::OnFailure {
                max_retries: Some(n),
            } => write!(f, "on-failure:{}", n),
        }
    }
}

impl<'de> Deserialize<'de> for RestartPolicy {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(de::Error::custom)
    }
}
