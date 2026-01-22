// ABOUTME: Integration tests for SSH module.
// ABOUTME: Tests run against a shared SSH container.

mod support;

use peleka::ssh::{Error, Session, SessionConfig};
use support::ssh_container::shared_container;

/// Test: Connect to SSH server and execute `echo hello`.
/// Expected: Returns "hello" with exit code 0.
#[tokio::test]
async fn connect_and_execute_echo() {
    let container = shared_container().await;
    let config = container.session_config();

    let session = Session::connect(config)
        .await
        .expect("connection should succeed");

    let output = session
        .exec("echo hello")
        .await
        .expect("command should succeed");

    assert!(output.success(), "exit code should be 0");
    assert_eq!(output.stdout.trim(), "hello");

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}

/// Test: Execute command that writes to stderr.
/// Expected: stderr is captured correctly.
#[tokio::test]
async fn capture_stderr() {
    let container = shared_container().await;
    let config = container.session_config();

    let session = Session::connect(config)
        .await
        .expect("connection should succeed");

    let output = session
        .exec("echo error >&2")
        .await
        .expect("command should succeed");

    assert!(output.success());
    assert!(output.stdout.is_empty() || output.stdout.trim().is_empty());
    assert_eq!(output.stderr.trim(), "error");

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}

/// Test: Execute command with non-zero exit code.
/// Expected: exit_code reflects the actual exit status.
#[tokio::test]
async fn nonzero_exit_code() {
    let container = shared_container().await;
    let config = container.session_config();

    let session = Session::connect(config)
        .await
        .expect("connection should succeed");

    let output = session
        .exec("exit 42")
        .await
        .expect("command should complete");

    assert_eq!(output.exit_code, 42);
    assert!(!output.success());

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}

/// Test: Connection to invalid host fails with connection error.
#[tokio::test]
async fn invalid_host_returns_connection_error() {
    let config = SessionConfig::new("nonexistent.invalid.host.example", "testuser");

    let result = Session::connect(config).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, Error::Connection(_)),
        "expected Connection error, got: {:?}",
        err
    );
}

/// Test: Connection with invalid key returns auth error.
#[tokio::test]
async fn invalid_key_returns_auth_error() {
    let container = shared_container().await;
    let mut config = container.session_config();

    // Use a non-existent key path
    config = config.key_path("/nonexistent/key/path");

    let result = Session::connect(config).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, Error::KeyLoadFailed { .. }),
        "expected KeyLoadFailed error, got: {:?}",
        err
    );
}

/// Test: file_exists returns true for existing file.
#[tokio::test]
async fn file_exists_returns_true_for_existing_file() {
    let container = shared_container().await;
    let config = container.session_config();

    let session = Session::connect(config)
        .await
        .expect("connection should succeed");

    let exists = session
        .file_exists("/etc/passwd")
        .await
        .expect("file_exists should succeed");

    assert!(exists, "/etc/passwd should exist");

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}

/// Test: file_exists returns false for non-existing file.
#[tokio::test]
async fn file_exists_returns_false_for_nonexistent_file() {
    let container = shared_container().await;
    let config = container.session_config();

    let session = Session::connect(config)
        .await
        .expect("connection should succeed");

    let exists = session
        .file_exists("/nonexistent/path/that/does/not/exist")
        .await
        .expect("file_exists should succeed");

    assert!(!exists, "nonexistent path should not exist");

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}

/// Test: Command times out when execution exceeds timeout.
#[tokio::test]
async fn command_timeout_returns_error() {
    use std::time::Duration;

    let container = shared_container().await;
    let config = container.session_config();

    let session = Session::connect(config)
        .await
        .expect("connection should succeed");

    // Execute a sleep command with a very short timeout
    let result = session
        .exec_with_timeout("sleep 10", Duration::from_millis(100))
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, Error::CommandTimeout(_)),
        "expected CommandTimeout error, got: {:?}",
        err
    );

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}
