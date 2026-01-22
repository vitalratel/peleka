// ABOUTME: Integration tests for SSH socket forwarding.
// ABOUTME: Tests tunnel local Unix socket to remote container runtime socket.

mod support;

use peleka::ssh::Session;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

/// Test: Forward local socket to remote Podman socket and ping API.
/// Expected: Can send HTTP request through forwarded socket.
#[tokio::test]
async fn forward_to_podman_socket() {
    support::init_tracing();
    let config = support::podman_session_config().await;

    let session = Session::connect(config)
        .await
        .expect("connection should succeed");

    // Detect the remote Podman socket path (rootful or rootless)
    let remote_socket = support::detect_podman_socket(&session).await;

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
async fn forward_socket_cleanup_on_disconnect() {
    let config = support::podman_session_config().await;

    let session = Session::connect(config)
        .await
        .expect("connection should succeed");

    let remote_socket = support::detect_podman_socket(&session).await;

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
async fn forward_multiple_connections() {
    let config = support::podman_session_config().await;

    let session = Session::connect(config)
        .await
        .expect("connection should succeed");

    let remote_socket = support::detect_podman_socket(&session).await;

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
