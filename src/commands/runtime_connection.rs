// ABOUTME: Shared helper for connecting to container runtimes via SSH.
// ABOUTME: Eliminates duplication across deploy, rollback, and exec commands.

use peleka::config::ServerConfig;
use peleka::error::Result;
use peleka::output::Output;
use peleka::runtime::{BollardRuntime, RuntimeError, connect_via_session, detect_runtime};
use peleka::ssh::Session;

/// Connect to the container runtime on a server via SSH.
///
/// This handles the common pattern of:
/// 1. Detecting the runtime type and socket path
/// 2. Outputting progress messages
/// 3. Establishing the connection
pub async fn connect_to_runtime(
    session: &Session,
    server: &ServerConfig,
    output: &Output,
) -> Result<BollardRuntime> {
    output.progress("  → Detecting runtime...");
    let runtime_info = detect_runtime(session, Some(&server.runtime_config()))
        .await
        .map_err(RuntimeError::from)?;

    output.progress(&format!(
        "  → Found {} at {}",
        runtime_info.runtime_type, runtime_info.socket_path
    ));

    let runtime = connect_via_session(session, runtime_info.runtime_type)
        .await
        .map_err(RuntimeError::from)?;

    Ok(runtime)
}
