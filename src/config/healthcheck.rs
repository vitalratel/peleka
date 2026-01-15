// ABOUTME: Container health check configuration.
// ABOUTME: User-defined health check command with timing parameters.

use serde::Deserialize;
use std::time::Duration;

/// Health check configuration.
///
/// The `cmd` field is a shell command that runs inside the container.
/// Exit code 0 = healthy, non-zero = unhealthy.
///
/// # Examples
///
/// ```yaml
/// healthcheck:
///   cmd: "curl -f http://localhost:3000/health"
///   interval: 10s
///   timeout: 5s
///   retries: 3
/// ```
///
/// Common patterns:
/// - HTTP with curl: `curl -f http://localhost:3000/health`
/// - HTTP with wget: `wget -q --spider http://localhost:80/health`
/// - TCP check: `nc -z localhost 3000`
/// - Custom binary: `/app/healthcheck`
/// - PostgreSQL: `pg_isready -U postgres`
/// - Redis: `redis-cli ping`
#[derive(Debug, Clone, Deserialize)]
pub struct HealthcheckConfig {
    /// Shell command to run inside the container.
    /// Exit code 0 = healthy, non-zero = unhealthy.
    pub cmd: String,

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
