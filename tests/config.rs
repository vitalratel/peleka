// ABOUTME: Integration tests for configuration parsing and validation.
// ABOUTME: Tests YAML parsing, env var interpolation, and destination merging.

use peleka::config::*;
use std::collections::HashMap;
use std::time::Duration;

mod parsing {
    use super::*;

    #[test]
    fn parse_minimal_config() {
        let yaml = r#"
service: myapp
image: nginx:latest
servers:
  - host: example.com
"#;
        let config = Config::from_yaml(yaml).unwrap();
        assert_eq!(config.service.as_str(), "myapp");
        assert_eq!(config.image.name(), "nginx");
        assert_eq!(config.servers.len(), 1);
    }

    #[test]
    fn parse_full_config() {
        let yaml = r#"
service: myapp
image: ghcr.io/org/app:v1.2.3

servers:
  - host: web1.example.com
  - deploy@web2.example.com:2222

ports:
  - "3000:3000"
  - "443:3000"

volumes:
  - "myapp-data:/app/data"

env:
  RAILS_ENV: production
  LOG_LEVEL: info

labels:
  traefik.enable: "true"

healthcheck:
  cmd: "curl -f http://localhost:3000/health"
  interval: 10s
  timeout: 5s
  retries: 3

restart: unless-stopped

stop:
  timeout: 30s
"#;
        let config = Config::from_yaml(yaml).unwrap();
        assert_eq!(config.service.as_str(), "myapp");
        assert_eq!(config.image.registry(), Some("ghcr.io"));
        assert_eq!(config.ports.len(), 2);
        assert_eq!(config.volumes.len(), 1);
        assert_eq!(
            config.env.get("RAILS_ENV"),
            Some(&EnvValue::Literal("production".to_string()))
        );
        assert_eq!(
            config.healthcheck.as_ref().unwrap().cmd,
            "curl -f http://localhost:3000/health"
        );
        assert_eq!(config.restart, RestartPolicy::UnlessStopped);
    }

    #[test]
    fn missing_service_returns_error() {
        let yaml = r#"
image: nginx:latest
servers:
  - host: example.com
"#;
        let err = Config::from_yaml(yaml).unwrap_err();
        assert!(err.to_string().contains("service"));
    }

    #[test]
    fn missing_image_returns_error() {
        let yaml = r#"
service: myapp
servers:
  - host: example.com
"#;
        let err = Config::from_yaml(yaml).unwrap_err();
        assert!(err.to_string().contains("image"));
    }

    #[test]
    fn invalid_image_returns_error() {
        let yaml = r#"
service: myapp
image: "invalid image!"
servers:
  - host: example.com
"#;
        let err = Config::from_yaml(yaml).unwrap_err();
        assert!(err.to_string().contains("image"));
    }

    #[test]
    fn empty_servers_returns_error() {
        let yaml = r#"
service: myapp
image: nginx:latest
servers: []
"#;
        let err = Config::from_yaml(yaml).unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("server")
                || err.to_string().to_lowercase().contains("empty"),
            "expected error about empty servers, got: {}",
            err
        );
    }

    #[test]
    fn missing_servers_returns_error() {
        let yaml = r#"
service: myapp
image: nginx:latest
"#;
        let err = Config::from_yaml(yaml).unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("server")
                || err.to_string().to_lowercase().contains("empty"),
            "expected error about missing servers, got: {}",
            err
        );
    }
}

mod server_parsing {
    use super::*;

    #[test]
    fn parse_simple_host() {
        let server = ServerConfig::parse("example.com").unwrap();
        assert_eq!(server.host, "example.com");
        assert_eq!(server.port, 22);
        assert!(server.user.is_none());
    }

    #[test]
    fn parse_with_user() {
        let server = ServerConfig::parse("deploy@example.com").unwrap();
        assert_eq!(server.host, "example.com");
        assert_eq!(server.user, Some("deploy".to_string()));
    }

    #[test]
    fn parse_with_port() {
        let server = ServerConfig::parse("example.com:2222").unwrap();
        assert_eq!(server.host, "example.com");
        assert_eq!(server.port, 2222);
    }

    #[test]
    fn parse_full_format() {
        let server = ServerConfig::parse("deploy@example.com:2222").unwrap();
        assert_eq!(server.host, "example.com");
        assert_eq!(server.port, 2222);
        assert_eq!(server.user, Some("deploy".to_string()));
    }
}

mod env_vars {
    use super::*;

    #[test]
    fn literal_value() {
        let yaml = r#"
service: myapp
image: nginx
servers:
  - host: example.com
env:
  KEY: "value"
"#;
        let config = Config::from_yaml(yaml).unwrap();
        assert_eq!(
            config.env.get("KEY"),
            Some(&EnvValue::Literal("value".to_string()))
        );
    }

    #[test]
    fn env_reference() {
        let yaml = r#"
service: myapp
image: nginx
servers:
  - host: example.com
env:
  SECRET:
    env: SECRET_VAR
"#;
        let config = Config::from_yaml(yaml).unwrap();
        match config.env.get("SECRET") {
            Some(EnvValue::FromEnv { var, default: None }) => {
                assert_eq!(var, "SECRET_VAR");
            }
            _ => panic!("Expected FromEnv variant"),
        }
    }

    #[test]
    fn env_reference_with_default() {
        let yaml = r#"
service: myapp
image: nginx
servers:
  - host: example.com
env:
  OPTIONAL:
    env: OPTIONAL_VAR
    default: "fallback"
"#;
        let config = Config::from_yaml(yaml).unwrap();
        match config.env.get("OPTIONAL") {
            Some(EnvValue::FromEnv {
                var,
                default: Some(def),
            }) => {
                assert_eq!(var, "OPTIONAL_VAR");
                assert_eq!(def, "fallback");
            }
            _ => panic!("Expected FromEnv with default"),
        }
    }

    #[test]
    fn resolve_env_values() {
        let mut env_map = HashMap::new();
        env_map.insert("KEY".to_string(), EnvValue::Literal("literal".to_string()));
        env_map.insert(
            "FROM_ENV".to_string(),
            EnvValue::FromEnv {
                var: "PELEKA_TEST_VAR".to_string(),
                default: None,
            },
        );
        env_map.insert(
            "WITH_DEFAULT".to_string(),
            EnvValue::FromEnv {
                var: "PELEKA_MISSING_VAR".to_string(),
                default: Some("default_value".to_string()),
            },
        );

        temp_env::with_var("PELEKA_TEST_VAR", Some("from_environment"), || {
            let resolved = resolve_env_map(&env_map).unwrap();

            assert_eq!(resolved.get("KEY"), Some(&"literal".to_string()));
            assert_eq!(
                resolved.get("FROM_ENV"),
                Some(&"from_environment".to_string())
            );
            assert_eq!(
                resolved.get("WITH_DEFAULT"),
                Some(&"default_value".to_string())
            );
        });
    }
}

mod destinations {
    use super::*;

    #[test]
    fn destination_overrides_servers() {
        let yaml = r#"
service: myapp
image: nginx
servers:
  - host: default.example.com

destinations:
  staging:
    servers:
      - host: staging.example.com
"#;
        let config = Config::from_yaml(yaml).unwrap();
        let merged = config.for_destination("staging").unwrap();
        assert_eq!(merged.servers.len(), 1);
        assert_eq!(merged.servers[0].host, "staging.example.com");
    }

    #[test]
    fn destination_merges_env() {
        let yaml = r#"
service: myapp
image: nginx
servers:
  - host: example.com
env:
  SHARED: base
  BASE_ONLY: value

destinations:
  staging:
    env:
      SHARED: overridden
      STAGING_ONLY: staging_value
"#;
        let config = Config::from_yaml(yaml).unwrap();
        let merged = config.for_destination("staging").unwrap();

        // SHARED should be overridden
        assert_eq!(
            merged.env.get("SHARED"),
            Some(&EnvValue::Literal("overridden".to_string()))
        );
        // BASE_ONLY should be preserved
        assert_eq!(
            merged.env.get("BASE_ONLY"),
            Some(&EnvValue::Literal("value".to_string()))
        );
        // STAGING_ONLY should be added
        assert_eq!(
            merged.env.get("STAGING_ONLY"),
            Some(&EnvValue::Literal("staging_value".to_string()))
        );
    }

    #[test]
    fn unknown_destination_returns_error() {
        let yaml = r#"
service: myapp
image: nginx
servers:
  - host: example.com
"#;
        let config = Config::from_yaml(yaml).unwrap();
        let err = config.for_destination("nonexistent").unwrap_err();
        assert!(err.to_string().contains("nonexistent"));
    }
}

mod restart_policy {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn parse_no() {
        assert_eq!(RestartPolicy::from_str("no").unwrap(), RestartPolicy::No);
    }

    #[test]
    fn parse_always() {
        assert_eq!(
            RestartPolicy::from_str("always").unwrap(),
            RestartPolicy::Always
        );
    }

    #[test]
    fn parse_unless_stopped() {
        assert_eq!(
            RestartPolicy::from_str("unless-stopped").unwrap(),
            RestartPolicy::UnlessStopped
        );
    }

    #[test]
    fn parse_on_failure() {
        assert_eq!(
            RestartPolicy::from_str("on-failure").unwrap(),
            RestartPolicy::OnFailure { max_retries: None }
        );
    }

    #[test]
    fn parse_on_failure_with_retries() {
        assert_eq!(
            RestartPolicy::from_str("on-failure:3").unwrap(),
            RestartPolicy::OnFailure {
                max_retries: Some(3)
            }
        );
    }

    #[test]
    fn default_is_unless_stopped() {
        assert_eq!(RestartPolicy::default(), RestartPolicy::UnlessStopped);
    }
}

mod healthcheck {
    use super::*;

    #[test]
    fn parse_healthcheck() {
        let yaml = r#"
service: myapp
image: nginx
servers:
  - host: example.com
healthcheck:
  cmd: "curl -f http://localhost:8080/health"
"#;
        let config = Config::from_yaml(yaml).unwrap();
        let hc = config.healthcheck.unwrap();
        assert_eq!(hc.cmd, "curl -f http://localhost:8080/health");
        // Check defaults
        assert_eq!(hc.interval, Duration::from_secs(10));
        assert_eq!(hc.timeout, Duration::from_secs(5));
        assert_eq!(hc.retries, 3);
        assert_eq!(hc.start_period, Duration::from_secs(30));
    }

    #[test]
    fn parse_healthcheck_with_custom_timing() {
        let yaml = r#"
service: myapp
image: nginx
servers:
  - host: example.com
healthcheck:
  cmd: "nc -z localhost 3000"
  interval: 5s
  timeout: 2s
  retries: 5
  start_period: 10s
"#;
        let config = Config::from_yaml(yaml).unwrap();
        let hc = config.healthcheck.unwrap();
        assert_eq!(hc.cmd, "nc -z localhost 3000");
        assert_eq!(hc.interval, Duration::from_secs(5));
        assert_eq!(hc.timeout, Duration::from_secs(2));
        assert_eq!(hc.retries, 5);
        assert_eq!(hc.start_period, Duration::from_secs(10));
    }
}

mod runtime_config {
    use super::*;
    use peleka::runtime::RuntimeType;

    #[test]
    fn parse_server_with_runtime() {
        let yaml = r#"
service: myapp
image: nginx
servers:
  - host: example.com
    runtime: podman
"#;
        let config = Config::from_yaml(yaml).unwrap();
        assert_eq!(config.servers[0].runtime, Some(RuntimeType::Podman));
    }

    #[test]
    fn parse_server_with_runtime_and_socket() {
        let yaml = r#"
service: myapp
image: nginx
servers:
  - host: example.com
    runtime: docker
    socket: /custom/docker.sock
"#;
        let config = Config::from_yaml(yaml).unwrap();
        assert_eq!(config.servers[0].runtime, Some(RuntimeType::Docker));
        assert_eq!(
            config.servers[0].socket,
            Some("/custom/docker.sock".to_string())
        );
    }

    #[test]
    fn parse_server_without_runtime_defaults_to_none() {
        let yaml = r#"
service: myapp
image: nginx
servers:
  - host: example.com
"#;
        let config = Config::from_yaml(yaml).unwrap();
        assert!(config.servers[0].runtime.is_none());
        assert!(config.servers[0].socket.is_none());
    }
}
