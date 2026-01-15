// ABOUTME: SSH socket forwarding implementation.
// ABOUTME: Tunnels local Unix socket to remote Unix socket via SSH.

use super::client::SshHandler;
use super::error::{Error, Result};
use russh::ChannelMsg;
use russh::client::Handle;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Notify;

/// Handle for managing a forwarded socket.
pub struct ForwardHandle {
    /// Path to the local socket.
    pub local_path: PathBuf,
    /// Signal to stop the forwarder.
    shutdown: Arc<AtomicBool>,
    /// Notification when shutdown is complete.
    shutdown_complete: Arc<Notify>,
}

impl ForwardHandle {
    /// Get the local socket path as a string.
    pub fn path(&self) -> &str {
        self.local_path.to_str().unwrap_or("")
    }

    /// Stop the forwarder and clean up the socket.
    pub async fn stop(self) {
        self.shutdown.store(true, Ordering::SeqCst);
        // Wait for shutdown to complete (with timeout)
        tokio::select! {
            _ = self.shutdown_complete.notified() => {}
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(2)) => {}
        }
        // Clean up socket file
        let _ = std::fs::remove_file(&self.local_path);
    }
}

impl Drop for ForwardHandle {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        let _ = std::fs::remove_file(&self.local_path);
    }
}

/// Start forwarding a local Unix socket to a remote Unix socket.
///
/// Creates a local socket at `/tmp/peleka-{pid}-{counter}.sock` and forwards
/// all connections through SSH to the specified remote socket path.
pub async fn start_forward(
    handle: Arc<Handle<SshHandler>>,
    remote_socket: String,
) -> Result<ForwardHandle> {
    // Generate unique local socket path
    let local_path = generate_socket_path();

    // Ensure parent directory exists and remove any stale socket
    if let Some(parent) = local_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let _ = std::fs::remove_file(&local_path);

    // Create listener
    let listener = UnixListener::bind(&local_path).map_err(|e| {
        Error::SocketForwardFailed(format!(
            "failed to bind local socket {:?}: {}",
            local_path, e
        ))
    })?;

    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_complete = Arc::new(Notify::new());

    let forward_handle = ForwardHandle {
        local_path: local_path.clone(),
        shutdown: shutdown.clone(),
        shutdown_complete: shutdown_complete.clone(),
    };

    // Spawn the forwarder task
    tokio::spawn(run_forwarder(
        listener,
        handle,
        remote_socket,
        shutdown,
        shutdown_complete,
    ));

    Ok(forward_handle)
}

/// Generate a unique local socket path.
fn generate_socket_path() -> PathBuf {
    use std::sync::atomic::AtomicU64;
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let pid = std::process::id();
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    PathBuf::from(format!("/tmp/peleka-{}-{}.sock", pid, count))
}

/// Run the forwarder, accepting connections and forwarding them.
async fn run_forwarder(
    listener: UnixListener,
    handle: Arc<Handle<SshHandler>>,
    remote_socket: String,
    shutdown: Arc<AtomicBool>,
    shutdown_complete: Arc<Notify>,
) {
    loop {
        if shutdown.load(Ordering::SeqCst) {
            break;
        }

        // Accept with timeout to check shutdown flag periodically
        let accept_result = tokio::select! {
            result = listener.accept() => result,
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => continue,
        };

        match accept_result {
            Ok((stream, _addr)) => {
                let handle_clone = Arc::clone(&handle);
                let remote_socket_clone = remote_socket.clone();
                tokio::spawn(async move {
                    if let Err(e) =
                        forward_connection(stream, &handle_clone, &remote_socket_clone).await
                    {
                        tracing::debug!("Forward connection error: {}", e);
                    }
                });
            }
            Err(e) => {
                if !shutdown.load(Ordering::SeqCst) {
                    tracing::warn!("Accept error on forwarded socket: {}", e);
                }
                break;
            }
        }
    }

    shutdown_complete.notify_one();
}

/// Forward a single connection through SSH.
async fn forward_connection(
    mut local_stream: UnixStream,
    handle: &Handle<SshHandler>,
    remote_socket: &str,
) -> Result<()> {
    // Open direct-streamlocal channel to remote Unix socket
    let mut channel = handle
        .channel_open_direct_streamlocal(remote_socket)
        .await
        .map_err(|e| {
            Error::SocketForwardFailed(format!(
                "failed to open streamlocal channel to {}: {}",
                remote_socket, e
            ))
        })?;

    let mut stream_closed = false;
    let mut channel_closed = false;
    let mut buf = vec![0u8; 65536];

    loop {
        tokio::select! {
            // Read from local socket
            r = local_stream.read(&mut buf), if !stream_closed => {
                match r {
                    Ok(0) => {
                        stream_closed = true;
                        let _ = channel.eof().await;
                    }
                    Ok(n) => {
                        if let Err(e) = channel.data(&buf[..n]).await {
                            tracing::debug!("Channel data error: {}", e);
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::debug!("Local stream read error: {}", e);
                        break;
                    }
                }
            }

            // Read from SSH channel
            msg = channel.wait(), if !channel_closed => {
                match msg {
                    Some(ChannelMsg::Data { ref data }) => {
                        if let Err(e) = local_stream.write_all(data).await {
                            tracing::debug!("Local stream write error: {}", e);
                            break;
                        }
                    }
                    Some(ChannelMsg::Eof) => {
                        channel_closed = true;
                        if stream_closed {
                            break;
                        }
                    }
                    Some(ChannelMsg::Close) => {
                        break;
                    }
                    Some(ChannelMsg::WindowAdjusted { .. }) => {}
                    Some(_) => {}
                    None => break,
                }
            }

            else => break,
        }
    }

    Ok(())
}
