# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- CLI with init, deploy, rollback, and exec commands
- SSH-native remote execution via russh
- Docker and Podman runtime support via bollard
- Blue-green zero-downtime deployment
- Health check verification with configurable timeout
- Deployment locking to prevent concurrent deploys
- Rollback to previous deployment
- Multi-destination support (staging, production, etc.)
- Environment variable references in config
- YAML configuration with peleka.yml
- JSON output mode for scripting
- Programmatic exit codes for CI/CD integration
- Explicit `strategy` config option (`blue-green` for stateless, `recreate` for stateful apps)
- Auto-detection of recreate strategy when host port bindings present
- Panic-safe deploy lock release via `with_lock` callback pattern

## [0.1.0] - 2026-01-26

Initial release (pre-1.0 API may change).
