#!/bin/bash
# ABOUTME: Entrypoint for Docker-in-Docker+SSH container.
# ABOUTME: Starts Docker daemon and SSH server.

set -e

# Setup authorized key if provided
if [ -n "$AUTHORIZED_KEY" ]; then
    echo "$AUTHORIZED_KEY" > /home/testuser/.ssh/authorized_keys
    chmod 600 /home/testuser/.ssh/authorized_keys
    chown testuser:testuser /home/testuser/.ssh/authorized_keys
fi

# Start containerd first
echo "Starting containerd..."
containerd &

# Wait for containerd socket
for i in $(seq 1 30); do
    if [ -S /run/containerd/containerd.sock ]; then
        echo "containerd ready"
        break
    fi
    sleep 1
done

# Start Docker daemon with vfs storage driver (required for nested containers)
echo "Starting Docker daemon..."
dockerd --storage-driver=vfs &

# Wait for Docker socket to be ready
echo "Waiting for Docker socket..."
for i in $(seq 1 60); do
    if [ -S /var/run/docker.sock ]; then
        # Make socket accessible to testuser (via docker group)
        chmod 666 /var/run/docker.sock
        echo "Docker socket ready"
        break
    fi
    sleep 1
done

# Verify Docker is working
docker info > /dev/null 2>&1 && echo "Docker is working" || echo "Warning: Docker may not be fully ready"

# Pre-pull test image
echo "Pre-pulling test image..."
docker pull alpine:3.19 2>&1 || echo "Warning: Failed to pre-pull test image"

# Start SSH server in foreground
echo "Starting SSH server..."
exec /usr/sbin/sshd -D -e
