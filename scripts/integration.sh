#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
#
# Starts a clean local Golem server (and Kafka when needed), deploys the
# component, runs the given integration test targets, and tears everything down.
#
# Usage: scripts/integration.sh [--kafka] <test-target>...
#   e.g. scripts/integration.sh smoke propagation
set -euo pipefail

cd "$(dirname "$0")/.."

WITH_KAFKA=false
if [ "${1:-}" = "--kafka" ]; then
    WITH_KAFKA=true
    shift
fi

COMPOSE_FILE=test/integration-harness/docker/compose.yml

cleanup() {
    echo "Stopping Golem..."
    [ -n "${GOLEM_PID:-}" ] && kill "$GOLEM_PID" 2>/dev/null || true
    if [ "$WITH_KAFKA" = true ]; then
        echo "Stopping Kafka..."
        docker compose -f "$COMPOSE_FILE" down -v 2>/dev/null || true
    fi
}
trap cleanup EXIT

wait_for_port() {
    local port=$1 name=$2 seconds=$3 pid=${4:-}
    for _ in $(seq 1 "$seconds"); do
        nc -z 127.0.0.1 "$port" 2>/dev/null && { echo "$name ready."; return 0; }
        if [ -n "$pid" ] && ! kill -0 "$pid" 2>/dev/null; then
            echo "ERROR: $name exited before listening on port $port" >&2
            return 1
        fi
        sleep 1
    done
    echo "ERROR: $name did not start on port $port" >&2
    return 1
}

# The server accepts connections slightly before all its services are ready,
# so the first deploy can fail transiently.
deploy_with_retry() {
    for attempt in 1 2 3; do
        golem deploy --yes && return 0
        echo "Deploy attempt $attempt failed; retrying in 5s..." >&2
        sleep 5
    done
    return 1
}

if nc -z 127.0.0.1 9881 2>/dev/null; then
    echo "ERROR: something is already listening on port 9881; stop it first." >&2
    exit 1
fi

echo "==> Starting Golem server (log: golem-temp/server.log)..."
mkdir -p golem-temp
golem server run --clean --yes > golem-temp/server.log 2>&1 &
GOLEM_PID=$!
wait_for_port 9881 "Golem server" 120 "$GOLEM_PID" || { tail -20 golem-temp/server.log >&2; exit 1; }

echo "==> Building and deploying component..."
golem build --yes
deploy_with_retry

if [ "$WITH_KAFKA" = true ]; then
    echo "==> Starting Kafka..."
    docker compose -f "$COMPOSE_FILE" up -d
    wait_for_port 9092 "Kafka" 60
fi

TEST_ARGS=()
for target in "$@"; do
    TEST_ARGS+=(--test "$target")
done

echo "==> Running tests: $*"
cargo test -p integration-harness "${TEST_ARGS[@]}" -- --ignored --nocapture --test-threads=1
echo "==> All tests passed!"
