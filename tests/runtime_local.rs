// ABOUTME: Integration tests for container runtime operations.
// ABOUTME: Tests run against local Docker/Podman daemon without SSH.

use futures::StreamExt;
use peleka::runtime::{
    BollardRuntime, ContainerConfig, ContainerFilters, ContainerOps, ExecConfig, ExecOps, ImageOps,
    LogOps, LogOptions, NetworkConfig, NetworkOps, RestartPolicyConfig, RuntimeInfoTrait,
    detect_local,
};
use peleka::types::ImageRef;
use std::collections::HashMap;
use std::time::Duration;

/// Get local runtime, skipping test if unavailable.
fn local_runtime() -> Option<BollardRuntime> {
    let info = detect_local().ok()?;
    BollardRuntime::connect(&info).ok()
}

/// Skip test if no local runtime available.
macro_rules! require_runtime {
    () => {
        match local_runtime() {
            Some(rt) => rt,
            None => {
                eprintln!("Skipping test: no local container runtime found");
                return;
            }
        }
    };
}

// =============================================================================
// RuntimeInfo Tests
// =============================================================================

#[tokio::test]
async fn runtime_info() {
    let runtime = require_runtime!();

    let info = runtime.info().await.expect("should get runtime info");

    assert!(
        !info.name.is_empty(),
        "runtime name should not be empty, got: {}",
        info.name
    );
    assert!(
        !info.version.is_empty(),
        "runtime version should not be empty"
    );
}

#[tokio::test]
async fn runtime_ping() {
    let runtime = require_runtime!();
    runtime.ping().await.expect("ping should succeed");
}

// =============================================================================
// ImageOps Tests
// =============================================================================

#[tokio::test]
async fn pull_public_image() {
    let runtime = require_runtime!();

    let image_ref = ImageRef::parse("alpine:latest").expect("valid image ref");

    runtime
        .pull_image(&image_ref, None)
        .await
        .expect("pull should succeed");

    let exists = runtime
        .image_exists(&image_ref)
        .await
        .expect("image_exists should succeed");
    assert!(exists, "image should exist after pull");
}

#[tokio::test]
async fn image_exists_false_for_nonexistent() {
    let runtime = require_runtime!();

    let image_ref = ImageRef::parse("this-image-definitely-does-not-exist-12345:v999")
        .expect("valid image ref");

    let exists = runtime
        .image_exists(&image_ref)
        .await
        .expect("image_exists should succeed");
    assert!(!exists, "non-existent image should return false");
}

// =============================================================================
// ContainerOps Tests
// =============================================================================

#[tokio::test]
async fn container_lifecycle() {
    let runtime = require_runtime!();

    // Ensure we have the alpine image
    let image_ref = ImageRef::parse("alpine:latest").expect("valid image ref");
    if !runtime.image_exists(&image_ref).await.unwrap_or(false) {
        runtime
            .pull_image(&image_ref, None)
            .await
            .expect("pull should succeed");
    }

    let container_name = format!("peleka-test-{}", std::process::id());
    let config = ContainerConfig {
        name: container_name.clone(),
        image: image_ref,
        env: HashMap::new(),
        labels: {
            let mut labels = HashMap::new();
            labels.insert("peleka.test".to_string(), "true".to_string());
            labels
        },
        ports: vec![],
        volumes: vec![],
        command: Some(vec!["sleep".to_string(), "30".to_string()]),
        entrypoint: None,
        working_dir: None,
        user: None,
        restart_policy: RestartPolicyConfig::No,
        resources: None,
        healthcheck: None,
        stop_timeout: Some(Duration::from_secs(5)),
        network: None,
        network_aliases: vec![],
    };

    // Create container
    let container_id = runtime
        .create_container(&config)
        .await
        .expect("create_container should succeed");

    // Start container
    runtime
        .start_container(&container_id)
        .await
        .expect("start_container should succeed");

    // Inspect container - should be running
    let info = runtime
        .inspect_container(&container_id)
        .await
        .expect("inspect_container should succeed");
    assert_eq!(
        info.state,
        peleka::runtime::ContainerState::Running,
        "container should be running"
    );

    // List containers with our label
    let filters = ContainerFilters {
        labels: {
            let mut labels = HashMap::new();
            labels.insert("peleka.test".to_string(), "true".to_string());
            labels
        },
        name: None,
        all: false,
    };
    let containers = runtime
        .list_containers(&filters)
        .await
        .expect("list_containers should succeed");
    assert!(
        containers.iter().any(|c| c.id == container_id),
        "our container should be in the list"
    );

    // Stop container
    runtime
        .stop_container(&container_id, Duration::from_secs(5))
        .await
        .expect("stop_container should succeed");

    // Remove container
    runtime
        .remove_container(&container_id, false)
        .await
        .expect("remove_container should succeed");

    // Verify container is gone
    let result = runtime.inspect_container(&container_id).await;
    assert!(result.is_err(), "container should not exist after removal");
}

#[tokio::test]
async fn rename_container() {
    let runtime = require_runtime!();

    let image_ref = ImageRef::parse("alpine:latest").expect("valid image ref");
    if !runtime.image_exists(&image_ref).await.unwrap_or(false) {
        runtime
            .pull_image(&image_ref, None)
            .await
            .expect("pull should succeed");
    }

    let original_name = format!("peleka-rename-test-{}", std::process::id());
    let new_name = format!("peleka-renamed-{}", std::process::id());

    let config = ContainerConfig {
        name: original_name.clone(),
        image: image_ref,
        env: HashMap::new(),
        labels: HashMap::new(),
        ports: vec![],
        volumes: vec![],
        command: Some(vec!["sleep".to_string(), "30".to_string()]),
        entrypoint: None,
        working_dir: None,
        user: None,
        restart_policy: RestartPolicyConfig::No,
        resources: None,
        healthcheck: None,
        stop_timeout: None,
        network: None,
        network_aliases: vec![],
    };

    let container_id = runtime
        .create_container(&config)
        .await
        .expect("create_container should succeed");

    runtime
        .rename_container(&container_id, &new_name)
        .await
        .expect("rename_container should succeed");

    let info = runtime
        .inspect_container(&container_id)
        .await
        .expect("inspect should succeed");
    assert_eq!(info.name, new_name, "container should have new name");

    // Cleanup
    runtime
        .remove_container(&container_id, true)
        .await
        .expect("cleanup should succeed");
}

// =============================================================================
// NetworkOps Tests
// =============================================================================

#[tokio::test]
async fn network_operations() {
    let runtime = require_runtime!();

    let image_ref = ImageRef::parse("alpine:latest").expect("valid image ref");
    if !runtime.image_exists(&image_ref).await.unwrap_or(false) {
        runtime
            .pull_image(&image_ref, None)
            .await
            .expect("pull should succeed");
    }

    let network_name = format!("peleka-net-test-{}", std::process::id());
    let container_name = format!("peleka-net-container-{}", std::process::id());

    // Create network
    let network_config = NetworkConfig {
        name: network_name.clone(),
        driver: Some("bridge".to_string()),
        labels: HashMap::new(),
    };
    let network_id = runtime
        .create_network(&network_config)
        .await
        .expect("create_network should succeed");

    let exists = runtime
        .network_exists(&network_name)
        .await
        .expect("network_exists should succeed");
    assert!(exists, "network should exist after creation");

    // Create and start a container
    let container_config = ContainerConfig {
        name: container_name.clone(),
        image: image_ref,
        env: HashMap::new(),
        labels: HashMap::new(),
        ports: vec![],
        volumes: vec![],
        command: Some(vec!["sleep".to_string(), "30".to_string()]),
        entrypoint: None,
        working_dir: None,
        user: None,
        restart_policy: RestartPolicyConfig::No,
        resources: None,
        healthcheck: None,
        stop_timeout: None,
        network: None,
        network_aliases: vec![],
    };
    let container_id = runtime
        .create_container(&container_config)
        .await
        .expect("create_container should succeed");

    runtime
        .start_container(&container_id)
        .await
        .expect("start_container should succeed");

    // Connect container to network with alias
    let alias = peleka::types::NetworkAlias::new("my-service").expect("valid alias");
    runtime
        .connect_to_network(&container_id, &network_id, &[alias])
        .await
        .expect("connect_to_network should succeed");

    let info = runtime
        .inspect_container(&container_id)
        .await
        .expect("inspect should succeed");
    assert!(
        info.network_settings.networks.contains_key(&network_name),
        "container should be connected to our network"
    );

    // Disconnect from network
    runtime
        .disconnect_from_network(&container_id, &network_id)
        .await
        .expect("disconnect_from_network should succeed");

    // Cleanup
    runtime
        .stop_container(&container_id, Duration::from_secs(5))
        .await
        .ok();
    runtime
        .remove_container(&container_id, true)
        .await
        .expect("remove_container should succeed");
    runtime
        .remove_network(&network_id)
        .await
        .expect("remove_network should succeed");

    let exists = runtime
        .network_exists(&network_name)
        .await
        .expect("network_exists should succeed");
    assert!(!exists, "network should not exist after removal");
}

// =============================================================================
// ExecOps Tests
// =============================================================================

#[tokio::test]
async fn exec_command() {
    let runtime = require_runtime!();

    let image_ref = ImageRef::parse("alpine:latest").expect("valid image ref");
    if !runtime.image_exists(&image_ref).await.unwrap_or(false) {
        runtime
            .pull_image(&image_ref, None)
            .await
            .expect("pull should succeed");
    }

    let container_name = format!("peleka-exec-test-{}", std::process::id());

    let container_config = ContainerConfig {
        name: container_name.clone(),
        image: image_ref,
        env: HashMap::new(),
        labels: HashMap::new(),
        ports: vec![],
        volumes: vec![],
        command: Some(vec!["sleep".to_string(), "60".to_string()]),
        entrypoint: None,
        working_dir: None,
        user: None,
        restart_policy: RestartPolicyConfig::No,
        resources: None,
        healthcheck: None,
        stop_timeout: None,
        network: None,
        network_aliases: vec![],
    };
    let container_id = runtime
        .create_container(&container_config)
        .await
        .expect("create_container should succeed");

    runtime
        .start_container(&container_id)
        .await
        .expect("start_container should succeed");

    // Execute a command
    let exec_config = ExecConfig {
        cmd: vec!["echo".to_string(), "hello world".to_string()],
        env: vec![],
        working_dir: None,
        user: None,
        attach_stdin: false,
        attach_stdout: true,
        attach_stderr: true,
        tty: false,
        privileged: false,
    };

    let result = runtime
        .exec(&container_id, &exec_config)
        .await
        .expect("exec should succeed");

    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(
        stdout.contains("hello world"),
        "stdout should contain 'hello world', got: {}",
        stdout
    );

    // Test exec with environment variable
    let exec_config_env = ExecConfig {
        cmd: vec![
            "sh".to_string(),
            "-c".to_string(),
            "echo $MY_VAR".to_string(),
        ],
        env: vec!["MY_VAR=test_value".to_string()],
        working_dir: None,
        user: None,
        attach_stdin: false,
        attach_stdout: true,
        attach_stderr: true,
        tty: false,
        privileged: false,
    };

    let result_env = runtime
        .exec(&container_id, &exec_config_env)
        .await
        .expect("exec with env should succeed");

    let stdout_env = String::from_utf8_lossy(&result_env.stdout);
    assert!(
        stdout_env.contains("test_value"),
        "stdout should contain env var value, got: {}",
        stdout_env
    );

    // Cleanup
    runtime
        .stop_container(&container_id, Duration::from_secs(5))
        .await
        .ok();
    runtime
        .remove_container(&container_id, true)
        .await
        .expect("cleanup should succeed");
}

// =============================================================================
// LogOps Tests
// =============================================================================

#[tokio::test]
async fn log_streaming() {
    let runtime = require_runtime!();

    let image_ref = ImageRef::parse("alpine:latest").expect("valid image ref");
    if !runtime.image_exists(&image_ref).await.unwrap_or(false) {
        runtime
            .pull_image(&image_ref, None)
            .await
            .expect("pull should succeed");
    }

    let container_name = format!("peleka-log-test-{}", std::process::id());

    let container_config = ContainerConfig {
        name: container_name.clone(),
        image: image_ref,
        env: HashMap::new(),
        labels: HashMap::new(),
        ports: vec![],
        volumes: vec![],
        command: Some(vec![
            "sh".to_string(),
            "-c".to_string(),
            "echo 'log line 1'; echo 'log line 2'; echo 'log line 3'; sleep 2".to_string(),
        ]),
        entrypoint: None,
        working_dir: None,
        user: None,
        restart_policy: RestartPolicyConfig::No,
        resources: None,
        healthcheck: None,
        stop_timeout: None,
        network: None,
        network_aliases: vec![],
    };
    let container_id = runtime
        .create_container(&container_config)
        .await
        .expect("create_container should succeed");

    runtime
        .start_container(&container_id)
        .await
        .expect("start_container should succeed");

    // Wait for logs to be generated
    tokio::time::sleep(Duration::from_millis(500)).await;

    let log_opts = LogOptions {
        stdout: true,
        stderr: true,
        follow: false,
        timestamps: false,
        tail: None,
        since: None,
        until: None,
    };

    let mut log_stream = runtime
        .container_logs(&container_id, &log_opts)
        .await
        .expect("container_logs should succeed");

    let mut log_content = String::new();
    while let Some(result) = log_stream.next().await {
        match result {
            Ok(line) => log_content.push_str(&line.content),
            Err(e) => panic!("log stream error: {}", e),
        }
    }

    assert!(
        log_content.contains("log line 1"),
        "logs should contain 'log line 1', got: {}",
        log_content
    );
    assert!(
        log_content.contains("log line 2"),
        "logs should contain 'log line 2'"
    );
    assert!(
        log_content.contains("log line 3"),
        "logs should contain 'log line 3'"
    );

    // Cleanup
    runtime
        .stop_container(&container_id, Duration::from_secs(5))
        .await
        .ok();
    runtime
        .remove_container(&container_id, true)
        .await
        .expect("cleanup should succeed");
}
