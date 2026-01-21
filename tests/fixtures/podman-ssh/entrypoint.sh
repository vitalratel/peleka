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

# Get gateway IP (how container reaches the host)
GATEWAY_IP=$(getent hosts host.containers.internal | awk '{print $1}')
echo "Gateway IP: $GATEWAY_IP"

# Add host IP to /etc/hosts mapping to gateway
# This allows container to reach Gitea when it redirects to host's external IP
if [ -n "$GITEA_HOST_IP" ] && [ -n "$GATEWAY_IP" ]; then
    echo "Adding hosts entry: $GITEA_HOST_IP -> $GATEWAY_IP"
    echo "$GATEWAY_IP $GITEA_HOST_IP" >> /etc/hosts
fi

# Configure insecure registry for local Gitea (HTTP only)
mkdir -p /etc/containers/registries.conf.d
cat > /etc/containers/registries.conf.d/gitea.conf << REGCONF
[[registry]]
location = "host.containers.internal:3000"
insecure = true
REGCONF

# Add host IP registry if provided
if [ -n "$GITEA_HOST_IP" ]; then
    cat >> /etc/containers/registries.conf.d/gitea.conf << REGCONF

[[registry]]
location = "${GITEA_HOST_IP}:3000"
insecure = true
REGCONF
fi

# Start rootful Podman socket (rootless doesn't work well in containers)
echo "Starting Podman socket service (rootful)..."
mkdir -p /run/podman
podman system service --time=0 unix:///run/podman/podman.sock &

# Wait for Podman socket to be ready
echo "Waiting for Podman socket..."
for i in $(seq 1 30); do
    if [ -S /run/podman/podman.sock ]; then
        # Make socket accessible to testuser
        chmod 666 /run/podman/podman.sock
        echo "Podman socket ready"
        break
    fi
    sleep 1
done

# Verify Podman is working
podman info > /dev/null 2>&1 && echo "Podman is working" || echo "Warning: Podman may not be fully ready"

# Pre-pull test image (alpine with shell for health checks)
echo "Pre-pulling test image..."
podman pull host.containers.internal:3000/vitalratel/alpine:3.19 2>&1 || echo "Warning: Failed to pre-pull test image"

# Start SSH server in foreground
echo "Starting SSH server..."
exec /usr/sbin/sshd -D -e
