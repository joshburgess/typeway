# typeway-interop

gRPC interoperability test suite for `typeway-grpc`.

This crate hosts the upstream [`grpc.testing.TestService`][grpc-interop]
on top of typeway-grpc's framing, status, and trailer-body primitives. It
exists to give the project a wire-level conformance signal independent of
the Rust unit tests.

[grpc-interop]: https://github.com/grpc/grpc/blob/master/doc/interop-test-descriptions.md

## What's covered

The suite is split across two layers:

1. **Rust integration tests** (`tests/unary_interop.rs`). These run as
   part of `cargo test` and exercise the same scenarios the official
   `interop_client` exercises, encoding requests with `prost` exactly as
   upstream does:

   - `empty_unary`
   - `large_unary` (271,828-byte request, 314,159-byte response)
   - `status_code_and_message`
   - `special_status_message` (tab/CR/LF/CJK round-trip via
     percent-encoded `grpc-message`)
   - `unimplemented_method`
   - `unimplemented_service`
   - `cacheable_unary` (smoke test, no GFE proxy)

2. **Standalone server binary** (`src/main.rs`). The
   `interop-server` binary listens on a TCP port and serves the same
   `grpc.testing.TestService` so the upstream `grpc-go` `interop_client`
   can drive it directly:

   ```sh
   cargo run --release --bin interop-server -- 127.0.0.1:50051
   # in another shell:
   interop_client --server_host=127.0.0.1 --server_port=50051 \
       --use_tls=false --test_case=empty_unary
   ```

   `scripts/run-grpc-interop.sh` automates that pattern across all
   supported test cases.

## What's not covered yet

The streaming scenarios (`server_streaming`, `client_streaming`,
`ping_pong`, `empty_stream`, `client_compressed_streaming`,
`server_compressed_streaming`) currently return `UNIMPLEMENTED`.
typeway-grpc's `DirectHandler` API is unary-only; streaming interop
is tracked in the typeway-grpc design doc as future work.

The TLS, OAuth, and JWT scenarios from the upstream description are
also out of scope: they exercise auth integrations rather than gRPC
wire compliance, and typeway-grpc's auth story is layered above the
transport.

## Running

```sh
# Rust integration tests:
cargo test -p typeway-interop

# Server binary, for use with the official grpc-go client:
cargo run --release --bin interop-server -- 127.0.0.1:50051
```

The Rust tests are wired into the workspace's standard `cargo test` run
and so cover the conformance signal automatically. The shell script that
drives the upstream `interop_client` is illustrative only and is not run
by CI (it depends on a Go toolchain).
