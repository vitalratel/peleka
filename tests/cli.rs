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
