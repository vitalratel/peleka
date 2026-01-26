// ABOUTME: Integration tests for deploy lock functionality.
// ABOUTME: Tests lock acquisition, stale detection, and force breaking.

mod support;

use peleka::deploy::{DeployError, DeployErrorKind, DeployLock, LockInfo};
use peleka::ssh::{Session, SessionConfig};
use peleka::types::ServiceName;

/// Get SSH config for the shared SSH test container.
async fn ssh_session_config() -> SessionConfig {
    support::ssh_container::shared_container()
        .await
        .session_config()
}

/// Test: Lock acquired prevents second deployment.
#[tokio::test]
async fn lock_acquired_prevents_second_deployment() {
    let ssh_config = ssh_session_config().await;

    let session = Session::connect(ssh_config.clone())
        .await
        .expect("connection should succeed");

    let service = ServiceName::new("test-lock-prevent").unwrap();

    // Acquire first lock
    let lock = DeployLock::acquire(&session, &service, false)
        .await
        .expect("first lock should succeed");

    // Try to acquire second lock from same session (simulates concurrent deploy)
    let session2 = Session::connect(ssh_config)
        .await
        .expect("second connection should succeed");

    let result = DeployLock::acquire(&session2, &service, false).await;
    assert!(result.is_err(), "second lock should fail");

    let err = result.unwrap_err();
    assert_eq!(err.kind(), DeployErrorKind::LockHeld);
    let info = err
        .lock_holder_info()
        .expect("should have lock holder info");
    assert!(!info.holder.is_empty(), "holder should be set");
    assert!(info.pid > 0, "pid should be set");

    // Release first lock
    lock.release().await.expect("release should succeed");

    // Now second lock should work
    let lock2 = DeployLock::acquire(&session2, &service, false)
        .await
        .expect("lock should succeed after release");

    lock2.release().await.expect("cleanup release");
    session2.disconnect().await.expect("disconnect");
}

/// Test: Lock held returns error with holder info.
#[tokio::test]
async fn lock_held_returns_holder_info() {
    let ssh_config = ssh_session_config().await;

    let session = Session::connect(ssh_config.clone())
        .await
        .expect("connection should succeed");

    let service = ServiceName::new("test-lock-info").unwrap();

    // Acquire lock
    let lock = DeployLock::acquire(&session, &service, false)
        .await
        .expect("lock should succeed");

    // Try from another session
    let session2 = Session::connect(ssh_config)
        .await
        .expect("second connection should succeed");

    let result = DeployLock::acquire(&session2, &service, false).await;

    let err = result.unwrap_err();
    assert_eq!(err.kind(), DeployErrorKind::LockHeld);
    let info = err
        .lock_holder_info()
        .expect("should have lock holder info");
    // Verify holder info matches what we expect
    assert!(!info.holder.is_empty(), "holder hostname should be set");
    assert_eq!(
        info.pid,
        std::process::id(),
        "pid should match current process"
    );
    // started_at should be recent (within last minute)
    let age = chrono::Utc::now() - info.started_at;
    assert!(age.num_seconds() < 60, "lock should be recent");

    lock.release().await.expect("cleanup");
    session2.disconnect().await.expect("disconnect");
}

/// Test: --force breaks active lock.
#[tokio::test]
async fn force_breaks_active_lock() {
    let ssh_config = ssh_session_config().await;

    let session = Session::connect(ssh_config.clone())
        .await
        .expect("connection should succeed");

    let service = ServiceName::new("test-lock-force").unwrap();

    // Acquire first lock
    let _lock = DeployLock::acquire(&session, &service, false)
        .await
        .expect("first lock should succeed");

    // Force acquire from another session
    let session2 = Session::connect(ssh_config)
        .await
        .expect("second connection should succeed");

    let lock2 = DeployLock::acquire(&session2, &service, true)
        .await
        .expect("force lock should succeed");

    lock2.release().await.expect("cleanup");
    session2.disconnect().await.expect("disconnect");
}

/// Test: Lock cleaned up on release.
#[tokio::test]
async fn lock_cleaned_up_on_release() {
    let ssh_config = ssh_session_config().await;

    let session = Session::connect(ssh_config.clone())
        .await
        .expect("connection should succeed");

    let service = ServiceName::new("test-lock-cleanup").unwrap();
    let lock_path = LockInfo::lock_path(&service);

    // Acquire and release lock
    let lock = DeployLock::acquire(&session, &service, false)
        .await
        .expect("lock should succeed");

    // Lock file should exist
    assert!(
        session.file_exists(&lock_path).await.unwrap(),
        "lock file should exist while held"
    );

    lock.release().await.expect("release should succeed");

    // Lock file should be gone
    assert!(
        !session.file_exists(&lock_path).await.unwrap(),
        "lock file should be removed after release"
    );

    session.disconnect().await.expect("disconnect");
}

/// Test: Stale lock auto-breaks with warning.
#[tokio::test]
async fn stale_lock_auto_breaks() {
    let ssh_config = ssh_session_config().await;

    let session = Session::connect(ssh_config.clone())
        .await
        .expect("connection should succeed");

    let service = ServiceName::new("test-lock-stale").unwrap();
    let lock_path = LockInfo::lock_path(&service);

    // Create a stale lock file manually (2 hours old)
    let stale_lock = serde_json::json!({
        "holder": "old-machine",
        "pid": 99999,
        "started_at": (chrono::Utc::now() - chrono::Duration::hours(2)).to_rfc3339(),
        "service": service.to_string()
    });

    let write_cmd = format!("echo '{}' > {}", stale_lock, lock_path);
    session.exec(&write_cmd).await.expect("write stale lock");

    // Acquire should succeed (auto-break stale)
    let lock = DeployLock::acquire(&session, &service, false)
        .await
        .expect("should auto-break stale lock");

    lock.release().await.expect("cleanup");
    session.disconnect().await.expect("disconnect");
}

/// Test: with_lock executes closure and releases lock.
#[tokio::test]
async fn with_lock_executes_and_releases() {
    let ssh_config = ssh_session_config().await;

    let session = Session::connect(ssh_config.clone())
        .await
        .expect("connection should succeed");

    let service = ServiceName::new("test-with-lock").unwrap();
    let lock_path = LockInfo::lock_path(&service);

    // Execute work within lock scope
    let result: i32 = DeployLock::with_lock(&session, &service, false, async {
        // Verify lock exists during execution
        assert!(
            session.file_exists(&lock_path).await.unwrap(),
            "lock should exist during closure"
        );
        Ok::<_, DeployError>(42)
    })
    .await
    .expect("with_lock should succeed");

    assert_eq!(result, 42, "closure result should be returned");

    // Lock should be released after with_lock returns
    assert!(
        !session.file_exists(&lock_path).await.unwrap(),
        "lock should be released after with_lock"
    );

    session.disconnect().await.expect("disconnect");
}

/// Test: with_lock releases lock even when closure returns error.
#[tokio::test]
async fn with_lock_releases_on_error() {
    let ssh_config = ssh_session_config().await;

    let session = Session::connect(ssh_config.clone())
        .await
        .expect("connection should succeed");

    let service = ServiceName::new("test-with-lock-err").unwrap();
    let lock_path = LockInfo::lock_path(&service);

    // Execute work that fails
    let result: Result<(), DeployError> = DeployLock::with_lock(&session, &service, false, async {
        Err(DeployError::lock_error("intentional failure".to_string()))
    })
    .await;

    assert!(result.is_err(), "should propagate error");

    // Lock should still be released
    assert!(
        !session.file_exists(&lock_path).await.unwrap(),
        "lock should be released even on error"
    );

    session.disconnect().await.expect("disconnect");
}

/// Test: with_lock can be acquired after previous with_lock completes.
#[tokio::test]
async fn with_lock_allows_subsequent_acquisition() {
    let ssh_config = ssh_session_config().await;

    let session = Session::connect(ssh_config.clone())
        .await
        .expect("connection should succeed");

    let service = ServiceName::new("test-with-lock-seq").unwrap();

    // First with_lock
    let r1: i32 =
        DeployLock::with_lock(&session, &service, false, async { Ok::<_, DeployError>(1) })
            .await
            .expect("first should succeed");
    assert_eq!(r1, 1);

    // Second with_lock should work immediately
    let r2: i32 =
        DeployLock::with_lock(&session, &service, false, async { Ok::<_, DeployError>(2) })
            .await
            .expect("second should succeed");
    assert_eq!(r2, 2);

    session.disconnect().await.expect("disconnect");
}

/// Test: with_lock releases lock even when closure panics.
#[tokio::test]
async fn with_lock_releases_on_panic() {
    let ssh_config = ssh_session_config().await;

    let session = Session::connect(ssh_config.clone())
        .await
        .expect("connection should succeed");

    let service = ServiceName::new("test-with-lock-panic").unwrap();
    let lock_path = LockInfo::lock_path(&service);

    // Spawn a task that panics inside with_lock
    let session_clone = Session::connect(ssh_config.clone())
        .await
        .expect("connection should succeed");

    let handle = tokio::spawn(async move {
        let _: Result<(), DeployError> =
            DeployLock::with_lock(&session_clone, &service, false, async {
                panic!("intentional panic for testing");
            })
            .await;
    });

    // Wait for the task to complete (it will panic)
    let result = handle.await;
    assert!(result.is_err(), "task should have panicked");

    // Lock should be released despite the panic
    assert!(
        !session.file_exists(&lock_path).await.unwrap(),
        "lock should be released even on panic"
    );

    session.disconnect().await.expect("disconnect");
}
