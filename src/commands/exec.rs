// ABOUTME: Exec command implementation.
// ABOUTME: Handles executing commands inside service containers.

use super::deploy::find_existing_container;
use peleka::config::{Config, ServerConfig};
use peleka::deploy::DeployError;
use peleka::diagnostics::{Diagnostics, Warning};
use peleka::error::{Error, Result};
use peleka::output::Output;
use peleka::runtime::{ExecConfig, ExecOps, RuntimeError, connect_via_session, detect_runtime};
use peleka::ssh::Session;

/// Execute a command in the service container.
pub async fn exec_command(config: Config, command: Vec<String>, output: Output) -> Result<()> {
    if config.servers.is_empty() {
        return Err(Error::NoServers);
    }

    let mut diag = Diagnostics::default();

    // Execute on first server only
    let server = &config.servers[0];
    let result = exec_on_server(&config, server, &command, &output, &mut diag).await;

    // Emit collected warnings
    for warning in diag.warnings() {
        output.warning(&warning.message);
    }

    result
}

/// Execute a command on a single server.
async fn exec_on_server(
    config: &Config,
    server: &ServerConfig,
    command: &[String],
    output: &Output,
    diag: &mut Diagnostics,
) -> Result<()> {
    output.progress(&format!("  → Connecting to {}...", server.host));

    let session = Session::connect(server.ssh_session_config()).await?;

    // Detect runtime
    output.progress("  → Detecting runtime...");
    let runtime_info = detect_runtime(&session, Some(&server.runtime_config()))
        .await
        .map_err(RuntimeError::from)?;

    output.progress(&format!(
        "  → Found {} at {}",
        runtime_info.runtime_type, runtime_info.socket_path
    ));

    // Connect to runtime via SSH tunnel
    let runtime = connect_via_session(&session, runtime_info.runtime_type)
        .await
        .map_err(RuntimeError::from)?;

    // Find running container for this service
    let container_id = find_existing_container(&runtime, &config.service)
        .await?
        .ok_or_else(|| DeployError::config_error("no running container found for service"))?;

    output.progress(&format!("  → Executing in container {}...", container_id));

    // Build exec config
    let exec_config = ExecConfig {
        cmd: command.to_vec(),
        env: vec![],
        working_dir: None,
        user: None,
        attach_stdin: false,
        attach_stdout: true,
        attach_stderr: true,
        tty: false,
        privileged: false,
        timeout: None, // No timeout for CLI exec commands
    };

    // Execute command
    let result = runtime
        .exec(&container_id, &exec_config)
        .await
        .map_err(|e| DeployError::config_error(format!("exec failed: {}", e)))?;

    // Print output
    if !result.stdout.is_empty() {
        let stdout = String::from_utf8_lossy(&result.stdout);
        print!("{}", stdout);
    }
    if !result.stderr.is_empty() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        eprint!("{}", stderr);
    }

    // Check exit code
    if result.exit_code != 0 {
        return Err(DeployError::config_error(format!(
            "command exited with code {}",
            result.exit_code
        ))
        .into());
    }

    // Disconnect SSH session (non-fatal if it fails)
    if let Err(e) = session.disconnect().await {
        diag.warn(Warning::ssh_disconnect(format!(
            "SSH disconnect failed for {}: {}",
            server.host, e
        )));
    }

    Ok(())
}
