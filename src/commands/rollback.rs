// ABOUTME: Rollback command implementation.
// ABOUTME: Handles reverting deployments to previous container versions.

use super::runtime_connection::connect_to_runtime;
use peleka::config::{Config, ServerConfig};
use peleka::deploy::manual_rollback;
use peleka::diagnostics::{Diagnostics, Warning};
use peleka::error::{Error, Result};
use peleka::output::Output;
use peleka::ssh::Session;

/// Rollback to previous deployment on all configured servers.
pub async fn rollback(config: Config, mut output: Output) -> Result<()> {
    if config.servers.is_empty() {
        return Err(Error::NoServers);
    }

    output.start_timer();
    let mut diag = Diagnostics::default();

    output.progress(&format!(
        "Rolling back {} on {} server(s)",
        config.service,
        config.servers.len()
    ));

    for server in &config.servers {
        if let Err(e) = rollback_on_server(&config, server, &output, &mut diag).await {
            eprintln!("Failed to rollback on {}: {}", server.host, e);
            return Err(e);
        }
    }

    // Emit collected warnings
    for warning in diag.warnings() {
        output.warning(&warning.message);
    }

    output.success("Rollback complete!");
    Ok(())
}

/// Rollback on a single server.
async fn rollback_on_server(
    config: &Config,
    server: &ServerConfig,
    output: &Output,
    diag: &mut Diagnostics,
) -> Result<()> {
    output.progress(&format!("  → Connecting to {}...", server.host));

    let session = Session::connect(server.ssh_session_config()).await?;
    let runtime = connect_to_runtime(&session, server, output).await?;

    // Get network ID
    let network_id = peleka::types::NetworkId::new(config.network_name().to_string());

    // Perform rollback
    output.progress("  → Swapping containers...");
    manual_rollback(
        &runtime,
        &config.service,
        &network_id,
        config.stop_timeout(),
    )
    .await?;

    output.progress("  ✓ Rollback successful");

    // Disconnect SSH session (non-fatal if it fails)
    if let Err(e) = session.disconnect().await {
        diag.warn(Warning::ssh_disconnect(format!(
            "SSH disconnect failed for {}: {}",
            server.host, e
        )));
    }

    Ok(())
}
