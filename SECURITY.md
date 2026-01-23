# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

## Reporting a Vulnerability

We take the security of peleka seriously. If you believe you have found a security vulnerability, please report it to us as described below.

### How to Report

**Please do not report security vulnerabilities through public GitHub issues.**

Instead, please report them via:
- **GitHub Security Advisories** (recommended): Navigate to the Security tab and click "Report a vulnerability"
- **Email**: security@vitalratel.com (if GitHub Security Advisories are unavailable)

Please include the following information:
- Type of issue (e.g., command injection, SSH key exposure, etc.)
- Full paths of source file(s) related to the manifestation of the issue
- The location of the affected source code (tag/branch/commit or direct URL)
- Any special configuration required to reproduce the issue
- Step-by-step instructions to reproduce the issue
- Proof-of-concept or exploit code (if possible)
- Impact of the issue, including how an attacker might exploit it

### Response Timeline

- **Initial Response**: Within 48 hours
- **Status Update**: Within 7 days
- **Fix Timeline**: Varies based on severity and complexity

### What to Expect

1. We will acknowledge receipt of your vulnerability report
2. We will investigate and validate the issue
3. We will work on a fix and coordinate disclosure timeline with you
4. We will release a security update and publicly disclose the issue (with credit to you, if desired)

## Security Measures

peleka implements several security practices:

### SSH Security
- **Key-based authentication**: Uses SSH keys via russh, no password storage
- **Host key verification**: Validates server identity (configurable trust-on-first-use)
- **No credential storage**: SSH keys are read at runtime, never stored by peleka

### Command Execution
- **Container exec uses API**: Commands passed to containers via Docker/Podman API with proper argument separation (no shell interpolation)
- **Internal SSH commands**: SSH commands used for runtime detection are internally generated, not user-controlled
- **User exec is intentional**: `peleka exec` allows users to run commands in their own containers - this is a feature, not a vulnerability

### Container Security
- **Runtime socket access**: Connects to Docker/Podman socket, inherits runtime's security model
- **No privilege escalation**: peleka runs with user's permissions
- **Network isolation**: Uses container runtime's network isolation features

### Configuration Security
- **Environment variable references**: Supports `{ env: "VAR" }` YAML syntax to reference environment variables, keeping secrets out of config files
- **Local-only config**: Configuration files are read locally, never transmitted

## Security Considerations for Users

### SSH Keys
- Use dedicated deployment keys with minimal permissions
- Rotate keys periodically
- Never commit SSH keys to version control

### Server Configuration
- Run container runtime with non-root user when possible
- Restrict SSH access to deployment user
- Use firewalls to limit access to management ports

### Configuration Files
- Do not commit secrets to peleka.yml
- Use environment variable interpolation for sensitive values
- Review configuration changes in version control

### Container Runtime
- Keep Docker/Podman updated
- Use trusted base images
- Scan images for vulnerabilities before deployment

## Known Security Considerations

### Trust-First-Connection
The `trust_first_connection` option allows accepting unknown SSH host keys on first connection. This is disabled by default. Enabling it trades security for convenience and should only be used in controlled environments.

### Container Runtime Access
peleka requires access to the container runtime socket. Users with this access can:
- Start/stop containers
- Pull images
- Access container logs and exec into containers

Ensure only authorized users have access to servers where peleka operates.

## Contact

For general security questions or concerns (non-vulnerability), please open a GitHub issue with the `security` label.
