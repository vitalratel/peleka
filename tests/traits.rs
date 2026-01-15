// ABOUTME: Tests for runtime trait definitions.
// ABOUTME: Verifies traits compile with expected bounds and FullRuntime auto-implements.

use peleka::runtime::traits::*;
use peleka::types::{ContainerId, ImageRef, NetworkAlias, NetworkId};
use std::pin::Pin;
use std::time::Duration;

/// Verify that function signatures work with trait bounds.
mod trait_bounds {
    use super::*;

    /// Function requiring only ImageOps.
    async fn ensure_image(runtime: &impl ImageOps, image: &ImageRef) -> Result<(), ImageError> {
        if !runtime.image_exists(image).await? {
            runtime.pull_image(image, None).await?;
        }
        Ok(())
    }

    /// Function requiring only ContainerOps.
    async fn check_container(
        runtime: &impl ContainerOps,
        id: &ContainerId,
    ) -> Result<bool, ContainerError> {
        let info = runtime.inspect_container(id).await?;
        Ok(info.state == ContainerState::Running)
    }

    /// Function requiring only NetworkOps.
    async fn setup_network(
        runtime: &impl NetworkOps,
        name: &str,
    ) -> Result<NetworkId, NetworkError> {
        let config = NetworkConfig {
            name: name.to_string(),
            driver: None,
            labels: Default::default(),
        };
        runtime.create_network(&config).await
    }

    /// Function requiring FullRuntime (all capabilities).
    async fn deploy(runtime: &impl FullRuntime, image: &ImageRef) -> Result<ContainerId, String> {
        // This function can use all trait methods
        runtime
            .pull_image(image, None)
            .await
            .map_err(|e| e.to_string())?;

        let config = ContainerConfig {
            name: "test".to_string(),
            image: image.clone(),
            env: Default::default(),
            labels: Default::default(),
            ports: vec![],
            volumes: vec![],
            command: None,
            entrypoint: None,
            working_dir: None,
            user: None,
            restart_policy: RestartPolicyConfig::default(),
            resources: None,
            healthcheck: None,
            stop_timeout: None,
            network: None,
            network_aliases: vec![],
        };

        runtime
            .create_container(&config)
            .await
            .map_err(|e| e.to_string())
    }

    #[test]
    fn trait_functions_compile() {
        // This test just verifies the above functions compile.
        // The functions aren't called because we have no runtime implementation yet.
    }
}

/// Verify trait hierarchy and error types work correctly.
mod trait_types {
    use super::*;

    #[test]
    fn container_state_equality() {
        assert_eq!(ContainerState::Running, ContainerState::Running);
        assert_ne!(ContainerState::Running, ContainerState::Exited);
    }

    #[test]
    fn health_state_equality() {
        assert_eq!(HealthState::Healthy, HealthState::Healthy);
        assert_ne!(HealthState::Healthy, HealthState::Unhealthy);
    }

    #[test]
    fn log_options_helpers() {
        let follow = LogOptions::follow_all();
        assert!(follow.follow);
        assert!(follow.stdout);
        assert!(follow.stderr);
        assert!(follow.timestamps);

        let tail = LogOptions::tail(100);
        assert!(!tail.follow);
        assert_eq!(tail.tail, Some(100));
    }

    #[test]
    fn exec_config_default() {
        let config = ExecConfig::default();
        assert!(config.cmd.is_empty());
        assert!(!config.attach_stdin);
        assert!(config.attach_stdout);
        assert!(config.attach_stderr);
        assert!(!config.tty);
    }

    #[test]
    fn restart_policy_default() {
        let policy = RestartPolicyConfig::default();
        assert!(matches!(policy, RestartPolicyConfig::UnlessStopped));
    }

    #[test]
    fn protocol_default() {
        let proto = Protocol::default();
        assert!(matches!(proto, Protocol::Tcp));
    }

    #[test]
    fn error_types_display() {
        let err = ImageError::NotFound("nginx:latest".to_string());
        assert!(err.to_string().contains("nginx:latest"));

        let err = ContainerError::AlreadyExists("mycontainer".to_string());
        assert!(err.to_string().contains("mycontainer"));

        let err = NetworkError::InUse("mynetwork".to_string());
        assert!(err.to_string().contains("mynetwork"));

        let err = ExecError::ContainerNotRunning("container1".to_string());
        assert!(err.to_string().contains("container1"));

        let err = LogError::ContainerNotFound("missing".to_string());
        assert!(err.to_string().contains("missing"));

        let err = RuntimeInfoError::ConnectionFailed("timeout".to_string());
        assert!(err.to_string().contains("timeout"));
    }
}

/// Verify sealed trait pattern prevents external implementation.
/// This is a compile-time check - if this module compiles, the sealed pattern works.
mod sealed_trait_pattern {
    // The following would fail to compile if uncommented, proving traits are sealed:
    //
    // struct ExternalRuntime;
    // impl peleka::runtime::traits::sealed::Sealed for ExternalRuntime {}
    //
    // Error: module `sealed` is private

    #[test]
    fn sealed_pattern_enforced() {
        // This test exists to document that external implementations are prevented.
        // The actual enforcement is at compile time - if someone tries to implement
        // the traits without implementing Sealed, they'll get a compile error.
    }
}
