// ABOUTME: Container graceful shutdown configuration.
// ABOUTME: Defines timeout and signal for stopping containers.

use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
pub struct StopConfig {
    #[serde(default = "default_timeout", with = "humantime_serde")]
    pub timeout: Duration,

    #[serde(default = "default_signal")]
    pub signal: String,
}

fn default_timeout() -> Duration {
    Duration::from_secs(30)
}

fn default_signal() -> String {
    "SIGTERM".to_string()
}

impl Default for StopConfig {
    fn default() -> Self {
        StopConfig {
            timeout: default_timeout(),
            signal: default_signal(),
        }
    }
}
