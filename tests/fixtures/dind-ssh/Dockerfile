# Docker-in-Docker with SSH server for integration tests.
# Build: docker build -t peleka-dind-ssh tests/fixtures/dind-ssh
# Run: docker run --privileged -p 2222:22 peleka-dind-ssh

FROM docker:27-dind

# Install OpenSSH server
RUN apk add --no-cache openssh-server

# Create test user
RUN adduser -D -s /bin/sh testuser && \
    echo "testuser:testuser" | chpasswd

# Setup SSH
RUN mkdir -p /home/testuser/.ssh && \
    chmod 700 /home/testuser/.ssh && \
    ssh-keygen -A

# Copy entrypoint
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

# SSH port
EXPOSE 22

# The public key will be mounted or passed via environment
# AUTHORIZED_KEY environment variable contains the public key

ENTRYPOINT ["/entrypoint.sh"]
