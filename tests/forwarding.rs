// ABOUTME: Integration tests for SSH socket forwarding.
// ABOUTME: Tests tunnel local Unix socket to remote container runtime socket.

use peleka::ssh::{Session, SessionConfig};
use std::env;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

/// Get test SSH configuration from environment.
fn test_config() -> Option<SessionConfig> {
    let host = env::var("SSH_TEST_HOST").ok()?;
    let user = env::var("SSH_TEST_USER").ok().or_else(whoami)?;
    let port: u16 = env::var("SSH_TEST_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(22);
    let key_path = env::var("SSH_KEY").ok();
    let tofu = env::var("SSH_TEST_TOFU").is_ok();

    let mut config = SessionConfig::new(host, user)
        .port(port)
        .trust_on_first_use(tofu);
    if let Some(path) = key_path {
        config = config.key_path(path);
    }
    Some(config)
}

fn whoami() -> Option<String> {
    env::var("USER").ok()
}

/// Test: Forward local socket to remote Podman socket and ping API.
/// Expected: Can send HTTP request through forwarded socket.
#[tokio::test]
#[ignore = "requires SSH_TEST_HOST with Podman"]
async fn forward_to_podman_socket() {
    let config = test_config().expect("SSH_TEST_HOST must be set");

    let mut session = Session::connect(config)
        .await
        .expect("connection should succeed");

    // Detect the remote Podman socket path
    let uid_output = session.exec("id -u").await.expect("should get uid");
    let uid = uid_output.stdout.trim();
    let remote_socket = format!("/run/user/{}/podman/podman.sock", uid);

    // Start socket forwarding - returns local socket path
    let local_socket_path = session
        .forward_socket(&remote_socket)
        .await
        .expect("forwarding should succeed");

    // Connect to local forwarded socket
    let mut stream = UnixStream::connect(&local_socket_path)
        .await
        .expect("should connect to local socket");

    // Send a simple HTTP request to Podman API (/_ping endpoint)
    let request = "GET /_ping HTTP/1.1\r\nHost: localhost\r\n\r\n";
    stream
        .write_all(request.as_bytes())
        .await
        .expect("should write request");

    // Read response
    let mut response = vec![0u8; 1024];
    let n = stream
        .read(&mut response)
        .await
        .expect("should read response");
    let response_str = String::from_utf8_lossy(&response[..n]);

    // Podman /_ping returns "OK" with 200 status
    assert!(
        response_str.contains("200") || response_str.contains("OK"),
        "expected successful response, got: {}",
        response_str
    );

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}

/// Test: Forward socket cleanup on disconnect.
/// Expected: Local socket is removed after session ends.
#[tokio::test]
#[ignore = "requires SSH_TEST_HOST with Podman"]
async fn forward_socket_cleanup_on_disconnect() {
    let config = test_config().expect("SSH_TEST_HOST must be set");

    let mut session = Session::connect(config)
        .await
        .expect("connection should succeed");

    let uid_output = session.exec("id -u").await.expect("should get uid");
    let uid = uid_output.stdout.trim();
    let remote_socket = format!("/run/user/{}/podman/podman.sock", uid);

    let local_socket_path = session
        .forward_socket(&remote_socket)
        .await
        .expect("forwarding should succeed");

    // Verify socket exists
    assert!(
        std::path::Path::new(&local_socket_path).exists(),
        "local socket should exist"
    );

    // Disconnect session
    session
        .disconnect()
        .await
        .expect("disconnect should succeed");

    // Give cleanup a moment
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Socket should be cleaned up
    assert!(
        !std::path::Path::new(&local_socket_path).exists(),
        "local socket should be cleaned up after disconnect"
    );
}

/// Test: Multiple connections through forwarded socket.
/// Expected: Can make multiple sequential requests.
#[tokio::test]
#[ignore = "requires SSH_TEST_HOST with Podman"]
async fn forward_multiple_connections() {
    let config = test_config().expect("SSH_TEST_HOST must be set");

    let mut session = Session::connect(config)
        .await
        .expect("connection should succeed");

    let uid_output = session.exec("id -u").await.expect("should get uid");
    let uid = uid_output.stdout.trim();
    let remote_socket = format!("/run/user/{}/podman/podman.sock", uid);

    let local_socket_path = session
        .forward_socket(&remote_socket)
        .await
        .expect("forwarding should succeed");

    // Make multiple sequential requests
    for i in 0..3 {
        let mut stream = UnixStream::connect(&local_socket_path)
            .await
            .expect("should connect to local socket");

        let request = "GET /_ping HTTP/1.1\r\nHost: localhost\r\n\r\n";
        stream
            .write_all(request.as_bytes())
            .await
            .expect("should write request");

        let mut response = vec![0u8; 1024];
        let n = stream
            .read(&mut response)
            .await
            .expect("should read response");
        let response_str = String::from_utf8_lossy(&response[..n]);

        assert!(
            response_str.contains("200") || response_str.contains("OK"),
            "request {} should succeed, got: {}",
            i,
            response_str
        );
    }

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}
