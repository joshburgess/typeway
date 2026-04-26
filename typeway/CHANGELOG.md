# Changelog

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.1.0

### added:

- **Facade crate** re-exporting `typeway-core`, `typeway-server`, `typeway-macros`, `typeway-client`, and `typeway-openapi`
- **`prelude` module** with common imports for quick setup
- **Feature flags**: `server` (default), `client`, `openapi`, `axum-interop`, `tls`, `ws`, `multipart`, `full`

### changed:

- **MSRV**: Rust 1.88

### ci:

- Test (default + all features + no-default-features)
- Clippy with `-D warnings`
- Format check
- Documentation build
- MSRV verification (Rust 1.88)
- Dependency auditing (`cargo-deny` for advisories + licenses, `cargo-audit` for CVEs)
- Compile-time tracking (`cargo build --timings` artifacts)

### testing:

- Comprehensive trybuild test suite (10 pass + 9 fail cases with `.stderr` fixtures)
- Criterion benchmarks for routing and handler dispatch
- Property-based fuzz tests (proptest) for path parsing, JSON deserialization, query strings, raw body handling
- Adversarial body limit tests: boundary conditions, chunked encoding, mismatched Content-Length
- Panic safety integration tests
- Security headers integration tests
- Client retry integration tests
- RealWorld example: 19-endpoint Medium clone with Elm frontend, PostgreSQL, JWT auth, Docker Compose
