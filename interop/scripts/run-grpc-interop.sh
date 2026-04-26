#!/usr/bin/env bash
# Run the official grpc-go interop_client against the typeway-grpc interop server.
#
# Prerequisites:
#   1. Build the official grpc-go interop_client. From a clone of grpc-go:
#        go install google.golang.org/grpc/interop/client@latest
#      (binary lands at $(go env GOPATH)/bin/client)
#   2. Build the typeway-grpc interop server in release mode (see below).
#
# Usage:
#   scripts/run-grpc-interop.sh
#
# This script is illustrative. CI does not run it because it depends on a
# Go toolchain and a separate binary; the Rust integration tests in
# tests/unary_interop.rs cover the same scenarios at the wire level.

set -euo pipefail

PORT="${INTEROP_PORT:-50051}"
HOST="${INTEROP_HOST:-127.0.0.1}"

if ! command -v interop_client >/dev/null 2>&1 && ! command -v client >/dev/null 2>&1; then
    echo "error: official grpc-go interop_client not found in PATH" >&2
    echo "       install with: go install google.golang.org/grpc/interop/client@latest" >&2
    exit 1
fi

CLIENT_BIN="$(command -v interop_client || command -v client)"

echo "starting typeway-grpc interop server on ${HOST}:${PORT}..."
cargo run --release --bin interop-server --manifest-path "$(dirname "$0")/../Cargo.toml" -- "${HOST}:${PORT}" &
SERVER_PID=$!
trap 'kill ${SERVER_PID} 2>/dev/null || true' EXIT

sleep 2

CASES=(
    empty_unary
    large_unary
    status_code_and_message
    special_status_message
    unimplemented_method
    unimplemented_service
    cacheable_unary
    server_streaming
    client_streaming
    ping_pong
    empty_stream
)

for tc in "${CASES[@]}"; do
    echo "=== ${tc} ==="
    "${CLIENT_BIN}" \
        --server_host="${HOST}" \
        --server_port="${PORT}" \
        --use_tls=false \
        --test_case="${tc}"
done

echo "all cases passed"
