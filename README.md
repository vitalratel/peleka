# peleka

> Zero-downtime container deployment for Docker and Podman

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

## Overview

Peleka deploys containers to remote servers with zero downtime using blue-green deployment. It connects via SSH, manages Docker or Podman containers, and handles health checks, rollbacks, and graceful shutdowns.

## Features

- **Zero-downtime deployments** - Blue-green deployment with health check verification
- **Docker & Podman support** - Works with both container runtimes
- **SSH-native** - Connects directly via SSH, no agents to install
- **Rollback support** - Instant rollback to previous deployment
- **Deployment locking** - Prevents concurrent deployments
- **Multi-destination** - Deploy to different environments (staging, production)
- **Environment variable references** - Reference env vars in config with `{ env: "VAR" }` syntax

## Installation

```bash
cargo install peleka
```

Or from source:
```bash
git clone https://github.com/vitalratel/peleka.git
cd peleka
cargo install --path .
```

## Quick Start

1. Initialize a configuration:
```bash
peleka init --service my-app --image registry.example.com/my-app:latest
```

2. Edit `peleka.yml` with your server details:
```yaml
service: my-app
image: registry.example.com/my-app:latest
servers:
  - host: server.example.com
    user: deploy
ports:
  - "8080:80"
healthcheck:
  cmd: "curl -f http://localhost:80/health"
```

3. Deploy:
```bash
peleka deploy
```

## Commands

| Command | Description |
|---------|-------------|
| `peleka init` | Create a new peleka.yml configuration |
| `peleka deploy` | Deploy the service to configured servers |
| `peleka rollback` | Rollback to the previous deployment |
| `peleka exec <cmd>` | Execute a command in the service container |

### Global Options

- `-v, --verbose` - Enable debug output
- `-q, --quiet` - Suppress progress output (CI mode)
- `--json` - Output as JSON lines (for scripting)
- `-d, --destination <name>` - Target a specific destination

## Configuration

Peleka looks for configuration in these locations (in order):
- `peleka.yml`
- `peleka.yaml`
- `.peleka/config.yml`

### Full Configuration Example

```yaml
service: my-app
image: registry.example.com/my-app:latest

servers:
  - host: server1.example.com
    user: deploy
    port: 22
  - host: server2.example.com
    user: deploy

ports:
  - "8080:80"

volumes:
  - "/data/my-app:/app/data"

env:
  DATABASE_URL:
    env: DATABASE_URL
  LOG_LEVEL: info

labels:
  app: my-app
  managed-by: peleka

healthcheck:
  cmd: "curl -f http://localhost:80/health"
  interval: 10s
  timeout: 5s
  retries: 3
  start_period: 30s

health_timeout: 2m
image_pull_timeout: 5m

resources:
  memory: 512m
  cpus: "1.0"

network:
  name: my-network
  aliases:
    - my-app

restart: unless-stopped

# Deployment strategy (optional, auto-detected by default)
# - blue-green: zero-downtime (default)
# - recreate: stop old first, brief downtime (for stateful apps)
strategy: blue-green

stop:
  timeout: 30s

cleanup:
  grace_period: 30s

logging:
  driver: json-file
  options:
    max-size: "10m"
    max-file: "3"

# Environment-specific overrides
destinations:
  staging:
    servers:
      - host: staging.example.com
        user: deploy
    env:
      LOG_LEVEL: debug

  production:
    servers:
      - host: prod1.example.com
        user: deploy
      - host: prod2.example.com
        user: deploy
    env:
      LOG_LEVEL: warn
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 2 | Deployment locked |
| 3 | Health check timeout |
| 4 | No previous deployment (rollback failed) |
| 5 | SSH connection failed |
| 6 | Configuration file not found |
| 7 | No servers configured |
| 8 | No container runtime found |
| 9 | Container runtime connection failed |
| 10 | Image pull timeout |

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## License

MIT License - see [LICENSE](LICENSE)
