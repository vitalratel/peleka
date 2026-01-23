# Contributing to peleka

Thank you for your interest in contributing to peleka!

## Development Setup

### Prerequisites
- Rust 1.85+ (install via [rustup](https://rustup.rs/))
- Git
- Docker or Podman (for integration tests)

### Getting Started

1. Clone the repository:
```bash
git clone https://github.com/vitalratel/peleka.git
cd peleka
```

2. Build the project:
```bash
cargo build
```

3. Run tests:
```bash
cargo test
```

4. Run the CLI:
```bash
cargo run -- --help
cargo run -- init --service my-app
```

## Project Structure

```
src/
├── main.rs          # Entry point, error handling
├── cli.rs           # CLI definition (clap)
├── lib.rs           # Library exports
├── error.rs         # Application error types
├── output.rs        # Output formatting (normal, quiet, JSON)
├── diagnostics.rs   # Warning collection
├── commands/        # Command implementations
│   ├── deploy.rs    # Deploy command
│   ├── rollback.rs  # Rollback command
│   └── exec.rs      # Exec command
├── config/          # Configuration parsing
├── deploy/          # Deployment orchestration, locking
├── runtime/         # Container runtime abstraction (Docker/Podman)
├── ssh/             # SSH connection handling
├── hooks/           # Lifecycle hooks
└── types/           # Domain types (ServiceName, ImageRef, etc.)
```

## How to Contribute

### Bug Reports
- Use GitHub Issues
- Include: OS, Rust version, container runtime, steps to reproduce

### Feature Requests
- Open an issue to discuss before implementing
- Consider how it fits with the zero-downtime deployment model

### Code Contributions

1. **Fork and create a branch**:
```bash
git checkout -b feature/your-feature-name
```

2. **Make your changes**:
- Follow Rust conventions (use `rustfmt`)
- Add tests for new functionality
- Update documentation as needed

3. **Run checks**:
```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo test
```

4. **Commit with descriptive message**:
```bash
git commit -m "feat: add health check retry configuration

Allows configuring the number of health check retries
before marking a deployment as failed."
```

5. **Push and create PR**:
```bash
git push origin feature/your-feature-name
```

## Coding Standards

### Rust Style
- Use `cargo fmt` (rustfmt)
- Use `cargo clippy` and fix warnings
- Use thiserror or SNAFU for error handling
- Add doc comments for public items
- All source files must start with two ABOUTME comment lines

### Commit Messages
Follow conventional commits:
- `feat:` New feature
- `fix:` Bug fix
- `docs:` Documentation changes
- `test:` Adding tests
- `refactor:` Code refactoring
- `chore:` Maintenance tasks

### Testing
- Unit tests for individual functions
- Integration tests for CLI commands
- Use `assert_cmd` for testing CLI behavior

## Questions?

- Open an issue for questions
- Review existing issues and PRs for context

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
