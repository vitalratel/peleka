// ABOUTME: Integration tests for the peleka CLI commands.
// ABOUTME: Validates --help output and init command behavior.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

fn peleka_cmd() -> Command {
    Command::new(assert_cmd::cargo::cargo_bin!("peleka"))
}

#[test]
fn help_shows_commands() {
    peleka_cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("init"))
        .stdout(predicate::str::contains("deploy"))
        .stdout(predicate::str::contains("status"));
}

#[test]
fn init_creates_config_file() {
    let temp_dir = tempfile::tempdir().unwrap();
    let config_path = temp_dir.path().join("peleka.yml");

    peleka_cmd()
        .current_dir(temp_dir.path())
        .arg("init")
        .assert()
        .success();

    assert!(config_path.exists(), "peleka.yml should be created");
    let content = fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("image:"), "Config should have image field");
}

#[test]
fn init_refuses_to_overwrite_existing_config() {
    let temp_dir = tempfile::tempdir().unwrap();
    let config_path = temp_dir.path().join("peleka.yml");

    fs::write(&config_path, "existing: config").unwrap();

    peleka_cmd()
        .current_dir(temp_dir.path())
        .arg("init")
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn init_force_overwrites_existing_config() {
    let temp_dir = tempfile::tempdir().unwrap();
    let config_path = temp_dir.path().join("peleka.yml");

    fs::write(&config_path, "existing: config").unwrap();

    peleka_cmd()
        .current_dir(temp_dir.path())
        .args(["init", "--force"])
        .assert()
        .success();

    let content = fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("service:"), "Config should be overwritten");
}

#[test]
fn init_with_service_name() {
    let temp_dir = tempfile::tempdir().unwrap();

    peleka_cmd()
        .current_dir(temp_dir.path())
        .args(["init", "--service", "myapp"])
        .assert()
        .success();

    let content = fs::read_to_string(temp_dir.path().join("peleka.yml")).unwrap();
    assert!(
        content.contains("service: myapp"),
        "Service name should be myapp"
    );
}

#[test]
fn init_with_image() {
    let temp_dir = tempfile::tempdir().unwrap();

    peleka_cmd()
        .current_dir(temp_dir.path())
        .args(["init", "--image", "ghcr.io/org/myapp:v1"])
        .assert()
        .success();

    let content = fs::read_to_string(temp_dir.path().join("peleka.yml")).unwrap();
    assert!(
        content.contains("ghcr.io/org/myapp:v1"),
        "Image should be set"
    );
}

#[test]
fn status_requires_config_file() {
    let temp_dir = tempfile::tempdir().unwrap();

    peleka_cmd()
        .current_dir(temp_dir.path())
        .arg("status")
        .assert()
        .failure()
        .stderr(predicate::str::contains("configuration file not found"));
}

#[test]
fn status_shows_service_info() {
    let temp_dir = tempfile::tempdir().unwrap();
    let config_content = r#"
service: myapp
image: ghcr.io/example/myapp:latest
servers:
  - host: server1.example.com
  - host: server2.example.com
"#;
    fs::write(temp_dir.path().join("peleka.yml"), config_content).unwrap();

    peleka_cmd()
        .current_dir(temp_dir.path())
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("Service: myapp"))
        .stdout(predicate::str::contains("Servers: 2"));
}

#[test]
fn logs_requires_config_file() {
    let temp_dir = tempfile::tempdir().unwrap();

    peleka_cmd()
        .current_dir(temp_dir.path())
        .arg("logs")
        .assert()
        .failure()
        .stderr(predicate::str::contains("configuration file not found"));
}

#[test]
fn help_shows_logs_command() {
    peleka_cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("logs"));
}

#[test]
fn verbose_flag_accepted() {
    peleka_cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--verbose"));
}

#[test]
fn verbose_works_with_subcommands() {
    let temp_dir = tempfile::tempdir().unwrap();
    let config_content = r#"
service: myapp
image: ghcr.io/example/myapp:latest
servers:
  - host: server1.example.com
"#;
    fs::write(temp_dir.path().join("peleka.yml"), config_content).unwrap();

    // Verbose before subcommand
    peleka_cmd()
        .current_dir(temp_dir.path())
        .args(["--verbose", "status"])
        .assert()
        .success();

    // Short form
    peleka_cmd()
        .current_dir(temp_dir.path())
        .args(["-v", "status"])
        .assert()
        .success();
}

#[test]
fn logs_accepts_options() {
    let temp_dir = tempfile::tempdir().unwrap();
    let config_content = r#"
service: myapp
image: ghcr.io/example/myapp:latest
servers:
  - host: server1.example.com
"#;
    fs::write(temp_dir.path().join("peleka.yml"), config_content).unwrap();

    // Test --tail option
    peleka_cmd()
        .current_dir(temp_dir.path())
        .args(["logs", "--tail", "100"])
        .assert()
        .success()
        .stdout(predicate::str::contains("tail=Some(100)"));

    // Test -f option
    peleka_cmd()
        .current_dir(temp_dir.path())
        .args(["logs", "-f"])
        .assert()
        .success()
        .stdout(predicate::str::contains("follow=true"));

    // Test --since option
    peleka_cmd()
        .current_dir(temp_dir.path())
        .args(["logs", "--since", "2024-01-15T10:00:00"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "since=Some(\"2024-01-15T10:00:00\")",
        ));

    // Test --stats option
    peleka_cmd()
        .current_dir(temp_dir.path())
        .args(["logs", "--stats"])
        .assert()
        .success()
        .stdout(predicate::str::contains("stats=true"));
}

#[test]
fn deploy_requires_config_file() {
    let temp_dir = tempfile::tempdir().unwrap();

    peleka_cmd()
        .current_dir(temp_dir.path())
        .arg("deploy")
        .assert()
        .failure()
        .stderr(predicate::str::contains("configuration file not found"));
}

#[test]
fn deploy_fails_with_no_servers() {
    let temp_dir = tempfile::tempdir().unwrap();
    let config_content = r#"
service: myapp
image: ghcr.io/example/myapp:latest
servers: []
"#;
    fs::write(temp_dir.path().join("peleka.yml"), config_content).unwrap();

    peleka_cmd()
        .current_dir(temp_dir.path())
        .arg("deploy")
        .assert()
        .failure()
        .stderr(predicate::str::contains("no servers configured"));
}

#[test]
fn deploy_fails_with_unknown_destination() {
    let temp_dir = tempfile::tempdir().unwrap();
    let config_content = r#"
service: myapp
image: ghcr.io/example/myapp:latest
servers:
  - host: server1.example.com
"#;
    fs::write(temp_dir.path().join("peleka.yml"), config_content).unwrap();

    peleka_cmd()
        .current_dir(temp_dir.path())
        .args(["deploy", "--destination", "nonexistent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown destination"));
}
