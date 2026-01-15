// ABOUTME: Integration tests for type-safe identifiers and validated types.
// ABOUTME: Tests parsing, validation, and type safety properties.

use peleka::types::*;

mod image_ref_tests {
    use super::*;

    #[test]
    fn parse_simple_name() {
        let img = ImageRef::parse("nginx").unwrap();
        assert_eq!(img.name(), "nginx");
        assert_eq!(img.tag(), Some("latest"));
        assert!(img.registry().is_none());
        assert!(img.digest().is_none());
    }

    #[test]
    fn parse_name_with_tag() {
        let img = ImageRef::parse("nginx:1.25").unwrap();
        assert_eq!(img.name(), "nginx");
        assert_eq!(img.tag(), Some("1.25"));
    }

    #[test]
    fn parse_with_registry() {
        let img = ImageRef::parse("registry.example.com/myapp:v1.2.3").unwrap();
        assert_eq!(img.registry(), Some("registry.example.com"));
        assert_eq!(img.name(), "myapp");
        assert_eq!(img.tag(), Some("v1.2.3"));
    }

    #[test]
    fn parse_with_org() {
        let img = ImageRef::parse("ghcr.io/org/repo:latest").unwrap();
        assert_eq!(img.registry(), Some("ghcr.io"));
        assert_eq!(img.name(), "org/repo");
        assert_eq!(img.tag(), Some("latest"));
    }

    #[test]
    fn parse_with_digest() {
        let digest = "sha256:abc123def456";
        let img = ImageRef::parse(&format!("nginx@{}", digest)).unwrap();
        assert_eq!(img.name(), "nginx");
        assert_eq!(img.digest(), Some(digest));
        assert!(img.tag().is_none());
    }

    #[test]
    fn parse_full_reference() {
        let img = ImageRef::parse("ghcr.io/org/repo:v1@sha256:abc123").unwrap();
        assert_eq!(img.registry(), Some("ghcr.io"));
        assert_eq!(img.name(), "org/repo");
        assert_eq!(img.tag(), Some("v1"));
        assert_eq!(img.digest(), Some("sha256:abc123"));
    }

    #[test]
    fn parse_empty_returns_error() {
        assert!(ImageRef::parse("").is_err());
    }

    #[test]
    fn parse_invalid_chars_returns_error() {
        assert!(ImageRef::parse("invalid image!").is_err());
    }

    #[test]
    fn display_formats_correctly() {
        let img = ImageRef::parse("ghcr.io/org/repo:v1").unwrap();
        assert_eq!(img.to_string(), "ghcr.io/org/repo:v1");
    }
}

mod network_alias_tests {
    use super::*;

    #[test]
    fn valid_alias() {
        let alias = NetworkAlias::new("my-service").unwrap();
        assert_eq!(alias.as_str(), "my-service");
    }

    #[test]
    fn empty_returns_error() {
        assert!(NetworkAlias::new("").is_err());
    }

    #[test]
    fn whitespace_only_returns_error() {
        assert!(NetworkAlias::new("   ").is_err());
    }

    #[test]
    fn invalid_chars_returns_error() {
        assert!(NetworkAlias::new("my service").is_err()); // space
        assert!(NetworkAlias::new("my:service").is_err()); // colon
    }

    #[test]
    fn valid_with_numbers_and_hyphens() {
        assert!(NetworkAlias::new("app-v2").is_ok());
        assert!(NetworkAlias::new("my-app-123").is_ok());
    }
}

mod service_name_tests {
    use super::*;

    #[test]
    fn valid_dns_name() {
        let name = ServiceName::new("my-service").unwrap();
        assert_eq!(name.as_str(), "my-service");
    }

    #[test]
    fn empty_returns_error() {
        assert!(ServiceName::new("").is_err());
    }

    #[test]
    fn too_long_returns_error() {
        let long_name = "a".repeat(64);
        assert!(ServiceName::new(&long_name).is_err());
    }

    #[test]
    fn starts_with_hyphen_returns_error() {
        assert!(ServiceName::new("-service").is_err());
    }

    #[test]
    fn ends_with_hyphen_returns_error() {
        assert!(ServiceName::new("service-").is_err());
    }

    #[test]
    fn uppercase_returns_error() {
        assert!(ServiceName::new("MyService").is_err());
    }

    #[test]
    fn valid_63_chars() {
        let name = "a".repeat(63);
        assert!(ServiceName::new(&name).is_ok());
    }
}

mod id_tests {
    use super::*;

    #[test]
    fn container_id_stores_value() {
        let id = ContainerId::new("abc123".to_string());
        assert_eq!(id.as_str(), "abc123");
    }

    #[test]
    fn network_id_stores_value() {
        let id = NetworkId::new("net456".to_string());
        assert_eq!(id.as_str(), "net456");
    }

    #[test]
    fn image_id_stores_value() {
        let id = ImageId::new("sha256:abc".to_string());
        assert_eq!(id.as_str(), "sha256:abc");
    }

    #[test]
    fn pod_id_stores_value() {
        let id = PodId::new("pod789".to_string());
        assert_eq!(id.as_str(), "pod789");
    }
}
