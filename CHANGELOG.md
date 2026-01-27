# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.2] - 2026-01-27

### Added
- `justfile` for standardized task running (`just test`, `just lint`, etc.)
- `cargo-nextest` for faster, more robust test execution
- `test-group` for categorizing tests by runtime requirement
- `.config/nextest.toml` configuration

### Changed
- CI now uses `just` commands for consistency with local development
- CI split into Docker and Podman integration test jobs
- Tests requiring Podman tagged with `#[test_group::group(podman)]`
- Updated `actions/checkout` to v6
- Forwarding tests converted from Podman to Docker

## [0.1.1] - 2026-01-27

### Changed
- Reduced binary size by 33% (12MB → 8.1MB) via LTO and stripping
- Fixed package description

## [0.1.0] - 2026-01-26

Initial release.

### Added
- CLI with init, deploy, rollback, and exec commands
- SSH-native remote execution via russh
- Docker and Podman runtime support via bollard
- Blue-green zero-downtime deployment
- Recreate strategy for stateful apps
- Health check verification with configurable timeout
- Deployment locking to prevent concurrent deploys
- Rollback to previous deployment
- Multi-destination support (staging, production, etc.)
- Environment variable references in config
- YAML configuration with peleka.yml
- JSON output mode for scripting
- Programmatic exit codes for CI/CD integration
- Image `pull_policy` config option (`always` or `never`) for local development
