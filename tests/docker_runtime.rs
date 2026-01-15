// ABOUTME: Integration tests for Docker runtime implementation.
// ABOUTME: Tests run against real Docker/Podman daemon via SSH tunnel.

use futures::StreamExt;
use peleka::runtime::{
    ContainerConfig, ContainerFilters, ContainerOps, ExecConfig, ExecOps, ImageOps, LogOps,
    LogOptions, NetworkConfig, NetworkOps, RestartPolicyConfig, RuntimeInfoTrait, RuntimeType,
};
use peleka::ssh::{Session, SessionConfig};
use peleka::types::ImageRef;
use std::collections::HashMap;
use std::env;
use std::time::Duration;

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

/// Test: Create Docker runtime via SSH tunnel and get info.
/// Requires user to be in docker group.
#[tokio::test]
#[ignore = "requires SSH_TEST_HOST with Docker and docker group membership"]
async fn docker_runtime_info() {
    let config = test_config().expect("SSH_TEST_HOST must be set");

    let mut session = Session::connect(config)
        .await
        .expect("connection should succeed");

    // Create Docker runtime connected via SSH tunnel
    let runtime = peleka::runtime::docker::connect_via_session(&mut session, RuntimeType::Docker)
        .await
        .expect("should create Docker runtime");

    // Get runtime info
    let info = runtime.info().await.expect("should get runtime info");

    assert!(
        info.name.to_lowercase().contains("docker"),
        "runtime name should contain 'docker', got: {}",
        info.name
    );

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}

/// Test: Ping Docker daemon via SSH tunnel.
/// Requires user to be in docker group.
#[tokio::test]
#[ignore = "requires SSH_TEST_HOST with Docker and docker group membership"]
async fn docker_runtime_ping() {
    let config = test_config().expect("SSH_TEST_HOST must be set");

    let mut session = Session::connect(config)
        .await
        .expect("connection should succeed");

    let runtime = peleka::runtime::docker::connect_via_session(&mut session, RuntimeType::Docker)
        .await
        .expect("should create Docker runtime");

    // Ping should succeed
    runtime.ping().await.expect("ping should succeed");

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}

/// Test: Create Podman runtime via SSH tunnel and get info.
/// Uses rootless Podman socket which doesn't require special permissions.
#[tokio::test]
#[ignore = "requires SSH_TEST_HOST with Podman"]
async fn podman_runtime_info() {
    let config = test_config().expect("SSH_TEST_HOST must be set");

    let mut session = Session::connect(config)
        .await
        .expect("connection should succeed");

    // Create Podman runtime connected via SSH tunnel
    let runtime = peleka::runtime::docker::connect_via_session(&mut session, RuntimeType::Podman)
        .await
        .expect("should create Podman runtime");

    // Get runtime info
    let info = runtime.info().await.expect("should get runtime info");

    // Podman returns "podman" or similar in the name field
    assert!(
        !info.name.is_empty(),
        "runtime name should not be empty, got: {}",
        info.name
    );

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}

/// Test: Ping Podman daemon via SSH tunnel.
#[tokio::test]
#[ignore = "requires SSH_TEST_HOST with Podman"]
async fn podman_runtime_ping() {
    let config = test_config().expect("SSH_TEST_HOST must be set");

    let mut session = Session::connect(config)
        .await
        .expect("connection should succeed");

    let runtime = peleka::runtime::docker::connect_via_session(&mut session, RuntimeType::Podman)
        .await
        .expect("should create Podman runtime");

    // Ping should succeed
    runtime.ping().await.expect("ping should succeed");

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}

// =============================================================================
// ImageOps Tests
// =============================================================================

/// Test: Pull a public image succeeds.
#[tokio::test]
#[ignore = "requires SSH_TEST_HOST with Docker and docker group membership"]
async fn docker_pull_public_image() {
    let config = test_config().expect("SSH_TEST_HOST must be set");

    let mut session = Session::connect(config)
        .await
        .expect("connection should succeed");

    let runtime = peleka::runtime::docker::connect_via_session(&mut session, RuntimeType::Docker)
        .await
        .expect("should create Docker runtime");

    // Use a small public image
    let image_ref = ImageRef::parse("alpine:latest").expect("valid image ref");

    // Pull the image (no auth needed for public image)
    runtime
        .pull_image(&image_ref, None)
        .await
        .expect("pull should succeed");

    // Verify image exists
    let exists = runtime
        .image_exists(&image_ref)
        .await
        .expect("image_exists should succeed");
    assert!(exists, "image should exist after pull");

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}

/// Test: Check if image exists returns false for non-existent image.
#[tokio::test]
#[ignore = "requires SSH_TEST_HOST with Docker and docker group membership"]
async fn docker_image_exists_false_for_nonexistent() {
    let config = test_config().expect("SSH_TEST_HOST must be set");

    let mut session = Session::connect(config)
        .await
        .expect("connection should succeed");

    let runtime = peleka::runtime::docker::connect_via_session(&mut session, RuntimeType::Docker)
        .await
        .expect("should create Docker runtime");

    // Use a non-existent image
    let image_ref = ImageRef::parse("this-image-definitely-does-not-exist-12345:v999")
        .expect("valid image ref");

    let exists = runtime
        .image_exists(&image_ref)
        .await
        .expect("image_exists should succeed");
    assert!(!exists, "non-existent image should return false");

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}

// =============================================================================
// ContainerOps Tests
// =============================================================================

/// Test: Full container lifecycle (create, start, stop, remove).
#[tokio::test]
#[ignore = "requires SSH_TEST_HOST with Docker and docker group membership"]
async fn docker_container_lifecycle() {
    let config = test_config().expect("SSH_TEST_HOST must be set");

    let mut session = Session::connect(config)
        .await
        .expect("connection should succeed");

    let runtime = peleka::runtime::docker::connect_via_session(&mut session, RuntimeType::Docker)
        .await
        .expect("should create Docker runtime");

    // Ensure we have the alpine image
    let image_ref = ImageRef::parse("alpine:latest").expect("valid image ref");
    if !runtime.image_exists(&image_ref).await.unwrap_or(false) {
        runtime
            .pull_image(&image_ref, None)
            .await
            .expect("pull should succeed");
    }

    // Create container config
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
        peleka::runtime::traits::ContainerState::Running,
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

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}

/// Test: Rename container.
#[tokio::test]
#[ignore = "requires SSH_TEST_HOST with Docker and docker group membership"]
async fn docker_rename_container() {
    let config = test_config().expect("SSH_TEST_HOST must be set");

    let mut session = Session::connect(config)
        .await
        .expect("connection should succeed");

    let runtime = peleka::runtime::docker::connect_via_session(&mut session, RuntimeType::Docker)
        .await
        .expect("should create Docker runtime");

    // Ensure we have the alpine image
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

    // Create container
    let container_id = runtime
        .create_container(&config)
        .await
        .expect("create_container should succeed");

    // Rename container
    runtime
        .rename_container(&container_id, &new_name)
        .await
        .expect("rename_container should succeed");

    // Verify new name
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

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}

// =============================================================================
// NetworkOps Tests
// =============================================================================

/// Test: Create network, connect container, verify alias, disconnect, remove.
#[tokio::test]
#[ignore = "requires SSH_TEST_HOST with Docker and docker group membership"]
async fn docker_network_operations() {
    let config = test_config().expect("SSH_TEST_HOST must be set");

    let mut session = Session::connect(config)
        .await
        .expect("connection should succeed");

    let runtime = peleka::runtime::docker::connect_via_session(&mut session, RuntimeType::Docker)
        .await
        .expect("should create Docker runtime");

    // Ensure we have the alpine image
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

    // Verify network exists
    let exists = runtime
        .network_exists(&network_name)
        .await
        .expect("network_exists should succeed");
    assert!(exists, "network should exist after creation");

    // Create a container
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

    // Start container
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

    // Inspect container to verify network connection
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
        .ok(); // Ignore errors
    runtime
        .remove_container(&container_id, true)
        .await
        .expect("remove_container should succeed");
    runtime
        .remove_network(&network_id)
        .await
        .expect("remove_network should succeed");

    // Verify network is gone
    let exists = runtime
        .network_exists(&network_name)
        .await
        .expect("network_exists should succeed");
    assert!(!exists, "network should not exist after removal");

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}

// =============================================================================
// ExecOps Tests
// =============================================================================

/// Test: Execute command in running container and get output.
#[tokio::test]
#[ignore = "requires SSH_TEST_HOST with Docker and docker group membership"]
async fn docker_exec_command() {
    let config = test_config().expect("SSH_TEST_HOST must be set");

    let mut session = Session::connect(config)
        .await
        .expect("connection should succeed");

    let runtime = peleka::runtime::docker::connect_via_session(&mut session, RuntimeType::Docker)
        .await
        .expect("should create Docker runtime");

    // Ensure we have the alpine image
    let image_ref = ImageRef::parse("alpine:latest").expect("valid image ref");
    if !runtime.image_exists(&image_ref).await.unwrap_or(false) {
        runtime
            .pull_image(&image_ref, None)
            .await
            .expect("pull should succeed");
    }

    let container_name = format!("peleka-exec-test-{}", std::process::id());

    // Create and start a container
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

    // Check output
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

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}

// =============================================================================
// LogOps Tests
// =============================================================================

/// Test: Stream logs from a container.
#[tokio::test]
#[ignore = "requires SSH_TEST_HOST with Docker and docker group membership"]
async fn docker_log_streaming() {
    let config = test_config().expect("SSH_TEST_HOST must be set");

    let mut session = Session::connect(config)
        .await
        .expect("connection should succeed");

    let runtime = peleka::runtime::docker::connect_via_session(&mut session, RuntimeType::Docker)
        .await
        .expect("should create Docker runtime");

    // Ensure we have the alpine image
    let image_ref = ImageRef::parse("alpine:latest").expect("valid image ref");
    if !runtime.image_exists(&image_ref).await.unwrap_or(false) {
        runtime
            .pull_image(&image_ref, None)
            .await
            .expect("pull should succeed");
    }

    let container_name = format!("peleka-log-test-{}", std::process::id());

    // Create container that outputs some logs
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

    // Wait a moment for logs to be generated
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Get logs
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

    // Collect log lines
    let mut log_content = String::new();
    while let Some(result) = log_stream.next().await {
        match result {
            Ok(line) => log_content.push_str(&line.content),
            Err(e) => panic!("log stream error: {}", e),
        }
    }

    // Verify we got our log lines
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

    session
        .disconnect()
        .await
        .expect("disconnect should succeed");
}
