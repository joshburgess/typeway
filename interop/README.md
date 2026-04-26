# typeway-interop

gRPC interoperability test suite for `typeway-grpc`.

This crate hosts the upstream [`grpc.testing.TestService`][grpc-interop]
on top of typeway-grpc's framing, status, and trailer-body primitives. It
exists to give the project a wire-level conformance signal independent of
the Rust unit tests.

[grpc-interop]: https://github.com/grpc/grpc/blob/master/doc/interop-test-descriptions.md

## What's covered

The suite is split across two layers:

1. **Rust integration tests.** These run as part of `cargo test` and
   exercise the same scenarios the official `interop_client` exercises,
   encoding requests with `prost` exactly as upstream does:

   Unary (`tests/unary_interop.rs`):
   - `empty_unary`
   - `large_unary` (271,828-byte request, 314,159-byte response)
   - `status_code_and_message`
   - `special_status_message` (tab/CR/LF/CJK round-trip via
     percent-encoded `grpc-message`)
   - `unimplemented_method`
   - `unimplemented_service`
   - `cacheable_unary` (smoke test, no GFE proxy)

   Streaming (`tests/streaming_interop.rs`):
   - `server_streaming` (StreamingOutputCall, sizes
     31415/9/2653/58979)
   - `client_streaming` (StreamingInputCall, sizes
     27182/8/1828/45904)
   - `ping_pong` (FullDuplexCall)
   - `empty_stream` (FullDuplexCall, no input or output frames)
   - `half_duplex` (smoke)

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

## What's not covered

- **Compression scenarios** (`client_compressed_streaming`,
  `server_compressed_streaming`, `client_compressed_unary`,
  `server_compressed_unary`). typeway-grpc's framing layer rejects
  compressed frames at decode time; gRPC compression is on the
  roadmap.
- **TLS / OAuth / JWT scenarios.** These exercise auth integrations
  rather than gRPC wire compliance, and typeway-grpc's auth story is
  layered above the transport.

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
