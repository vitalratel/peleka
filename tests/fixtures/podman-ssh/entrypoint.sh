#!/bin/bash
# Entrypoint for Podman+SSH container.
# Starts Podman socket service and SSH server.

set -e

# Setup authorized key if provided
if [ -n "$AUTHORIZED_KEY" ]; then
    echo "$AUTHORIZED_KEY" > /home/testuser/.ssh/authorized_keys
    chmod 600 /home/testuser/.ssh/authorized_keys
    chown testuser:testuser /home/testuser/.ssh/authorized_keys
fi

# Create XDG_RUNTIME_DIR for testuser (needed for rootless Podman)
TESTUSER_UID=$(id -u testuser)
mkdir -p /run/user/$TESTUSER_UID/podman
chown -R testuser:testuser /run/user/$TESTUSER_UID
chmod 700 /run/user/$TESTUSER_UID

# Start Podman socket service as testuser in background
echo "Starting Podman socket service..."
su - testuser -c "XDG_RUNTIME_DIR=/run/user/$TESTUSER_UID podman system service --time=0 unix:///run/user/$TESTUSER_UID/podman/podman.sock &"

# Wait for Podman socket to be ready
echo "Waiting for Podman socket..."
for i in $(seq 1 30); do
    if [ -S /run/user/$TESTUSER_UID/podman/podman.sock ]; then
        echo "Podman socket ready"
        break
    fi
    sleep 1
done

# Verify Podman is working
su - testuser -c "XDG_RUNTIME_DIR=/run/user/$TESTUSER_UID podman info" > /dev/null 2>&1 && echo "Podman is working" || echo "Warning: Podman may not be fully ready"

# Start SSH server in foreground
echo "Starting SSH server..."
exec /usr/sbin/sshd -D -e
