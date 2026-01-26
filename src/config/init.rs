// ABOUTME: Config scaffolding for new projects.
// ABOUTME: Creates peleka.yml template files.

use std::path::Path;

use crate::error::{Error, Result};
use crate::types::{ImageRef, ServiceName};

use super::{CONFIG_FILENAME, Config};

pub fn init_config(
    dir: &Path,
    service: Option<&str>,
    image: Option<&str>,
    force: bool,
) -> Result<()> {
    let config_path = dir.join(CONFIG_FILENAME);

    if config_path.exists() && !force {
        return Err(Error::AlreadyExists(config_path));
    }

    let mut config = Config::template();

    if let Some(s) = service {
        config.service = ServiceName::new(s).map_err(|e| Error::InvalidConfig(e.to_string()))?;
    }

    if let Some(i) = image {
        config.image = ImageRef::parse(i).map_err(|e| Error::InvalidConfig(e.to_string()))?;
    }

    let yaml = generate_template_yaml(&config);
    std::fs::write(&config_path, yaml)?;

    Ok(())
}

fn generate_template_yaml(config: &Config) -> String {
    let first_server = config.servers.first();
    format!(
        r#"service: {}
image: {}
servers:
  - host: {}
    port: {}
    user: {}
    # SSH host key verification (default: false for security)
    # Set to true to enable Trust-On-First-Use, or pre-populate ~/.ssh/known_hosts
    # trust_first_connection: true
"#,
        config.service,
        config.image,
        first_server.host,
        first_server.port,
        first_server.user.as_deref().unwrap_or("deploy")
    )
}
