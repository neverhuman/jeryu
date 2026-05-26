# Docker-in-Docker + sshd image for remote-node integration tests.
#
# This image runs:
#   1. dockerd — so the SSH user can run `docker` commands
#   2. sshd     — so jeryu can SSH in and orchestrate containers
#
# Usage (see docker-compose.yml in this directory):
#   docker build -t jeryu-ssh-node -f tests/ssh_node/Dockerfile tests/ssh_node/
#   docker run --privileged -p 2222:22 jeryu-ssh-node
#
# The test injects a known keypair via $JERYU_TEST_SSH_PUBKEY (mounted or ENV).

FROM docker:27-dind

# Install openssh-server and bash
RUN apk add --no-cache openssh-server bash curl

# Generate host keys
RUN ssh-keygen -A

# Create a 'runner' user that can manage Docker containers.
# In tests, we inject the test key via /authorized_keys mount.
RUN adduser -D -s /bin/bash runner && \
    addgroup runner docker

# Authorized keys directory
RUN mkdir -p /home/runner/.ssh && chmod 700 /home/runner/.ssh && chown runner:runner /home/runner/.ssh

# sshd config: allow runner login, forward agent, no password auth.
RUN echo "PermitRootLogin no" >> /etc/ssh/sshd_config && \
    echo "PasswordAuthentication no" >> /etc/ssh/sshd_config && \
    echo "AllowTcpForwarding yes" >> /etc/ssh/sshd_config && \
    echo "PubkeyAuthentication yes" >> /etc/ssh/sshd_config && \
    echo "AuthorizedKeysFile .ssh/authorized_keys" >> /etc/ssh/sshd_config

# Entrypoint: start dockerd in background, then sshd in foreground.
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

EXPOSE 22

ENTRYPOINT ["/entrypoint.sh"]
