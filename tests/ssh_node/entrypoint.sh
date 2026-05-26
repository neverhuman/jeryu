#!/bin/bash
# Entrypoint for the SSH+DinD test node.
# Starts dockerd in the background, waits for it, then starts sshd.

set -e

# Inject public key if provided via environment variable.
if [ -n "$JERYU_TEST_SSH_PUBKEY" ]; then
    echo "$JERYU_TEST_SSH_PUBKEY" > /home/runner/.ssh/authorized_keys
    chown runner:runner /home/runner/.ssh/authorized_keys
    chmod 600 /home/runner/.ssh/authorized_keys
fi

# Start dockerd in the background.
echo "[entrypoint] Starting dockerd..."
dockerd --host=unix:///var/run/docker.sock \
        --host=tcp://0.0.0.0:2375 \
        --log-level=warn &
DOCKERD_PID=$!

# Wait for dockerd to be ready.
MAX_WAIT=30
WAITED=0
until docker info >/dev/null 2>&1; do
    sleep 1
    WAITED=$((WAITED + 1))
    if [ $WAITED -ge $MAX_WAIT ]; then
        echo "[entrypoint] dockerd did not start in ${MAX_WAIT}s" >&2
        exit 1
    fi
done
echo "[entrypoint] dockerd ready."

# Make the docker socket group-accessible for the runner user.
chmod 660 /var/run/docker.sock

# Start sshd in the foreground.
echo "[entrypoint] Starting sshd on port 22..."
exec /usr/sbin/sshd -D -e
