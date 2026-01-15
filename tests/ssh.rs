// ABOUTME: Integration tests for SSH module.
// ABOUTME: Tests run against real SSH servers (Podman container or VPS).

use peleka::ssh::{Error, Session, SessionConfig};
use std::env;

/// Get test SSH configuration from environment.
/// Set SSH_TEST_HOST, SSH_TEST_PORT, SSH_TEST_USER, SSH_KEY to configure.
/// Set SSH_TEST_TOFU=1 to enable trust-on-first-use for testing.
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

/// Test: Connect to SSH server and execute `echo hello`.
/// Expected: Returns "hello" with exit code 0.
#[tokio::test]
#[ignore = "requires SSH_TEST_HOST environment variable"]
async fn connect_and_execute_echo() {
    let config = test_config().expect("SSH_TEST_HOST must be set");

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
#[ignore = "requires SSH_TEST_HOST environment variable"]
async fn capture_stderr() {
    let config = test_config().expect("SSH_TEST_HOST must be set");

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
#[ignore = "requires SSH_TEST_HOST environment variable"]
async fn nonzero_exit_code() {
    let config = test_config().expect("SSH_TEST_HOST must be set");

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
#[ignore = "requires SSH_TEST_HOST environment variable"]
async fn invalid_key_returns_auth_error() {
    let Some(mut config) = test_config() else {
        panic!("SSH_TEST_HOST must be set");
    };

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

/// Test: Unknown host is rejected when TOFU is disabled.
/// Uses 127.0.0.1 which has a different known_hosts entry than localhost.
#[tokio::test]
#[ignore = "requires SSH_TEST_HOST environment variable"]
async fn unknown_host_rejected_without_tofu() {
    let Some(base_config) = test_config() else {
        panic!("SSH_TEST_HOST must be set");
    };

    // Use 127.0.0.1 instead of localhost - different known_hosts entry
    let config = SessionConfig::new("127.0.0.1", &base_config.user)
        .port(base_config.port)
        .key_path(env::var("SSH_KEY").expect("SSH_KEY must be set for this test"))
        .trust_on_first_use(false);

    let result = Session::connect(config).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    // Should fail during connection due to unknown host key
    assert!(
        matches!(err, Error::Connection(_)),
        "expected Connection error for unknown host, got: {:?}",
        err
    );
}
