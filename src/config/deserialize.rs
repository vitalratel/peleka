// ABOUTME: Custom serde deserializers for config types.
// ABOUTME: Handles service names, image refs, and server lists.

use nonempty::NonEmpty;
use serde::Deserialize;

use super::ServerConfig;
use crate::types::{ImageRef, ServiceName};

pub fn deserialize_service_name<'de, D>(deserializer: D) -> Result<ServiceName, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    ServiceName::new(&s).map_err(serde::de::Error::custom)
}

pub fn deserialize_image_ref<'de, D>(deserializer: D) -> Result<ImageRef, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    ImageRef::parse(&s).map_err(serde::de::Error::custom)
}

pub fn deserialize_servers<'de, D>(deserializer: D) -> Result<NonEmpty<ServerConfig>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let values: Vec<ServerEntry> = Vec::deserialize(deserializer)?;
    let servers = values
        .into_iter()
        .map(|entry| entry.into_server_config())
        .collect::<Result<Vec<_>, _>>()
        .map_err(serde::de::Error::custom)?;

    NonEmpty::from_vec(servers)
        .ok_or_else(|| serde::de::Error::custom("at least one server is required"))
}

pub fn deserialize_servers_option<'de, D>(
    deserializer: D,
) -> Result<Option<NonEmpty<ServerConfig>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt: Option<Vec<ServerEntry>> = Option::deserialize(deserializer)?;
    match opt {
        None => Ok(None),
        Some(values) => {
            let servers = values
                .into_iter()
                .map(|entry| entry.into_server_config())
                .collect::<Result<Vec<_>, _>>()
                .map_err(serde::de::Error::custom)?;

            let nonempty = NonEmpty::from_vec(servers).ok_or_else(|| {
                serde::de::Error::custom("destination servers list cannot be empty")
            })?;
            Ok(Some(nonempty))
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ServerEntry {
    Simple(String),
    Detailed(ServerConfig),
}

impl ServerEntry {
    fn into_server_config(self) -> Result<ServerConfig, String> {
        match self {
            ServerEntry::Simple(s) => ServerConfig::parse(&s),
            ServerEntry::Detailed(c) => Ok(c),
        }
    }
}
