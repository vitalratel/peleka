# Podman with SSH server for integration tests.
# Build: docker build -t peleka-podman-ssh tests/fixtures/podman-ssh
# Run: docker run --privileged -p 2222:22 peleka-podman-ssh

FROM quay.io/podman/stable:latest

# Install OpenSSH server
RUN dnf install -y openssh-server passwd && \
    dnf clean all

# Create test user with password
RUN useradd -m -s /bin/bash testuser && \
    echo "testuser:testuser" | chpasswd

# Setup SSH
RUN mkdir -p /home/testuser/.ssh && \
    chmod 700 /home/testuser/.ssh && \
    chown -R testuser:testuser /home/testuser/.ssh && \
    ssh-keygen -A

# Setup Podman for testuser (rootless)
RUN mkdir -p /home/testuser/.config/containers && \
    chown -R testuser:testuser /home/testuser/.config

# Copy entrypoint
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

# SSH port
EXPOSE 22

# The public key will be passed via AUTHORIZED_KEY environment variable

ENTRYPOINT ["/entrypoint.sh"]
