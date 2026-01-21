// ABOUTME: Server configuration for SSH connections.
// ABOUTME: Parses formats like "host", "user@host", "host:port", "user@host:port".

use crate::runtime::RuntimeType;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub runtime: Option<RuntimeType>,
    #[serde(default)]
    pub socket: Option<String>,
    #[serde(default = "default_trust_first_connection")]
    pub trust_first_connection: bool,
}

fn default_port() -> u16 {
    22
}

fn default_trust_first_connection() -> bool {
    true
}

impl ServerConfig {
    pub fn parse(s: &str) -> Result<Self, String> {
        let s = s.trim();
        if s.is_empty() {
            return Err("server address cannot be empty".to_string());
        }

        // Parse format: [user@]host[:port]
        let (user_part, rest) = if let Some(at_pos) = s.find('@') {
            (Some(&s[..at_pos]), &s[at_pos + 1..])
        } else {
            (None, s)
        };

        let (host, port) = if let Some(colon_pos) = rest.rfind(':') {
            let port_str = &rest[colon_pos + 1..];
            let port = port_str
                .parse::<u16>()
                .map_err(|_| format!("invalid port: {}", port_str))?;
            (&rest[..colon_pos], port)
        } else {
            (rest, 22)
        };

        if host.is_empty() {
            return Err("hostname cannot be empty".to_string());
        }

        Ok(ServerConfig {
            host: host.to_string(),
            port,
            user: user_part.map(|s| s.to_string()),
            runtime: None,
            socket: None,
            trust_first_connection: true,
        })
    }

    /// Convert to RuntimeConfig for use with detect_runtime.
    pub fn runtime_config(&self) -> crate::runtime::RuntimeConfig {
        crate::runtime::RuntimeConfig {
            runtime: self.runtime,
            socket: self.socket.clone(),
        }
    }
}
