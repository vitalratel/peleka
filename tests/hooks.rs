// ABOUTME: Integration tests for hooks system.
// ABOUTME: Tests hook discovery, execution, and environment variable passing.

use peleka::hooks::{HookContext, HookPoint, HookRunner};
use peleka::types::ServiceName;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;

fn create_hook(dir: &TempDir, name: &str, script: &str) {
    let hooks_dir = dir.path().join(".peleka").join("hooks");
    fs::create_dir_all(&hooks_dir).unwrap();

    let hook_path = hooks_dir.join(name);
    fs::write(&hook_path, script).unwrap();

    // Make executable
    let mut perms = fs::metadata(&hook_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&hook_path, perms).unwrap();
}

fn test_context() -> HookContext {
    HookContext {
        service: ServiceName::new("testapp").unwrap(),
        image: "testapp:v1.0.0".to_string(),
        server: "test.example.com".to_string(),
        runtime: "docker".to_string(),
        previous_version: Some("v0.9.0".to_string()),
    }
}

/// Test: pre-deploy hook runs before deployment.
#[tokio::test]
async fn pre_deploy_hook_runs() {
    let temp_dir = TempDir::new().unwrap();
    create_hook(
        &temp_dir,
        "pre-deploy",
        "#!/bin/sh\necho 'pre-deploy ran'\nexit 0\n",
    );

    let runner = HookRunner::new(temp_dir.path());
    assert!(runner.hook_exists(HookPoint::PreDeploy));

    let result = runner.run(HookPoint::PreDeploy, &test_context()).await;
    assert!(result.is_some());

    let result = result.unwrap();
    assert!(result.success);
    assert!(result.stdout.contains("pre-deploy ran"));
}

/// Test: post-deploy hook runs after successful deployment.
#[tokio::test]
async fn post_deploy_hook_runs() {
    let temp_dir = TempDir::new().unwrap();
    create_hook(
        &temp_dir,
        "post-deploy",
        "#!/bin/sh\necho 'post-deploy ran'\nexit 0\n",
    );

    let runner = HookRunner::new(temp_dir.path());
    assert!(runner.hook_exists(HookPoint::PostDeploy));

    let result = runner.run(HookPoint::PostDeploy, &test_context()).await;
    assert!(result.is_some());

    let result = result.unwrap();
    assert!(result.success);
    assert!(result.stdout.contains("post-deploy ran"));
}

/// Test: on-error hook runs on failure.
#[tokio::test]
async fn on_error_hook_runs() {
    let temp_dir = TempDir::new().unwrap();
    create_hook(
        &temp_dir,
        "on-error",
        "#!/bin/sh\necho 'on-error ran'\nexit 0\n",
    );

    let runner = HookRunner::new(temp_dir.path());
    assert!(runner.hook_exists(HookPoint::OnError));

    let result = runner.run(HookPoint::OnError, &test_context()).await;
    assert!(result.is_some());

    let result = result.unwrap();
    assert!(result.success);
    assert!(result.stdout.contains("on-error ran"));
}

/// Test: Hook failure in pre-deploy is detectable.
#[tokio::test]
async fn pre_deploy_failure_detected() {
    let temp_dir = TempDir::new().unwrap();
    create_hook(
        &temp_dir,
        "pre-deploy",
        "#!/bin/sh\necho 'failing' >&2\nexit 1\n",
    );

    let runner = HookRunner::new(temp_dir.path());
    let result = runner.run(HookPoint::PreDeploy, &test_context()).await;

    assert!(result.is_some());
    let result = result.unwrap();
    assert!(!result.success);
    assert_eq!(result.exit_code, Some(1));
    assert!(result.stderr.contains("failing"));
}

/// Test: Hook receives correct environment variables.
#[tokio::test]
async fn hook_receives_environment_variables() {
    let temp_dir = TempDir::new().unwrap();
    create_hook(
        &temp_dir,
        "pre-deploy",
        r#"#!/bin/sh
echo "SERVICE=$PELEKA_SERVICE"
echo "IMAGE=$PELEKA_IMAGE"
echo "SERVER=$PELEKA_SERVER"
echo "RUNTIME=$PELEKA_RUNTIME"
echo "PREVIOUS=$PELEKA_PREVIOUS_VERSION"
exit 0
"#,
    );

    let runner = HookRunner::new(temp_dir.path());
    let result = runner.run(HookPoint::PreDeploy, &test_context()).await;

    assert!(result.is_some());
    let result = result.unwrap();
    assert!(result.success);
    assert!(result.stdout.contains("SERVICE=testapp"));
    assert!(result.stdout.contains("IMAGE=testapp:v1.0.0"));
    assert!(result.stdout.contains("SERVER=test.example.com"));
    assert!(result.stdout.contains("RUNTIME=docker"));
    assert!(result.stdout.contains("PREVIOUS=v0.9.0"));
}

/// Test: Missing hook returns None.
#[tokio::test]
async fn missing_hook_returns_none() {
    let temp_dir = TempDir::new().unwrap();
    // Don't create any hooks

    let runner = HookRunner::new(temp_dir.path());
    assert!(!runner.hook_exists(HookPoint::PreDeploy));

    let result = runner.run(HookPoint::PreDeploy, &test_context()).await;
    assert!(result.is_none());
}
