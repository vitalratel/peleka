// ABOUTME: Container health check configuration.
// ABOUTME: Defines HTTP health check parameters with sensible defaults.

use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
pub struct HealthcheckConfig {
    pub path: String,
    pub port: u16,

    #[serde(default = "default_interval", with = "humantime_serde")]
    pub interval: Duration,

    #[serde(default = "default_timeout", with = "humantime_serde")]
    pub timeout: Duration,

    #[serde(default = "default_retries")]
    pub retries: u32,

    #[serde(default = "default_start_period", with = "humantime_serde")]
    pub start_period: Duration,
}

fn default_interval() -> Duration {
    Duration::from_secs(10)
}

fn default_timeout() -> Duration {
    Duration::from_secs(5)
}

fn default_retries() -> u32 {
    3
}

fn default_start_period() -> Duration {
    Duration::from_secs(30)
}
