# ABOUTME: Docker-in-Docker with SSH server for integration tests.
# ABOUTME: Uses Debian with Docker for better SSH compatibility.

FROM debian:bookworm-slim

# Install Docker CE and OpenSSH
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        gnupg \
        openssh-server \
    && install -m 0755 -d /etc/apt/keyrings \
    && curl -fsSL https://download.docker.com/linux/debian/gpg | gpg --dearmor -o /etc/apt/keyrings/docker.gpg \
    && chmod a+r /etc/apt/keyrings/docker.gpg \
    && echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/debian bookworm stable" > /etc/apt/sources.list.d/docker.list \
    && apt-get update \
    && apt-get install -y --no-install-recommends docker-ce docker-ce-cli containerd.io \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

# Setup SSH
RUN mkdir /run/sshd && \
    ssh-keygen -A && \
    echo "AllowStreamLocalForwarding yes" >> /etc/ssh/sshd_config

# Create test user
RUN useradd -m -s /bin/bash testuser && \
    echo "testuser:testuser" | chpasswd && \
    mkdir -p /home/testuser/.ssh && \
    chmod 700 /home/testuser/.ssh && \
    chown -R testuser:testuser /home/testuser/.ssh && \
    usermod -aG docker testuser

COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

EXPOSE 22

ENTRYPOINT ["/entrypoint.sh"]
