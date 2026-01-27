# ABOUTME: Task runner commands for development workflow.
# ABOUTME: Install just: cargo install just

# List available recipes
default:
    @just --list

# Run all tests with nextest
test:
    cargo nextest run

# Run only Podman tests (requires Podman runtime)
test-podman:
    cargo nextest run --profile ci -E 'test(::podman::)'

# Run non-Podman tests (Docker + unit tests)
test-docker:
    cargo nextest run --profile ci -E 'not test(::podman::)'

# Run unit tests only (no integration tests)
test-unit:
    cargo test --lib

# Run doc tests
test-doc:
    cargo test --doc

# Run clippy and format check
lint:
    cargo fmt --all -- --check
    cargo clippy --all-targets -- -D warnings

# Format code
fmt:
    cargo fmt --all

# Build release binary
build:
    cargo build --release

# Check compilation without building
check:
    cargo check --all-targets

# Run security and license checks (requires cargo-deny)
security:
    cargo deny check

# Generate and open documentation
docs:
    cargo doc --no-deps --open

# Generate documentation without opening
docs-build:
    cargo doc --no-deps

# Verify Cargo.lock is up to date
check-lockfile:
    cargo update --workspace --locked

# CI: Run all checks that don't need container runtimes
ci-lint: lint check-lockfile

# CI: Run unit and doc tests, build release
ci-test: test-unit test-doc build
