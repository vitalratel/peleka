#!/bin/sh
# ABOUTME: Entrypoint for SSH-only test container.
# ABOUTME: Sets up authorized key and starts SSH server.

set -e

if [ -n "$AUTHORIZED_KEY" ]; then
    echo "$AUTHORIZED_KEY" > /home/testuser/.ssh/authorized_keys
    chmod 600 /home/testuser/.ssh/authorized_keys
    chown testuser:testuser /home/testuser/.ssh/authorized_keys
fi

exec /usr/sbin/sshd -D -e
