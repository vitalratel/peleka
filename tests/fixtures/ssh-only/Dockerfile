# ABOUTME: Minimal SSH server for testing lock and other non-container operations.
# ABOUTME: Uses alpine from local Gitea registry to avoid Docker Hub rate limits.

FROM localhost:3000/vitalratel/alpine:3.19

RUN apk add --no-cache openssh-server && \
    ssh-keygen -A && \
    adduser -D -s /bin/sh testuser && \
    passwd -u testuser && \
    mkdir -p /home/testuser/.ssh && \
    chmod 700 /home/testuser/.ssh && \
    chown testuser:testuser /home/testuser/.ssh

COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

EXPOSE 22

CMD ["/entrypoint.sh"]
