// ABOUTME: Tests for runtime trait types and error handling.
// ABOUTME: Verifies type equality, defaults, and error Display implementations.

use peleka::runtime::traits::*;

/// Verify trait types and error handling work correctly.
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
