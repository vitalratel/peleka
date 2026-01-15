#!/bin/sh
# Entrypoint for DinD+SSH container.
# Starts Docker daemon and SSH server.

set -e

# Setup authorized key if provided
if [ -n "$AUTHORIZED_KEY" ]; then
    echo "$AUTHORIZED_KEY" > /home/testuser/.ssh/authorized_keys
    chmod 600 /home/testuser/.ssh/authorized_keys
    chown -R testuser:testuser /home/testuser/.ssh
fi

# Add testuser to docker group so they can access the socket
addgroup testuser docker 2>/dev/null || true

# Start Docker daemon in background
dockerd-entrypoint.sh dockerd &
DOCKER_PID=$!

# Wait for Docker to be ready
echo "Waiting for Docker daemon..."
for i in $(seq 1 30); do
    if docker info >/dev/null 2>&1; then
        echo "Docker daemon ready"
        break
    fi
    sleep 1
done

# Start SSH server in foreground
echo "Starting SSH server..."
exec /usr/sbin/sshd -D -e
